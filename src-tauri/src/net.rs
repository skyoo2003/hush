// SPDX-License-Identifier: GPL-3.0-or-later
//! The single outbound HTTP client. Every request, and every redirect hop, passes `AirGapGuard::check` first.

pub mod error;

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use reqwest::header::{LOCATION, RANGE};
use reqwest::redirect::Policy;
use reqwest::{Response, StatusCode, Url};

use crate::security::airgap::{AirGapGuard, Hop, InFlight, NetPurpose};
use crate::security::error::SecurityError;
use error::NetError;

/// Hugging Face takes one redirect to its CDN; a few more leave room without allowing a loop.
const MAX_REDIRECTS: usize = 3;

pub struct Client {
    http: reqwest::Client,
    guard: Arc<AirGapGuard>,
    sent: AtomicUsize,
    /// Tests talk plain HTTP to a local server that the client resolves allowed hosts to.
    #[cfg(test)]
    allow_http: bool,
}

/// A response whose request stays in flight until the body is read or dropped, so turning
/// Air-Gap ON stops a download mid-body and `set_enabled(true)` waits for it.
pub struct Body<'a> {
    _req: InFlight<'a>,
    guard: &'a AirGapGuard,
    resp: Response,
}

impl Body<'_> {
    pub fn status(&self) -> StatusCode {
        self.resp.status()
    }

    pub fn header(&self, name: &str) -> Option<&str> {
        self.resp.headers().get(name)?.to_str().ok()
    }

    /// The next piece of the body, or None at its end. Air-Gap ON ends it with an error.
    pub async fn chunk(&mut self) -> Result<Option<Vec<u8>>, NetError> {
        tokio::select! {
            biased;
            () = self.guard.turned_on() => Err(SecurityError::AirGapEnabled.into()),
            c = self.resp.chunk() => c
                .map(|c| c.map(|b| b.to_vec()))
                .map_err(|e| NetError::Http(e.without_url())),
        }
    }
}

impl Client {
    pub fn new(guard: Arc<AirGapGuard>) -> Result<Self, NetError> {
        Self::build(reqwest::Client::builder(), guard)
    }

    fn build(b: reqwest::ClientBuilder, guard: Arc<AirGapGuard>) -> Result<Self, NetError> {
        let http = b
            // Redirects are followed by `get`, one guard check per hop. A proxy would reach a host the guard never saw.
            .redirect(Policy::none())
            .no_proxy()
            .build()
            .map_err(|e| NetError::Http(e.without_url()))?;
        Ok(Self {
            http,
            guard,
            sent: AtomicUsize::new(0),
            #[cfg(test)]
            allow_http: false,
        })
    }

    /// Resolves these hosts to `addr` and allows plain HTTP. The guard's checks still apply.
    #[cfg(test)]
    pub(crate) fn for_local_server(
        guard: Arc<AirGapGuard>,
        hosts: &[&str],
        addr: std::net::SocketAddr,
    ) -> Self {
        let b = hosts
            .iter()
            .fold(reqwest::Client::builder(), |b, h| b.resolve(h, addr));
        let mut c = Self::build(b, guard).unwrap();
        c.allow_http = true;
        c
    }

    pub fn guard(&self) -> &AirGapGuard {
        &self.guard
    }

    fn scheme_ok(&self, scheme: &str) -> bool {
        #[cfg(test)]
        if self.allow_http && scheme == "http" {
            return true;
        }
        scheme == "https"
    }

    /// GETs `url` from byte `from` on (no Range header when 0), following redirects. Every hop
    /// passes the guard before it is sent.
    pub async fn get(
        &self,
        purpose: NetPurpose,
        url: &str,
        from: u64,
    ) -> Result<Body<'_>, NetError> {
        let mut url = Url::parse(url).map_err(|_| NetError::InvalidUrl)?;
        for n in 0..=MAX_REDIRECTS {
            let hop = if n == 0 { Hop::First } else { Hop::Redirect };
            let req = match url.host_str() {
                Some(host) if self.scheme_ok(url.scheme()) => {
                    self.guard.begin(purpose, host, hop)?
                }
                _ => return Err(NetError::InvalidUrl),
            };
            self.sent.fetch_add(1, Ordering::SeqCst);
            let mut get = self.http.get(url.clone());
            if from > 0 {
                get = get.header(RANGE, format!("bytes={from}-"));
            }
            let resp = tokio::select! {
                biased;
                () = self.guard.turned_on() => return Err(SecurityError::AirGapEnabled.into()),
                r = get.send() => r.map_err(|e| NetError::Http(e.without_url()))?,
            };
            if !resp.status().is_redirection() {
                return Ok(Body {
                    _req: req,
                    guard: &self.guard,
                    resp,
                });
            }
            url = resp
                .headers()
                .get(LOCATION)
                .and_then(|l| l.to_str().ok())
                .and_then(|l| url.join(l).ok())
                .ok_or(NetError::InvalidUrl)?;
        }
        Err(NetError::TooManyRedirects)
    }

    /// Requests that passed the guard and were handed to reqwest. Test hook.
    pub fn sent_count(&self) -> usize {
        self.sent.load(Ordering::SeqCst)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::db::tests::temp_db;
    use tauri::async_runtime::block_on;

    #[test]
    fn blocked_requests_never_reach_reqwest() {
        let (_dir, db) = temp_db();
        let client = Client::new(Arc::new(AirGapGuard::load(&db).unwrap())).unwrap();

        for p in NetPurpose::ALL {
            let url = format!("https://{}/x", p.host());
            assert!(matches!(
                block_on(client.get(p, &url, 0)),
                Err(NetError::Blocked(SecurityError::AirGapEnabled))
            ));
        }

        client.guard().set_enabled(&db, false).unwrap();
        let cases = [
            "https://evil.example/x",
            "https://huggingface.co.evil.example/x",
        ];
        for url in cases {
            assert!(matches!(
                block_on(client.get(NetPurpose::ModelDownload, url, 0)),
                Err(NetError::Blocked(SecurityError::HostNotAllowed))
            ));
        }
        for url in ["http://huggingface.co/x", "not a url", "file:///etc/passwd"] {
            assert!(matches!(
                block_on(client.get(NetPurpose::ModelDownload, url, 0)),
                Err(NetError::InvalidUrl)
            ));
        }
        assert_eq!(client.sent_count(), 0);
    }
}
