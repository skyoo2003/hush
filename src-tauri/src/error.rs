use serde::Serialize;

/// IPC boundary error. Messages must never contain paths, file names, queries or document text.
#[derive(Debug, Serialize, thiserror::Error)]
#[error("{code:?}: {message}")]
pub struct AppError {
    pub code: ErrorCode,
    pub message: String,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    UpgradeRequired,
    NotImplemented,
    Internal,
}

impl AppError {
    pub fn internal(message: &str) -> Self {
        Self {
            code: ErrorCode::Internal,
            message: message.into(),
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
            serde_json::to_value(ErrorCode::UpgradeRequired).unwrap(),
            "upgrade_required"
        );
    }
}
