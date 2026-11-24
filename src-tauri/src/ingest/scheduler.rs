// SPDX-License-Identifier: GPL-3.0-or-later
//! Decides how much indexing runs from the user's schedule settings and the machine's state
//! (search focus, power, idle time, thermal), and drives the `IndexQueue` with it. `decide`
//! and `plan` are pure; `Scheduler` is the thread that feeds them signals.

use std::sync::mpsc::{channel, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use super::queue::IndexQueue;
use crate::storage::db::Db;
use crate::storage::error::StorageError;

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
use macos::{local_minute_of_day, MacIdle, MacPower};

pub const SCHEDULE_EVENT: &str = "index://schedule";
const SETTINGS_KEY: &str = "index_schedule";
pub const IDLE_AFTER: Duration = Duration::from_secs(5 * 60);
const LOW_BATTERY_PERCENT: u8 = 20;
/// Power, idle, and the clock are polled; focus and settings changes apply at once.
const POLL: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    #[default]
    Auto,
    Paused,
    /// Full speed, ignoring search, power, and allowed hours.
    RunNow,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "snake_case")]
pub enum LowPowerPolicy {
    #[default]
    Pause,
    OneWorker,
}

/// Local minutes since midnight, `[start, end)`; wraps past midnight when `end < start`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct AllowedHours {
    pub start_minute: u16,
    pub end_minute: u16,
}

impl AllowedHours {
    fn is_valid(self) -> bool {
        self.start_minute < 1440 && self.end_minute < 1440 && self.start_minute != self.end_minute
    }

    fn contains(self, minute: u16) -> bool {
        let (s, e) = (self.start_minute, self.end_minute);
        if s < e {
            (s..e).contains(&minute)
        } else {
            minute >= s || minute < e
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct ScheduleSettings {
    pub mode: Mode,
    /// None: any time.
    pub allowed_hours: Option<AllowedHours>,
    pub low_power: LowPowerPolicy,
}

impl ScheduleSettings {
    pub fn is_valid(&self) -> bool {
        self.allowed_hours.is_none_or(AllowedHours::is_valid)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "snake_case")]
pub enum SchedState {
    Paused,
    RunNow,
    LowPower,
    Interactive,
    Idle,
    Normal,
}

/// Why the scheduler is in its state. Earlier variants win.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "snake_case")]
pub enum Reason {
    UserPaused,
    UserRunNow,
    OutsideHours,
    Thermal,
    LowPowerMode,
    LowBattery,
    Searching,
    UserIdle,
    Default,
}

impl Reason {
    pub fn state(self) -> SchedState {
        match self {
            Self::UserPaused | Self::OutsideHours => SchedState::Paused,
            Self::UserRunNow => SchedState::RunNow,
            Self::Thermal | Self::LowPowerMode | Self::LowBattery => SchedState::LowPower,
            Self::Searching => SchedState::Interactive,
            Self::UserIdle => SchedState::Idle,
            Self::Default => SchedState::Normal,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord)]
pub enum Thermal {
    #[default]
    Nominal,
    Fair,
    Serious,
    Critical,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Power {
    pub on_battery: bool,
    /// None without a battery.
    pub charge_percent: Option<u8>,
    pub low_power_mode: bool,
    pub thermal: Thermal,
}

pub trait PowerSource: Send {
    fn read(&self) -> Power;
}

pub trait IdleSource: Send {
    /// Time since the last keyboard or pointer input.
    fn idle(&self) -> Duration;
}

#[derive(Debug, Clone, Copy, Default)]
pub struct Signals {
    /// The quick-search window is open, or a search just ran.
    pub searching: bool,
    pub power: Power,
    pub idle: Duration,
    /// Local minutes since midnight.
    pub minute_of_day: u16,
}

pub fn decide(s: &Signals, settings: &ScheduleSettings) -> Reason {
    match settings.mode {
        Mode::Paused => return Reason::UserPaused,
        Mode::RunNow => return Reason::UserRunNow,
        Mode::Auto => {}
    }
    if settings
        .allowed_hours
        .is_some_and(|h| !h.contains(s.minute_of_day))
    {
        return Reason::OutsideHours;
    }
    let p = s.power;
    if p.thermal >= Thermal::Serious {
        Reason::Thermal
    } else if p.low_power_mode {
        Reason::LowPowerMode
    } else if p.on_battery && p.charge_percent.is_some_and(|c| c <= LOW_BATTERY_PERCENT) {
        Reason::LowBattery
    } else if s.searching {
        Reason::Searching
    } else if s.idle >= IDLE_AFTER {
        Reason::UserIdle
    } else {
        Reason::Default
    }
}

/// What the queue runs. `max` is the number of parse workers spawned (cores / 2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct Plan {
    #[cfg_attr(test, ts(type = "number"))]
    pub parse_workers: usize,
    pub embedding: bool,
}

pub fn plan(reason: Reason, low_power: LowPowerPolicy, max: usize) -> Plan {
    let (parse_workers, embedding) = match reason {
        Reason::UserPaused | Reason::OutsideHours => (0, false),
        Reason::UserRunNow | Reason::UserIdle => (max, true),
        Reason::Thermal | Reason::LowPowerMode | Reason::LowBattery => match low_power {
            LowPowerPolicy::Pause => (0, false),
            LowPowerPolicy::OneWorker => (1, true),
        },
        Reason::Searching => (1, false),
        Reason::Default => ((max / 2).max(1), true),
    };
    Plan {
        parse_workers: parse_workers.min(max),
        embedding,
    }
}

/// Runs a plan: zero workers pauses the queue, so its progress reads "paused".
pub fn apply(queue: &IndexQueue, p: Plan) {
    queue.set_embedding(p.embedding);
    if p.parse_workers == 0 {
        queue.pause();
    } else {
        queue.set_concurrency(p.parse_workers);
        queue.resume();
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct Current {
    pub state: SchedState,
    pub reason: Reason,
    pub plan: Plan,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct ScheduleStatus {
    pub settings: ScheduleSettings,
    /// None until indexing starts.
    pub current: Option<Current>,
}

/// Saved settings; unreadable or invalid ones fall back to the defaults.
pub fn load(db: &Db) -> ScheduleSettings {
    db.setting(SETTINGS_KEY)
        .ok()
        .flatten()
        .and_then(|s| serde_json::from_str::<ScheduleSettings>(&s).ok())
        .filter(ScheduleSettings::is_valid)
        .unwrap_or_default()
}

pub fn save(db: &Db, settings: &ScheduleSettings) -> Result<(), StorageError> {
    // Serializing plain enums and integers cannot fail.
    let json = serde_json::to_string(settings).unwrap_or_default();
    db.set_setting(SETTINGS_KEY, &json)
}

pub struct Sources {
    pub power: Box<dyn PowerSource>,
    pub idle: Box<dyn IdleSource>,
    pub clock: fn() -> u16,
}

/// This machine's signals. Other platforms report AC power, no idle time, and noon.
pub fn system_sources() -> Sources {
    #[cfg(target_os = "macos")]
    return Sources {
        power: Box::new(MacPower),
        idle: Box::new(MacIdle),
        clock: local_minute_of_day,
    };
    #[cfg(not(target_os = "macos"))]
    {
        struct Unknown;
        impl PowerSource for Unknown {
            fn read(&self) -> Power {
                Power::default()
            }
        }
        impl IdleSource for Unknown {
            fn idle(&self) -> Duration {
                Duration::ZERO
            }
        }
        Sources {
            power: Box::new(Unknown),
            idle: Box::new(Unknown),
            clock: || 12 * 60,
        }
    }
}

enum Msg {
    Searching(bool),
    Settings(ScheduleSettings),
}

pub struct Scheduler {
    tx: Mutex<Sender<Msg>>,
    status: Arc<Mutex<Current>>,
    settings: Arc<Mutex<ScheduleSettings>>,
    thread: Option<JoinHandle<()>>,
}

impl Scheduler {
    /// Applies the first plan before returning, then re-decides on every message and poll.
    /// `on_change` runs on the scheduler thread with each new decision.
    pub fn start(
        settings: ScheduleSettings,
        sources: Sources,
        max_workers: usize,
        on_change: impl Fn(Current) + Send + 'static,
    ) -> std::io::Result<Self> {
        let mut signals = Signals::default();
        let read = move |s: &mut Signals| {
            s.power = sources.power.read();
            s.idle = sources.idle.idle();
            s.minute_of_day = (sources.clock)();
        };
        read(&mut signals);
        let current = move |sig: &Signals, set: &ScheduleSettings| {
            let reason = decide(sig, set);
            Current {
                state: reason.state(),
                reason,
                plan: plan(reason, set.low_power, max_workers),
            }
        };
        let first = current(&signals, &settings);
        on_change(first);
        let status = Arc::new(Mutex::new(first));
        let shared_settings = Arc::new(Mutex::new(settings));
        let (tx, rx) = channel();
        let st = status.clone();
        let thread = std::thread::Builder::new()
            .name("index-scheduler".into())
            .spawn(move || {
                let mut settings = settings;
                loop {
                    match rx.recv_timeout(POLL) {
                        Ok(Msg::Searching(on)) => signals.searching = on,
                        Ok(Msg::Settings(s)) => settings = s,
                        Err(RecvTimeoutError::Timeout) => {}
                        Err(RecvTimeoutError::Disconnected) => return,
                    }
                    read(&mut signals);
                    let next = current(&signals, &settings);
                    let mut cur = st.lock().unwrap_or_else(|e| e.into_inner());
                    if *cur != next {
                        *cur = next;
                        drop(cur);
                        tracing::info!(state = ?next.state, reason = ?next.reason, "index schedule changed");
                        on_change(next);
                    }
                }
            })?;
        Ok(Self {
            tx: Mutex::new(tx),
            status,
            settings: shared_settings,
            thread: Some(thread),
        })
    }

    fn send(&self, m: Msg) {
        // The thread only exits once this sender is dropped, so a send never fails.
        let _ = self.tx.lock().unwrap_or_else(|e| e.into_inner()).send(m);
    }

    pub fn set_searching(&self, on: bool) {
        self.send(Msg::Searching(on));
    }

    /// Saves the settings, then applies them.
    pub fn change(&self, db: &Db, s: ScheduleSettings) -> Result<(), StorageError> {
        save(db, &s)?;
        self.set_settings(s);
        Ok(())
    }

    pub fn set_settings(&self, s: ScheduleSettings) {
        *self.settings.lock().unwrap_or_else(|e| e.into_inner()) = s;
        self.send(Msg::Settings(s));
    }

    pub fn settings(&self) -> ScheduleSettings {
        *self.settings.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn current(&self) -> Current {
        *self.status.lock().unwrap_or_else(|e| e.into_inner())
    }
}

impl Drop for Scheduler {
    fn drop(&mut self) {
        // Replacing the sender drops the only one, which ends the thread's loop.
        *self.tx.lock().unwrap_or_else(|e| e.into_inner()) = channel().0;
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc::Receiver;
    use std::time::Instant;

    const MIN: Duration = Duration::from_secs(60);

    fn hours(start_h: u16, end_h: u16) -> Option<AllowedHours> {
        Some(AllowedHours {
            start_minute: start_h * 60,
            end_minute: end_h * 60,
        })
    }

    fn battery(pct: u8) -> Power {
        Power {
            on_battery: true,
            charge_percent: Some(pct),
            ..Power::default()
        }
    }

    #[test]
    fn decide_table() {
        let auto = ScheduleSettings::default();
        let night = ScheduleSettings {
            allowed_hours: hours(22, 7),
            ..auto
        };
        let day = ScheduleSettings {
            allowed_hours: hours(9, 17),
            ..auto
        };
        let mode = |mode| ScheduleSettings { mode, ..night };
        let at = |h: u16| h * 60;
        let thermal = |thermal| Power {
            thermal,
            ..Power::default()
        };
        let lpm = Power {
            low_power_mode: true,
            ..Power::default()
        };
        let charging_low = Power {
            on_battery: false,
            charge_percent: Some(5),
            ..Power::default()
        };
        #[rustfmt::skip]
        let table: &[(&str, bool, Power, Duration, u16, ScheduleSettings, Reason)] = &[
            ("default", false, Power::default(), MIN, 12, auto, Reason::Default),
            ("search window open", true, Power::default(), MIN, 12, auto, Reason::Searching),
            ("idle 5 min", false, Power::default(), 5 * MIN, 12, auto, Reason::UserIdle),
            ("idle 4:59", false, Power::default(), 5 * MIN - Duration::from_secs(1), 12, auto, Reason::Default),
            ("searching beats idle", true, Power::default(), 9 * MIN, 12, auto, Reason::Searching),
            ("battery 20%", false, battery(20), MIN, 12, auto, Reason::LowBattery),
            ("battery 21%", false, battery(21), MIN, 12, auto, Reason::Default),
            ("low and charging", false, charging_low, MIN, 12, auto, Reason::Default),
            ("low power mode", false, lpm, MIN, 12, auto, Reason::LowPowerMode),
            ("thermal fair", false, thermal(Thermal::Fair), MIN, 12, auto, Reason::Default),
            ("thermal serious", false, thermal(Thermal::Serious), MIN, 12, auto, Reason::Thermal),
            ("thermal critical", false, thermal(Thermal::Critical), MIN, 12, auto, Reason::Thermal),
            ("low power beats searching", true, battery(10), MIN, 12, auto, Reason::LowBattery),
            ("low power beats idle", false, lpm, 30 * MIN, 12, auto, Reason::LowPowerMode),
            ("inside wrapped hours late", false, Power::default(), MIN, 23, night, Reason::Default),
            ("inside wrapped hours early", false, Power::default(), MIN, 6, night, Reason::Default),
            ("end hour excluded", false, Power::default(), MIN, 7, night, Reason::OutsideHours),
("inside day hours", false, Power::default(), MIN, 9, day, Reason::Default),
("day end hour excluded", false, Power::default(), MIN, 17, day, Reason::OutsideHours),
            ("outside hours", false, Power::default(), 30 * MIN, 12, night, Reason::OutsideHours),
            ("outside hours beats low power", false, lpm, MIN, 12, night, Reason::OutsideHours),
            ("pause beats everything", true, battery(5), 30 * MIN, 23, mode(Mode::Paused), Reason::UserPaused),
            ("run now beats hours", false, Power::default(), MIN, 12, mode(Mode::RunNow), Reason::UserRunNow),
            ("run now beats power and search", true, lpm, MIN, 23, mode(Mode::RunNow), Reason::UserRunNow),
        ];
        for (name, searching, power, idle, hour, settings, want) in table {
            let s = Signals {
                searching: *searching,
                power: *power,
                idle: *idle,
                minute_of_day: at(*hour),
            };
            assert_eq!(decide(&s, settings), *want, "{name}");
        }
    }

    #[test]
    fn plan_table() {
        use LowPowerPolicy::*;
        let p = |parse_workers, embedding| Plan {
            parse_workers,
            embedding,
        };
        #[rustfmt::skip]
        let table = [
            (Reason::Default, Pause, 5, p(2, true)),
            (Reason::Default, Pause, 1, p(1, true)),
            (Reason::UserIdle, Pause, 5, p(5, true)),
            (Reason::UserRunNow, Pause, 5, p(5, true)),
            (Reason::Searching, Pause, 5, p(1, false)),
            (Reason::LowBattery, Pause, 5, p(0, false)),
            (Reason::Thermal, OneWorker, 5, p(1, true)),
            (Reason::LowPowerMode, OneWorker, 5, p(1, true)),
            (Reason::UserPaused, OneWorker, 5, p(0, false)),
            (Reason::OutsideHours, OneWorker, 5, p(0, false)),
        ];
        for (reason, policy, max, want) in table {
            assert_eq!(
                plan(reason, policy, max),
                want,
                "{reason:?} {policy:?} {max}"
            );
        }
    }

    #[test]
    fn allowed_hours_validation() {
        let h = |s, e| AllowedHours {
            start_minute: s,
            end_minute: e,
        };
        assert!(h(22 * 60, 7 * 60).is_valid());
        assert!(!h(60, 60).is_valid(), "empty range");
        assert!(!h(0, 1440).is_valid(), "past midnight");
    }

    #[test]
    fn settings_round_trip_and_bad_json_falls_back() {
        let (_d, db) = crate::storage::db::tests::temp_db();
        assert_eq!(load(&db), ScheduleSettings::default());
        let s = ScheduleSettings {
            mode: Mode::Paused,
            allowed_hours: hours(22, 7),
            low_power: LowPowerPolicy::OneWorker,
        };
        save(&db, &s).unwrap();
        assert_eq!(load(&db), s);
        db.set_setting(SETTINGS_KEY, r#"{"mode":"warp"}"#).unwrap();
        assert_eq!(load(&db), ScheduleSettings::default());
        let empty_range = r#"{"mode":"paused","allowedHours":{"startMinute":60,"endMinute":60},"lowPower":"pause"}"#;
        db.set_setting(SETTINGS_KEY, empty_range).unwrap();
        assert_eq!(load(&db), ScheduleSettings::default());
    }

    /// Values the test sets; the scheduler thread reads them on its next poll or message.
    #[derive(Clone, Default)]
    struct Fake(Arc<Mutex<(Power, Duration)>>);

    impl PowerSource for Fake {
        fn read(&self) -> Power {
            self.0.lock().unwrap().0
        }
    }

    impl IdleSource for Fake {
        fn idle(&self) -> Duration {
            self.0.lock().unwrap().1
        }
    }

    fn noon() -> u16 {
        12 * 60
    }

    fn recv(rx: &Receiver<Current>) -> Current {
        rx.recv_timeout(Duration::from_secs(5)).expect("a decision")
    }

    #[test]
    fn scheduler_follows_focus_power_and_manual_controls() {
        let fake = Fake::default();
        let (tx, rx) = channel();
        let sched = Scheduler::start(
            ScheduleSettings::default(),
            Sources {
                power: Box::new(fake.clone()),
                idle: Box::new(fake.clone()),
                clock: noon,
            },
            4,
            move |c| tx.send(c).unwrap(),
        )
        .unwrap();
        assert_eq!(recv(&rx).reason, Reason::Default);

        let started = Instant::now();
        sched.set_searching(true);
        let c = recv(&rx);
        assert_eq!(
            (c.state, c.plan.parse_workers, c.plan.embedding),
            (SchedState::Interactive, 1, false)
        );
        assert!(
            started.elapsed() < POLL,
            "focus applies without waiting for a poll"
        );

        // Power is read on every message, so a low battery shows up with the next one.
        fake.0.lock().unwrap().0 = battery(15);
        sched.set_searching(false);
        let c = recv(&rx);
        assert_eq!(
            (c.state, c.reason, c.plan.parse_workers),
            (SchedState::LowPower, Reason::LowBattery, 0)
        );

        sched.set_settings(ScheduleSettings {
            mode: Mode::RunNow,
            ..ScheduleSettings::default()
        });
        assert_eq!(
            recv(&rx).plan,
            Plan {
                parse_workers: 4,
                embedding: true
            }
        );
        assert_eq!(sched.settings().mode, Mode::RunNow);

        sched.set_settings(ScheduleSettings {
            mode: Mode::Paused,
            ..ScheduleSettings::default()
        });
        assert_eq!(recv(&rx).reason, Reason::UserPaused);
        assert_eq!(sched.current().state, SchedState::Paused);

        // An unchanged decision is not reported again.
        sched.set_searching(true);
        assert!(rx.recv_timeout(Duration::from_millis(200)).is_err());
        drop(sched);
    }
}
