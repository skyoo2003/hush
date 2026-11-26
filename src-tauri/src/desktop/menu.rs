// SPDX-License-Identifier: GPL-3.0-or-later
//! The macOS menu bar. Items that act inside a page go to the main window as `menu://action`;
//! the page reports what it shows with `set_menu_context`. Which items are enabled is a pure
//! function of [`Ctx`].

use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use tauri::menu::{
    CheckMenuItem, IsMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu, SubmenuBuilder,
};
use tauri::{AppHandle, Emitter, Manager, Runtime};

use super::strings::{lang, s, S};
use crate::brand;
use crate::ingest::scheduler::{Current, Mode, Reason, Scheduler};
use crate::ingest::watcher::{self, FolderWatcher};
use crate::sponsor;
use crate::storage::db::Db;

/// Payload [`MenuAction`], sent to the main window.
pub const MENU_EVENT: &str = "menu://action";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "snake_case")]
pub enum Section {
    Search,
    Ask,
    Compare,
    Changes,
    Folder,
}

/// What the main window shows, as it reports it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct MenuContext {
    pub section: Section,
    #[cfg_attr(test, ts(type = "number | null"))]
    pub folder_id: Option<i64>,
    pub has_selected_hit: bool,
    pub sidebar_visible: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum MenuAction {
    Navigate { section: Section },
    ToggleSidebar,
    Find,
    AddFolder,
    CopyCitation,
    ShowShortcuts,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    Main,
    Settings,
    Other,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct Ctx {
    pub key: Option<Key>,
    pub page: Option<MenuContext>,
    /// The scheduler's last decision; None until indexing starts.
    pub schedule: Option<Current>,
    pub shortcut_registered: bool,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Enabled {
    pub quick_search: bool,
    pub main_items: bool,
    pub copy_citation: bool,
    pub pause: bool,
    pub index_now: bool,
    pub rescan: bool,
}

pub fn enabled(c: &Ctx) -> Enabled {
    let main = c.key == Some(Key::Main);
    let page = c.page.filter(|_| main);
    let reason = c.schedule.map(|s| s.reason);
    Enabled {
        quick_search: c.shortcut_registered,
        main_items: main,
        copy_citation: page.is_some_and(|p| p.has_selected_hit),
        pause: c.schedule.is_some(),
        index_now: reason.is_some_and(|r| !matches!(r, Reason::UserPaused | Reason::UserRunNow)),
        rescan: page.is_some_and(|p| p.section == Section::Folder && p.folder_id.is_some()),
    }
}

const NAV: [(Section, S, &str); 4] = [
    (Section::Search, S::Search, "CmdOrCtrl+1"),
    (Section::Ask, S::Ask, "CmdOrCtrl+2"),
    (Section::Compare, S::Compare, "CmdOrCtrl+3"),
    (Section::Changes, S::Changes, "CmdOrCtrl+4"),
];

struct Items<R: Runtime> {
    quick_search: MenuItem<R>,
    copy_citation: MenuItem<R>,
    find: MenuItem<R>,
    sidebar: MenuItem<R>,
    nav: Vec<CheckMenuItem<R>>,
    pause: MenuItem<R>,
    index_now: MenuItem<R>,
    rescan: MenuItem<R>,
}

pub struct AppMenu<R: Runtime> {
    items: Items<R>,
    ctx: Mutex<Ctx>,
}

pub fn install<R: Runtime>(
    app: &AppHandle<R>,
    shortcut: &str,
    registered: bool,
) -> tauri::Result<()> {
    let l = lang();
    let t = |k| s(l, k);
    let item =
        |id: &str, k, accel: Option<&str>, on: bool| MenuItem::with_id(app, id, t(k), on, accel);
    let sep = || PredefinedMenuItem::separator(app);

    let quick_search = item(
        "quick_search",
        S::OpenQuickSearch,
        registered.then_some(shortcut),
        registered,
    )?;
    let copy_citation = item("copy_citation", S::CopyCitation, None, false)?;
    let find = item("find", S::Find, Some("CmdOrCtrl+F"), false)?;
    let sidebar = item(
        "toggle_sidebar",
        S::HideSidebar,
        Some("Ctrl+CmdOrCtrl+S"),
        false,
    )?;
    let nav = NAV
        .iter()
        .map(|(sec, k, accel)| {
            CheckMenuItem::with_id(app, nav_id(*sec), t(*k), false, false, Some(*accel))
        })
        .collect::<tauri::Result<Vec<_>>>()?;
    let pause = item("pause", S::PauseIndexing, None, false)?;
    let index_now = item("index_now", S::IndexNow, None, false)?;
    let rescan = item("rescan", S::RescanFolder, None, false)?;

    let app_menu = SubmenuBuilder::new(app, brand::APP_NAME)
        .item(&item("about", S::About, None, true)?)
        .separator()
        .item(&item("settings", S::Settings, Some("CmdOrCtrl+,"), true)?)
        .item(&item("sponsor", S::Sponsor, None, true)?)
        .separator()
        .item(&PredefinedMenuItem::services(app, Some(&t(S::Services)))?)
        .separator()
        .item(&PredefinedMenuItem::hide(app, Some(&t(S::Hide)))?)
        .item(&PredefinedMenuItem::hide_others(
            app,
            Some(&t(S::HideOthers)),
        )?)
        .item(&PredefinedMenuItem::show_all(app, Some(&t(S::ShowAll)))?)
        .separator()
        .item(&PredefinedMenuItem::quit(app, Some(&t(S::Quit)))?)
        .build()?;
    let file = Submenu::with_items(
        app,
        t(S::File),
        true,
        &[
            &item("add_folder", S::AddFolder, Some("CmdOrCtrl+O"), true)?,
            &quick_search,
            &sep()?,
            // Disabled until `.locitepack` export exists (roadmap week 11).
            &item(
                "export_pack",
                S::ExportPack,
                Some("Shift+CmdOrCtrl+E"),
                false,
            )?,
            &sep()?,
            &PredefinedMenuItem::close_window(app, Some(&t(S::CloseWindow)))?,
        ],
    )?;
    let edit = Submenu::with_items(
        app,
        t(S::Edit),
        true,
        &[
            &PredefinedMenuItem::undo(app, Some(&t(S::Undo)))?,
            &PredefinedMenuItem::redo(app, Some(&t(S::Redo)))?,
            &sep()?,
            &PredefinedMenuItem::cut(app, Some(&t(S::Cut)))?,
            &PredefinedMenuItem::copy(app, Some(&t(S::Copy)))?,
            &PredefinedMenuItem::paste(app, Some(&t(S::Paste)))?,
            &PredefinedMenuItem::select_all(app, Some(&t(S::SelectAll)))?,
            &sep()?,
            &copy_citation,
            // Disabled until masking exists: `pii_guard::before_copy` is a no-op (roadmap week 9).
            &item(
                "copy_masked",
                S::CopyMasked,
                Some("Shift+CmdOrCtrl+C"),
                false,
            )?,
            &sep()?,
            &find,
        ],
    )?;
    let mut view_items: Vec<&dyn IsMenuItem<R>> = vec![&sidebar];
    let view_sep = sep()?;
    view_items.push(&view_sep);
    view_items.extend(nav.iter().map(|n| n as &dyn IsMenuItem<R>));
    let view_sep2 = sep()?;
    let full = PredefinedMenuItem::fullscreen(app, Some(&t(S::EnterFullScreen)))?;
    view_items.push(&view_sep2);
    view_items.push(&full);
    let view = Submenu::with_items(app, t(S::View), true, &view_items)?;
    let indexing = Submenu::with_items(
        app,
        t(S::Indexing),
        true,
        &[&pause, &index_now, &sep()?, &rescan],
    )?;
    let window = Submenu::with_items(
        app,
        t(S::Window),
        true,
        &[
            &PredefinedMenuItem::minimize(app, Some(&t(S::Minimize)))?,
            &PredefinedMenuItem::maximize(app, Some(&t(S::Zoom)))?,
            &sep()?,
            &PredefinedMenuItem::bring_all_to_front(app, Some(&t(S::BringAllToFront)))?,
        ],
    )?;
    let help = Submenu::with_items(
        app,
        t(S::Help),
        true,
        &[
            &item("help", S::AppHelp, None, true)?,
            &item("shortcuts", S::KeyboardShortcuts, Some("CmdOrCtrl+/"), true)?,
            &item("verify_airgap", S::VerifyAirGap, None, true)?,
        ],
    )?;
    let menu = Menu::with_items(
        app,
        &[&app_menu, &file, &edit, &view, &indexing, &window, &help],
    )?;
    app.set_menu(menu)?;
    #[cfg(target_os = "macos")]
    {
        window.set_as_windows_menu_for_nsapp()?;
        help.set_as_help_menu_for_nsapp()?;
    }
    app.on_menu_event(|app, e| on_menu(app, e.id().as_ref()));
    app.manage(AppMenu {
        items: Items {
            quick_search,
            copy_citation,
            find,
            sidebar,
            nav,
            pause,
            index_now,
            rescan,
        },
        ctx: Mutex::new(Ctx {
            shortcut_registered: registered,
            ..Ctx::default()
        }),
    });
    Ok(())
}

fn nav_id(section: Section) -> String {
    format!("nav:{}", section_name(section))
}

fn section_name(section: Section) -> &'static str {
    match section {
        Section::Search => "search",
        Section::Ask => "ask",
        Section::Compare => "compare",
        Section::Changes => "changes",
        Section::Folder => "folder",
    }
}

/// Changes the menu context and re-applies enablement. A no-op before `install`.
pub fn update<R: Runtime>(app: &AppHandle<R>, change: impl FnOnce(&mut Ctx)) {
    let Some(menu) = app.try_state::<AppMenu<R>>() else {
        return;
    };
    let Ok(mut ctx) = menu.ctx.lock() else {
        return;
    };
    change(&mut ctx);
    apply(&menu.items, &ctx);
}

fn apply<R: Runtime>(i: &Items<R>, c: &Ctx) {
    let e = enabled(c);
    let l = lang();
    let _ = i.quick_search.set_enabled(e.quick_search);
    let _ = i.copy_citation.set_enabled(e.copy_citation);
    let _ = i.find.set_enabled(e.main_items);
    let _ = i.sidebar.set_enabled(e.main_items);
    let visible = c.page.is_none_or(|p| p.sidebar_visible);
    let _ = i.sidebar.set_text(s(
        l,
        if visible {
            S::HideSidebar
        } else {
            S::ShowSidebar
        },
    ));
    for (n, (sec, ..)) in i.nav.iter().zip(NAV) {
        let _ = n.set_enabled(e.main_items);
        let _ = n.set_checked(c.page.is_some_and(|p| p.section == sec));
    }
    let paused = c.schedule.is_some_and(|s| s.reason == Reason::UserPaused);
    let _ = i.pause.set_text(s(
        l,
        if paused {
            S::ResumeIndexing
        } else {
            S::PauseIndexing
        },
    ));
    let _ = i.pause.set_enabled(e.pause);
    let _ = i.index_now.set_enabled(e.index_now);
    let _ = i.rescan.set_enabled(e.rescan);
}

/// Shows the new global shortcut on Open Quick Search.
pub fn set_shortcut<R: Runtime>(app: &AppHandle<R>, shortcut: &str, registered: bool) {
    if let Some(menu) = app.try_state::<AppMenu<R>>() {
        let _ = menu
            .items
            .quick_search
            .set_accelerator(registered.then_some(shortcut));
    }
    update(app, |c| c.shortcut_registered = registered);
}

fn emit_main<R: Runtime>(app: &AppHandle<R>, action: MenuAction) {
    super::show_main(app);
    if let Err(e) = app.emit_to(super::MAIN, MENU_EVENT, action) {
        tracing::warn!(error = %e, "menu action not sent");
    }
}

fn on_menu<R: Runtime>(app: &AppHandle<R>, id: &str) {
    use super::SettingsPane;
    match id {
        "about" => super::show_settings(app, Some(SettingsPane::About)),
        "settings" => super::show_settings(app, None),
        "sponsor" => super::show_settings(app, Some(SettingsPane::Sponsor)),
        "add_folder" => emit_main(app, MenuAction::AddFolder),
        "quick_search" => super::toggle_quick_search(app),
        "copy_citation" => emit_main(app, MenuAction::CopyCitation),
        "find" => emit_main(app, MenuAction::Find),
        "toggle_sidebar" => emit_main(app, MenuAction::ToggleSidebar),
        "shortcuts" => emit_main(app, MenuAction::ShowShortcuts),
        "pause" => super::toggle_pause(app),
        "index_now" => set_mode(app, Mode::RunNow),
        "rescan" => rescan(app),
        "help" => sponsor::open_in_browser(&format!("{}/tree/main/docs", brand::REPO_URL)),
        "verify_airgap" => sponsor::open_in_browser(&format!(
            "{}/blob/main/docs/airgap-verification.md",
            brand::REPO_URL
        )),
        _ => {
            let section = NAV.iter().map(|n| n.0).find(|s| nav_id(*s) == id);
            if let Some(section) = section {
                emit_main(app, MenuAction::Navigate { section });
                // A CheckMenuItem toggles itself on click; the page's next context restores it.
                update(app, |_| {});
            }
        }
    }
}

pub(super) fn set_mode<R: Runtime>(app: &AppHandle<R>, mode: Mode) {
    let (Some(s), Some(db)) = (
        app.try_state::<Arc<Scheduler>>(),
        app.try_state::<Arc<Db>>(),
    ) else {
        return;
    };
    let mut settings = s.settings();
    settings.mode = mode;
    // The scheduler reports the change, which relabels the tray and this menu.
    if let Err(e) = s.change(&db, settings) {
        tracing::warn!(error = %e, "schedule not saved");
    }
}

fn rescan<R: Runtime>(app: &AppHandle<R>) {
    let folder = app
        .try_state::<AppMenu<R>>()
        .and_then(|m| m.ctx.lock().ok()?.page?.folder_id);
    let (Some(id), Some(db)) = (folder, app.try_state::<Arc<Db>>()) else {
        return;
    };
    let w = app.try_state::<FolderWatcher>();
    // WatchError messages carry no paths.
    if let Err(e) = watcher::rescan_folder(&db, w.as_deref(), id) {
        tracing::warn!(error = %e, "rescan failed");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ingest::scheduler::{Plan, SchedState};

    fn page(section: Section, folder_id: Option<i64>, hit: bool) -> Option<MenuContext> {
        Some(MenuContext {
            section,
            folder_id,
            has_selected_hit: hit,
            sidebar_visible: true,
        })
    }

    fn current(reason: Reason) -> Option<Current> {
        Some(Current {
            state: SchedState::Normal,
            reason,
            plan: Plan {
                parse_workers: 1,
                embedding: true,
            },
        })
    }

    #[test]
    fn main_items_follow_the_key_window() {
        let mut c = Ctx {
            key: Some(Key::Main),
            page: page(Section::Search, None, true),
            ..Ctx::default()
        };
        let e = enabled(&c);
        assert!(e.main_items && e.copy_citation && !e.rescan);
        c.key = Some(Key::Settings);
        let e = enabled(&c);
        assert!(!e.main_items && !e.copy_citation);
        c.key = None;
        assert!(!enabled(&c).main_items);
    }

    #[test]
    fn rescan_needs_a_folder_page() {
        let c = |p| Ctx {
            key: Some(Key::Main),
            page: p,
            ..Ctx::default()
        };
        assert!(enabled(&c(page(Section::Folder, Some(3), false))).rescan);
        assert!(!enabled(&c(page(Section::Folder, None, false))).rescan);
        assert!(!enabled(&c(page(Section::Search, Some(3), false))).rescan);
        assert!(!enabled(&c(page(Section::Folder, Some(3), false))).copy_citation);
    }

    #[test]
    fn indexing_items_follow_the_schedule() {
        let c = |schedule| Ctx {
            schedule,
            ..Ctx::default()
        };
        let none = enabled(&c(None));
        assert!(!none.pause && !none.index_now);
        let running = enabled(&c(current(Reason::Default)));
        assert!(running.pause && running.index_now);
        let paused = enabled(&c(current(Reason::UserPaused)));
        assert!(paused.pause && !paused.index_now);
        assert!(!enabled(&c(current(Reason::UserRunNow))).index_now);
        assert!(enabled(&c(current(Reason::Thermal))).index_now);
    }

    #[test]
    fn quick_search_needs_a_registered_shortcut() {
        assert!(!enabled(&Ctx::default()).quick_search);
        let c = Ctx {
            shortcut_registered: true,
            ..Ctx::default()
        };
        assert!(enabled(&c).quick_search);
    }

    #[test]
    fn menu_action_wire_format() {
        let j = |a| serde_json::to_value(a).unwrap();
        assert_eq!(
            j(MenuAction::Navigate {
                section: Section::Compare
            }),
            serde_json::json!({"kind": "navigate", "section": "compare"})
        );
        assert_eq!(j(MenuAction::Find), serde_json::json!({"kind": "find"}));
    }
}
