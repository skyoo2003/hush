// SPDX-License-Identifier: GPL-3.0-or-later
//! IPC entry points. Order inside each command: service call, then AppError mapping.
//! Which window may call which command is decided by `capabilities/*.json` (Tauri ACL).

use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::Arc;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, Runtime, State};

use tauri_plugin_dialog::DialogExt;

use crate::desktop::{self, ShortcutError};
use crate::error::{AppError, ErrorCode};
use crate::ingest::error::WatchError;
use crate::ingest::queue::{IndexProgress, IndexQueue};
use crate::ingest::watcher::{self, AddedFolder, FolderWatcher};
use crate::net;
use crate::rag::hybrid::{self, SearchMode, SearchResults, SearchService};
use crate::rag::query_parser;
use crate::security::paths::ScopedPath;
use crate::security::pii_guard;
use crate::storage::db::{ChunkId, Db};
use crate::storage::documents::FolderInfo;

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

/// Opens the file behind a search hit in its default app. The webview passes only the chunk id.
#[tauri::command]
pub async fn open_source<R: Runtime>(app: AppHandle<R>, chunk_id: i64) -> Result<(), AppError> {
    run_open(app, chunk_id, false).await
}

/// Shows the file behind a search hit selected in Finder.
#[tauri::command]
pub async fn reveal_source<R: Runtime>(app: AppHandle<R>, chunk_id: i64) -> Result<(), AppError> {
    run_open(app, chunk_id, true).await
}

async fn run_open<R: Runtime>(
    app: AppHandle<R>,
    chunk_id: i64,
    reveal: bool,
) -> Result<(), AppError> {
    tauri::async_runtime::spawn_blocking(move || {
        let db = app
            .try_state::<Arc<Db>>()
            .ok_or_else(|| AppError::internal("storage is not open"))?;
        let (root, rel) = db
            .chunk_file(ChunkId(chunk_id))
            .map_err(|_| AppError::internal("source could not be read"))?
            .ok_or_else(source_not_found)?;
        let path =
            ScopedPath::new(Path::new(&root), Path::new(&rel)).map_err(|_| source_not_found())?;
        let mut open = Command::new("/usr/bin/open");
        if reveal {
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
}

#[tauri::command]
pub fn get_settings<R: Runtime>(
    app: AppHandle<R>,
    db: State<'_, Arc<Db>>,
    net: State<'_, net::Client>,
) -> Settings {
    let shortcut = desktop::saved_shortcut(&db);
    Settings {
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
    net.guard()
        .set_enabled(&db, enabled)
        .map_err(|_| AppError::internal("failed to save setting"))?;
    desktop::set_tray_air_gap(&app, enabled);
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

#[tauri::command]
pub fn list_models() -> Result<(), AppError> {
    Err(AppError::not_implemented())
}

#[tauri::command]
pub fn export_pack() -> Result<(), AppError> {
    Err(AppError::not_implemented())
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
