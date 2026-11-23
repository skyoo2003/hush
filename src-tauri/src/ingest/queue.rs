//! Index jobs, deduplicated by path, run by `parse_workers` threads (stat, hash, parse,
//! chunk) feeding one embed-and-commit thread through a bounded channel. Each path is in
//! the queue or in flight at most once: a job submitted while its path runs is held and
//! runs after it.

use std::collections::{HashMap, VecDeque};
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{sync_channel, Receiver, SyncSender};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use serde::Serialize;
use tokio_util::sync::CancellationToken;

use super::chunker::{ChunkerConfig, TokenCounter};
use super::error::PipelineError;
use super::parser::ParserRegistry;
use super::pipeline::{self, Commit, Parsed, Prepared};
use super::scanner::{self, ScanPlan};
use crate::rag::embedder::Embedder;
use crate::storage::db::Db;

pub const PROGRESS_EVENT: &str = "index://progress";
const PROGRESS_INTERVAL: Duration = Duration::from_millis(250);
const MAX_RETRIES: u32 = 3;
/// Idle workers recheck cancel this often, since the token can be cancelled from outside.
const POLL: Duration = Duration::from_millis(100);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reason {
    New,
    Reindex,
    Delete,
}

/// `root` is the folder's canonical path; `rel_path` is checked by `ScopedPath` when used.
#[derive(Clone)]
pub struct IndexJob {
    pub folder_id: i64,
    pub root: Arc<Path>,
    pub rel_path: String,
    pub reason: Reason,
}

type Key = (i64, String);

impl IndexJob {
    fn key(&self) -> Key {
        (self.folder_id, self.rel_path.clone())
    }
}

/// Counts only: never paths or names.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct IndexProgress {
    pub queued: u32,
    pub running: u32,
    pub done: u32,
    pub failed: u32,
    pub paused: bool,
}

/// How many parse workers may run at once, from 0 (none) to the number spawned.
pub struct ConcurrencyBudget {
    limit: AtomicUsize,
    max: usize,
}

impl ConcurrencyBudget {
    pub fn get(&self) -> usize {
        self.limit.load(Ordering::Relaxed)
    }

    pub fn max(&self) -> usize {
        self.max
    }
}

pub struct QueueConfig {
    pub parse_workers: usize,
    /// Retry n waits `retry_backoff * 2^n`.
    pub retry_backoff: Duration,
}

impl Default for QueueConfig {
    fn default() -> Self {
        let cores = std::thread::available_parallelism().map_or(2, |n| n.get());
        Self {
            parse_workers: (cores / 2).max(1),
            retry_backoff: Duration::from_secs(1),
        }
    }
}

pub struct Services {
    pub db: Arc<Db>,
    pub registry: ParserRegistry,
    pub chunker: ChunkerConfig,
    pub counter: Arc<dyn TokenCounter>,
    pub embedder: Arc<dyn Embedder>,
}

struct Entry {
    job: IndexJob,
    attempts: u32,
    due: Instant,
}

enum Slot {
    Queued(Entry),
    /// A job submitted for the same path while this one runs; it runs next.
    Running {
        rerun: Option<IndexJob>,
    },
}

#[derive(Default)]
struct State {
    order: VecDeque<Key>,
    /// Retries waiting for their backoff.
    delayed: Vec<Key>,
    slots: HashMap<Key, Slot>,
    running: u32,
    done: u32,
    failed: u32,
    paused: bool,
}

impl State {
    fn enqueue(&mut self, key: Key, entry: Entry) {
        if entry.due > Instant::now() {
            self.delayed.push(key.clone());
        } else {
            self.order.push_back(key.clone());
        }
        self.slots.insert(key, Slot::Queued(entry));
    }

    fn promote_due(&mut self, now: Instant) -> Option<Instant> {
        let mut next = None;
        let slots = &self.slots;
        let order = &mut self.order;
        self.delayed.retain(|k| match slots.get(k) {
            Some(Slot::Queued(e)) if e.due > now => {
                next = Some(next.map_or(e.due, |n: Instant| n.min(e.due)));
                true
            }
            _ => {
                order.push_back(k.clone());
                false
            }
        });
        next
    }

    fn progress(&self) -> IndexProgress {
        IndexProgress {
            queued: u32::try_from(self.order.len() + self.delayed.len()).unwrap_or(u32::MAX),
            running: self.running,
            done: self.done,
            failed: self.failed,
            paused: self.paused,
        }
    }
}

/// How a job ended, before retries are applied.
enum Settle {
    Done,
    /// The file changed under the worker; index it again.
    Requeue,
    /// Cancelled: nothing was committed.
    Dropped,
}

struct Shared {
    svc: Services,
    state: Mutex<State>,
    wake: Condvar,
    budget: ConcurrencyBudget,
    cancel: CancellationToken,
    dirty: AtomicBool,
    retry_backoff: Duration,
}

pub struct IndexQueue {
    shared: Arc<Shared>,
    threads: Vec<JoinHandle<()>>,
}

type Work = (IndexJob, u32, Parsed);

impl IndexQueue {
    /// Spawns the workers and the progress thread. `emit` is called at most every 250 ms,
    /// and only when something changed. Cancelling `cancel` stops the queue for good.
    pub fn start(
        svc: Services,
        cfg: QueueConfig,
        cancel: CancellationToken,
        emit: impl Fn(IndexProgress) + Send + 'static,
    ) -> Result<Self, PipelineError> {
        let workers = cfg.parse_workers.max(1);
        let shared = Arc::new(Shared {
            svc,
            state: Mutex::new(State::default()),
            wake: Condvar::new(),
            budget: ConcurrencyBudget {
                limit: AtomicUsize::new(workers),
                max: workers,
            },
            cancel,
            dirty: AtomicBool::new(false),
            retry_backoff: cfg.retry_backoff,
        });
        // One parsed document per parse worker waits for the embedder; then parsers block.
        let (tx, rx) = sync_channel::<Work>(workers);
        let mut threads = Vec::with_capacity(workers + 2);
        let spawn = |name: &str, f: Box<dyn FnOnce() + Send>| {
            std::thread::Builder::new()
                .name(name.into())
                .spawn(f)
                .map_err(PipelineError::Io)
        };
        for i in 0..workers {
            let (s, tx) = (shared.clone(), tx.clone());
            threads.push(spawn("index-parse", Box::new(move || s.parse_loop(i, tx)))?);
        }
        drop(tx);
        let s = shared.clone();
        threads.push(spawn("index-embed", Box::new(move || s.embed_loop(rx)))?);
        let s = shared.clone();
        threads.push(spawn(
            "index-progress",
            Box::new(move || s.progress_loop(emit)),
        )?);
        Ok(Self { shared, threads })
    }

    /// Adds a job, or merges it into the one already queued for its path (the newer reason
    /// wins). A path that is running gets this job once it finishes.
    pub fn submit(&self, job: IndexJob) {
        if self.shared.cancel.is_cancelled() {
            return;
        }
        let mut st = self.shared.lock();
        let key = job.key();
        match st.slots.get_mut(&key) {
            Some(Slot::Queued(e)) => e.job.reason = job.reason,
            Some(Slot::Running { rerun }) => *rerun = Some(job),
            None => {
                let due = Instant::now();
                st.enqueue(
                    key,
                    Entry {
                        job,
                        attempts: 0,
                        due,
                    },
                );
            }
        }
        self.shared.changed(st);
    }

    /// Scans the folder and submits its plan. Run it off the async runtime.
    pub fn rescan(&self, folder_id: i64, root: &Path) -> Result<ScanPlan, PipelineError> {
        let plan = scanner::scan(
            &self.shared.svc.db,
            &self.shared.svc.registry,
            folder_id,
            root,
        )?;
        let root: Arc<Path> = root.into();
        let jobs = [
            (&plan.new, Reason::New),
            (&plan.reindex, Reason::Reindex),
            (&plan.delete, Reason::Delete),
        ];
        for (paths, reason) in jobs {
            for rel in paths {
                self.submit(IndexJob {
                    folder_id,
                    root: root.clone(),
                    rel_path: rel.clone(),
                    reason,
                });
            }
        }
        Ok(plan)
    }

    /// Workers finish the job they hold; no new job starts until `resume`.
    pub fn pause(&self) {
        let mut st = self.shared.lock();
        st.paused = true;
        self.shared.changed(st);
    }

    pub fn resume(&self) {
        let mut st = self.shared.lock();
        st.paused = false;
        self.shared.changed(st);
    }

    /// Stops the queue. A document in flight is dropped before its commit.
    pub fn cancel(&self) {
        self.shared.cancel.cancel();
        let st = self.shared.lock();
        self.shared.changed(st);
    }

    pub fn budget(&self) -> &ConcurrencyBudget {
        &self.shared.budget
    }

    /// Sets how many parse workers may run, clamped to the number spawned.
    pub fn set_concurrency(&self, n: usize) {
        let b = &self.shared.budget;
        b.limit.store(n.min(b.max), Ordering::Relaxed);
        let st = self.shared.lock();
        self.shared.changed(st);
    }

    pub fn progress(&self) -> IndexProgress {
        self.shared.lock().progress()
    }

    /// Waits until nothing is queued or running. Returns false on timeout.
    pub fn wait_idle(&self, timeout: Duration) -> bool {
        let end = Instant::now() + timeout;
        let mut st = self.shared.lock();
        loop {
            if st.slots.is_empty() {
                return true;
            }
            let now = Instant::now();
            if now >= end {
                return false;
            }
            st = self
                .shared
                .wake
                .wait_timeout(st, (end - now).min(POLL))
                .unwrap_or_else(|e| e.into_inner())
                .0;
        }
    }
}

impl Drop for IndexQueue {
    fn drop(&mut self) {
        self.cancel();
        for t in self.threads.drain(..) {
            let _ = t.join();
        }
    }
}

impl Shared {
    fn lock(&self) -> MutexGuard<'_, State> {
        // A worker that panicked mid-update leaves counts, not torn data; keep going.
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn changed(&self, st: MutexGuard<'_, State>) {
        drop(st);
        self.dirty.store(true, Ordering::Relaxed);
        self.wake.notify_all();
    }

    /// The next due job for parse worker `i`, or `None` once cancelled.
    fn next(&self, i: usize) -> Option<(IndexJob, u32)> {
        let mut st = self.lock();
        loop {
            if self.cancel.is_cancelled() {
                st.order.clear();
                st.delayed.clear();
                st.slots.retain(|_, s| matches!(s, Slot::Running { .. }));
                self.changed(st);
                return None;
            }
            let now = Instant::now();
            let next_due = st.promote_due(now);
            if !st.paused && i < self.budget.get() {
                if let Some(key) = st.order.pop_front() {
                    let slot = st.slots.insert(key, Slot::Running { rerun: None });
                    if let Some(Slot::Queued(e)) = slot {
                        st.running += 1;
                        self.changed(st);
                        return Some((e.job, e.attempts));
                    }
                    continue;
                }
            }
            let wait = next_due.map_or(POLL, |d| (d - now).min(POLL));
            st = self
                .wake
                .wait_timeout(st, wait)
                .unwrap_or_else(|e| e.into_inner())
                .0;
        }
    }

    fn parse_loop(&self, i: usize, tx: SyncSender<Work>) {
        while let Some((job, attempts)) = self.next(i) {
            let svc = &self.svc;
            let result = match job.reason {
                Reason::Delete => self.delete(&job).map(|()| Settle::Done),
                Reason::New | Reason::Reindex => match pipeline::prepare(
                    &svc.db,
                    &svc.registry,
                    job.folder_id,
                    &job.root,
                    &job.rel_path,
                    &svc.chunker,
                    svc.counter.as_ref(),
                ) {
                    Ok(Prepared::Skip(_)) => Ok(Settle::Done),
                    Ok(Prepared::Ready(parsed)) => {
                        // The embed worker settles it. It only exits after every sender
                        // is gone, so a send never fails.
                        let _ = tx.send((job, attempts, parsed));
                        continue;
                    }
                    Err(PipelineError::Io(e)) if e.kind() == std::io::ErrorKind::NotFound => {
                        self.delete(&job).map(|()| Settle::Done)
                    }
                    Err(e) => Err(e),
                },
            };
            self.settle(job, attempts, result);
        }
    }

    fn embed_loop(&self, rx: Receiver<Work>) {
        for (job, attempts, parsed) in rx {
            let result = if self.cancel.is_cancelled() {
                Ok(Settle::Dropped)
            } else {
                self.embed_and_commit(parsed)
            };
            self.settle(job, attempts, result);
        }
    }

    fn embed_and_commit(&self, parsed: Parsed) -> Result<Settle, PipelineError> {
        let embeddings = pipeline::embed(&self.svc.db, &parsed, self.svc.embedder.as_ref())?;
        // The last check before the write; the commit itself is one transaction.
        if self.cancel.is_cancelled() {
            return Ok(Settle::Dropped);
        }
        Ok(match pipeline::commit(&self.svc.db, parsed, &embeddings)? {
            Commit::Done(_) | Commit::Gone(_) => Settle::Done,
            Commit::Changed => Settle::Requeue,
        })
    }

    fn delete(&self, job: &IndexJob) -> Result<(), PipelineError> {
        if let Some((doc, _)) = self.svc.db.document_state(job.folder_id, &job.rel_path)? {
            self.svc.db.delete_document(doc)?;
        }
        Ok(())
    }

    fn settle(&self, job: IndexJob, attempts: u32, result: Result<Settle, PipelineError>) {
        let mut st = self.lock();
        st.running -= 1;
        let key = job.key();
        let rerun = match st.slots.remove(&key) {
            Some(Slot::Running { rerun }) => rerun,
            _ => None,
        };
        let now = Instant::now();
        let next = match result {
            Ok(Settle::Done) => {
                st.done += 1;
                None
            }
            Ok(Settle::Dropped) => None,
            Ok(Settle::Requeue) => Some(Entry {
                job: IndexJob {
                    reason: Reason::Reindex,
                    ..job
                },
                attempts: 0,
                due: now,
            }),
            Err(e) if attempts < MAX_RETRIES => {
                // PipelineError messages carry no paths or content.
                tracing::warn!(error = %e, attempt = attempts + 1, "index job failed; retrying");
                Some(Entry {
                    job,
                    attempts: attempts + 1,
                    due: now + self.retry_backoff * 2u32.pow(attempts),
                })
            }
            Err(e) => {
                tracing::warn!(error = %e, "index job failed; giving up");
                st.failed += 1;
                None
            }
        };
        // A job submitted during the run is newer than any retry, so it replaces it.
        let rerun = rerun.map(|job| Entry {
            job,
            attempts: 0,
            due: now,
        });
        if let Some(entry) = rerun.or(next) {
            if !self.cancel.is_cancelled() {
                st.enqueue(key, entry);
            }
        }
        self.changed(st);
    }

    fn progress_loop(&self, emit: impl Fn(IndexProgress)) {
        loop {
            std::thread::sleep(PROGRESS_INTERVAL);
            let stop = self.cancel.is_cancelled();
            if self.dirty.swap(false, Ordering::Relaxed) {
                emit(self.lock().progress());
            }
            if stop {
                return;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ingest::chunker::CharApprox;
    use crate::ingest::pipeline::register_folder;
    use crate::rag::embedder::FakeEmbedder;
    use crate::rag::error::RagError;
    use crate::storage::db::tests::temp_db;

    const WAIT: Duration = Duration::from_secs(20);

    type Hook = Box<dyn FnOnce() + Send>;

    /// FakeEmbedder that counts calls and runs `hook` once, inside the first call.
    #[derive(Default)]
    struct HookEmbedder {
        calls: AtomicUsize,
        hook: Mutex<Option<Hook>>,
    }

    impl HookEmbedder {
        fn with(hook: impl FnOnce() + Send + 'static) -> Self {
            Self {
                hook: Mutex::new(Some(Box::new(hook))),
                ..Self::default()
            }
        }
    }

    impl Embedder for HookEmbedder {
        fn dim(&self) -> usize {
            FakeEmbedder.dim()
        }
        fn model_id(&self) -> &str {
            FakeEmbedder.model_id()
        }
        fn embed_passages(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>, RagError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            if let Some(hook) = self.hook.lock().unwrap().take() {
                hook();
            }
            FakeEmbedder.embed_passages(texts)
        }
        fn embed_query(&self, text: &str) -> Result<Vec<f32>, RagError> {
            FakeEmbedder.embed_query(text)
        }
    }

    struct Fixture {
        _db_dir: tempfile::TempDir,
        _dir: tempfile::TempDir,
        db: Arc<Db>,
        folder: i64,
        root: std::path::PathBuf,
    }

    fn fixture(files: &[(&str, &str)]) -> Fixture {
        let (db_dir, db) = temp_db();
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("Notes");
        std::fs::create_dir_all(&root).unwrap();
        for (name, body) in files {
            std::fs::write(root.join(name), body).unwrap();
        }
        let (folder, root) = register_folder(&db, &root).unwrap();
        Fixture {
            _db_dir: db_dir,
            _dir: dir,
            db: Arc::new(db),
            folder,
            root,
        }
    }

    fn start(
        f: &Fixture,
        embedder: Arc<dyn Embedder>,
        cancel: CancellationToken,
        emit: impl Fn(IndexProgress) + Send + 'static,
    ) -> IndexQueue {
        let svc = Services {
            db: f.db.clone(),
            registry: ParserRegistry::new(),
            chunker: ChunkerConfig::default(),
            counter: Arc::new(CharApprox),
            embedder,
        };
        let cfg = QueueConfig {
            parse_workers: 4,
            retry_backoff: Duration::from_millis(1),
        };
        IndexQueue::start(svc, cfg, cancel, emit).unwrap()
    }

    fn count(db: &Db, sql: &str) -> i64 {
        db.read(|c| c.query_row(sql, [], |r| r.get(0))).unwrap()
    }

    fn job(f: &Fixture, rel: &str, reason: Reason) -> IndexJob {
        IndexJob {
            folder_id: f.folder,
            root: f.root.as_path().into(),
            rel_path: rel.into(),
            reason,
        }
    }

    #[test]
    fn indexes_100_files_and_throttles_progress() {
        let names: Vec<String> = (0..100).map(|i| format!("f{i:03}.txt")).collect();
        let files: Vec<(&str, &str)> = names.iter().map(|n| (n.as_str(), "alpha beta")).collect();
        let f = fixture(&files);
        let emits = Arc::new(Mutex::new(Vec::new()));
        let sink = emits.clone();
        let started = Instant::now();
        let q = start(
            &f,
            Arc::new(FakeEmbedder),
            CancellationToken::new(),
            move |p| sink.lock().unwrap().push((Instant::now(), p)),
        );
        let plan = q.rescan(f.folder, &f.root).unwrap();
        assert_eq!(plan.new.len(), 100);
        assert!(q.wait_idle(WAIT));
        let elapsed = started.elapsed();
        assert_eq!(q.progress().done, 100);
        assert_eq!(
            count(
                &f.db,
                "SELECT count(*) FROM documents WHERE status = 'indexed'"
            ),
            100
        );
        assert_eq!(
            count(&f.db, "SELECT count(*) FROM chunks"),
            count(&f.db, "SELECT count(*) FROM vec_chunks")
        );

        std::thread::sleep(PROGRESS_INTERVAL * 2);
        let emits = emits.lock().unwrap();
        assert_eq!(
            emits.last().unwrap().1.done,
            100,
            "the final state is emitted"
        );
        for w in emits.windows(2) {
            assert!(w[1].0 - w[0].0 >= PROGRESS_INTERVAL - Duration::from_millis(5));
        }
        let secs = (elapsed + PROGRESS_INTERVAL * 2).as_secs_f64();
        assert!(emits.len() as f64 <= 4.0 * secs + 1.0);
        drop(emits);

        // A second scan finds nothing to do.
        let plan = q.rescan(f.folder, &f.root).unwrap();
        assert_eq!((plan.new.len(), plan.unchanged), (0, 100));
    }

    #[test]
    fn duplicate_jobs_merge_into_one() {
        let f = fixture(&[("a.txt", "alpha")]);
        let embedder = Arc::new(HookEmbedder::default());
        let q = start(&f, embedder.clone(), CancellationToken::new(), |_| {});
        q.pause();
        for reason in [Reason::New, Reason::Reindex, Reason::Delete, Reason::New] {
            q.submit(job(&f, "a.txt", reason));
        }
        assert_eq!(q.progress().queued, 1);
        std::thread::sleep(Duration::from_millis(200));
        assert_eq!(q.progress().done, 0, "paused");
        q.resume();
        assert!(q.wait_idle(WAIT));
        assert_eq!(embedder.calls.load(Ordering::SeqCst), 1);
        assert_eq!(q.progress().done, 1);
        assert_eq!(count(&f.db, "SELECT count(*) FROM doc_versions"), 1);
    }

    #[test]
    fn cancel_never_commits_the_document_in_flight() {
        let f = fixture(&[("a.txt", "alpha beta")]);
        let cancel = CancellationToken::new();
        let token = cancel.clone();
        let q = start(
            &f,
            Arc::new(HookEmbedder::with(move || token.cancel())),
            cancel,
            |_| {},
        );
        q.submit(job(&f, "a.txt", Reason::New));
        assert!(q.wait_idle(WAIT));
        assert_eq!(q.progress().done, 0);
        assert_eq!(count(&f.db, "SELECT count(*) FROM chunks"), 0);
        assert_eq!(count(&f.db, "SELECT count(*) FROM vec_chunks"), 0);
        assert_eq!(count(&f.db, "SELECT count(*) FROM doc_versions"), 0);
        assert_eq!(
            count(
                &f.db,
                "SELECT count(*) FROM documents WHERE sha256 IS NOT NULL"
            ),
            0
        );
        // A cancelled queue takes no new work.
        q.submit(job(&f, "a.txt", Reason::New));
        assert_eq!(q.progress().queued, 0);
    }

    #[test]
    fn a_file_overwritten_during_indexing_is_reindexed() {
        let f = fixture(&[("a.txt", "alpha beta")]);
        let path = f.root.join("a.txt");
        let hook = move || std::fs::write(&path, "gamma delta epsilon").unwrap();
        let embedder = Arc::new(HookEmbedder::with(hook));
        let q = start(&f, embedder.clone(), CancellationToken::new(), |_| {});
        q.submit(job(&f, "a.txt", Reason::New));
        assert!(q.wait_idle(WAIT));
        assert_eq!(embedder.calls.load(Ordering::SeqCst), 2);
        // The stale parse was dropped, so only the new content was ever committed.
        assert_eq!(count(&f.db, "SELECT count(*) FROM doc_versions"), 1);
        let text: String =
            f.db.read(|c| c.query_row("SELECT text FROM chunks", [], |r| r.get(0)))
                .unwrap();
        assert!(text.contains("gamma"));
    }

    #[test]
    fn a_file_deleted_during_indexing_is_removed() {
        let f = fixture(&[("a.txt", "alpha beta")]);
        let path = f.root.join("a.txt");
        let hook = move || std::fs::remove_file(&path).unwrap();
        let q = start(
            &f,
            Arc::new(HookEmbedder::with(hook)),
            CancellationToken::new(),
            |_| {},
        );
        q.submit(job(&f, "a.txt", Reason::New));
        assert!(q.wait_idle(WAIT));
        assert_eq!(count(&f.db, "SELECT count(*) FROM documents"), 0);
        assert_eq!(count(&f.db, "SELECT count(*) FROM chunks"), 0);
    }

    #[test]
    fn deleted_files_are_removed_on_rescan() {
        let f = fixture(&[("a.txt", "alpha"), ("b.txt", "beta")]);
        let q = start(&f, Arc::new(FakeEmbedder), CancellationToken::new(), |_| {});
        q.rescan(f.folder, &f.root).unwrap();
        assert!(q.wait_idle(WAIT));
        std::fs::remove_file(f.root.join("a.txt")).unwrap();
        assert_eq!(q.rescan(f.folder, &f.root).unwrap().delete, ["a.txt"]);
        assert!(q.wait_idle(WAIT));
        assert_eq!(count(&f.db, "SELECT count(*) FROM documents"), 1);
    }

    struct FailEmbedder(AtomicUsize);

    impl Embedder for FailEmbedder {
        fn dim(&self) -> usize {
            FakeEmbedder.dim()
        }
        fn model_id(&self) -> &str {
            "fail"
        }
        fn embed_passages(&self, _: &[&str]) -> Result<Vec<Vec<f32>>, RagError> {
            self.0.fetch_add(1, Ordering::SeqCst);
            Err(RagError::Tokenizer)
        }
        fn embed_query(&self, _: &str) -> Result<Vec<f32>, RagError> {
            Err(RagError::Tokenizer)
        }
    }

    #[test]
    fn failures_store_an_error_code_and_retry_three_times() {
        let f = fixture(&[("a.txt", "alpha")]);
        let embedder = Arc::new(FailEmbedder(AtomicUsize::new(0)));
        let q = start(&f, embedder.clone(), CancellationToken::new(), |_| {});
        q.submit(job(&f, "a.txt", Reason::New));
        assert!(q.wait_idle(WAIT));
        assert_eq!(
            embedder.0.load(Ordering::SeqCst),
            4,
            "one try and three retries"
        );
        assert_eq!((q.progress().done, q.progress().failed), (0, 1));
        assert_eq!(
            count(
                &f.db,
                "SELECT count(*) FROM documents WHERE status = 'error' AND error_code = 'embed_failed'"
            ),
            1
        );
        assert_eq!(count(&f.db, "SELECT count(*) FROM chunks"), 0);
    }

    #[test]
    fn a_zero_budget_holds_work_until_raised() {
        let f = fixture(&[("a.txt", "alpha")]);
        let q = start(&f, Arc::new(FakeEmbedder), CancellationToken::new(), |_| {});
        q.set_concurrency(0);
        assert_eq!(q.budget().get(), 0);
        q.submit(job(&f, "a.txt", Reason::New));
        assert!(!q.wait_idle(Duration::from_millis(200)));
        q.set_concurrency(usize::MAX);
        assert_eq!(q.budget().get(), q.budget().max());
        assert!(q.wait_idle(WAIT));
        assert_eq!(q.progress().done, 1);
    }
}
