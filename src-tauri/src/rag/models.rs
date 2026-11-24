// SPDX-License-Identifier: GPL-3.0-or-later
//! The model catalog and `ModelManager` (ADR-017). The catalog is `models/catalog.json`, built
//! into the app; there is no remote catalog. A file is installed by download
//! (`NetPurpose::ModelDownload`) or by importing a file the user picked, and reaches its final
//! name only by a rename after its sha256 matched. So a file at its final name is verified.
//! Calls block on file I/O; run them on a blocking thread.

use std::collections::HashMap;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{LazyLock, Mutex, PoisonError};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio_util::sync::CancellationToken;

use super::embedder::hex;
use super::error::ModelError;
use crate::net::{Client, StatusCode};
use crate::security::airgap::NetPurpose;
use crate::security::paths::ScopedPath;

pub const PROGRESS_EVENT: &str = "models://progress";
const PROGRESS_EVERY: Duration = Duration::from_millis(200);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "snake_case")]
pub enum ModelKind {
    Embedding,
    Reranker,
    Llm,
}

#[derive(Debug, Deserialize)]
struct Catalog {
    models: Vec<CatalogModel>,
}

#[derive(Debug, Deserialize)]
pub struct CatalogModel {
    pub id: String,
    pub kind: ModelKind,
    pub license: String,
    /// The license allows the file in the offline kit.
    pub redistributable: bool,
    pub files: Vec<CatalogFile>,
}

#[derive(Debug, Deserialize)]
pub struct CatalogFile {
    pub name: String,
    pub url: String,
    pub size: u64,
    /// Lowercase hex.
    pub sha256: String,
}

static CATALOG: LazyLock<Vec<CatalogModel>> = LazyLock::new(|| {
    // `the_built_in_catalog_is_valid` fails if this does not parse.
    serde_json::from_str::<Catalog>(include_str!("../../../models/catalog.json"))
        .map(|c| c.models)
        .unwrap_or_default()
});

pub fn find(id: &str) -> Option<&'static CatalogModel> {
    CATALOG.iter().find(|m| m.id == id)
}

#[derive(Debug, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct ModelInfo {
    pub id: String,
    pub kind: ModelKind,
    pub license: String,
    pub redistributable: bool,
    #[cfg_attr(test, ts(type = "number"))]
    pub size: u64,
    pub installed: bool,
    pub downloading: bool,
    /// What to fetch on another computer for a manual import.
    pub files: Vec<ModelFile>,
}

#[derive(Debug, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct ModelFile {
    pub name: String,
    pub url: String,
    #[cfg_attr(test, ts(type = "number"))]
    pub size: u64,
    pub sha256: String,
    pub installed: bool,
}

/// Bytes of the whole model, counting files already installed.
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
pub struct DownloadProgress {
    pub id: String,
    #[cfg_attr(test, ts(type = "number"))]
    pub done: u64,
    #[cfg_attr(test, ts(type = "number"))]
    pub total: u64,
}

#[derive(Debug, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct ImportedFile {
    pub id: String,
    pub name: String,
    /// Every file of the model is now in place.
    pub model_installed: bool,
}

pub struct ModelManager {
    dir: PathBuf,
    catalog: &'static [CatalogModel],
    downloads: Mutex<HashMap<String, CancellationToken>>,
    imports: AtomicU64,
    embedder_claimed: AtomicBool,
}

impl ModelManager {
    /// `dir` is `app_data_dir/models`; model files go to `dir/{id}/{name}`.
    pub fn new(dir: PathBuf) -> Self {
        Self::with_catalog(dir, &CATALOG)
    }

    fn with_catalog(dir: PathBuf, catalog: &'static [CatalogModel]) -> Self {
        // An import that crashed mid-copy leaves its temp file; no import is running yet.
        for e in fs::read_dir(&dir).into_iter().flatten().flatten() {
            let name = e.file_name();
            let name = name.to_string_lossy();
            if name.starts_with(".import-") && name.ends_with(".part") {
                let _ = fs::remove_file(e.path());
            }
        }
        Self {
            dir,
            catalog,
            downloads: Mutex::default(),
            imports: AtomicU64::new(0),
            embedder_claimed: AtomicBool::new(false),
        }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    fn find(&self, id: &str) -> Result<&'static CatalogModel, ModelError> {
        self.catalog
            .iter()
            .find(|m| m.id == id)
            .ok_or(ModelError::UnknownModel)
    }

    fn path(&self, m: &CatalogModel, f: &CatalogFile) -> PathBuf {
        self.dir.join(&m.id).join(&f.name)
    }

    fn has(&self, m: &CatalogModel, f: &CatalogFile) -> bool {
        fs::metadata(self.path(m, f)).is_ok_and(|md| md.is_file() && md.len() == f.size)
    }

    pub fn is_installed(&self, id: &str) -> bool {
        self.find(id)
            .is_ok_and(|m| m.files.iter().all(|f| self.has(m, f)))
    }

    pub fn list(&self) -> Vec<ModelInfo> {
        let downloads = lock(&self.downloads);
        self.catalog
            .iter()
            .map(|m| {
                let files: Vec<ModelFile> = m
                    .files
                    .iter()
                    .map(|f| ModelFile {
                        name: f.name.clone(),
                        url: f.url.clone(),
                        size: f.size,
                        sha256: f.sha256.clone(),
                        installed: self.has(m, f),
                    })
                    .collect();
                ModelInfo {
                    id: m.id.clone(),
                    kind: m.kind,
                    license: m.license.clone(),
                    redistributable: m.redistributable,
                    size: m.files.iter().map(|f| f.size).sum(),
                    installed: files.iter().all(|f| f.installed),
                    downloading: downloads.contains_key(&m.id),
                    files,
                }
            })
            .collect()
    }

    /// True once per manager, so the embedder is loaded by one caller. `release` after a failure.
    pub fn claim_embedder_load(&self) -> bool {
        !self.embedder_claimed.swap(true, Ordering::SeqCst)
    }

    pub fn release_embedder_load(&self) {
        self.embedder_claimed.store(false, Ordering::SeqCst);
    }

    /// Cancels a running download. Its `.part` files stay, so the next download resumes.
    pub fn cancel(&self, id: &str) -> bool {
        lock(&self.downloads).get(id).map(|t| t.cancel()).is_some()
    }

    /// Downloads every missing file of `id`. A `.part` left by an earlier attempt is resumed
    /// with a Range request. Air-Gap ON refuses it, or stops it mid-file.
    pub async fn download(
        &self,
        net: &Client,
        id: &str,
        progress: impl Fn(DownloadProgress),
    ) -> Result<(), ModelError> {
        let m = self.find(id)?;
        let cancel = CancellationToken::new();
        {
            let mut d = lock(&self.downloads);
            if d.contains_key(id) {
                return Err(ModelError::Busy);
            }
            d.insert(id.to_owned(), cancel.clone());
        }
        let r = self.download_files(net, m, &cancel, &progress).await;
        lock(&self.downloads).remove(id);
        r
    }

    async fn download_files(
        &self,
        net: &Client,
        m: &CatalogModel,
        cancel: &CancellationToken,
        progress: &dyn Fn(DownloadProgress),
    ) -> Result<(), ModelError> {
        let total = m.files.iter().map(|f| f.size).sum();
        let mut last: Option<Instant> = None;
        let mut report = |done: u64| {
            if done == total || last.is_none_or(|t| t.elapsed() >= PROGRESS_EVERY) {
                last = Some(Instant::now());
                progress(DownloadProgress {
                    id: m.id.clone(),
                    done,
                    total,
                });
            }
        };
        fs::create_dir_all(self.dir.join(&m.id))?;
        let mut before = 0;
        for f in &m.files {
            if !self.has(m, f) {
                self.download_file(net, m, f, cancel, &mut |n| report(before + n))
                    .await?;
            }
            before += f.size;
        }
        report(total);
        Ok(())
    }

    async fn download_file(
        &self,
        net: &Client,
        m: &CatalogModel,
        f: &CatalogFile,
        cancel: &CancellationToken,
        on_bytes: &mut dyn FnMut(u64),
    ) -> Result<(), ModelError> {
        let part = self.dir.join(&m.id).join(format!("{}.part", f.name));
        let mut out = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(&part)?;
        let mut have = out.metadata()?.len();
        if have > f.size {
            have = 0;
        }
        let mut hasher = Sha256::new();
        // The kept prefix is hashed again rather than trusted, so a resumed file is checked whole.
        copy_hashing((&mut out).take(have), &mut hasher, |_| Ok(()))?;
        out.set_len(have)?;
        out.seek(SeekFrom::Start(have))?;

        if have < f.size {
            let mut body = net.get(NetPurpose::ModelDownload, &f.url, have).await?;
            let resumed = format!("bytes {have}-");
            match body.status() {
                StatusCode::PARTIAL_CONTENT
                    if have > 0
                        && body
                            .header("content-range")
                            .is_some_and(|r| r.starts_with(&resumed)) => {}
                // The server ignored the Range header: start over.
                StatusCode::OK => {
                    have = 0;
                    hasher = Sha256::new();
                    out.set_len(0)?;
                    out.seek(SeekFrom::Start(0))?;
                }
                s => return Err(ModelError::Http(s.as_u16())),
            }
            on_bytes(have);
            loop {
                let chunk = tokio::select! {
                    biased;
                    () = cancel.cancelled() => return Err(ModelError::Cancelled),
                    c = body.chunk() => c?,
                };
                let Some(chunk) = chunk else { break };
                have += chunk.len() as u64;
                if have > f.size {
                    drop(out);
                    fs::remove_file(&part)?;
                    return Err(ModelError::HashMismatch);
                }
                out.write_all(&chunk)?;
                hasher.update(&chunk);
                on_bytes(have);
            }
        }
        if have < f.size {
            return Err(ModelError::Incomplete);
        }
        if hex(&hasher.finalize()) != f.sha256 {
            drop(out);
            fs::remove_file(&part)?;
            return Err(ModelError::HashMismatch);
        }
        out.sync_all()?;
        fs::rename(&part, self.path(m, f))?;
        Ok(())
    }

    /// Installs a file downloaded on another computer: it is copied, hashed while copying, and
    /// renamed into place only if the copy's hash matches a catalog file.
    pub fn import(&self, picked: &Path) -> Result<ImportedFile, ModelError> {
        // The picked folder is the scope, so a picked symlink cannot read a file elsewhere.
        let (Some(parent), Some(name)) = (picked.parent(), picked.file_name()) else {
            return Err(ModelError::NotInCatalog);
        };
        let src = ScopedPath::new(parent, Path::new(name))?;
        let size = fs::metadata(src.as_path())?.len();
        // A file of no catalog size is rejected before a byte of it is read.
        if !self
            .catalog
            .iter()
            .flat_map(|m| &m.files)
            .any(|f| f.size == size)
        {
            return Err(ModelError::NotInCatalog);
        }
        fs::create_dir_all(&self.dir)?;
        let n = self.imports.fetch_add(1, Ordering::SeqCst);
        let tmp = self.dir.join(format!(".import-{n}.part"));
        let r = self.import_via(src.as_path(), &tmp);
        if r.is_err() {
            let _ = fs::remove_file(&tmp);
        }
        r
    }

    fn import_via(&self, src: &Path, tmp: &Path) -> Result<ImportedFile, ModelError> {
        let mut out = File::create(tmp)?;
        let mut hasher = Sha256::new();
        let n = copy_hashing(File::open(src)?, &mut hasher, |b| out.write_all(b))?;
        let sha = hex(&hasher.finalize());
        let (m, f) = self
            .catalog
            .iter()
            .flat_map(|m| m.files.iter().map(move |f| (m, f)))
            .find(|(_, f)| f.sha256 == sha && f.size == n)
            // It has a catalog file's size, so it is most likely that file, damaged.
            .ok_or(ModelError::HashMismatch)?;
        out.sync_all()?;
        fs::create_dir_all(self.dir.join(&m.id))?;
        fs::rename(tmp, self.path(m, f))?;
        Ok(ImportedFile {
            id: m.id.clone(),
            name: f.name.clone(),
            model_installed: self.is_installed(&m.id),
        })
    }
}

/// Feeds everything `r` yields to `hasher` and `sink`. Returns the byte count.
fn copy_hashing(
    mut r: impl Read,
    hasher: &mut Sha256,
    mut sink: impl FnMut(&[u8]) -> io::Result<()>,
) -> io::Result<u64> {
    let mut buf = vec![0; 1 << 20];
    let mut n = 0;
    loop {
        match r.read(&mut buf)? {
            0 => return Ok(n),
            k => {
                hasher.update(&buf[..k]);
                sink(&buf[..k])?;
                n += k as u64;
            }
        }
    }
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::net::SocketAddr;
    use crate::security::airgap::AirGapGuard;
    use crate::storage::db::tests::temp_db;
    use std::sync::Arc;
    use tauri::async_runtime::block_on;

    const ID: &str = "test-model";

    fn content() -> Vec<u8> {
        (0..300_000u32).map(|i| (i % 251) as u8).collect()
    }

    fn sha(data: &[u8]) -> String {
        hex(&Sha256::digest(data))
    }

    /// One model with one file served at `http://huggingface.co:{port}/f.bin`.
    fn catalog(port: u16, data: &[u8]) -> &'static [CatalogModel] {
        Box::leak(Box::new([CatalogModel {
            id: ID.into(),
            kind: ModelKind::Embedding,
            license: "MIT".into(),
            redistributable: true,
            files: vec![CatalogFile {
                name: "f.bin".into(),
                url: format!("http://huggingface.co:{port}/f.bin"),
                size: data.len() as u64,
                sha256: sha(data),
            }],
        }]))
    }

    /// What the mock server saw for each request.
    #[derive(Debug, Clone)]
    struct Seen {
        host: String,
        range: Option<u64>,
    }

    /// A local HTTP/1.1 server for `body`. `/f.bin` on `huggingface.co` redirects to the CDN
    /// host when `redirect`; the CDN host serves the bytes, honoring `Range: bytes=N-`. The first
    /// response with a body ends after `cut_first` bytes, like a dropped connection.
    struct Server {
        addr: SocketAddr,
        seen: Arc<Mutex<Vec<Seen>>>,
    }

    fn serve(body: Vec<u8>, redirect: bool, cut_first: Option<usize>) -> Server {
        let (listener, addr) = Client::local_listener();
        let seen = Arc::new(Mutex::new(Vec::new()));
        let log = seen.clone();
        std::thread::spawn(move || {
            let mut bodies = 0;
            for stream in listener.incoming() {
                let Ok(mut s) = stream else { return };
                let mut req = Vec::new();
                let mut b = [0u8; 1];
                while !req.ends_with(b"\r\n\r\n") && s.read(&mut b).unwrap_or(0) == 1 {
                    req.push(b[0]);
                }
                let req = String::from_utf8_lossy(&req).to_lowercase();
                let header = |name: &str| {
                    req.lines()
                        .find_map(|l| l.strip_prefix(&format!("{name}: ")).map(str::to_owned))
                };
                let host = header("host").unwrap_or_default();
                let host = host.split(':').next().unwrap_or_default().to_owned();
                let range = header("range")
                    .and_then(|r| r.strip_prefix("bytes=")?.strip_suffix('-')?.parse().ok());
                lock(&log).push(Seen {
                    host: host.clone(),
                    range,
                });
                if redirect && host == "huggingface.co" {
                    let to = format!("http://us.aws.cdn.hf.co:{}/f.bin", addr.port());
                    let _ = write!(
                        s,
                        "HTTP/1.1 302 Found\r\nLocation: {to}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                    );
                    continue;
                }
                let from = range.unwrap_or(0) as usize;
                let rest = &body[from.min(body.len())..];
                let head = match range {
                    Some(r) => format!(
                        "HTTP/1.1 206 Partial Content\r\nContent-Range: bytes {r}-{}/{}\r\n",
                        body.len() - 1,
                        body.len()
                    ),
                    None => "HTTP/1.1 200 OK\r\n".to_owned(),
                };
                let _ = write!(
                    s,
                    "{head}Content-Length: {}\r\nConnection: close\r\n\r\n",
                    rest.len()
                );
                let sent = match cut_first {
                    Some(n) if bodies == 0 => &rest[..n],
                    _ => rest,
                };
                bodies += 1;
                let _ = s.write_all(sent);
            }
        });
        Server { addr, seen }
    }

    fn client(addr: SocketAddr, air_gap: bool) -> (tempfile::TempDir, Client) {
        let (dir, db) = temp_db();
        let guard = AirGapGuard::load(&db).unwrap();
        guard.set_enabled(&db, air_gap).unwrap();
        let hosts = ["huggingface.co", "us.aws.cdn.hf.co"];
        (dir, Client::for_local_server(Arc::new(guard), &hosts, addr))
    }

    #[test]
    fn the_built_in_catalog_is_valid() {
        assert!(!CATALOG.is_empty());
        let mut ids = std::collections::HashSet::new();
        for m in CATALOG.iter() {
            assert!(ids.insert(&m.id), "duplicate {}", m.id);
            for f in &m.files {
                assert!(
                    !f.name.is_empty() && !f.name.contains(['/', '\\']) && !f.name.starts_with('.'),
                    "{}",
                    f.name
                );
                assert!(f.url.starts_with("https://huggingface.co/"), "{}", f.url);
                assert!(f.size > 0);
                assert!(
                    f.sha256.len() == 64
                        && f.sha256
                            .bytes()
                            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
                    "{}",
                    f.name
                );
            }
        }
        for spec in super::super::embedder::MODELS {
            let m = find(spec.id).unwrap();
            assert_eq!(m.kind, ModelKind::Embedding);
            for name in ["config.json", "tokenizer.json", "model.safetensors"] {
                assert!(m.files.iter().any(|f| f.name == name), "{} {name}", spec.id);
            }
        }
    }

    #[test]
    fn a_dropped_download_resumes_from_its_part_file_through_the_cdn_redirect() {
        let data = content();
        let cut = 100_000;
        let server = serve(data.clone(), true, Some(cut));
        let (_db, net) = client(server.addr, false);
        let dir = tempfile::tempdir().unwrap();
        let mgr = ModelManager::with_catalog(dir.path().into(), catalog(server.addr.port(), &data));

        let first = block_on(mgr.download(&net, ID, |_| {}));
        assert!(
            matches!(first, Err(ModelError::Incomplete | ModelError::Net(_))),
            "{first:?}"
        );
        assert!(!mgr.is_installed(ID));
        let part = dir.path().join(ID).join("f.bin.part");
        assert_eq!(fs::metadata(&part).unwrap().len(), cut as u64);

        let progress = Mutex::new(Vec::new());
        block_on(mgr.download(&net, ID, |p| lock(&progress).push(p.done))).unwrap();
        assert!(mgr.is_installed(ID));
        assert_eq!(fs::read(dir.path().join(ID).join("f.bin")).unwrap(), data);
        assert!(!part.exists());

        let seen = lock(&server.seen).clone();
        let hosts: Vec<&str> = seen.iter().map(|s| s.host.as_str()).collect();
        assert_eq!(
            hosts,
            [
                "huggingface.co",
                "us.aws.cdn.hf.co",
                "huggingface.co",
                "us.aws.cdn.hf.co"
            ]
        );
        // The second attempt asked for the rest, at both hops.
        assert_eq!(seen[2].range, Some(cut as u64));
        assert_eq!(seen[3].range, Some(cut as u64));
        let progress = lock(&progress).clone();
        assert_eq!(progress.first(), Some(&(cut as u64)));
        assert_eq!(progress.last(), Some(&(data.len() as u64)));
        assert!(mgr.list()[0].installed);

        // Installed files are not fetched again.
        block_on(mgr.download(&net, ID, |_| {})).unwrap();
        assert_eq!(lock(&server.seen).len(), 4);
    }

    #[test]
    fn a_download_with_a_wrong_hash_is_never_installed() {
        let data = content();
        let mut wrong = data.clone();
        wrong[1234] ^= 1;
        let server = serve(wrong, false, None);
        let (_db, net) = client(server.addr, false);
        let dir = tempfile::tempdir().unwrap();
        let mgr = ModelManager::with_catalog(dir.path().into(), catalog(server.addr.port(), &data));

        let r = block_on(mgr.download(&net, ID, |_| {}));
        assert!(matches!(r, Err(ModelError::HashMismatch)), "{r:?}");
        assert!(!mgr.is_installed(ID));
        // Neither the final file nor a part to resume from is left.
        assert_eq!(fs::read_dir(dir.path().join(ID)).unwrap().count(), 0);
    }

    #[test]
    fn air_gap_on_refuses_a_download_before_any_request() {
        let data = content();
        let server = serve(data.clone(), true, None);
        let (_db, net) = client(server.addr, true);
        let dir = tempfile::tempdir().unwrap();
        let mgr = ModelManager::with_catalog(dir.path().into(), catalog(server.addr.port(), &data));

        let r = block_on(mgr.download(&net, ID, |_| {}));
        assert!(matches!(r, Err(ModelError::AirGapEnabled)), "{r:?}");
        assert_eq!(net.sent_count(), 0);
        assert!(lock(&server.seen).is_empty());
        assert!(!mgr.is_installed(ID));
        assert!(!mgr.list()[0].downloading);
    }

    #[test]
    fn a_cdn_host_is_reachable_only_by_redirect() {
        let server = serve(content(), true, None);
        let (_db, net) = client(server.addr, false);
        let r = block_on(net.get(
            NetPurpose::ModelDownload,
            &format!("http://us.aws.cdn.hf.co:{}/f.bin", server.addr.port()),
            0,
        ));
        assert!(
            matches!(
                r,
                Err(crate::net::error::NetError::Blocked(
                    crate::security::error::SecurityError::HostNotAllowed
                ))
            ),
            "{:?}",
            r.err()
        );
        assert_eq!(net.sent_count(), 0);
    }

    #[test]
    fn turning_air_gap_on_stops_a_download_mid_body() {
        let (listener, addr) = Client::local_listener();
        // Sends part of the body, then stalls with the connection open.
        std::thread::spawn(move || {
            let (mut s, _) = listener.accept().unwrap();
            let _ = s.read(&mut [0u8; 4096]);
            let _ = write!(s, "HTTP/1.1 200 OK\r\nContent-Length: 300000\r\n\r\n");
            let _ = s.write_all(&[0u8; 1000]);
            std::thread::sleep(Duration::from_secs(30));
        });
        let (_dir, db) = temp_db();
        let guard = Arc::new(AirGapGuard::load(&db).unwrap());
        guard.set_enabled(&db, false).unwrap();
        let net = Client::for_local_server(guard.clone(), &["huggingface.co"], addr);
        let dir = tempfile::tempdir().unwrap();
        let mgr = ModelManager::with_catalog(dir.path().into(), catalog(addr.port(), &content()));
        let part = dir.path().join(ID).join("f.bin.part");

        std::thread::scope(|sc| {
            let (tx, rx) = std::sync::mpsc::channel();
            let (mgr, net) = (&mgr, &net);
            sc.spawn(move || tx.send(block_on(mgr.download(net, ID, |_| {}))).unwrap());
            let start = Instant::now();
            while fs::metadata(&part).map_or(0, |m| m.len()) < 1000 {
                assert!(start.elapsed() < Duration::from_secs(5), "no bytes arrived");
                std::thread::sleep(Duration::from_millis(10));
            }
            // Returns only once the download has let go of its request.
            guard.set_enabled(&db, true).unwrap();
            let r = rx.recv_timeout(Duration::from_secs(5)).unwrap();
            assert!(matches!(r, Err(ModelError::AirGapEnabled)), "{r:?}");
        });
        assert!(!mgr.is_installed(ID));
    }

    #[test]
    fn cancel_stops_a_download_and_keeps_the_part() {
        let data = content();
        let server = serve(data.clone(), false, None);
        let (_db, net) = client(server.addr, false);
        let dir = tempfile::tempdir().unwrap();
        let mgr = ModelManager::with_catalog(dir.path().into(), catalog(server.addr.port(), &data));
        // The first report comes once the response arrives, before any body is read.
        let r = block_on(mgr.download(&net, ID, |_| {
            mgr.cancel(ID);
        }));
        assert!(matches!(r, Err(ModelError::Cancelled)), "{r:?}");
        assert!(!mgr.is_installed(ID));
        assert!(dir.path().join(ID).join("f.bin.part").exists());
        assert!(!mgr.cancel(ID), "no download is running");
    }

    #[test]
    fn a_new_manager_removes_import_temp_files_left_by_a_crash() {
        let dir = tempfile::tempdir().unwrap();
        let stale = dir.path().join(".import-3.part");
        let download_part = dir.path().join("keep.bin.part");
        fs::write(&stale, b"half").unwrap();
        fs::write(&download_part, b"resume me").unwrap();
        ModelManager::with_catalog(dir.path().into(), catalog(1, &content()));
        assert!(!stale.exists());
        assert!(
            download_part.exists(),
            "download parts resume, so they stay"
        );
    }

    #[test]
    fn import_installs_only_a_file_whose_hash_is_in_the_catalog() {
        let data = content();
        let dir = tempfile::tempdir().unwrap();
        let mgr = ModelManager::with_catalog(dir.path().join("models"), catalog(1, &data));
        let picked = dir.path().join("downloaded.bin");

        let mut wrong = data.clone();
        wrong[0] ^= 1;
        fs::write(&picked, &wrong).unwrap();
        assert!(matches!(mgr.import(&picked), Err(ModelError::HashMismatch)));
        fs::write(&picked, b"some other file").unwrap();
        assert!(matches!(mgr.import(&picked), Err(ModelError::NotInCatalog)));
        assert!(!mgr.is_installed(ID));

        fs::write(&picked, &data).unwrap();
        let got = mgr.import(&picked).unwrap();
        assert_eq!((got.id.as_str(), got.name.as_str()), (ID, "f.bin"));
        assert!(got.model_installed && mgr.is_installed(ID));
        assert_eq!(
            fs::read(dir.path().join("models").join(ID).join("f.bin")).unwrap(),
            data
        );
        // No temp files are left behind, and the picked file is untouched.
        let left: Vec<_> = fs::read_dir(dir.path().join("models")).unwrap().collect();
        assert_eq!(left.len(), 1);
        assert!(picked.exists());
    }
}
