// SPDX-License-Identifier: GPL-3.0-or-later
//! Watched folders -> index jobs. One actor thread owns the debouncer and the folder map;
//! the debouncer's handler and the IPC side only send it messages. ADR-015.
//!
//! Every event path is handled by what is on disk now, not by the event kind: a file is
//! reindexed (the pipeline skips unchanged hashes), a directory is rescanned, a missing
//! path deletes its documents. A missing root, e.g. an unplugged drive, only marks the
//! folder unavailable and keeps its documents.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;

use notify_debouncer_full::notify::{RecommendedWatcher, RecursiveMode};
use notify_debouncer_full::{
    new_debouncer, DebounceEventResult, DebouncedEvent, Debouncer, RecommendedCache,
};
use serde::Serialize;

use super::error::WatchError;
use super::parser::ParserRegistry;
use super::pipeline;
use super::queue::{IndexJob, IndexQueue, Reason};
use super::scanner;
use crate::storage::db::Db;
use crate::storage::documents::{FolderInfo, FolderStatus};

pub const DEBOUNCE: Duration = Duration::from_millis(1500);
/// Mount points. A recursive watch on a volume's folder gets nothing when the volume
/// unmounts; a watch on this directory does.
const VOLUMES: &str = "/Volumes";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "snake_case")]
pub enum FolderWarning {
    /// `/`, a top-level directory, the home folder, or one of its ancestors. Added anyway.
    BroadPath,
}

#[derive(Debug, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct AddedFolder {
    pub folder: FolderInfo,
    pub warning: Option<FolderWarning>,
}

enum Msg {
    Events(DebounceEventResult),
    Add {
        id: i64,
        root: Arc<Path>,
    },
    /// Acknowledged once the actor has forgotten the folder, so it submits no more jobs.
    Remove(i64, Sender<()>),
    Stop,
}

pub struct FolderWatcher {
    tx: Sender<Msg>,
    thread: Option<JoinHandle<()>>,
}

impl FolderWatcher {
    /// Watches `/Volumes` (when present) and every enabled folder, bringing each
    /// folder's status in line with the disk and rescanning the ones that are present.
    pub fn start(db: Arc<Db>, queue: Arc<IndexQueue>) -> Result<Self, WatchError> {
        let (tx, rx) = channel();
        let events = tx.clone();
        let mut debouncer = new_debouncer(DEBOUNCE, None, move |r| {
            let _ = events.send(Msg::Events(r));
        })
        .map_err(WatchError::Notify)?;
        if Path::new(VOLUMES).is_dir() {
            debouncer
                .watch(VOLUMES, RecursiveMode::NonRecursive)
                .map_err(WatchError::Notify)?;
        }
        let folders = db
            .folders()?
            .into_iter()
            .filter(|f| f.enabled)
            .map(|f| {
                let root = Folder {
                    root: PathBuf::from(f.path).into(),
                    status: f.status,
                };
                (f.id, root)
            })
            .collect();
        let actor = Actor {
            db,
            queue,
            registry: ParserRegistry::new(),
            debouncer,
            folders,
        };
        let thread = std::thread::Builder::new()
            .name("folder-watcher".into())
            .spawn(move || actor.run(rx))
            .map_err(|e| WatchError::Pipeline(super::error::PipelineError::Io(e)))?;
        Ok(Self {
            tx,
            thread: Some(thread),
        })
    }

    /// Starts watching a registered folder and scans it.
    pub fn add(&self, id: i64, root: &Path) {
        let _ = self.tx.send(Msg::Add {
            id,
            root: root.into(),
        });
    }

    /// Stops watching the folder. Returns once no more of its jobs will be submitted.
    pub fn remove(&self, id: i64) {
        let (done, wait) = channel();
        if self.tx.send(Msg::Remove(id, done)).is_ok() {
            let _ = wait.recv();
        }
    }
}

impl Drop for FolderWatcher {
    fn drop(&mut self) {
        let _ = self.tx.send(Msg::Stop);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

struct Folder {
    /// Canonical.
    root: Arc<Path>,
    status: FolderStatus,
}

struct Actor {
    db: Arc<Db>,
    queue: Arc<IndexQueue>,
    registry: ParserRegistry,
    debouncer: Debouncer<RecommendedWatcher, RecommendedCache>,
    folders: HashMap<i64, Folder>,
}

impl Actor {
    fn run(mut self, rx: Receiver<Msg>) {
        let ids: Vec<i64> = self.folders.keys().copied().collect();
        for id in ids {
            self.reconcile(id, true);
        }
        while let Ok(msg) = rx.recv() {
            match msg {
                Msg::Events(Ok(events)) => self.on_events(&events),
                // notify errors can carry paths; log the count only.
                Msg::Events(Err(errors)) => {
                    tracing::warn!(errors = errors.len(), "file watcher reported errors")
                }
                Msg::Add { id, root } => {
                    let status = FolderStatus::Active;
                    self.folders.insert(id, Folder { root, status });
                    self.reconcile(id, true);
                }
                Msg::Remove(id, done) => {
                    if let Some(f) = self.folders.remove(&id) {
                        let _ = self.debouncer.unwatch(&f.root);
                    }
                    let _ = done.send(());
                }
                Msg::Stop => return,
            }
        }
    }

    /// Converges the folder's status and watch to the disk. A present root that was
    /// unavailable (or every root when `force`) gets a fresh watch and a rescan.
    fn reconcile(&mut self, id: i64, force: bool) {
        let Some(f) = self.folders.get_mut(&id) else {
            return;
        };
        let present = f.root.is_dir();
        let status = if present {
            FolderStatus::Active
        } else {
            FolderStatus::Unavailable
        };
        if f.status == status && !force {
            return;
        }
        f.status = status;
        let root = f.root.clone();
        if let Err(e) = self.db.set_folder_status(id, status) {
            tracing::warn!(folder_id = id, error = %e, "folder status not saved");
        }
        tracing::info!(folder_id = id, ?status, "folder status");
        if present {
            // A remounted volume needs a new watch; the old one may be dead.
            let _ = self.debouncer.unwatch(&root);
            if self
                .debouncer
                .watch(&root, RecursiveMode::Recursive)
                .is_err()
            {
                tracing::warn!(folder_id = id, "folder watch failed");
            }
            self.rescan(id);
        }
    }

    fn rescan(&self, id: i64) {
        let Some(f) = self.folders.get(&id) else {
            return;
        };
        if f.status == FolderStatus::Active {
            if let Err(e) = self.queue.rescan(id, &f.root) {
                tracing::warn!(folder_id = id, error = %e, "folder rescan failed");
            }
        }
    }

    fn on_events(&mut self, events: &[DebouncedEvent]) {
        let mut reconcile = BTreeSet::new();
        let mut rescan = BTreeSet::new();
        let mut touched: BTreeMap<i64, BTreeSet<&Path>> = BTreeMap::new();
        for ev in events {
            for (&id, f) in &self.folders {
                let related = |p: &PathBuf| p.starts_with(&f.root) || f.root.starts_with(p);
                if ev.need_rescan() && (ev.paths.is_empty() || ev.paths.iter().any(related)) {
                    rescan.insert(id);
                }
                for p in &ev.paths {
                    if f.root.starts_with(p) {
                        // The root itself or an ancestor, e.g. `/Volumes/Drive`.
                        reconcile.insert(id);
                    } else if p.starts_with(&f.root) {
                        match f.status {
                            // It may be back; reconcile rescans it.
                            FolderStatus::Unavailable => reconcile.insert(id),
                            FolderStatus::Active => touched.entry(id).or_default().insert(p),
                        };
                    }
                }
            }
        }
        for id in reconcile {
            self.reconcile(id, false);
        }
        for id in rescan {
            self.rescan(id);
        }
        for (id, paths) in touched {
            self.apply(id, &paths);
        }
    }

    /// Turns changed paths inside an active folder into jobs, by what is on disk now.
    fn apply(&mut self, id: i64, paths: &BTreeSet<&Path>) {
        let Some(f) = self.folders.get(&id) else {
            return;
        };
        let root = f.root.clone();
        if f.status != FolderStatus::Active {
            return;
        }
        if !root.is_dir() {
            self.reconcile(id, false);
            return;
        }
        let filter = match scanner::filter(&self.db, &root) {
            Ok(filter) => filter,
            Err(e) => {
                tracing::warn!(folder_id = id, error = %e, "exclude rules not read");
                return;
            }
        };
        let submit = |rel: &str, reason| {
            self.queue.submit(IndexJob {
                folder_id: id,
                root: root.clone(),
                rel_path: rel.to_owned(),
                reason,
            })
        };
        let known: Vec<String> = match self.db.folder_documents(id) {
            Ok(docs) => docs.into_iter().map(|(_, rel, _)| rel).collect(),
            Err(e) => {
                tracing::warn!(folder_id = id, error = %e, "documents not read");
                return;
            }
        };
        let missing = |p: &Path| matches!(std::fs::symlink_metadata(p), Err(e) if e.kind() == std::io::ErrorKind::NotFound);
        let mut dirs = BTreeSet::new();
        let mut dir_changed = false;
        for &p in paths {
            let Some(rel) = p.strip_prefix(&root).ok().and_then(|r| r.to_str()) else {
                continue;
            };
            dirs.extend(p.parent());
            let gone = match std::fs::symlink_metadata(p) {
                // A moved-in directory brings no per-file events.
                Ok(md) if md.is_dir() => {
                    dir_changed |= filter.admits_dir(&root, p);
                    continue;
                }
                Ok(md) if md.is_file() && filter.admits_file(&self.registry, &root, p) => {
                    submit(rel, Reason::Reindex);
                    continue;
                }
                // A file the scan would skip, a symlink, or nothing: drop what was indexed there.
                Ok(_) => false,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => true,
                // Unreadable right now; the next scan decides.
                Err(_) => continue,
            };
            for doc in &known {
                // A file at `rel` keeps only its own document; a missing dir drops all below it.
                let under = gone && doc.strip_prefix(rel).is_some_and(|r| r.starts_with('/'));
                if doc == rel || under {
                    submit(doc, Reason::Delete);
                }
            }
        }
        // FSEvents keeps a recent file's "created" flag, so the debouncer can report its rename
        // as a create at the new path only. Drop vanished documents next to every touched path.
        for doc in &known {
            let path = root.join(doc);
            if path.parent().is_some_and(|d| dirs.contains(d)) && missing(&path) {
                submit(doc, Reason::Delete);
            }
        }
        if dir_changed {
            self.rescan(id);
        }
    }
}

/// Rejects a folder that equals, contains, or is inside a registered one (by path
/// component, so `/a/b` and `/a/bc` do not overlap). Warns about a broad folder: `/`, a
/// top-level directory, the home folder, or an ancestor of it.
pub fn check_new_folder(
    existing: &[PathBuf],
    new: &Path,
    home: Option<&Path>,
) -> Result<Option<FolderWarning>, WatchError> {
    if existing
        .iter()
        .any(|e| e.starts_with(new) || new.starts_with(e))
    {
        return Err(WatchError::Overlap);
    }
    // `/` plus at most one name.
    let broad = new.components().count() <= 2 || home.is_some_and(|h| h.starts_with(new));
    Ok(broad.then_some(FolderWarning::BroadPath))
}

/// Registers a folder the user picked and starts watching it. Without a running watcher
/// (the model is not loaded yet) it is scanned at the next launch.
pub fn add_folder(
    db: &Db,
    watcher: Option<&FolderWatcher>,
    path: &Path,
) -> Result<AddedFolder, WatchError> {
    let root = path.canonicalize().map_err(|_| WatchError::Unreadable)?;
    if !root.is_dir() {
        return Err(WatchError::Unreadable);
    }
    let existing: Vec<PathBuf> = db.folders()?.into_iter().map(|f| f.path.into()).collect();
    let home = std::env::home_dir().and_then(|h| h.canonicalize().ok());
    let warning = check_new_folder(&existing, &root, home.as_deref())?;
    let (id, root) = pipeline::register_folder(db, &root)?;
    if let Some(w) = watcher {
        w.add(id, &root);
    }
    let folder = db.folder(id)?.ok_or(WatchError::NotFound)?;
    Ok(AddedFolder { folder, warning })
}

/// Stops watching the folder, drops its queued jobs, then deletes it and its documents.
pub fn remove_folder(
    db: &Db,
    watcher: Option<&FolderWatcher>,
    queue: Option<&IndexQueue>,
    id: i64,
) -> Result<(), WatchError> {
    if db.folder(id)?.is_none() {
        return Err(WatchError::NotFound);
    }
    if let Some(w) = watcher {
        w.remove(id);
    }
    if let Some(q) = queue {
        q.forget_folder(id);
    }
    db.delete_folder(id)?;
    Ok(())
}

/// Stops or resumes watching and indexing a folder. Its documents stay either way; search
/// leaves out a disabled folder's documents.
pub fn set_folder_enabled(
    db: &Db,
    watcher: Option<&FolderWatcher>,
    queue: Option<&IndexQueue>,
    id: i64,
    enabled: bool,
) -> Result<(), WatchError> {
    let folder = db.folder(id)?.ok_or(WatchError::NotFound)?;
    db.set_folder_enabled(id, enabled)?;
    if enabled {
        if let Some(w) = watcher {
            w.add(id, Path::new(&folder.path));
        }
        return Ok(());
    }
    if let Some(w) = watcher {
        w.remove(id);
    }
    if let Some(q) = queue {
        q.forget_folder(id);
    }
    Ok(())
}

/// Rescans every enabled folder, so a changed exclude pattern takes effect.
pub fn rescan_enabled(db: &Db, watcher: Option<&FolderWatcher>) -> Result<(), WatchError> {
    let Some(w) = watcher else {
        return Ok(());
    };
    for f in db.folders()?.into_iter().filter(|f| f.enabled) {
        w.add(f.id, Path::new(&f.path));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ingest::chunker::{CharApprox, ChunkerConfig};
    use crate::ingest::queue::{QueueConfig, Services};
    use crate::rag::embedder::FakeEmbedder;
    use crate::rag::hybrid::{NoopReranker, SearchMode, SearchService};
    use crate::rag::vector_store::SearchFilter;
    use crate::storage::db::tests::temp_db;
    use std::time::Instant;
    use tokio_util::sync::CancellationToken;

    const TIMEOUT: Duration = Duration::from_secs(10);

    fn wait_until(what: &str, mut f: impl FnMut() -> bool) {
        let end = Instant::now() + TIMEOUT;
        while !f() {
            assert!(Instant::now() < end, "timed out waiting until {what}");
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    #[test]
    fn check_new_folder_table() {
        let existing = [PathBuf::from("/a/b"), PathBuf::from("/x/y/z")];
        let home = Path::new("/Users/me");
        let check = |p: &str| check_new_folder(&existing, Path::new(p), Some(home));
        for overlap in ["/a/b", "/a/b/c", "/a", "/x/y", "/x/y/z/deep", "/"] {
            assert!(
                matches!(check(overlap), Err(WatchError::Overlap)),
                "{overlap}"
            );
        }
        for (path, warning) in [
            ("/a/bc", None),
            ("/x/yz", None),
            ("/Users/me/Documents", None),
            ("/Users", Some(FolderWarning::BroadPath)),
            ("/Volumes", Some(FolderWarning::BroadPath)),
            ("/Users/me", Some(FolderWarning::BroadPath)),
        ] {
            assert_eq!(check(path).unwrap(), warning, "{path}");
        }
        assert_eq!(
            check_new_folder(&[], Path::new("/"), None).unwrap(),
            Some(FolderWarning::BroadPath)
        );
        assert_eq!(
            check_new_folder(&[], Path::new("/Users/me"), None).unwrap(),
            None
        );
    }

    struct Env {
        _db_dir: tempfile::TempDir,
        db: Arc<Db>,
        search: SearchService,
        // Dropped before the queue it holds.
        watcher: FolderWatcher,
        queue: Arc<IndexQueue>,
    }

    fn env() -> Env {
        let (db_dir, db) = temp_db();
        let db = Arc::new(db);
        let svc = Services {
            db: db.clone(),
            registry: ParserRegistry::new(),
            chunker: ChunkerConfig::default(),
            counter: Arc::new(CharApprox),
            embedder: Arc::new(FakeEmbedder),
        };
        let queue = Arc::new(
            IndexQueue::start(
                svc,
                QueueConfig::default(),
                CancellationToken::new(),
                |_| {},
            )
            .unwrap(),
        );
        let watcher = FolderWatcher::start(db.clone(), queue.clone()).unwrap();
        Env {
            _db_dir: db_dir,
            search: SearchService::new(Arc::new(FakeEmbedder), Box::new(NoopReranker)),
            db,
            watcher,
            queue,
        }
    }

    impl Env {
        fn add(&self, root: &Path) -> FolderInfo {
            add_folder(&self.db, Some(&self.watcher), root)
                .unwrap()
                .folder
        }

        /// Relative paths of hits whose snippet contains `word`, as a user would see them.
        fn found(&self, word: &str) -> Vec<String> {
            let res = self
                .search
                .search(
                    &self.db,
                    word,
                    SearchMode::Hybrid,
                    &SearchFilter::default(),
                    10,
                )
                .unwrap();
            let mut paths: Vec<String> = res
                .hits
                .into_iter()
                .filter(|h| h.snippet.contains(word))
                .map(|h| h.rel_path)
                .collect();
            paths.sort();
            paths
        }

        fn status(&self, id: i64) -> FolderStatus {
            self.db.folder(id).unwrap().unwrap().status
        }
    }

    #[test]
    fn create_modify_rename_and_delete_reach_search() {
        let e = env();
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("Notes");
        std::fs::create_dir_all(&root).unwrap();
        e.add(&root);

        std::fs::write(root.join("a.txt"), "the apricot harvest").unwrap();
        wait_until("created", || e.found("apricot") == ["a.txt"]);

        std::fs::write(root.join("a.txt"), "the blueberry harvest").unwrap();
        wait_until("modified", || {
            e.found("blueberry") == ["a.txt"] && e.found("apricot").is_empty()
        });

        std::fs::rename(root.join("a.txt"), root.join("b.txt")).unwrap();
        wait_until("renamed", || e.found("blueberry") == ["b.txt"]);

        std::fs::create_dir(root.join("sub")).unwrap();
        std::fs::write(root.join("sub/c.txt"), "a cherry tree").unwrap();
        wait_until("created in a new dir", || {
            e.found("cherry") == ["sub/c.txt"]
        });

        // A moved-in directory brings no per-file events; a moved-out one none either.
        let outside = dir.path().join("Outside");
        std::fs::create_dir(&outside).unwrap();
        std::fs::write(outside.join("d.txt"), "a date palm").unwrap();
        std::fs::rename(&outside, root.join("moved")).unwrap();
        wait_until("moved in", || e.found("date") == ["moved/d.txt"]);
        std::fs::rename(root.join("moved"), &outside).unwrap();
        wait_until("moved out", || e.found("date").is_empty());

        std::fs::remove_file(root.join("b.txt")).unwrap();
        std::fs::remove_dir_all(root.join("sub")).unwrap();
        wait_until("deleted", || {
            e.found("blueberry").is_empty() && e.found("cherry").is_empty()
        });
    }

    #[test]
    fn a_moved_away_root_keeps_its_documents_and_comes_back() {
        let e = env();
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("Notes");
        let away = dir.path().join("Away");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("a.txt"), "the apricot harvest").unwrap();
        let folder = e.add(&root);
        wait_until("indexed", || e.found("apricot") == ["a.txt"]);

        std::fs::rename(&root, &away).unwrap();
        wait_until("unavailable", || {
            e.status(folder.id) == FolderStatus::Unavailable
        });
        assert!(e.queue.wait_idle(TIMEOUT));
        assert_eq!(e.found("apricot"), ["a.txt"], "documents are kept");

        std::fs::write(away.join("b.txt"), "a blueberry pie").unwrap();
        std::fs::rename(&away, &root).unwrap();
        wait_until("back and rescanned", || {
            e.status(folder.id) == FolderStatus::Active && e.found("blueberry") == ["b.txt"]
        });
        assert_eq!(e.found("apricot"), ["a.txt"]);
    }

    #[test]
    fn remove_folder_stops_watching_and_deletes_its_documents() {
        let e = env();
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("Notes");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("a.txt"), "the apricot harvest").unwrap();
        let folder = e.add(&root);
        assert!(matches!(
            add_folder(&e.db, Some(&e.watcher), &root.join(".")),
            Err(WatchError::Overlap)
        ));
        wait_until("indexed", || e.found("apricot") == ["a.txt"]);

        remove_folder(&e.db, Some(&e.watcher), Some(&e.queue), folder.id).unwrap();
        assert!(e.db.folders().unwrap().is_empty());
        assert!(e.found("apricot").is_empty());
        assert!(matches!(
            remove_folder(&e.db, Some(&e.watcher), Some(&e.queue), folder.id),
            Err(WatchError::NotFound)
        ));
        std::fs::write(root.join("b.txt"), "a blueberry pie").unwrap();
        // Two debounce windows: nothing may come back for the removed folder.
        std::thread::sleep(DEBOUNCE * 2);
        assert!(e.queue.wait_idle(TIMEOUT));
        assert!(e.found("blueberry").is_empty());
        assert_eq!(
            e.queue.progress().failed,
            0,
            "no job ran for the removed folder"
        );
    }

    fn hdiutil(args: &[&str]) -> String {
        let out = std::process::Command::new("hdiutil")
            .args(args)
            .output()
            .unwrap();
        assert!(out.status.success(), "hdiutil {args:?} failed");
        String::from_utf8(out.stdout).unwrap()
    }

    fn attach(image: &Path) -> PathBuf {
        let out = hdiutil(&["attach", "-nobrowse", image.to_str().unwrap()]);
        out.lines()
            .find_map(|l| l.split('\t').next_back().filter(|m| m.starts_with(VOLUMES)))
            .map(|m| PathBuf::from(m.trim()))
            .expect("mount point")
    }

    #[test]
    #[ignore = "mounts a disk image with hdiutil (macOS)"]
    fn an_unplugged_volume_keeps_its_documents_and_comes_back() {
        let e = env();
        let dir = tempfile::tempdir().unwrap();
        let image = dir.path().join("vol.dmg");
        let name = format!("LociteWatch{}", std::process::id());
        hdiutil(&[
            "create",
            "-size",
            "2m",
            "-fs",
            "HFS+",
            "-volname",
            &name,
            image.to_str().unwrap(),
        ]);
        let mount = attach(&image);
        let root = mount.join("Docs");
        std::fs::create_dir(&root).unwrap();
        std::fs::write(root.join("a.txt"), "the apricot harvest").unwrap();
        let folder = e.add(&root);
        wait_until("indexed", || e.found("apricot") == ["a.txt"]);

        hdiutil(&["detach", mount.to_str().unwrap()]);
        wait_until("unavailable", || {
            e.status(folder.id) == FolderStatus::Unavailable
        });
        assert!(e.queue.wait_idle(TIMEOUT));
        assert_eq!(e.found("apricot"), ["a.txt"], "documents are kept");

        assert_eq!(attach(&image), mount);
        wait_until("active", || e.status(folder.id) == FolderStatus::Active);
        std::fs::write(root.join("b.txt"), "a blueberry pie").unwrap();
        wait_until("watched again", || e.found("blueberry") == ["b.txt"]);
        assert_eq!(e.found("apricot"), ["a.txt"]);
        hdiutil(&["detach", mount.to_str().unwrap()]);
    }
}
