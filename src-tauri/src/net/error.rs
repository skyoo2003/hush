// SPDX-License-Identifier: GPL-3.0-or-later
use crate::security::error::SecurityError;

#[derive(Debug, thiserror::Error)]
pub enum NetError {
    #[error(transparent)]
    Blocked(#[from] SecurityError),
    #[error("only https URLs with a host are allowed")]
    InvalidUrl,
    #[error("too many redirects")]
    TooManyRedirects,
    /// Built with `without_url()`, so it never carries the URL.
    #[error("http request failed")]
    Http(#[source] reqwest::Error),
}
