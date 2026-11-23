// SPDX-License-Identifier: GPL-3.0-or-later
/// Messages never carry paths, keys, or document text.
#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    #[error("database key does not open the database")]
    WrongKey,
    #[error("database exists but its key is missing")]
    KeyMissing,
    #[error("stored database key is malformed")]
    MalformedKey,
    #[error("keychain error (OSStatus {0})")]
    Keychain(i32),
    #[error("system random number generator failed")]
    Rng,
    #[error("database schema version {0} is newer than this app")]
    SchemaTooNew(i64),
    #[error("embedding count or dimension does not match")]
    EmbeddingShape,
    #[error(transparent)]
    Sqlite(#[from] rusqlite::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}
