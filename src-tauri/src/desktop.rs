// SPDX-License-Identifier: GPL-3.0-or-later
//! Desktop shell: the main, settings, and quick-search windows, the global shortcut, the menu
//! bar, the tray, and the Dock policy (Accessory, Regular only while the main or settings window
//! is open). No API here needs the Accessibility permission.

pub mod menu;
pub mod strings;

use std::str::FromStr;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use tauri::image::Image;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::tray::{TrayIcon, TrayIconBuilder};
use tauri::{AppHandle, Emitter, LogicalPosition, LogicalSize, Manager, Runtime, WindowEvent};
use tauri_plugin_global_shortcut::{GlobalShortcut, GlobalShortcutExt, Shortcut, ShortcutState};

use crate::brand;
use crate::ingest::queue::IndexProgress;
use crate::ingest::scheduler::{Current, Mode, Reason, SchedState};
use crate::net;
use crate::sponsor::{self, SponsorLink};
use crate::storage::db::Db;
use crate::storage::error::StorageError;

/// Payload `true` when the quick-search window opens and on each search; `false` when it hides.
pub const FOCUS_EVENT: &str = "search://focus";
/// Not Alt+Space: Alfred and Raycast use it, and macOS lets both apps register it, so
/// registration succeeds while the other app takes focus and the popup hides on blur.
pub const DEFAULT_SHORTCUT: &str = "Cmd+Shift+Space";
const SHORTCUT_SETTING: &str = "quick_search_shortcut";
use strings::{index_label, lang, s, schedule_label, S};

const MAIN: &str = "main";
const SETTINGS: &str = "settings";
const QUICK_SEARCH: &str = "quick-search";
/// Payload `bool`, Air-Gap on or off, to every window after it changes.
pub const AIR_GAP_EVENT: &str = "settings://air-gap";
/// Payload [`SettingsPane`], sent to the settings window.
pub const SETTINGS_PANE_EVENT: &str = "settings://pane";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "snake_case")]
pub enum SettingsPane {
    General,
    Folders,
    Models,
    Privacy,
    Shortcuts,
    Appearance,
    Sponsor,
    About,
}
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
    let shortcut = saved_shortcut(db);
    let registered = Shortcut::from_str(&shortcut)
        .ok()
        .is_some_and(|s| app.global_shortcut().register(s).is_ok());
    if !registered {
        tracing::warn!("quick-search shortcut not registered");
    }
    menu::install(app, &shortcut, registered)?;
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
    menu::set_shortcut(app, &text, true);
    Ok(text)
}

pub fn on_window_event<R: Runtime>(window: &tauri::Window<R>, event: &WindowEvent) {
    let app = window.app_handle();
    match (window.label(), event) {
        (MAIN | SETTINGS, WindowEvent::CloseRequested { api, .. }) => {
            api.prevent_close();
            let _ = window.hide();
            #[cfg(target_os = "macos")]
            if !is_visible(app, MAIN) && !is_visible(app, SETTINGS) {
                let _ = app.set_activation_policy(tauri::ActivationPolicy::Accessory);
            }
        }
        (QUICK_SEARCH, WindowEvent::Focused(false)) => hide_quick_search(app),
        _ => {}
    }
    if let WindowEvent::Focused(focused) = event {
        let key = match window.label() {
            MAIN => menu::Key::Main,
            SETTINGS => menu::Key::Settings,
            _ => menu::Key::Other,
        };
        let focused = *focused;
        menu::update(app, |c| {
            if focused {
                c.key = Some(key);
            } else if c.key == Some(key) {
                c.key = None;
            }
        });
    }
}

fn is_visible<R: Runtime>(app: &AppHandle<R>, label: &str) -> bool {
    app.get_webview_window(label)
        .is_some_and(|w| w.is_visible().unwrap_or(false))
}

fn show_window<R: Runtime>(app: &AppHandle<R>, label: &str) {
    let Some(w) = app.get_webview_window(label) else {
        return;
    };
    #[cfg(target_os = "macos")]
    let _ = app.set_activation_policy(tauri::ActivationPolicy::Regular);
    if let Err(e) = w
        .show()
        .and_then(|()| w.unminimize())
        .and_then(|()| w.set_focus())
    {
        tracing::warn!(error = %e, label, "window show failed");
    }
}

pub fn show_main<R: Runtime>(app: &AppHandle<R>) {
    show_window(app, MAIN);
}

/// Shows the one settings window, switched to `pane` when given (else the last pane used).
pub fn show_settings<R: Runtime>(app: &AppHandle<R>, pane: Option<SettingsPane>) {
    if let Some(pane) = pane {
        // Before show, so the window never paints the previous pane first.
        let _ = app.emit_to(SETTINGS, SETTINGS_PANE_EVENT, pane);
    }
    show_window(app, SETTINGS);
}

pub(crate) fn toggle_quick_search<R: Runtime>(app: &AppHandle<R>) {
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
            if !is_visible(app, MAIN) && !is_visible(app, SETTINGS) {
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
    icon: TrayIcon<R>,
    index: MenuItem<R>,
    schedule: MenuItem<R>,
    pause: MenuItem<R>,
    air_gap: MenuItem<R>,
    state: Mutex<TrayState>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct TrayState {
    air_gap: bool,
    paused: bool,
    indexing: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TrayGlyph {
    Idle,
    Indexing,
    Paused,
    AirGapOff,
}

/// Air-Gap off wins: it is the one state the user must not miss.
fn tray_glyph(s: TrayState) -> TrayGlyph {
    if !s.air_gap {
        TrayGlyph::AirGapOff
    } else if s.paused {
        TrayGlyph::Paused
    } else if s.indexing {
        TrayGlyph::Indexing
    } else {
        TrayGlyph::Idle
    }
}

/// Template images (black + alpha) from `scripts/make-tray-icons.mjs`; macOS tints them.
fn glyph_png(g: TrayGlyph) -> &'static [u8] {
    match g {
        TrayGlyph::Idle => include_bytes!("../icons/tray/idle.png"),
        TrayGlyph::Indexing => include_bytes!("../icons/tray/indexing.png"),
        TrayGlyph::Paused => include_bytes!("../icons/tray/paused.png"),
        TrayGlyph::AirGapOff => include_bytes!("../icons/tray/airgap-off.png"),
    }
}

fn tooltip(g: TrayGlyph) -> String {
    let l = lang();
    let detail = match g {
        TrayGlyph::AirGapOff => s(l, S::AirGapOff),
        TrayGlyph::Paused => s(l, S::IndexingPaused),
        TrayGlyph::Indexing => s(l, S::Indexing),
        TrayGlyph::Idle => s(l, S::AirGapOn),
    };
    format!("{} — {detail}", brand::APP_NAME)
}

fn build_tray<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<()> {
    let l = lang();
    let item =
        |id: &str, text: &str, enabled| MenuItem::with_id(app, id, text, enabled, None::<&str>);
    let index = item("index", &index_label(l, None), false)?;
    let schedule = item("schedule", &schedule_label(l, None), false)?;
    let pause = item("pause", &s(l, S::PauseIndexing), false)?;
    let air_gap = item("air_gap", &s(l, S::AirGapOn), false)?;
    let sponsors = sponsor::CHANNEL_NAMES
        .iter()
        .enumerate()
        .map(|(i, name)| item(&format!("sponsor:{i}"), name, true))
        .collect::<tauri::Result<Vec<_>>>()?;
    let sponsor_refs: Vec<&dyn tauri::menu::IsMenuItem<R>> =
        sponsors.iter().map(|s| s as _).collect();
    let menu = Menu::with_items(
        app,
        &[
            &item("search", &s(l, S::Search), true)?,
            &PredefinedMenuItem::separator(app)?,
            &index,
            &schedule,
            &pause,
            &air_gap,
            &PredefinedMenuItem::separator(app)?,
            &Submenu::with_items(app, s(l, S::SponsorSubmenu), true, &sponsor_refs)?,
            &item("settings", &s(l, S::Settings), true)?,
            &PredefinedMenuItem::separator(app)?,
            &item("quit", &s(l, S::Quit), true)?,
        ],
    )?;
    let state = TrayState {
        air_gap: true,
        paused: false,
        indexing: false,
    };
    let glyph = tray_glyph(state);
    let icon = TrayIconBuilder::with_id("tray")
        .tooltip(tooltip(glyph))
        .icon(Image::from_bytes(glyph_png(glyph))?)
        .icon_as_template(true)
        .menu(&menu)
        .on_menu_event(|app, e| on_tray_menu(app, e.id().as_ref()))
        .build(app)?;
    app.manage(Tray {
        icon,
        index,
        schedule,
        pause,
        air_gap,
        state: Mutex::new(state),
    });
    Ok(())
}

fn update_tray_state<R: Runtime>(tray: &Tray<R>, change: impl FnOnce(&mut TrayState)) {
    let Ok(mut state) = tray.state.lock() else {
        return;
    };
    let before = tray_glyph(*state);
    change(&mut state);
    let glyph = tray_glyph(*state);
    if glyph == before {
        return;
    }
    if let Ok(image) = Image::from_bytes(glyph_png(glyph)) {
        let _ = tray.icon.set_icon(Some(image));
        let _ = tray.icon.set_icon_as_template(true);
    }
    let _ = tray.icon.set_tooltip(Some(tooltip(glyph)));
}

pub(crate) fn toggle_pause<R: Runtime>(app: &AppHandle<R>) {
    let Some(s) = app.try_state::<std::sync::Arc<crate::ingest::scheduler::Scheduler>>() else {
        return;
    };
    let paused = s.settings().mode == Mode::Paused;
    menu::set_mode(app, if paused { Mode::Auto } else { Mode::Paused });
}

fn on_tray_menu<R: Runtime>(app: &AppHandle<R>, id: &str) {
    match id {
        "search" => toggle_quick_search(app),
        "settings" => show_settings(app, None),
        "quit" => app.exit(0),
        "pause" => toggle_pause(app),
        _ => {
            let channel = id.strip_prefix("sponsor:").and_then(|i| i.parse().ok());
            let Some(channel) = channel else {
                return;
            };
            // Read the guard on every click: a missing client counts as Air-Gap on.
            let air_gap = app
                .try_state::<net::Client>()
                .is_none_or(|n| n.guard().is_enabled());
            if let Some(qr @ SponsorLink::Qr { .. }) =
                sponsor::link(channel, air_gap, sponsor::open_in_browser)
            {
                show_settings(app, Some(SettingsPane::Sponsor));
                let _ = app.emit_to(SETTINGS, sponsor::QR_EVENT, qr);
            }
        }
    }
}

/// Called with each `index://progress`. `None` until the queue starts.
pub fn set_tray_index<R: Runtime>(app: &AppHandle<R>, progress: Option<IndexProgress>) {
    let Some(tray) = app.try_state::<Tray<R>>() else {
        return;
    };
    let _ = tray.index.set_text(index_label(lang(), progress));
    update_tray_state(&tray, |s| {
        s.indexing = progress.is_some_and(|p| p.queued + p.running > 0);
        s.paused = progress.is_some_and(|p| p.paused);
    });
}

/// Called with each scheduler decision. `None` until indexing starts.
pub fn set_tray_schedule<R: Runtime>(app: &AppHandle<R>, current: Option<Current>) {
    menu::update(app, |c| c.schedule = current);
    let Some(tray) = app.try_state::<Tray<R>>() else {
        return;
    };
    let l = lang();
    let _ = tray.schedule.set_text(schedule_label(l, current));
    let user_paused = current.is_some_and(|c| c.reason == Reason::UserPaused);
    let _ = tray.pause.set_text(s(
        l,
        if user_paused {
            S::ResumeIndexing
        } else {
            S::PauseIndexing
        },
    ));
    let _ = tray.pause.set_enabled(current.is_some());
    update_tray_state(&tray, |s| {
        s.paused = current.is_some_and(|c| c.state == SchedState::Paused)
    });
}

/// Shows the Air-Gap state in the tray and tells every window, so no indicator goes stale.
pub fn set_tray_air_gap<R: Runtime>(app: &AppHandle<R>, on: bool) {
    let _ = app.emit(AIR_GAP_EVENT, on);
    if let Some(tray) = app.try_state::<Tray<R>>() {
        let _ = tray
            .air_gap
            .set_text(s(lang(), if on { S::AirGapOn } else { S::AirGapOff }));
        update_tray_state(&tray, |s| s.air_gap = on);
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
    fn tray_glyph_puts_air_gap_off_first() {
        let st = |air_gap, paused, indexing| TrayState {
            air_gap,
            paused,
            indexing,
        };
        assert_eq!(tray_glyph(st(false, true, true)), TrayGlyph::AirGapOff);
        assert_eq!(tray_glyph(st(true, true, true)), TrayGlyph::Paused);
        assert_eq!(tray_glyph(st(true, false, true)), TrayGlyph::Indexing);
        assert_eq!(tray_glyph(st(true, false, false)), TrayGlyph::Idle);
    }

    #[test]
    fn tray_glyphs_are_template_pngs() {
        for g in [
            TrayGlyph::Idle,
            TrayGlyph::Indexing,
            TrayGlyph::Paused,
            TrayGlyph::AirGapOff,
        ] {
            let img = Image::from_bytes(glyph_png(g)).unwrap();
            assert_eq!((img.width(), img.height()), (36, 36), "{g:?}");
            // Template: every pixel is black, only alpha varies.
            assert!(img.rgba().chunks(4).all(|p| p[..3] == [0, 0, 0]), "{g:?}");
        }
    }

    #[test]
    fn default_shortcut_parses() {
        assert_eq!(
            Shortcut::from_str(DEFAULT_SHORTCUT).unwrap().into_string(),
            "shift+super+Space"
        );
    }
}
