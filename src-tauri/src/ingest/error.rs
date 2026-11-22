/// Parse failures. Messages never carry paths or document content.
#[derive(Debug, thiserror::Error)]
pub enum ParseError {
    #[error("unsupported file type")]
    UnsupportedType,
    #[error("file content does not match its extension")]
    MagicMismatch,
    #[error("file exceeds the size limit")]
    TooLarge,
    #[error("archive exceeds the decompressed size limit")]
    DecompressedTooLarge,
    #[error("corrupt document: {0}")]
    Corrupt(&'static str),
    #[error("file read failed")]
    Io(#[source] std::io::Error),
    #[error("corrupt zip archive")]
    Zip(#[from] zip::result::ZipError),
    #[error("malformed XML")]
    Xml(#[from] quick_xml::Error),
}
