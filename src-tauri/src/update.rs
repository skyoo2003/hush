// SPDX-License-Identifier: GPL-3.0-or-later
//! Manual, signed app updates (ADR-022). `net::Client` fetches the manifest and the package under
//! `NetPurpose::UpdateCheck`. tauri-plugin-updater only parses the manifest (`restore_update`) and
//! verifies, then installs, the bytes (`Update::install`). Its own HTTP client is never called;
//! scripts/check-net-usage.sh enforces that. No `Update` leaves this module.

pub mod error;

use std::sync::{Mutex, MutexGuard, PoisonError};

use serde::Serialize;
use tauri::{AppHandle, Runtime};
use tauri_plugin_updater::{Update, Updater, UpdaterExt};

use crate::brand;
use crate::net::{self, StatusCode};
use crate::security::airgap::NetPurpose;
use error::UpdateError;

const MANIFEST_MAX: usize = 64 * 1024;
/// The app archive is tens of MB; this only stops a server from filling memory.
const PACKAGE_MAX: usize = 512 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
pub struct AvailableUpdate {
    pub version: String,
}

/// The update the last check found. `install` installs only this one.
#[derive(Default)]
pub struct Pending(Mutex<Option<Update>>);

impl Pending {
    fn lock(&self) -> MutexGuard<'_, Option<Update>> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// The updater with the public key from `tauri.conf.json` `plugins.updater.pubkey`. Fails closed
/// when the key is empty or missing.
pub fn updater<R: Runtime>(app: &AppHandle<R>) -> Result<Updater, UpdateError> {
    let config = &app.config().plugins.0;
    let pubkey = config
        .get("updater")
        .and_then(|u| u.get("pubkey"))
        .and_then(|k| k.as_str())
        .unwrap_or_default();
    if pubkey.trim().is_empty() {
        return Err(UpdateError::NotConfigured);
    }
    // build() requires an endpoint, though the plugin never requests it.
    let url = brand::UPDATE_MANIFEST_URL
        .parse()
        .map_err(|_| UpdateError::NotConfigured)?;
    Ok(app.updater_builder().endpoints(vec![url])?.build()?)
}

/// GETs `url` under UpdateCheck, at most `max` bytes. Air-Gap ON stops it mid-body.
async fn fetch(net: &net::Client, url: &str, max: usize) -> Result<Vec<u8>, UpdateError> {
    let mut body = net.get(NetPurpose::UpdateCheck, url, 0).await?;
    if body.status() != StatusCode::OK {
        return Err(UpdateError::Http(body.status().as_u16()));
    }
    let mut bytes = Vec::new();
    while let Some(c) = body.chunk().await? {
        if bytes.len() + c.len() > max {
            return Err(UpdateError::TooLarge);
        }
        bytes.extend_from_slice(&c);
    }
    Ok(bytes)
}

/// Fetches the manifest at `manifest_url` and remembers the update it announces, if it is newer
/// than this app. None means up to date.
pub async fn check(
    net: &net::Client,
    manifest_url: &str,
    updater: &Updater,
    pending: &Pending,
) -> Result<Option<AvailableUpdate>, UpdateError> {
    let raw = fetch(net, manifest_url, MANIFEST_MAX).await?;
    let json = serde_json::from_slice(&raw).map_err(|_| UpdateError::Manifest)?;
    // restore_update makes no request and requires the signature to carry the announced version.
    let update = updater
        .restore_update(json)
        .map_err(|_| UpdateError::Manifest)?;
    let found = update.as_ref().map(|u| AvailableUpdate {
        version: u.version.clone(),
    });
    *pending.lock() = update;
    Ok(found)
}

/// Downloads the pending update and installs it once its signature verifies. Returns its
/// version. Extracting the archive blocks, so run this on a blocking thread. The app must
/// relaunch to run the new version.
pub async fn install(net: &net::Client, pending: &Pending) -> Result<String, UpdateError> {
    let update = pending.lock().clone().ok_or(UpdateError::NothingPending)?;
    let bytes = fetch(net, update.download_url.as_str(), PACKAGE_MAX).await?;
    // Verifies the minisign signature and its signed version before touching the app.
    update.install(&bytes)?;
    *pending.lock() = None;
    Ok(update.version)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::net::error::NetError;
    use crate::net::Client;
    use crate::security::airgap::AirGapGuard;
    use crate::security::error::SecurityError;
    use crate::storage::db::tests::temp_db;
    use base64::engine::general_purpose::STANDARD as B64;
    use base64::Engine;
    use ring::rand::SystemRandom;
    use ring::signature::{Ed25519KeyPair, KeyPair};
    use std::collections::HashMap;
    use std::io::{Read, Write};
    use std::path::Path;
    use std::sync::Arc;
    use tauri::async_runtime::block_on;
    use tauri::test::MockRuntime;

    const GH: &str = "github.com";
    const ASSETS: &str = "release-assets.githubusercontent.com";
    const VERSION: &str = "9.9.9";

    /// A throwaway minisign key. It makes legacy (`Ed`) signatures, which the plugin accepts, so
    /// no BLAKE2b is needed. The trusted comment has the `tauri signer sign --app-version` layout.
    struct Key {
        pair: Ed25519KeyPair,
        id: [u8; 8],
    }

    impl Key {
        fn new() -> Self {
            let pkcs8 = Ed25519KeyPair::generate_pkcs8(&SystemRandom::new()).unwrap();
            let pair = Ed25519KeyPair::from_pkcs8(pkcs8.as_ref()).unwrap();
            // Every key shares one id, so a wrong key reaches the Ed25519 check, not the id check.
            Self {
                pair,
                id: *b"testkey1",
            }
        }

        /// The value for `plugins.updater.pubkey`.
        fn public(&self) -> String {
            let bin = [b"Ed".as_slice(), &self.id, self.pair.public_key().as_ref()].concat();
            B64.encode(format!(
                "untrusted comment: test key\n{}\n",
                B64.encode(bin)
            ))
        }

        /// The value for a manifest's `signature`.
        fn sign(&self, data: &[u8], version: &str) -> String {
            let sig = self.pair.sign(data);
            let tc = format!("timestamp:0\tfile:App.app.tar.gz\tversion:{version}");
            let global = self.pair.sign(&[sig.as_ref(), tc.as_bytes()].concat());
            let bin = [b"Ed".as_slice(), &self.id, sig.as_ref()].concat();
            B64.encode(format!(
                "untrusted comment: test\n{}\ntrusted comment: {tc}\n{}\n",
                B64.encode(bin),
                B64.encode(global.as_ref())
            ))
        }
    }

    enum Reply {
        Ok(Vec<u8>),
        Redirect(String),
    }

    type Routes = HashMap<(&'static str, String), Reply>;

    /// Serves `routes(port)` by (Host, path) on a local port and logs the host of each request.
    fn serve(routes: impl FnOnce(u16) -> Routes) -> (u16, Arc<Mutex<Vec<String>>>) {
        let (listener, addr) = Client::local_listener();
        let routes = routes(addr.port());
        let seen = Arc::new(Mutex::new(Vec::new()));
        let log = seen.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut s) = stream else { return };
                let mut req = Vec::new();
                let mut b = [0u8; 1];
                while !req.ends_with(b"\r\n\r\n") && s.read(&mut b).unwrap_or(0) == 1 {
                    req.push(b[0]);
                }
                let req = String::from_utf8_lossy(&req).to_lowercase();
                let path = req.split(' ').nth(1).unwrap_or_default().to_owned();
                let host = req
                    .lines()
                    .find_map(|l| l.strip_prefix("host: "))
                    .and_then(|h| h.split(':').next())
                    .unwrap_or_default()
                    .to_owned();
                let reply = routes.iter().find(|((h, p), _)| *h == host && *p == path);
                log.lock().unwrap().push(host);
                let _ = match reply {
                    Some((_, Reply::Ok(body))) => write!(
                        s,
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        body.len()
                    )
                    .and_then(|()| s.write_all(body)),
                    Some((_, Reply::Redirect(to))) => write!(
                        s,
                        "HTTP/1.1 302 Found\r\nLocation: {to}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                    ),
                    None => write!(
                        s,
                        "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                    ),
                };
            }
        });
        (addr.port(), seen)
    }

    /// A gzipped tar of `App.app` holding `Contents/MacOS/marker` = `marker`, the layout of
    /// the bundler's updater archive.
    fn package(dir: &Path, marker: &str) -> Vec<u8> {
        let macos = dir.join("pkg/App.app/Contents/MacOS");
        std::fs::create_dir_all(&macos).unwrap();
        std::fs::write(macos.join("marker"), marker).unwrap();
        let out = std::process::Command::new("/usr/bin/tar")
            .env("COPYFILE_DISABLE", "1")
            .arg("-czf")
            .arg("-")
            .arg("-C")
            .arg(dir.join("pkg"))
            .arg("App.app")
            .output()
            .unwrap();
        assert!(out.status.success());
        out.stdout
    }

    fn mock_app() -> tauri::App<MockRuntime> {
        tauri::test::mock_builder()
            .plugin(tauri_plugin_updater::Builder::new().build())
            .build(tauri::generate_context!(test = true))
            .unwrap()
    }

    /// An updater that trusts `key` and installs over the app whose executable is `exe`.
    fn test_updater(app: &tauri::App<MockRuntime>, key: &Key, exe: &Path) -> Updater {
        app.updater_builder()
            .pubkey(key.public())
            .executable_path(exe)
            .endpoints(vec!["https://github.com/x".parse().unwrap()])
            .unwrap()
            .build()
            .unwrap()
    }

    fn client(port: u16, air_gap: bool) -> (tempfile::TempDir, Client) {
        let (dir, db) = temp_db();
        let guard = AirGapGuard::load(&db).unwrap();
        guard.set_enabled(&db, air_gap).unwrap();
        let addr = ([127, 0, 0, 1], port).into();
        (
            dir,
            Client::for_local_server(Arc::new(guard), &[GH, ASSETS], addr),
        )
    }

    fn manifest(port: u16, signature: Option<String>) -> Vec<u8> {
        let mut platform = serde_json::json!({
            "url": format!("http://{GH}:{port}/o/r/releases/latest/download/App.app.tar.gz"),
        });
        if let Some(s) = signature {
            platform["signature"] = s.into();
        }
        let target = format!("darwin-{}", std::env::consts::ARCH);
        serde_json::to_vec(&serde_json::json!({
            "version": VERSION,
            "notes": "test",
            "pub_date": "2026-11-25T00:00:00Z",
            "platforms": { target: platform },
        }))
        .unwrap()
    }

    /// GitHub's measured chain: latest/download, then download/<tag>, then the asset host.
    fn release(port: u16, manifest: Vec<u8>, package: Vec<u8>) -> Routes {
        let tagged = format!("/o/r/releases/download/v{VERSION}/app.app.tar.gz");
        HashMap::from([
            ((GH, "/latest.json".to_owned()), Reply::Ok(manifest)),
            (
                (
                    GH,
                    "/o/r/releases/latest/download/app.app.tar.gz".to_owned(),
                ),
                Reply::Redirect(format!("http://{GH}:{port}{tagged}")),
            ),
            (
                (GH, tagged),
                Reply::Redirect(format!("http://{ASSETS}:{port}/pkg?sig=x")),
            ),
            ((ASSETS, "/pkg?sig=x".to_owned()), Reply::Ok(package)),
        ])
    }

    struct Run {
        check: Result<Option<AvailableUpdate>, UpdateError>,
        install: Result<String, UpdateError>,
        hosts: Vec<String>,
        sent: usize,
        marker: String,
    }

    type Sign = Box<dyn FnOnce(&Key, &[u8]) -> Option<String>>;
    type Tamper = fn(&mut Vec<u8>);

    /// Installs an app with marker `old`, serves a release of an archive with marker `new`
    /// (signed by `sign`, served after `tamper`), then checks for and installs it.
    fn check_and_install(air_gap: bool, sign: Sign, tamper: Tamper) -> Run {
        let dir = tempfile::tempdir().unwrap();
        let macos = dir.path().join("apps/App.app/Contents/MacOS");
        std::fs::create_dir_all(&macos).unwrap();
        std::fs::write(macos.join("marker"), "old").unwrap();
        let key = Key::new();
        let mut pkg = package(dir.path(), "new");
        let sig = sign(&key, &pkg);
        tamper(&mut pkg);
        let (port, seen) = serve(|port| release(port, manifest(port, sig), pkg));
        let (_db, net) = client(port, air_gap);
        let app = mock_app();
        let updater = test_updater(&app, &key, &macos.join("app"));
        let pending = Pending::default();
        let url = format!("http://{GH}:{port}/latest.json");
        let check = block_on(check(&net, &url, &updater, &pending));
        let install = block_on(install(&net, &pending));
        let hosts = seen.lock().unwrap().clone();
        Run {
            check,
            install,
            hosts,
            sent: net.sent_count(),
            marker: std::fs::read_to_string(macos.join("marker")).unwrap(),
        }
    }

    fn signed() -> Sign {
        Box::new(|k, p| Some(k.sign(p, VERSION)))
    }

    #[test]
    fn a_signed_release_installs_through_the_github_redirect_chain() {
        let r = check_and_install(false, signed(), |_| {});
        assert_eq!(r.check.unwrap().unwrap().version, VERSION);
        assert_eq!(r.install.unwrap(), VERSION);
        assert_eq!(r.marker, "new");
        assert_eq!(r.hosts, [GH, GH, GH, ASSETS]);
    }

    #[test]
    fn unsigned_tampered_or_mismatched_packages_are_never_installed() {
        let flip: Tamper = |p| {
            let last = p.len() - 1;
            p[last] ^= 1;
        };
        let other = Key::new();
        let cases: [(&str, Sign, Tamper); 4] = [
            ("tampered bytes", signed(), flip),
            (
                "another key",
                Box::new(move |_, p| Some(other.sign(p, VERSION))),
                |_| {},
            ),
            (
                "another signed version",
                Box::new(|k, p| Some(k.sign(p, "9.9.8"))),
                |_| {},
            ),
            (
                "empty signature",
                Box::new(|_, _| Some(String::new())),
                |_| {},
            ),
        ];
        for (name, sign, tamper) in cases {
            let r = check_and_install(false, sign, tamper);
            assert!(r.check.is_ok_and(|u| u.is_some()), "{name}");
            assert!(
                matches!(r.install, Err(UpdateError::Signature)),
                "{name}: {:?}",
                r.install
            );
            assert_eq!(r.marker, "old", "{name}");
        }
    }

    #[test]
    fn a_manifest_without_a_signature_is_rejected() {
        let r = check_and_install(false, Box::new(|_, _| None), |_| {});
        assert!(
            matches!(r.check, Err(UpdateError::Manifest)),
            "{:?}",
            r.check
        );
        assert!(matches!(r.install, Err(UpdateError::NothingPending)));
        assert_eq!(r.hosts, [GH], "the package is never requested");
        assert_eq!(r.marker, "old");
    }

    #[test]
    fn air_gap_on_sends_no_request() {
        let r = check_and_install(true, signed(), |_| {});
        assert!(
            matches!(r.check, Err(UpdateError::AirGapEnabled)),
            "{:?}",
            r.check
        );
        assert!(matches!(r.install, Err(UpdateError::NothingPending)));
        assert_eq!(r.sent, 0);
        assert!(r.hosts.is_empty());
        assert_eq!(r.marker, "old");
    }

    #[test]
    fn the_asset_host_is_refused_as_a_first_hop() {
        let (port, seen) = serve(|_| Routes::new());
        let (_db, net) = client(port, false);
        let app = mock_app();
        let dir = tempfile::tempdir().unwrap();
        let updater = test_updater(&app, &Key::new(), &dir.path().join("app"));
        let url = format!("http://{ASSETS}:{port}/latest.json");
        let r = block_on(check(&net, &url, &updater, &Pending::default()));
        assert!(
            matches!(
                r,
                Err(UpdateError::Net(NetError::Blocked(
                    SecurityError::HostNotAllowed
                )))
            ),
            "{r:?}"
        );
        assert_eq!(net.sent_count(), 0);
        assert!(seen.lock().unwrap().is_empty());
    }

    #[test]
    fn an_oversized_manifest_is_refused() {
        let big = vec![b' '; MANIFEST_MAX + 1];
        let (port, _) =
            serve(|_| Routes::from([((GH, "/latest.json".to_owned()), Reply::Ok(big))]));
        let (_db, net) = client(port, false);
        let url = format!("http://{GH}:{port}/latest.json");
        let r = block_on(fetch(&net, &url, MANIFEST_MAX));
        assert!(matches!(r, Err(UpdateError::TooLarge)), "{r:?}");
    }

    #[test]
    fn the_updater_fails_closed_without_a_public_key() {
        for key in ["", "  "] {
            let mut ctx = tauri::generate_context!(test = true);
            ctx.config_mut()
                .plugins
                .0
                .insert("updater".into(), serde_json::json!({ "pubkey": key }));
            let app = tauri::test::mock_builder()
                .plugin(tauri_plugin_updater::Builder::new().build())
                .build(ctx)
                .unwrap();
            assert!(matches!(
                updater(app.handle()),
                Err(UpdateError::NotConfigured)
            ));
        }
        assert!(updater(mock_app().handle()).is_ok(), "committed key");
    }
}
