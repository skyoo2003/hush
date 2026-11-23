// SPDX-License-Identifier: GPL-3.0-or-later
//! IPC entry points. Order inside each command: service call, then AppError mapping.
//! Which window may call which command is decided by `capabilities/*.json` (Tauri ACL).

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
use crate::storage::db::Db;
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

#[tauri::command]
pub fn open_source() -> Result<(), AppError> {
    Err(AppError::not_implemented())
}

#[tauri::command]
pub fn copy_citation() -> Result<(), AppError> {
    Err(AppError::not_implemented())
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
