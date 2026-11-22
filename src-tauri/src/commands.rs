//! IPC entry points. Order inside each command: capability check, service call, AppError mapping.
//! Which window may call which command is decided by `capabilities/*.json` (Tauri ACL).

use serde::Serialize;
use tauri::{Runtime, State, WebviewWindow};

use crate::error::AppError;
use crate::net;
use crate::storage::db::Db;

// Quick-search window set.

#[tauri::command]
pub fn search() -> Result<(), AppError> {
    Err(AppError::not_implemented())
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

/// Not tier-gated: Air-Gap is a safety feature (rule 10).
#[tauri::command]
pub fn set_air_gap(
    enabled: bool,
    db: State<'_, Db>,
    net: State<'_, net::Client>,
) -> Result<(), AppError> {
    net.guard()
        .set_enabled(&db, enabled)
        .map_err(|_| AppError::internal("failed to save setting"))
}

#[tauri::command]
pub fn get_license_status() -> Result<(), AppError> {
    Err(AppError::not_implemented())
}

#[tauri::command]
pub fn list_models() -> Result<(), AppError> {
    Err(AppError::not_implemented())
}

#[tauri::command]
pub fn export_pack() -> Result<(), AppError> {
    Err(AppError::not_implemented())
}
