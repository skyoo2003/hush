// SPDX-License-Identifier: GPL-3.0-or-later
use crate::net::error::NetError;
use crate::security::error::SecurityError;

/// Messages never carry URLs, hosts, or paths.
#[derive(Debug, thiserror::Error)]
pub enum UpdateError {
    #[error("no update signing key is configured")]
    NotConfigured,
    #[error("update checks are off while Air-Gap mode is on")]
    AirGapEnabled,
    #[error("update server answered HTTP {0}")]
    Http(u16),
    #[error("update file is larger than allowed")]
    TooLarge,
    #[error("update manifest is malformed or has no entry for this Mac")]
    Manifest,
    #[error("update signature is missing or does not match")]
    Signature,
    #[error("no update to install; check for updates first")]
    NothingPending,
    #[error(transparent)]
    Net(NetError),
    // Fixed message: the plugin's errors can carry paths.
    #[error("updater failed")]
    Plugin(#[source] tauri_plugin_updater::Error),
}

impl From<NetError> for UpdateError {
    fn from(e: NetError) -> Self {
        match e {
            NetError::Blocked(SecurityError::AirGapEnabled) => Self::AirGapEnabled,
            e => Self::Net(e),
        }
    }
}

impl From<tauri_plugin_updater::Error> for UpdateError {
    fn from(e: tauri_plugin_updater::Error) -> Self {
        use tauri_plugin_updater::Error as E;
        match e {
            E::Minisign(_)
            | E::Base64(_)
            | E::SignatureUtf8(_)
            | E::MissingSignedVersion
            | E::SignedVersionMismatch { .. } => Self::Signature,
            e => Self::Plugin(e),
        }
    }
}
