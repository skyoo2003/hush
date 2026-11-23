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
