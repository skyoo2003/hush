// SPDX-License-Identifier: GPL-3.0-or-later
use crate::security::error::SecurityError;

/// Messages never carry paths, document text, or queries.
#[derive(Debug, thiserror::Error)]
pub enum RagError {
    #[error("unknown embedding model")]
    UnknownModel,
    #[error("model config is malformed")]
    Config(#[source] serde_json::Error),
    #[error("model file {0} does not match its catalog sha256")]
    HashMismatch(&'static str),
    #[error("model file read failed")]
    Io(#[source] std::io::Error),
    #[error("model path is outside the models folder")]
    Scope(#[from] SecurityError),
    #[error("model failed")]
    Model(#[from] candle_core::Error),
    // Fixed message: tokenizer errors can quote their input.
    #[error("tokenizer failed")]
    Tokenizer,
    #[error("embedding count or dimension does not match")]
    EmbeddingShape,
    #[error(transparent)]
    Storage(#[from] crate::storage::error::StorageError),
}

/// Messages never carry paths, URLs, or hosts.
#[derive(Debug, thiserror::Error)]
pub enum ModelError {
    #[error("model is not in the catalog")]
    UnknownModel,
    #[error("downloads are off while Air-Gap mode is on")]
    AirGapEnabled,
    #[error("this model is already downloading")]
    Busy,
    #[error("download cancelled")]
    Cancelled,
    #[error("file does not match the catalog sha256")]
    HashMismatch,
    #[error("file matches no catalog entry")]
    NotInCatalog,
    #[error("download ended before the whole file arrived")]
    Incomplete,
    #[error("server answered HTTP {0}")]
    Http(u16),
    #[error(transparent)]
    Net(crate::net::error::NetError),
    #[error("model file write failed")]
    Io(#[from] std::io::Error),
    #[error("model path is outside the models folder")]
    Scope(#[from] SecurityError),
}

impl From<crate::net::error::NetError> for ModelError {
    fn from(e: crate::net::error::NetError) -> Self {
        match e {
            crate::net::error::NetError::Blocked(SecurityError::AirGapEnabled) => {
                Self::AirGapEnabled
            }
            e => Self::Net(e),
        }
    }
}
