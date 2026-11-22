//! IPC entry points. Order inside each command: capability check, service call, AppError mapping.
//! Which window may call which command is decided by `capabilities/*.json` (Tauri ACL).

use tauri::{Runtime, WebviewWindow};

use crate::error::{AppError, ErrorCode};

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
    window.hide().map_err(|_| AppError {
        code: ErrorCode::Internal,
        message: "failed to hide window".into(),
    })
}

// Main window set.

#[tauri::command]
pub fn list_folders() -> Result<(), AppError> {
    Err(AppError::not_implemented())
}

#[tauri::command]
pub fn get_settings() -> Result<(), AppError> {
    Err(AppError::not_implemented())
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
