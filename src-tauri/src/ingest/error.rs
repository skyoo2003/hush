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
    #[error("document is password-protected")]
    Encrypted,
    #[error("PDF library could not be loaded")]
    PdfiumUnavailable,
    #[error("PDF library failed: {0}")]
    Pdfium(&'static str),
    #[error("image could not be decoded")]
    Image,
    #[error("OCR failed")]
    Ocr(#[from] OcrError),
}

/// OCR failures. Fixed messages: recognized text never reaches an error.
#[derive(Debug, thiserror::Error)]
pub enum OcrError {
    #[error("image is too large or empty for OCR")]
    BadImage,
    #[error("OCR engine failed: {0}")]
    Engine(&'static str),
}

impl From<pdfium_render::prelude::PdfiumError> for ParseError {
    fn from(e: pdfium_render::prelude::PdfiumError) -> Self {
        use pdfium_render::prelude::{PdfiumError as E, PdfiumInternalError as I};
        match e {
            E::PdfiumLibraryInternalError(I::PasswordError) => Self::Encrypted,
            E::PdfiumLibraryInternalError(I::FormatError) => Self::Corrupt("malformed PDF"),
            E::PdfiumLibraryInternalError(I::SecurityError) => {
                Self::Pdfium("unsupported security handler")
            }
            E::LoadLibraryError(_) | E::PdfiumLibraryBindingsAlreadyInitialized => {
                Self::PdfiumUnavailable
            }
            _ => Self::Pdfium("operation failed"),
        }
    }
}

impl From<image::ImageError> for ParseError {
    fn from(e: image::ImageError) -> Self {
        match e {
            image::ImageError::Limits(_) => Self::TooLarge,
            _ => Self::Image,
        }
    }
}

/// Pipeline failures. Messages never carry paths or document content.
#[derive(Debug, thiserror::Error)]
pub enum PipelineError {
    #[error(transparent)]
    Scope(#[from] crate::security::error::SecurityError),
    #[error("file read failed")]
    Io(#[source] std::io::Error),
    #[error(transparent)]
    Parse(#[from] ParseError),
    #[error(transparent)]
    Storage(#[from] crate::storage::error::StorageError),
}
