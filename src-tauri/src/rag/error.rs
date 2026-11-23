use crate::security::error::SecurityError;

/// Messages never carry paths, document text, or queries.
#[derive(Debug, thiserror::Error)]
pub enum RagError {
    #[error("unknown embedding model")]
    UnknownModel,
    #[error("model manifest or config is malformed")]
    Manifest(#[source] serde_json::Error),
    #[error("model file {0} does not match its manifest sha256")]
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
}
