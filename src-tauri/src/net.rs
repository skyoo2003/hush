//! The single outbound HTTP client. Every request passes `AirGapGuard::check` first.

pub mod error;

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use reqwest::redirect::Policy;
use reqwest::{Response, Url};

use crate::security::airgap::{AirGapGuard, NetPurpose};
use error::NetError;

pub struct Client {
    http: reqwest::Client,
    guard: Arc<AirGapGuard>,
    sent: AtomicUsize,
}

impl Client {
    pub fn new(guard: Arc<AirGapGuard>) -> Result<Self, NetError> {
        let http = reqwest::Client::builder()
            // A redirect or a proxy would reach a host the guard never saw.
            .redirect(Policy::none())
            .no_proxy()
            .build()
            .map_err(|e| NetError::Http(e.without_url()))?;
        Ok(Self {
            http,
            guard,
            sent: AtomicUsize::new(0),
        })
    }

    pub fn guard(&self) -> &AirGapGuard {
        &self.guard
    }

    pub async fn get(&self, purpose: NetPurpose, url: &str) -> Result<Response, NetError> {
        let url = Url::parse(url).map_err(|_| NetError::InvalidUrl)?;
        match (url.scheme(), url.host_str()) {
            ("https", Some(host)) => self.guard.check(purpose, host)?,
            _ => return Err(NetError::InvalidUrl),
        }
        self.sent.fetch_add(1, Ordering::SeqCst);
        self.http
            .get(url)
            .send()
            .await
            .map_err(|e| NetError::Http(e.without_url()))
    }

    /// Requests that passed the guard and were handed to reqwest. Test hook.
    pub fn sent_count(&self) -> usize {
        self.sent.load(Ordering::SeqCst)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::security::error::SecurityError;
    use crate::storage::db::tests::temp_db;
    use tauri::async_runtime::block_on;

    #[test]
    fn blocked_requests_never_reach_reqwest() {
        let (_dir, db) = temp_db();
        let client = Client::new(Arc::new(AirGapGuard::load(&db).unwrap())).unwrap();

        for p in NetPurpose::ALL {
            let url = format!("https://{}/x", p.host());
            assert!(matches!(
                block_on(client.get(p, &url)),
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
                block_on(client.get(NetPurpose::ModelDownload, url)),
                Err(NetError::Blocked(SecurityError::HostNotAllowed))
            ));
        }
        for url in ["http://huggingface.co/x", "not a url", "file:///etc/passwd"] {
            assert!(matches!(
                block_on(client.get(NetPurpose::ModelDownload, url)),
                Err(NetError::InvalidUrl)
            ));
        }
        assert_eq!(client.sent_count(), 0);
    }
}
