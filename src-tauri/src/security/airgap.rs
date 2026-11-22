//! Air-Gap mode. ON (the default) blocks every outbound request. OFF allows exactly one host per purpose.

use std::sync::atomic::{AtomicBool, Ordering};

use super::error::SecurityError;
use crate::brand;
use crate::storage::db::Db;
use crate::storage::error::StorageError;

const SETTING: &str = "airgap_enabled";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetPurpose {
    License,
    ModelDownload,
    UpdateCheck,
}

impl NetPurpose {
    pub const ALL: [Self; 3] = [Self::License, Self::ModelDownload, Self::UpdateCheck];

    /// The one host this purpose may reach. Compared exactly, so subdomains do not match.
    pub fn host(self) -> &'static str {
        match self {
            // shortcut: placeholder until the license provider is chosen; `.invalid` never resolves.
            Self::License => "license-provider.invalid",
            Self::ModelDownload => "huggingface.co",
            Self::UpdateCheck => {
                let rest = brand::SITE_URL.split_once("://").map_or("", |(_, r)| r);
                rest.split('/').next().unwrap_or(rest)
            }
        }
    }
}

pub struct AirGapGuard {
    enabled: AtomicBool,
}

impl AirGapGuard {
    /// A missing or unreadable setting means ON.
    pub fn load(db: &Db) -> Result<Self, StorageError> {
        let enabled = db.setting(SETTING)?.as_deref() != Some("0");
        Ok(Self {
            enabled: AtomicBool::new(enabled),
        })
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled.load(Ordering::SeqCst)
    }

    /// Persists first, so a failed write leaves the current mode in force.
    pub fn set_enabled(&self, db: &Db, on: bool) -> Result<(), StorageError> {
        db.set_setting(SETTING, if on { "1" } else { "0" })?;
        self.enabled.store(on, Ordering::SeqCst);
        Ok(())
    }

    pub fn check(&self, purpose: NetPurpose, host: &str) -> Result<(), SecurityError> {
        if self.is_enabled() {
            Err(SecurityError::AirGapEnabled)
        } else if host.eq_ignore_ascii_case(purpose.host()) {
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
                let allowed = guard.check(p, p.host());
                if on {
                    assert!(
                        matches!(allowed, Err(SecurityError::AirGapEnabled)),
                        "{p:?}"
                    );
                } else {
                    assert!(allowed.is_ok(), "{p:?}");
                    assert!(guard.check(p, &p.host().to_uppercase()).is_ok());
                }
                for host in denied {
                    let r = guard.check(p, &host);
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
    fn hosts_are_bare_and_distinct() {
        let hosts = NetPurpose::ALL.map(NetPurpose::host);
        for h in hosts {
            assert!(!h.is_empty() && !h.contains(['/', ':']), "{h}");
        }
        assert!(hosts[0] != hosts[1] && hosts[1] != hosts[2] && hosts[0] != hosts[2]);
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
