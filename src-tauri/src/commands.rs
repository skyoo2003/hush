// SPDX-License-Identifier: GPL-3.0-or-later
//! IPC entry points. Order inside each command: service call, then AppError mapping.
//! Which window may call which command is decided by `capabilities/*.json` (Tauri ACL).

use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, Runtime, State};

use tauri_plugin_dialog::DialogExt;

use crate::brand;
use crate::desktop::{self, ShortcutError};
use crate::error::{AppError, ErrorCode};
use crate::ingest::error::WatchError;
use crate::ingest::queue::{IndexJob, IndexProgress, IndexQueue, Reason};
use crate::ingest::scanner::EXCLUDES_SETTING;
use crate::ingest::scheduler::{self, ScheduleSettings, ScheduleStatus, Scheduler};
use crate::ingest::watcher::{self, AddedFolder, FolderWatcher};
use crate::net;
use crate::rag::error::ModelError;
use crate::rag::hybrid::{self, SearchMode, SearchResults, SearchService};
use crate::rag::models::{self, ImportedFile, ModelInfo, ModelManager};
use crate::rag::query_parser;
use crate::security::error::SecurityError;
use crate::security::paths::ScopedPath;
use crate::security::pii_guard;
use crate::sponsor::{self, SponsorLink};
use crate::storage::db::{ChunkId, Db, DocId};
use crate::storage::documents::{FailedDocument, FolderInfo};
use crate::update::{self, error::UpdateError, AvailableUpdate};

// Quick-search window set.

const DEFAULT_LIMIT: u32 = 10;
const MAX_LIMIT: u32 = 50;

#[tauri::command]
pub async fn search<R: Runtime>(
    app: AppHandle<R>,
    query: String,
    limit: Option<u32>,
    utc_offset_minutes: Option<i32>,
) -> Result<SearchResults, AppError> {
    let limit = limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT) as usize;
    // Each search means the user is typing; the indexer yields while they do.
    let _ = app.emit(desktop::FOCUS_EVENT, true);
    tauri::async_runtime::spawn_blocking(move || {
        let service = app
            .try_state::<SearchService>()
            .ok_or_else(AppError::model_missing)?;
        let db = app
            .try_state::<Arc<Db>>()
            .ok_or_else(|| AppError::internal("storage is not open"))?;
        let parsed = query_parser::parse(&query);
        let folders = db
            .folders()
            .map_err(|_| AppError::internal("folders could not be read"))?;
        let mut results = match parsed.filter(&folders, utc_offset_minutes.unwrap_or(0)) {
            Some(filter) => service
                .search(&db, &parsed.text, SearchMode::Hybrid, &filter, limit)
                .map_err(|e| {
                    // RagError messages are fixed and carry no query text.
                    tracing::warn!(error = %e, "search failed");
                    AppError {
                        code: ErrorCode::SearchFailed,
                        message: "search failed".into(),
                    }
                })?,
            None => SearchResults::default(),
        };
        if !parsed.text.trim().is_empty() {
            if let Err(e) = db.increment_search_count() {
                tracing::warn!(error = %e, "search count not saved");
            }
        }
        results.filters = parsed.filters;
        Ok(results)
    })
    .await
    .map_err(|_| AppError::internal("search task failed"))?
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "snake_case")]
pub enum OpenMode {
    /// In the file type's default app.
    Open,
    /// Selected in Finder.
    Reveal,
}

/// Opens the file behind a search hit. The webview passes only the chunk id; the path comes
/// from the DB and is checked again by `ScopedPath`, so a row pointing outside its folder is
/// rejected. A missing file is queued for reindexing, which drops its document.
#[tauri::command]
pub async fn open_source<R: Runtime>(
    app: AppHandle<R>,
    chunk_id: i64,
    mode: OpenMode,
) -> Result<(), AppError> {
    tauri::async_runtime::spawn_blocking(move || {
        let db = app
            .try_state::<Arc<Db>>()
            .ok_or_else(|| AppError::internal("storage is not open"))?;
        let file = db
            .chunk_file(ChunkId(chunk_id))
            .map_err(|_| AppError::internal("source could not be read"))?
            .ok_or_else(source_not_found)?;
        let path = match ScopedPath::new(Path::new(&file.root), Path::new(&file.rel_path)) {
            Ok(path) => path,
            Err(SecurityError::Unresolvable(e)) if e.kind() == std::io::ErrorKind::NotFound => {
                if let Some(queue) = app.try_state::<Arc<IndexQueue>>() {
                    queue.submit(IndexJob {
                        folder_id: file.folder_id,
                        root: Path::new(&file.root).into(),
                        rel_path: file.rel_path,
                        reason: Reason::Reindex,
                    });
                }
                return Err(source_not_found());
            }
            Err(e) => {
                tracing::warn!(chunk_id, error = %e, "source rejected");
                return Err(source_not_found());
            }
        };
        let mut open = Command::new("/usr/bin/open");
        if let OpenMode::Reveal = mode {
            open.arg("-R");
        }
        // The path is canonical and absolute, so `open` cannot read it as a flag.
        let ok = open.arg(path.as_path()).status().is_ok_and(|s| s.success());
        if ok {
            Ok(())
        } else {
            Err(AppError::internal("source could not be opened"))
        }
    })
    .await
    .map_err(|_| AppError::internal("open source task failed"))?
}

/// Puts `[file name, p.X] "excerpt"` for a hit on the clipboard, formatted here from the chunk.
#[tauri::command]
pub async fn copy_citation<R: Runtime>(app: AppHandle<R>, chunk_id: i64) -> Result<(), AppError> {
    tauri::async_runtime::spawn_blocking(move || {
        let db = app
            .try_state::<Arc<Db>>()
            .ok_or_else(|| AppError::internal("storage is not open"))?;
        let id = ChunkId(chunk_id);
        let detail = db
            .chunk_details(&[id])
            .map_err(|_| AppError::internal("source could not be read"))?
            .remove(&id)
            .ok_or_else(source_not_found)?;
        let text = pii_guard::before_copy(hybrid::citation(&detail));
        pbcopy(&text).map_err(|_| AppError {
            code: ErrorCode::ClipboardFailed,
            message: "citation could not be copied".into(),
        })
    })
    .await
    .map_err(|_| AppError::internal("copy citation task failed"))?
}

fn pbcopy(text: &str) -> std::io::Result<()> {
    // pbcopy reads stdin in the locale's encoding; an app bundle has no LANG, so set UTF-8.
    let mut child = Command::new("/usr/bin/pbcopy")
        .env("LANG", "en_US.UTF-8")
        .stdin(Stdio::piped())
        .spawn()?;
    let written = child
        .stdin
        .take()
        .map(|mut stdin| stdin.write_all(text.as_bytes()))
        .unwrap_or(Ok(()));
    let status = child.wait()?;
    written?;
    if status.success() {
        Ok(())
    } else {
        Err(std::io::Error::other("pbcopy failed"))
    }
}

fn source_not_found() -> AppError {
    AppError {
        code: ErrorCode::SourceNotFound,
        message: "source file not found".into(),
    }
}

/// Hides the quick-search window, whichever window calls it (the ACL allows only that one).
#[tauri::command]
pub fn hide_window<R: Runtime>(app: AppHandle<R>) {
    desktop::hide_quick_search(&app);
}

// Main window set.

#[tauri::command]
pub fn list_folders(db: State<'_, Arc<Db>>) -> Result<Vec<FolderInfo>, AppError> {
    db.folders()
        .map_err(|_| AppError::internal("folders could not be read"))
}

/// Opens the native folder picker; the webview never passes a path. `None` when cancelled.
#[tauri::command]
pub async fn add_watched_folder<R: Runtime>(
    app: AppHandle<R>,
) -> Result<Option<AddedFolder>, AppError> {
    tauri::async_runtime::spawn_blocking(move || {
        let Some(picked) = app.dialog().file().blocking_pick_folder() else {
            return Ok(None);
        };
        let path = picked
            .into_path()
            .map_err(|_| folder_error(WatchError::Unreadable))?;
        let db = app
            .try_state::<Arc<Db>>()
            .ok_or_else(|| AppError::internal("storage is not open"))?;
        let watcher = app.try_state::<FolderWatcher>();
        watcher::add_folder(&db, watcher.as_deref(), &path)
            .map(Some)
            .map_err(folder_error)
    })
    .await
    .map_err(|_| AppError::internal("add folder task failed"))?
}

#[tauri::command]
pub async fn remove_watched_folder<R: Runtime>(app: AppHandle<R>, id: i64) -> Result<(), AppError> {
    tauri::async_runtime::spawn_blocking(move || {
        let db = app
            .try_state::<Arc<Db>>()
            .ok_or_else(|| AppError::internal("storage is not open"))?;
        let watcher = app.try_state::<FolderWatcher>();
        let queue = app.try_state::<Arc<IndexQueue>>();
        watcher::remove_folder(&db, watcher.as_deref(), queue.as_deref().map(|q| &**q), id)
            .map_err(folder_error)
    })
    .await
    .map_err(|_| AppError::internal("remove folder task failed"))?
}

/// Stops or resumes watching and indexing a folder; its documents stay but leave search.
#[tauri::command]
pub async fn set_folder_enabled<R: Runtime>(
    app: AppHandle<R>,
    id: i64,
    enabled: bool,
) -> Result<(), AppError> {
    tauri::async_runtime::spawn_blocking(move || {
        let db = app
            .try_state::<Arc<Db>>()
            .ok_or_else(|| AppError::internal("storage is not open"))?;
        let watcher = app.try_state::<FolderWatcher>();
        let queue = app.try_state::<Arc<IndexQueue>>();
        watcher::set_folder_enabled(
            &db,
            watcher.as_deref(),
            queue.as_deref().map(|q| &**q),
            id,
            enabled,
        )
        .map_err(folder_error)
    })
    .await
    .map_err(|_| AppError::internal("folder toggle task failed"))?
}

#[tauri::command]
pub fn list_failed_documents(
    db: State<'_, Arc<Db>>,
    folder_id: i64,
) -> Result<Vec<FailedDocument>, AppError> {
    db.failed_documents(folder_id)
        .map_err(|_| AppError::internal("documents could not be read"))
}

/// Queues a failed document again. Does nothing for a disabled folder.
#[tauri::command]
pub fn retry_document<R: Runtime>(
    app: AppHandle<R>,
    db: State<'_, Arc<Db>>,
    doc_id: i64,
) -> Result<(), AppError> {
    let queue = app
        .try_state::<Arc<IndexQueue>>()
        .ok_or_else(AppError::model_missing)?;
    let file = db
        .document_file(DocId(doc_id))
        .map_err(|_| AppError::internal("document could not be read"))?
        .ok_or_else(source_not_found)?;
    if file.enabled {
        // The worker checks the path with `ScopedPath` before reading it.
        queue.submit(IndexJob {
            folder_id: file.folder_id,
            root: Path::new(&file.root).into(),
            rel_path: file.rel_path,
            reason: Reason::Reindex,
        });
    }
    Ok(())
}

#[derive(Debug, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct IndexStatus {
    /// False until the embedding model loads and the queue starts.
    pub indexing: bool,
    pub progress: IndexProgress,
}

#[tauri::command]
pub fn get_index_status<R: Runtime>(app: AppHandle<R>) -> IndexStatus {
    let queue = app.try_state::<Arc<IndexQueue>>();
    IndexStatus {
        indexing: queue.is_some(),
        progress: queue.map(|q| q.progress()).unwrap_or_default(),
    }
}

#[tauri::command]
pub fn get_schedule<R: Runtime>(app: AppHandle<R>, db: State<'_, Arc<Db>>) -> ScheduleStatus {
    match app.try_state::<Arc<Scheduler>>() {
        Some(s) => ScheduleStatus {
            settings: s.settings(),
            current: Some(s.current()),
        },
        None => ScheduleStatus {
            settings: scheduler::load(&db),
            current: None,
        },
    }
}

/// Saves the schedule and applies it at once. `index://schedule` reports the new state.
#[tauri::command]
pub fn set_schedule<R: Runtime>(
    app: AppHandle<R>,
    settings: ScheduleSettings,
    db: State<'_, Arc<Db>>,
) -> Result<ScheduleStatus, AppError> {
    if !settings.is_valid() {
        return Err(AppError {
            code: ErrorCode::ScheduleInvalid,
            message: "allowed hours must be two different times of day".into(),
        });
    }
    let saved = match app.try_state::<Arc<Scheduler>>() {
        Some(s) => s.change(&db, settings),
        None => scheduler::save(&db, &settings),
    };
    saved.map_err(|_| AppError::internal("failed to save setting"))?;
    Ok(get_schedule(app, db))
}

/// WatchError messages are fixed and carry no paths.
fn folder_error(e: WatchError) -> AppError {
    let code = match e {
        WatchError::Overlap => ErrorCode::FolderOverlap,
        WatchError::NotFound => ErrorCode::FolderNotFound,
        WatchError::Unreadable => ErrorCode::FolderUnreadable,
        _ => {
            tracing::warn!(error = %e, "folder operation failed");
            return AppError::internal("folder operation failed");
        }
    };
    AppError {
        code,
        message: e.to_string(),
    }
}

#[derive(Debug, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub air_gap: bool,
    /// Quick-search shortcut in global-hotkey syntax, such as `alt+Space`.
    pub shortcut: String,
    /// False when another app holds the shortcut, so it does nothing.
    pub shortcut_registered: bool,
    /// gitignore lines, one per line, applied to every folder. Invalid lines are ignored.
    pub exclude_patterns: String,
    /// Self-reported "I sponsored", never verified. Only the thank-you badge reads it
    /// (`scripts/check-supporter.sh`): it unlocks nothing.
    pub supporter: bool,
    /// Every theme is selectable by everyone.
    pub theme: Theme,
    /// Show the one-time milestone banner: 100 searches or 100 indexed documents, not dismissed.
    pub sponsor_banner: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "snake_case")]
pub enum Theme {
    #[default]
    System,
    OledDark,
    Terminal,
}

const SUPPORTER_SETTING: &str = "supporter";
const THEME_SETTING: &str = "theme";
const BANNER_DISMISSED_SETTING: &str = "sponsor_banner_dismissed";
const MILESTONE: i64 = 100;

fn flag(db: &Db, key: &str) -> bool {
    db.setting(key)
        .inspect_err(|e| tracing::warn!(error = %e, key, "setting not read"))
        .ok()
        .flatten()
        .is_some_and(|v| v == "1")
}

fn sponsor_banner(db: &Db) -> bool {
    if flag(db, BANNER_DISMISSED_SETTING) {
        return false;
    }
    db.milestone_counts()
        .inspect_err(|e| tracing::warn!(error = %e, "milestone counts not read"))
        .is_ok_and(|(searches, docs)| searches >= MILESTONE || docs >= MILESTONE)
}

#[tauri::command]
pub fn get_settings<R: Runtime>(
    app: AppHandle<R>,
    db: State<'_, Arc<Db>>,
    net: State<'_, net::Client>,
) -> Settings {
    let shortcut = desktop::saved_shortcut(&db);
    let exclude_patterns = db.setting(EXCLUDES_SETTING).unwrap_or_else(|e| {
        tracing::warn!(error = %e, "exclude patterns not read");
        None
    });
    let theme = db
        .setting(THEME_SETTING)
        .ok()
        .flatten()
        .and_then(|v| serde_json::from_str(&v).ok())
        .unwrap_or_default();
    Settings {
        exclude_patterns: exclude_patterns.unwrap_or_default(),
        supporter: flag(&db, SUPPORTER_SETTING),
        theme,
        sponsor_banner: sponsor_banner(&db),
        air_gap: net.guard().is_enabled(),
        shortcut_registered: desktop::is_registered(&app, &shortcut),
        shortcut,
    }
}

#[tauri::command]
pub fn set_air_gap<R: Runtime>(
    app: AppHandle<R>,
    enabled: bool,
    db: State<'_, Arc<Db>>,
    net: State<'_, net::Client>,
) -> Result<(), AppError> {
    apply_air_gap(&app, &db, &net, enabled)
}

fn apply_air_gap<R: Runtime>(
    app: &AppHandle<R>,
    db: &Db,
    net: &net::Client,
    on: bool,
) -> Result<(), AppError> {
    net.guard()
        .set_enabled(db, on)
        .map_err(|_| AppError::internal("failed to save setting"))?;
    desktop::set_tray_air_gap(app, on);
    Ok(())
}

/// Replaces the quick-search shortcut. On failure the old one stays active and saved.
#[tauri::command]
pub fn set_shortcut<R: Runtime>(
    app: AppHandle<R>,
    shortcut: String,
    db: State<'_, Arc<Db>>,
    net: State<'_, net::Client>,
) -> Result<Settings, AppError> {
    desktop::set_shortcut(&app, &db, &shortcut).map_err(|e| match e {
        ShortcutError::Invalid => AppError {
            code: ErrorCode::ShortcutInvalid,
            message: e.to_string(),
        },
        ShortcutError::Unavailable => AppError {
            code: ErrorCode::ShortcutUnavailable,
            message: e.to_string(),
        },
        ShortcutError::Storage(_) => AppError::internal("failed to save setting"),
    })?;
    Ok(get_settings(app, db, net))
}

/// Saves the exclude patterns and rescans every enabled folder, which drops newly excluded
/// documents and indexes newly included files.
#[tauri::command]
pub fn set_exclude_patterns<R: Runtime>(
    app: AppHandle<R>,
    patterns: String,
    db: State<'_, Arc<Db>>,
    net: State<'_, net::Client>,
) -> Result<Settings, AppError> {
    db.set_setting(EXCLUDES_SETTING, &patterns)
        .map_err(|_| AppError::internal("failed to save setting"))?;
    let watcher = app.try_state::<FolderWatcher>();
    watcher::rescan_enabled(&db, watcher.as_deref()).map_err(folder_error)?;
    Ok(get_settings(app, db, net))
}

fn save(db: &Db, key: &str, value: &str) -> Result<(), AppError> {
    db.set_setting(key, value)
        .map_err(|_| AppError::internal("failed to save setting"))
}

/// Saves the self-reported supporter flag. Turning it on also dismisses the milestone banner
/// for good, so the banner never needs to read the flag.
#[tauri::command]
pub fn set_supporter<R: Runtime>(
    app: AppHandle<R>,
    on: bool,
    db: State<'_, Arc<Db>>,
    net: State<'_, net::Client>,
) -> Result<Settings, AppError> {
    save(&db, SUPPORTER_SETTING, if on { "1" } else { "0" })?;
    if on {
        save(&db, BANNER_DISMISSED_SETTING, "1")?;
    }
    Ok(get_settings(app, db, net))
}

#[tauri::command]
pub fn set_theme<R: Runtime>(
    app: AppHandle<R>,
    theme: Theme,
    db: State<'_, Arc<Db>>,
    net: State<'_, net::Client>,
) -> Result<Settings, AppError> {
    let value = serde_json::to_string(&theme).map_err(|e| AppError::internal(&e.to_string()))?;
    save(&db, THEME_SETTING, &value)?;
    Ok(get_settings(app, db, net))
}

#[tauri::command]
pub fn dismiss_sponsor_banner<R: Runtime>(
    app: AppHandle<R>,
    db: State<'_, Arc<Db>>,
    net: State<'_, net::Client>,
) -> Result<Settings, AppError> {
    save(&db, BANNER_DISMISSED_SETTING, "1")?;
    Ok(get_settings(app, db, net))
}

/// Opens sponsor channel `channel` in the external browser, or with Air-Gap on returns its QR
/// code and opens nothing. No request is made either way.
#[tauri::command]
pub fn open_sponsor(channel: usize, net: State<'_, net::Client>) -> Result<SponsorLink, AppError> {
    sponsor::link(channel, net.guard().is_enabled(), sponsor::open_in_browser)
        .ok_or_else(|| AppError::internal("unknown sponsor channel"))
}

#[tauri::command]
pub fn list_models(models: State<'_, ModelManager>) -> Vec<ModelInfo> {
    models.list()
}

fn model_error(e: ModelError) -> AppError {
    let code = match e {
        ModelError::AirGapEnabled => ErrorCode::AirGapEnabled,
        ModelError::HashMismatch => ErrorCode::HashMismatch,
        ModelError::NotInCatalog | ModelError::UnknownModel => ErrorCode::NotInCatalog,
        ModelError::Cancelled => ErrorCode::DownloadCancelled,
        _ => {
            // ModelError messages carry no paths, URLs, or hosts.
            tracing::warn!(error = %e, "model install failed");
            ErrorCode::DownloadFailed
        }
    };
    AppError {
        code,
        message: e.to_string(),
    }
}

/// Downloads a catalog model; progress goes to `models::PROGRESS_EVENT`. With
/// `air_gap_off_for_download` (sent only by the user's click on that offer), Air-Gap is turned
/// OFF for this download and back ON when it ends, however it ends. Otherwise Air-Gap ON refuses.
#[tauri::command]
pub async fn download_model<R: Runtime>(
    app: AppHandle<R>,
    id: String,
    air_gap_off_for_download: bool,
) -> Result<(), AppError> {
    tauri::async_runtime::spawn_blocking(move || {
        let (Some(db), Some(net), Some(models)) = (
            app.try_state::<Arc<Db>>(),
            app.try_state::<net::Client>(),
            app.try_state::<ModelManager>(),
        ) else {
            return Err(AppError::internal("app is not ready"));
        };
        let restore = air_gap_off_for_download && net.guard().is_enabled();
        if restore {
            apply_air_gap(&app, &db, &net, false)?;
        }
        // A dedicated blocking thread drives the download, so its file writes never block the runtime.
        let r = tauri::async_runtime::block_on(models.download(&net, &id, |p| {
            let _ = app.emit(models::PROGRESS_EVENT, p);
        }));
        if restore {
            apply_air_gap(&app, &db, &net, true)?;
        }
        r.map_err(model_error)?;
        crate::load_model(&app);
        Ok(())
    })
    .await
    .map_err(|_| AppError::internal("download task failed"))?
}

/// True if a download of `id` was running. Its partial files stay for the next download.
#[tauri::command]
pub fn cancel_model_download(id: String, models: State<'_, ModelManager>) -> bool {
    models.cancel(&id)
}

/// Opens the native file picker from Rust, so no path comes from the webview, then installs the
/// picked file, or each file of a picked models kit, if its sha256 is in the catalog. None if the
/// user cancelled the picker.
#[tauri::command]
pub async fn import_model_file<R: Runtime>(
    app: AppHandle<R>,
) -> Result<Option<Vec<ImportedFile>>, AppError> {
    tauri::async_runtime::spawn_blocking(move || {
        let Some(picked) = app.dialog().file().blocking_pick_file() else {
            return Ok(None);
        };
        let path = picked
            .into_path()
            .map_err(|_| model_error(ModelError::NotInCatalog))?;
        let models = app
            .try_state::<ModelManager>()
            .ok_or_else(|| AppError::internal("app is not ready"))?;
        let imported = models.import(&path).map_err(model_error)?;
        if imported.iter().any(|f| f.model_installed) {
            crate::load_model(&app);
        }
        Ok(Some(imported))
    })
    .await
    .map_err(|_| AppError::internal("import task failed"))?
}

fn update_error(e: UpdateError) -> AppError {
    let code = match e {
        UpdateError::NotConfigured => ErrorCode::UpdaterNotConfigured,
        UpdateError::AirGapEnabled => ErrorCode::AirGapEnabled,
        UpdateError::Signature => ErrorCode::UpdateSignatureInvalid,
        _ => {
            // UpdateError messages carry no paths, URLs, or hosts.
            tracing::warn!(error = %e, "update failed");
            ErrorCode::UpdateFailed
        }
    };
    AppError {
        code,
        message: e.to_string(),
    }
}

/// Checks the latest release's signed manifest. Only a click runs it; nothing checks on its own.
/// None means this is the latest version.
#[tauri::command]
pub async fn check_for_update<R: Runtime>(
    app: AppHandle<R>,
    net: State<'_, net::Client>,
    pending: State<'_, update::Pending>,
) -> Result<Option<AvailableUpdate>, AppError> {
    let updater = update::updater(&app).map_err(update_error)?;
    update::check(&net, brand::UPDATE_MANIFEST_URL, &updater, &pending)
        .await
        .map_err(update_error)
}

/// Downloads and installs the update the last check found once its signature verifies, then
/// relaunches the app.
#[tauri::command]
pub async fn install_update<R: Runtime>(app: AppHandle<R>) -> Result<(), AppError> {
    let a = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let (Some(net), Some(pending)) = (
            a.try_state::<net::Client>(),
            a.try_state::<update::Pending>(),
        ) else {
            return Err(AppError::internal("app is not ready"));
        };
        // A dedicated blocking thread, because extracting the archive writes files.
        tauri::async_runtime::block_on(update::install(&net, &pending)).map_err(update_error)
    })
    .await
    .map_err(|_| AppError::internal("update task failed"))??;
    app.restart()
}

#[tauri::command]
pub fn export_pack() -> Result<(), AppError> {
    Err(AppError::not_implemented())
}

/// Saves a diagnostics zip where the user picks (see `diagnostics::export`). Nothing is sent.
/// False when the save dialog was cancelled.
#[tauri::command]
pub async fn export_diagnostics<R: Runtime>(app: AppHandle<R>) -> Result<bool, AppError> {
    tauri::async_runtime::spawn_blocking(move || {
        let name = format!("{}-diagnostics.zip", brand::APP_SLUG);
        let Some(picked) = app.dialog().file().set_file_name(name).blocking_save_file() else {
            return Ok(false);
        };
        let failed = || AppError {
            code: ErrorCode::DiagnosticsFailed,
            message: "diagnostics could not be saved".into(),
        };
        let dest = picked.into_path().map_err(|_| failed())?;
        let log_dir = app.path().app_log_dir().map_err(|_| failed())?;
        let db = app.try_state::<Arc<Db>>();
        let models = app.try_state::<ModelManager>();
        let state = crate::diagnostics::AppState {
            db: db.as_deref().map(|d| &**d),
            air_gap: app
                .try_state::<net::Client>()
                .is_none_or(|n| n.guard().is_enabled()),
            installed_models: crate::rag::embedder::MODELS
                .iter()
                .filter(|m| models.as_ref().is_some_and(|mm| mm.is_installed(m.id)))
                .map(|m| m.id)
                .collect(),
        };
        crate::diagnostics::export(&dest, &log_dir, &state).map_err(|e| {
            tracing::warn!(error = %e, "diagnostics export failed");
            failed()
        })?;
        Ok(true)
    })
    .await
    .map_err(|_| AppError::internal("diagnostics task failed"))?
}

/// Deletes the index DB (folders, indexed text, vectors, keyword index, change history, and
/// settings) and its keychain key, then restarts into an empty index. Source files, models,
/// and logs stay.
#[tauri::command]
pub async fn delete_all_indexes<R: Runtime>(app: AppHandle<R>) -> Result<(), AppError> {
    let failed = || AppError {
        code: ErrorCode::DeleteFailed,
        message: "indexes could not be deleted".into(),
    };
    let dir = app.path().app_data_dir().map_err(|_| failed())?;
    if let Some(q) = app.try_state::<Arc<IndexQueue>>() {
        q.cancel();
    }
    tauri::async_runtime::spawn_blocking(move || {
        crate::storage::db::destroy(
            &crate::storage::db::db_path(&dir),
            &crate::storage::keychain::KeychainKeyProvider,
        )
    })
    .await
    .map_err(|_| failed())?
    .map_err(|e| {
        tracing::warn!(error = %e, "delete all indexes failed");
        failed()
    })?;
    tracing::info!("all indexes deleted; restarting");
    app.restart()
}

#[tauri::command]
pub fn get_about() -> crate::about::About {
    crate::about::about()
}

#[tauri::command]
pub fn read_legal_text(which: crate::about::LegalText) -> &'static str {
    crate::about::legal_text(which)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Overwrites the clipboard, so it runs only on request.
    #[test]
    #[ignore]
    fn pbcopy_round_trips_utf8() {
        let text = "\u{bb38}\u{c11c} \u{2014} p. 3";
        pbcopy(text).unwrap();
        let out = Command::new("/usr/bin/pbpaste")
            .env("LANG", "en_US.UTF-8")
            .output()
            .unwrap();
        assert_eq!(String::from_utf8(out.stdout).unwrap(), text);
    }
}
