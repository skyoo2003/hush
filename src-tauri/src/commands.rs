// SPDX-License-Identifier: GPL-3.0-or-later
//! IPC entry points. Order inside each command: service call, then AppError mapping.
//! Which window may call which command is decided by `capabilities/*.json` (Tauri ACL).

use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::Arc;

use serde::Serialize;
use tauri::{AppHandle, Manager, Runtime, State, WebviewWindow};

use tauri_plugin_dialog::DialogExt;

use crate::error::{AppError, ErrorCode};
use crate::ingest::error::WatchError;
use crate::ingest::queue::{IndexProgress, IndexQueue};
use crate::ingest::watcher::{self, AddedFolder, FolderWatcher};
use crate::net;
use crate::rag::hybrid::{SearchMode, SearchResults, SearchService};
use crate::rag::vector_store::SearchFilter;
use crate::security::paths::ScopedPath;
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
) -> Result<SearchResults, AppError> {
    let limit = limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT) as usize;
    tauri::async_runtime::spawn_blocking(move || {
        let service = app
            .try_state::<SearchService>()
            .ok_or_else(AppError::model_missing)?;
        let db = app
            .try_state::<Arc<Db>>()
            .ok_or_else(|| AppError::internal("storage is not open"))?;
        service
            .search(
                &db,
                &query,
                SearchMode::Hybrid,
                &SearchFilter::default(),
                limit,
            )
            .map_err(|e| {
                // RagError messages are fixed and carry no query text.
                tracing::warn!(error = %e, "search failed");
                AppError {
                    code: ErrorCode::SearchFailed,
                    message: "search failed".into(),
                }
            })
    })
    .await
    .map_err(|_| AppError::internal("search task failed"))?
}

/// Opens the file behind a search hit in its default app. The webview passes only the chunk id.
#[tauri::command]
pub async fn open_source<R: Runtime>(app: AppHandle<R>, chunk_id: i64) -> Result<(), AppError> {
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
        // The path is canonical and absolute, so `open` cannot read it as a flag.
        let ok = Command::new("/usr/bin/open")
            .arg(path.as_path())
            .status()
            .is_ok_and(|s| s.success());
        if ok {
            Ok(())
        } else {
            Err(AppError::internal("source could not be opened"))
        }
    })
    .await
    .map_err(|_| AppError::internal("open source task failed"))?
}

/// Puts `text` on the clipboard. The webview formats it from the hit it shows.
#[tauri::command]
pub async fn copy_citation(text: String) -> Result<(), AppError> {
    tauri::async_runtime::spawn_blocking(move || {
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

#[tauri::command]
pub fn hide_window<R: Runtime>(window: WebviewWindow<R>) -> Result<(), AppError> {
    window
        .hide()
        .map_err(|_| AppError::internal("failed to hide window"))
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
}

#[tauri::command]
pub fn get_settings(net: State<'_, net::Client>) -> Settings {
    Settings {
        air_gap: net.guard().is_enabled(),
    }
}

#[tauri::command]
pub fn set_air_gap(
    enabled: bool,
    db: State<'_, Arc<Db>>,
    net: State<'_, net::Client>,
) -> Result<(), AppError> {
    net.guard()
        .set_enabled(&db, enabled)
        .map_err(|_| AppError::internal("failed to save setting"))
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
