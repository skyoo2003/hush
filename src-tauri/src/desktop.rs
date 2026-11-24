// SPDX-License-Identifier: GPL-3.0-or-later
//! Desktop shell: the quick-search window and its global shortcut, the tray, and the
//! Dock policy (Accessory, Regular only while the main window is open). No API here
//! needs the Accessibility permission.

use std::process::Command;
use std::str::FromStr;
use std::sync::Arc;

use tauri::menu::{Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Emitter, LogicalPosition, LogicalSize, Manager, Runtime, WindowEvent};
use tauri_plugin_global_shortcut::{GlobalShortcut, GlobalShortcutExt, Shortcut, ShortcutState};

use crate::brand;
use crate::ingest::queue::IndexProgress;
use crate::ingest::scheduler::{Current, Mode, Reason, SchedState, Scheduler};
use crate::storage::db::Db;
use crate::storage::error::StorageError;

/// Payload `true` when the quick-search window opens and on each search; `false` when it hides.
pub const FOCUS_EVENT: &str = "search://focus";
/// Not Alt+Space: Alfred and Raycast use it, and macOS lets both apps register it, so
/// registration succeeds while the other app takes focus and the popup hides on blur.
pub const DEFAULT_SHORTCUT: &str = "Cmd+Shift+Space";
const SHORTCUT_SETTING: &str = "quick_search_shortcut";
const MAIN: &str = "main";
const QUICK_SEARCH: &str = "quick-search";
/// Must match the quick-search window size in tauri.conf.json.
const QUICK_SEARCH_SIZE: LogicalSize<f64> = LogicalSize::new(680.0, 460.0);

#[derive(Debug, thiserror::Error)]
pub enum ShortcutError {
    #[error("shortcut is not valid")]
    Invalid,
    #[error("shortcut is in use by another app")]
    Unavailable,
    #[error(transparent)]
    Storage(#[from] StorageError),
}

/// Runs once in the app's setup hook, after storage and `net::Client` are managed.
pub fn setup<R: Runtime>(app: &AppHandle<R>, db: &Db, air_gap: bool) -> tauri::Result<()> {
    #[cfg(target_os = "macos")]
    app.set_activation_policy(tauri::ActivationPolicy::Accessory)?;
    app.plugin(
        tauri_plugin_global_shortcut::Builder::new()
            .with_handler(|app, _, event| {
                if event.state() == ShortcutState::Pressed {
                    toggle_quick_search(app);
                }
            })
            .build(),
    )?;
    build_tray(app)?;
    set_tray_air_gap(app, air_gap);
    let registered = Shortcut::from_str(&saved_shortcut(db))
        .ok()
        .is_some_and(|s| app.global_shortcut().register(s).is_ok());
    if !registered {
        tracing::warn!("quick-search shortcut not registered");
    }
    // The main window opens at launch only when it has something to tell or ask.
    let no_folders = db.folders().map(|f| f.is_empty()).unwrap_or(true);
    if !registered || no_folders {
        show_main(app);
    }
    Ok(())
}

pub fn saved_shortcut(db: &Db) -> String {
    db.setting(SHORTCUT_SETTING)
        .ok()
        .flatten()
        .unwrap_or_else(|| DEFAULT_SHORTCUT.into())
}

/// Whether this app holds `shortcut`. False before the plugin starts.
pub fn is_registered<R: Runtime>(app: &AppHandle<R>, shortcut: &str) -> bool {
    app.try_state::<GlobalShortcut<R>>()
        .is_some_and(|g| g.is_registered(shortcut))
}

/// Swaps the quick-search shortcut and saves it. On any failure the old one stays registered.
pub fn set_shortcut<R: Runtime>(
    app: &AppHandle<R>,
    db: &Db,
    shortcut: &str,
) -> Result<String, ShortcutError> {
    let new = Shortcut::from_str(shortcut).map_err(|_| ShortcutError::Invalid)?;
    let gs = app.global_shortcut();
    let old = Shortcut::from_str(&saved_shortcut(db)).ok();
    let _ = gs.unregister_all();
    let restore = || {
        let _ = gs.unregister_all();
        if let Some(old) = old {
            let _ = gs.register(old);
        }
    };
    if gs.register(new).is_err() {
        restore();
        return Err(ShortcutError::Unavailable);
    }
    let text = new.into_string();
    if let Err(e) = db.set_setting(SHORTCUT_SETTING, &text) {
        restore();
        return Err(e.into());
    }
    Ok(text)
}

pub fn on_window_event<R: Runtime>(window: &tauri::Window<R>, event: &WindowEvent) {
    match (window.label(), event) {
        (MAIN, WindowEvent::CloseRequested { api, .. }) => {
            api.prevent_close();
            let _ = window.hide();
            #[cfg(target_os = "macos")]
            let _ = window
                .app_handle()
                .set_activation_policy(tauri::ActivationPolicy::Accessory);
        }
        (QUICK_SEARCH, WindowEvent::Focused(false)) => hide_quick_search(window.app_handle()),
        _ => {}
    }
}

pub fn show_main<R: Runtime>(app: &AppHandle<R>) {
    let Some(w) = app.get_webview_window(MAIN) else {
        return;
    };
    #[cfg(target_os = "macos")]
    let _ = app.set_activation_policy(tauri::ActivationPolicy::Regular);
    if let Err(e) = w
        .show()
        .and_then(|()| w.unminimize())
        .and_then(|()| w.set_focus())
    {
        tracing::warn!(error = %e, "main window show failed");
    }
}

fn toggle_quick_search<R: Runtime>(app: &AppHandle<R>) {
    let Some(w) = app.get_webview_window(QUICK_SEARCH) else {
        return;
    };
    if w.is_visible().unwrap_or(false) {
        hide_quick_search(app);
        return;
    }
    if let Some(p) = cursor_origin(app) {
        let _ = w.set_position(p);
    }
    // A hidden app (see `hide_quick_search`) shows no windows until it is unhidden.
    #[cfg(target_os = "macos")]
    let _ = app.show();
    match w.show().and_then(|()| w.set_focus()) {
        Ok(()) => {
            let _ = app.emit(FOCUS_EVENT, true);
        }
        Err(e) => tracing::warn!(error = %e, "quick-search show failed"),
    }
}

/// Idempotent: a hidden window emits nothing, so blur after Esc reports `false` once.
pub fn hide_quick_search<R: Runtime>(app: &AppHandle<R>) {
    let Some(w) = app.get_webview_window(QUICK_SEARCH) else {
        return;
    };
    if !w.is_visible().unwrap_or(false) {
        return;
    }
    match w.hide() {
        Ok(()) => {
            // Hiding the app returns focus to the app the user came from.
            #[cfg(target_os = "macos")]
            if !app
                .get_webview_window(MAIN)
                .is_some_and(|m| m.is_visible().unwrap_or(false))
            {
                let _ = app.hide();
            }
            let _ = app.emit(FOCUS_EVENT, false);
        }
        Err(e) => tracing::warn!(error = %e, "quick-search hide failed"),
    }
}

/// On macOS the cursor is scaled by the primary monitor's factor and each monitor by its
/// own, so everything goes back to logical points, which `set_position` takes as is.
fn cursor_origin<R: Runtime>(app: &AppHandle<R>) -> Option<LogicalPosition<f64>> {
    let primary = app.primary_monitor().ok()??.scale_factor();
    let cursor = app.cursor_position().ok()?.to_logical(primary);
    let screens: Vec<_> = app
        .available_monitors()
        .ok()?
        .iter()
        .map(|m| {
            let s = m.scale_factor();
            (m.position().to_logical(s), m.size().to_logical(s))
        })
        .collect();
    quick_search_origin(cursor, &screens)
}

/// Centered horizontally on the screen under the cursor, top edge a quarter of the way down,
/// kept on screen. Falls back to the first screen if none holds the cursor.
fn quick_search_origin(
    cursor: LogicalPosition<f64>,
    screens: &[(LogicalPosition<f64>, LogicalSize<f64>)],
) -> Option<LogicalPosition<f64>> {
    let (pos, size) = screens
        .iter()
        .find(|(p, s)| {
            (p.x..p.x + s.width).contains(&cursor.x) && (p.y..p.y + s.height).contains(&cursor.y)
        })
        .or(screens.first())?;
    let w = QUICK_SEARCH_SIZE;
    let y = (pos.y + size.height / 4.0).min(pos.y + size.height - w.height);
    Some(LogicalPosition::new(
        pos.x + (size.width - w.width) / 2.0,
        y.max(pos.y),
    ))
}

// Tray.

struct Tray<R: Runtime> {
    index: MenuItem<R>,
    schedule: MenuItem<R>,
    pause: MenuItem<R>,
    air_gap: MenuItem<R>,
}

fn build_tray<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<()> {
    let item =
        |id: &str, text: &str, enabled| MenuItem::with_id(app, id, text, enabled, None::<&str>);
    let index = item("index", &index_label(None), false)?;
    let schedule = item("schedule", &schedule_label(None), false)?;
    let pause = item("pause", "Pause Indexing", false)?;
    let air_gap = item("air_gap", "Air-Gap: On", false)?;
    let sponsors = brand::SPONSOR_URLS
        .iter()
        .enumerate()
        .map(|(i, url)| {
            item(
                &format!("sponsor:{i}"),
                url.trim_start_matches("https://"),
                true,
            )
        })
        .collect::<tauri::Result<Vec<_>>>()?;
    let sponsor_refs: Vec<&dyn tauri::menu::IsMenuItem<R>> =
        sponsors.iter().map(|s| s as _).collect();
    let menu = Menu::with_items(
        app,
        &[
            &item("search", "Search", true)?,
            &PredefinedMenuItem::separator(app)?,
            &index,
            &schedule,
            &pause,
            &air_gap,
            &PredefinedMenuItem::separator(app)?,
            &Submenu::with_items(app, "Sponsor", true, &sponsor_refs)?,
            &item("settings", "Settings…", true)?,
            &PredefinedMenuItem::separator(app)?,
            &item("quit", &format!("Quit {}", brand::APP_NAME), true)?,
        ],
    )?;
    let mut tray = TrayIconBuilder::with_id("tray")
        .tooltip(brand::APP_NAME)
        .menu(&menu)
        .on_menu_event(|app, e| on_tray_menu(app, e.id().as_ref()));
    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }
    tray.build(app)?;
    app.manage(Tray {
        index,
        schedule,
        pause,
        air_gap,
    });
    Ok(())
}

fn on_tray_menu<R: Runtime>(app: &AppHandle<R>, id: &str) {
    match id {
        "search" => toggle_quick_search(app),
        "settings" => show_main(app),
        "quit" => app.exit(0),
        "pause" => {
            let (Some(s), Some(db)) = (
                app.try_state::<Arc<Scheduler>>(),
                app.try_state::<Arc<Db>>(),
            ) else {
                return;
            };
            let mut settings = s.settings();
            settings.mode = if settings.mode == Mode::Paused {
                Mode::Auto
            } else {
                Mode::Paused
            };
            // The scheduler reports the change, which relabels this item.
            if let Err(e) = s.change(&db, settings) {
                tracing::warn!(error = %e, "schedule not saved");
            }
        }
        _ => {
            let url = id
                .strip_prefix("sponsor:")
                .and_then(|i| i.parse::<usize>().ok())
                .and_then(|i| brand::SPONSOR_URLS.get(i));
            if let Some(url) = url {
                // The browser makes the request; the app sends nothing.
                std::thread::spawn(move || Command::new("/usr/bin/open").arg(url).status());
            }
        }
    }
}

/// Called with each `index://progress`. `None` until the queue starts.
pub fn set_tray_index<R: Runtime>(app: &AppHandle<R>, progress: Option<IndexProgress>) {
    let Some(tray) = app.try_state::<Tray<R>>() else {
        return;
    };
    let _ = tray.index.set_text(index_label(progress));
}

/// Called with each scheduler decision. `None` until indexing starts.
pub fn set_tray_schedule<R: Runtime>(app: &AppHandle<R>, current: Option<Current>) {
    let Some(tray) = app.try_state::<Tray<R>>() else {
        return;
    };
    let _ = tray.schedule.set_text(schedule_label(current));
    let user_paused = current.is_some_and(|c| c.reason == Reason::UserPaused);
    let _ = tray.pause.set_text(if user_paused {
        "Resume Indexing"
    } else {
        "Pause Indexing"
    });
    let _ = tray.pause.set_enabled(current.is_some());
}

fn schedule_label(current: Option<Current>) -> String {
    let Some(c) = current else {
        return "Schedule: not started".into();
    };
    let state = match c.state {
        SchedState::Paused => "Paused",
        SchedState::RunNow => "Running now",
        SchedState::LowPower => "Low power",
        SchedState::Interactive => "Interactive",
        SchedState::Idle => "Idle",
        SchedState::Normal => "Normal",
    };
    let why = match c.reason {
        Reason::UserPaused => "paused by you",
        Reason::UserRunNow => "run now",
        Reason::OutsideHours => "outside allowed hours",
        Reason::Thermal => "Mac is hot",
        Reason::LowPowerMode => "Low Power Mode",
        Reason::LowBattery => "battery low",
        Reason::Searching => "searching",
        Reason::UserIdle => "you are away",
        Reason::Default => "",
    };
    if why.is_empty() {
        format!("Schedule: {state}")
    } else {
        format!("Schedule: {state} ({why})")
    }
}

pub fn set_tray_air_gap<R: Runtime>(app: &AppHandle<R>, on: bool) {
    if let Some(tray) = app.try_state::<Tray<R>>() {
        let _ = tray
            .air_gap
            .set_text(if on { "Air-Gap: On" } else { "Air-Gap: Off" });
    }
}

fn index_label(progress: Option<IndexProgress>) -> String {
    let Some(p) = progress else {
        return "Indexing not started".into();
    };
    let finished = p.done + p.failed;
    let total = finished + p.queued + p.running;
    match (p.paused, total > finished) {
        (true, _) => format!("Indexing paused {finished}/{total}"),
        (false, true) => format!("Indexing {finished}/{total}"),
        (false, false) => "Up to date".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn screen(x: f64, y: f64, w: f64, h: f64) -> (LogicalPosition<f64>, LogicalSize<f64>) {
        (LogicalPosition::new(x, y), LogicalSize::new(w, h))
    }

    #[test]
    fn origin_is_on_the_screen_under_the_cursor() {
        let screens = [
            screen(0.0, 0.0, 1440.0, 900.0),
            screen(1440.0, -200.0, 2560.0, 1440.0),
        ];
        let at = |x, y| quick_search_origin(LogicalPosition::new(x, y), &screens).unwrap();
        assert_eq!(at(10.0, 10.0), LogicalPosition::new(380.0, 225.0));
        // Second screen, above the primary's top edge.
        assert_eq!(at(2000.0, -100.0), LogicalPosition::new(2380.0, 160.0));
        // Outside every screen: first screen.
        assert_eq!(at(-50.0, 5000.0), LogicalPosition::new(380.0, 225.0));
    }

    #[test]
    fn origin_keeps_the_window_on_a_short_screen() {
        let screens = [screen(0.0, 0.0, 800.0, 500.0)];
        let p = quick_search_origin(LogicalPosition::new(1.0, 1.0), &screens).unwrap();
        assert_eq!(p, LogicalPosition::new(60.0, 40.0));
        let tiny = [screen(0.0, 0.0, 600.0, 400.0)];
        let p = quick_search_origin(LogicalPosition::new(1.0, 1.0), &tiny).unwrap();
        assert_eq!(p.y, 0.0);
        assert_eq!(
            quick_search_origin(LogicalPosition::new(1.0, 1.0), &[]),
            None
        );
    }

    #[test]
    fn index_label_counts_finished_of_total() {
        let p = |queued, running, done, failed, paused| IndexProgress {
            queued,
            running,
            done,
            failed,
            paused,
        };
        assert_eq!(index_label(None), "Indexing not started");
        assert_eq!(index_label(Some(p(5, 2, 10, 1, false))), "Indexing 11/18");
        assert_eq!(
            index_label(Some(p(5, 0, 3, 0, true))),
            "Indexing paused 3/8"
        );
        assert_eq!(index_label(Some(p(0, 0, 9, 1, false))), "Up to date");
        assert_eq!(index_label(Some(p(0, 0, 0, 0, false))), "Up to date");
    }

    #[test]
    fn default_shortcut_parses() {
        assert_eq!(
            Shortcut::from_str(DEFAULT_SHORTCUT).unwrap().into_string(),
            "shift+super+Space"
        );
    }
}
