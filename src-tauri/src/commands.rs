//! IPC entry points. Order inside each command: service call, then AppError mapping.
//! Which window may call which command is decided by `capabilities/*.json` (Tauri ACL).

use std::sync::Arc;

use serde::Serialize;
use tauri::{AppHandle, Manager, Runtime, State, WebviewWindow};

use crate::error::{AppError, ErrorCode};
use crate::net;
use crate::rag::hybrid::{SearchMode, SearchResults, SearchService};
use crate::rag::vector_store::SearchFilter;
use crate::storage::db::Db;

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
pub fn list_folders() -> Result<(), AppError> {
    Err(AppError::not_implemented())
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
