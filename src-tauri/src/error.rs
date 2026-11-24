// SPDX-License-Identifier: GPL-3.0-or-later
use serde::Serialize;

/// IPC boundary error. Messages must never contain paths, file names, queries or document text.
#[derive(Debug, Serialize, thiserror::Error)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[error("{code:?}: {message}")]
pub struct AppError {
    pub code: ErrorCode,
    pub message: String,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    NotImplemented,
    Internal,
    /// The embedding model is not installed or failed its checks, so search is off.
    ModelMissing,
    SearchFailed,
    /// The picked folder equals, contains, or is inside a watched folder.
    FolderOverlap,
    FolderNotFound,
    FolderUnreadable,
    /// The result's file was moved, deleted, or is no longer inside its folder.
    SourceNotFound,
    /// The citation could not be put on the clipboard.
    ClipboardFailed,
    /// The quick-search shortcut could not be parsed.
    ShortcutInvalid,
    /// Another app holds the quick-search shortcut.
    ShortcutUnavailable,
    /// A model download was refused or stopped because Air-Gap mode is on.
    AirGapEnabled,
    /// A downloaded or imported model file does not match its catalog sha256.
    HashMismatch,
    /// The model id, or the imported file, is not in the built-in catalog.
    NotInCatalog,
    DownloadCancelled,
    DownloadFailed,
    /// Allowed hours are out of range, or start and end are the same.
    ScheduleInvalid,
    /// No update public key is configured, so updates cannot be verified.
    UpdaterNotConfigured,
    /// The update has no valid signature from the configured key for the announced version.
    UpdateSignatureInvalid,
    UpdateFailed,
}

impl AppError {
    pub fn internal(message: &str) -> Self {
        Self {
            code: ErrorCode::Internal,
            message: message.into(),
        }
    }

    pub fn model_missing() -> Self {
        Self {
            code: ErrorCode::ModelMissing,
            message: "embedding model is not available".into(),
        }
    }

    pub fn not_implemented() -> Self {
        Self {
            code: ErrorCode::NotImplemented,
            message: "not implemented".into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializes_snake_case_code() {
        let v = serde_json::to_value(AppError::not_implemented()).unwrap();
        assert_eq!(
            v,
            serde_json::json!({"code": "not_implemented", "message": "not implemented"})
        );
        assert_eq!(
            serde_json::to_value(ErrorCode::Internal).unwrap(),
            "internal"
        );
    }
}
