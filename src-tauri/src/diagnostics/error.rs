// SPDX-License-Identifier: GPL-3.0-or-later
/// Export failures. Messages never carry paths.
#[derive(Debug, thiserror::Error)]
pub enum DiagnosticsError {
    #[error("diagnostics file write failed")]
    Io(#[from] std::io::Error),
    #[error("diagnostics zip failed")]
    Zip(#[from] zip::result::ZipError),
    #[error(transparent)]
    Storage(#[from] crate::storage::error::StorageError),
}
