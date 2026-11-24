// SPDX-License-Identifier: GPL-3.0-or-later
//! Air-Gap mode. ON (the default) blocks every outbound request. OFF allows one starting host per purpose (plus `*.hf.co` redirects for model downloads).

use std::sync::{Condvar, Mutex, MutexGuard, PoisonError};

use tokio::sync::watch;

use super::error::SecurityError;
use crate::brand;
use crate::storage::db::Db;
use crate::storage::error::StorageError;

const SETTING: &str = "airgap_enabled";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetPurpose {
    ModelDownload,
    UpdateCheck,
}

impl NetPurpose {
    pub const ALL: [Self; 2] = [Self::ModelDownload, Self::UpdateCheck];

    /// The one host a request for this purpose starts at. Compared exactly, so subdomains do not match.
    pub fn host(self) -> &'static str {
        match self {
            Self::ModelDownload => "huggingface.co",
            Self::UpdateCheck => {
                let rest = brand::SITE_URL.split_once("://").map_or("", |(_, r)| r);
                rest.split('/').next().unwrap_or(rest)
            }
        }
    }
}

/// Where a request is in its redirect chain.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hop {
    First,
    Redirect,
}

impl NetPurpose {
    /// Model weights redirect from `huggingface.co` to a regional `*.hf.co` CDN host (ADR-017).
    /// Only a redirect may reach those hosts; the first request must go to `host()`.
    fn allows(self, host: &str, hop: Hop) -> bool {
        let host = host.to_ascii_lowercase();
        host == self.host()
            || (hop == Hop::Redirect
                && self == Self::ModelDownload
                && host
                    .strip_suffix(".hf.co")
                    .is_some_and(|sub| !sub.is_empty()))
    }
}

pub struct AirGapGuard {
    /// The mode. Requests subscribe to it so that turning ON cancels them.
    on: watch::Sender<bool>,
    /// Serializes `set_enabled`, so the DB row and memory always end on the same toggle.
    toggle: Mutex<()>,
    /// Requests between `begin` and the drop of their `InFlight`.
    in_flight: Mutex<usize>,
    drained: Condvar,
}

/// A request that passed the guard. Dropping it lets a pending `set_enabled(true)` return.
pub struct InFlight<'a>(&'a AirGapGuard);

impl Drop for InFlight<'_> {
    fn drop(&mut self) {
        *lock(&self.0.in_flight) -= 1;
        self.0.drained.notify_all();
    }
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    // Nothing panics while these are held, and the data stays valid if something did.
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

impl AirGapGuard {
    /// A missing or unreadable setting means ON.
    pub fn load(db: &Db) -> Result<Self, StorageError> {
        let enabled = db.setting(SETTING)?.as_deref() != Some("0");
        Ok(Self {
            on: watch::Sender::new(enabled),
            toggle: Mutex::new(()),
            in_flight: Mutex::new(0),
            drained: Condvar::new(),
        })
    }

    pub fn is_enabled(&self) -> bool {
        *self.on.borrow()
    }

    /// Persists first, so a failed write leaves the current mode in force.
    /// Turning ON returns only after every in-flight request has been cancelled or has finished,
    /// so once the caller sees Ok, no request leaves until Air-Gap is turned OFF again.
    pub fn set_enabled(&self, db: &Db, on: bool) -> Result<(), StorageError> {
        let _toggle = lock(&self.toggle);
        db.set_setting(SETTING, if on { "1" } else { "0" })?;
        self.on.send_replace(on);
        if on {
            let mut n = lock(&self.in_flight);
            while *n > 0 {
                n = self.drained.wait(n).unwrap_or_else(PoisonError::into_inner);
            }
        }
        Ok(())
    }

    /// Registers the request, then checks it. Registering first is what lets `set_enabled(true)`
    /// wait for every request that could have seen OFF. The caller must keep driving the request
    /// to completion or drop it, and must race it against `turned_on`.
    pub fn begin(
        &self,
        purpose: NetPurpose,
        host: &str,
        hop: Hop,
    ) -> Result<InFlight<'_>, SecurityError> {
        *lock(&self.in_flight) += 1;
        let req = InFlight(self);
        self.check(purpose, host, hop)?;
        Ok(req)
    }

    /// Resolves once Air-Gap is ON.
    pub async fn turned_on(&self) {
        // Err only if the sender is dropped, which cannot happen while `self` is borrowed.
        let _ = self.on.subscribe().wait_for(|on| *on).await;
    }

    pub fn check(&self, purpose: NetPurpose, host: &str, hop: Hop) -> Result<(), SecurityError> {
        if self.is_enabled() {
            Err(SecurityError::AirGapEnabled)
        } else if purpose.allows(host, hop) {
            Ok(())
        } else {
            Err(SecurityError::HostNotAllowed)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::db::tests::temp_db;
    use std::time::Duration;

    #[test]
    fn check_matrix() {
        let (_dir, db) = temp_db();
        let guard = AirGapGuard::load(&db).unwrap();
        for on in [true, false] {
            guard.set_enabled(&db, on).unwrap();
            for p in NetPurpose::ALL {
                let others = NetPurpose::ALL.into_iter().filter(|o| *o != p);
                let denied = ["evil.example", &format!("x.{}", p.host())]
                    .map(String::from)
                    .into_iter()
                    .chain(others.map(|o| o.host().to_owned()));
                let allowed = guard.check(p, p.host(), Hop::First);
                if on {
                    assert!(
                        matches!(allowed, Err(SecurityError::AirGapEnabled)),
                        "{p:?}"
                    );
                } else {
                    assert!(allowed.is_ok(), "{p:?}");
                    assert!(guard.check(p, &p.host().to_uppercase(), Hop::First).is_ok());
                }
                for host in denied {
                    let r = guard.check(p, &host, Hop::First);
                    if on {
                        assert!(matches!(r, Err(SecurityError::AirGapEnabled)));
                    } else {
                        assert!(
                            matches!(r, Err(SecurityError::HostNotAllowed)),
                            "{p:?} {host}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn only_a_model_redirect_may_reach_an_hf_cdn_host() {
        let (_dir, db) = temp_db();
        let guard = AirGapGuard::load(&db).unwrap();
        let md = NetPurpose::ModelDownload;
        let cdn = "us.aws.cdn.hf.co";
        assert!(matches!(
            guard.check(md, cdn, Hop::Redirect),
            Err(SecurityError::AirGapEnabled)
        ));
        guard.set_enabled(&db, false).unwrap();
        assert!(guard.check(md, cdn, Hop::Redirect).is_ok());
        assert!(guard.check(md, "CDN-LFS.HF.CO", Hop::Redirect).is_ok());
        assert!(guard.check(md, md.host(), Hop::Redirect).is_ok());
        for (p, host, hop) in [
            (md, cdn, Hop::First),
            (md, "hf.co", Hop::Redirect),
            (md, "evil-hf.co", Hop::Redirect),
            (md, "hf.co.evil.example", Hop::Redirect),
            (NetPurpose::UpdateCheck, cdn, Hop::Redirect),
        ] {
            assert!(
                matches!(
                    guard.check(p, host, hop),
                    Err(SecurityError::HostNotAllowed)
                ),
                "{p:?} {host} {hop:?}"
            );
        }
    }

    #[test]
    fn turning_on_waits_for_in_flight_requests() {
        let (_dir, db) = temp_db();
        let (guard, p) = (AirGapGuard::load(&db).unwrap(), NetPurpose::ModelDownload);
        guard.set_enabled(&db, false).unwrap();
        let req = guard.begin(p, p.host(), Hop::First).unwrap();
        let (g, d) = (&guard, &db);
        std::thread::scope(|s| {
            let (tx, rx) = std::sync::mpsc::channel();
            s.spawn(move || {
                g.set_enabled(d, true).unwrap();
                tx.send(()).unwrap();
            });
            // The in-flight request is told to stop...
            tauri::async_runtime::block_on(g.turned_on());
            // ...and ON is not acknowledged while it is still in flight.
            assert!(rx.recv_timeout(Duration::from_millis(200)).is_err());
            drop(req);
            rx.recv_timeout(Duration::from_secs(5)).unwrap();
        });
        assert!(matches!(
            guard.begin(p, p.host(), Hop::First),
            Err(SecurityError::AirGapEnabled)
        ));
        // A blocked request is not left in flight, so the next ON returns at once.
        guard.set_enabled(&db, false).unwrap();
        guard.set_enabled(&db, true).unwrap();
    }

    #[test]
    fn hosts_are_bare_and_distinct() {
        let hosts = NetPurpose::ALL.map(NetPurpose::host);
        for h in hosts {
            assert!(!h.is_empty() && !h.contains(['/', ':']), "{h}");
        }
        assert_ne!(hosts[0], hosts[1]);
    }

    #[test]
    fn default_on_and_persisted() {
        let (dir, db) = temp_db();
        assert!(AirGapGuard::load(&db).unwrap().is_enabled());
        AirGapGuard::load(&db)
            .unwrap()
            .set_enabled(&db, false)
            .unwrap();
        drop(db);
        let db = Db::open(
            &dir.path().join("test.db"),
            &crate::storage::keychain::DbKey::from_slice(&[1; 32]).unwrap(),
        )
        .unwrap();
        assert!(!AirGapGuard::load(&db).unwrap().is_enabled());
        db.set_setting(SETTING, "garbage").unwrap();
        assert!(AirGapGuard::load(&db).unwrap().is_enabled());
    }
}
