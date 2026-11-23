// SPDX-License-Identifier: GPL-3.0-or-later
//! One file -> stored chunks: scope, stat, hash, parse, chunk, embed, write.
//! Sync on purpose: callers run it in `spawn_blocking` or a worker thread. This is the one
//! ingest file that calls storage. `prepare`, `embed`, and `commit` are the stages the
//! queue runs on separate workers; `process_file` runs them in a row.

use std::path::Path;

use super::chunker::{self, Chunk, ChunkerConfig, TokenCounter};
use super::error::PipelineError;
use super::hash::{self, Decision, FileState};
use super::parser::ParserRegistry;
use super::scanner;
use crate::rag::embedder::Embedder;
use crate::security::paths::ScopedPath;
use crate::storage::db::{Db, DocId};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Change {
    Skip,
    Reindex,
    New,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Outcome {
    pub doc_id: DocId,
    pub change: Change,
    /// Chunks written by this call; 0 on Skip.
    pub chunks: usize,
}

/// A parsed and chunked file, waiting for its embeddings and commit.
pub struct Parsed {
    root: std::path::PathBuf,
    path: ScopedPath,
    doc_id: DocId,
    state: FileState,
    change: Change,
    chunks: Vec<Chunk>,
    needs_ocr: bool,
}

pub enum Prepared {
    Skip(Outcome),
    Ready(Parsed),
}

pub enum Commit {
    Done(Outcome),
    /// The file's stat changed after it was hashed; nothing was written.
    Changed,
    /// The file is gone; its document row was deleted.
    Gone(DocId),
    /// The folder root is gone (an unplugged drive); nothing was written or deleted.
    Unavailable,
}

/// Stat, hash, parse, and chunk. Creates a pending row for a new file. Idempotent: a crash
/// or cancel after the pending row leaves its sha256 NULL, so the next run treats the file
/// as new.
pub fn prepare(
    db: &Db,
    registry: &ParserRegistry,
    folder_id: i64,
    root: &Path,
    rel_path: &str,
    cfg: &ChunkerConfig,
    counter: &dyn TokenCounter,
) -> Result<Prepared, PipelineError> {
    let path = ScopedPath::new(root, Path::new(rel_path))?;
    let stat = hash::stat(&path).map_err(PipelineError::Io)?;
    let (doc_id, prev) = match db.document_state(folder_id, rel_path)? {
        Some(row) => row,
        None => {
            let ext = Path::new(rel_path)
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_ascii_lowercase();
            (db.insert_pending_document(folder_id, rel_path, &ext)?, None)
        }
    };
    let decision = match hash::decide(prev.as_ref(), stat, || hash::sha256_file(&path)) {
        Ok(d) => d,
        Err(e) => {
            db.mark_error(doc_id, "read_failed")?;
            return Err(PipelineError::Io(e));
        }
    };
    let (state, change) = match decision {
        Decision::Skip => {
            if prev.is_some_and(|p| p.stat != stat) {
                db.update_stat(doc_id, stat)?;
            }
            return Ok(Prepared::Skip(Outcome {
                doc_id,
                change: Change::Skip,
                chunks: 0,
            }));
        }
        Decision::Reindex(s) => (s, Change::Reindex),
        Decision::New(s) => (s, Change::New),
    };
    let doc = match registry.parse(&path) {
        Ok(d) => d,
        Err(e) => {
            db.mark_error(doc_id, "parse_failed")?;
            return Err(e.into());
        }
    };
    Ok(Prepared::Ready(Parsed {
        chunks: chunker::chunk(doc_id, &doc, cfg, counter),
        needs_ocr: doc.pages.iter().any(|p| p.needs_ocr),
        root: root.to_path_buf(),
        path,
        doc_id,
        state,
        change,
    }))
}

pub fn embed(
    db: &Db,
    parsed: &Parsed,
    embedder: &dyn Embedder,
) -> Result<Vec<Vec<f32>>, PipelineError> {
    let texts: Vec<&str> = parsed.chunks.iter().map(|c| c.text.as_str()).collect();
    embedder.embed_passages(&texts).map_err(|e| {
        match db.mark_error(parsed.doc_id, "embed_failed") {
            Ok(()) => PipelineError::Embed(e),
            Err(s) => s.into(),
        }
    })
}

/// Re-stats the file, then writes the chunks in one transaction only if the stat still
/// matches the one that was hashed. The parser read the file after hashing, so a changed
/// stat means the chunks may not match the stored hash.
pub fn commit(db: &Db, parsed: Parsed, embeddings: &[Vec<f32>]) -> Result<Commit, PipelineError> {
    match hash::stat(&parsed.path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound && !parsed.root.exists() => {
            return Ok(Commit::Unavailable);
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            db.delete_document(parsed.doc_id)?;
            return Ok(Commit::Gone(parsed.doc_id));
        }
        Err(e) => return Err(PipelineError::Io(e)),
        Ok(s) if s != parsed.state.stat => return Ok(Commit::Changed),
        Ok(_) => {}
    }
    db.replace_chunks(
        parsed.doc_id,
        &parsed.state,
        parsed.needs_ocr,
        &parsed.chunks,
        embeddings,
    )?;
    Ok(Commit::Done(Outcome {
        doc_id: parsed.doc_id,
        change: parsed.change,
        chunks: parsed.chunks.len(),
    }))
}

/// How many times `process_file` restarts a file that changed while it was indexed.
const MAX_RESTARTS: usize = 3;

/// `prepare`, `embed`, and `commit` in a row. A file that changes during indexing is
/// restarted; a file that vanishes returns `Io(NotFound)` after its row is deleted.
#[allow(clippy::too_many_arguments)]
pub fn process_file(
    db: &Db,
    registry: &ParserRegistry,
    folder_id: i64,
    root: &Path,
    rel_path: &str,
    cfg: &ChunkerConfig,
    counter: &dyn TokenCounter,
    embedder: &dyn Embedder,
) -> Result<Outcome, PipelineError> {
    for _ in 0..MAX_RESTARTS {
        let parsed = match prepare(db, registry, folder_id, root, rel_path, cfg, counter)? {
            Prepared::Skip(o) => return Ok(o),
            Prepared::Ready(p) => p,
        };
        let embeddings = embed(db, &parsed, embedder)?;
        match commit(db, parsed, &embeddings)? {
            Commit::Done(o) => return Ok(o),
            Commit::Changed => continue,
            Commit::Gone(_) | Commit::Unavailable => {
                return Err(PipelineError::Io(std::io::ErrorKind::NotFound.into()))
            }
        }
    }
    Err(PipelineError::Unstable)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IndexReport {
    pub folder_id: i64,
    pub new: usize,
    pub reindexed: usize,
    pub skipped: usize,
    /// Files whose `process_file` failed, plus entries that could not be read.
    pub failed: usize,
    /// Documents deleted because their file is gone, hidden, excluded, or no longer supported.
    pub removed: usize,
}

/// Registers `root`, canonicalized. Returns the folder id and the canonical root.
pub fn register_folder(db: &Db, root: &Path) -> Result<(i64, std::path::PathBuf), PipelineError> {
    let root = root.canonicalize().map_err(PipelineError::Io)?;
    let path = root
        .to_str()
        .ok_or_else(|| PipelineError::Io(std::io::ErrorKind::InvalidData.into()))?;
    let label = root.file_name().and_then(|n| n.to_str()).unwrap_or(path);
    Ok((db.upsert_folder(path, label)?, root))
}

/// Registers `root` and indexes it once, in this thread, from a `scanner::scan` plan.
/// Per-file errors are counted, not returned. A changed embedding model reindexes every
/// document. The app uses `queue::IndexQueue`; this is for tools and tests.
pub fn index_folder_once(
    db: &Db,
    registry: &ParserRegistry,
    root: &Path,
    cfg: &ChunkerConfig,
    counter: &dyn TokenCounter,
    embedder: &dyn Embedder,
) -> Result<IndexReport, PipelineError> {
    db.sync_embedding_model(embedder.model_id())?;
    let (folder_id, root) = register_folder(db, root)?;
    let plan = scanner::scan(db, registry, folder_id, &root)?;
    let mut report = IndexReport {
        folder_id,
        new: 0,
        reindexed: 0,
        skipped: plan.unchanged,
        failed: plan.failed,
        removed: 0,
    };
    for rel in plan.new.iter().chain(&plan.reindex) {
        match process_file(db, registry, folder_id, &root, rel, cfg, counter, embedder) {
            Ok(o) => match o.change {
                Change::New => report.new += 1,
                Change::Reindex => report.reindexed += 1,
                Change::Skip => report.skipped += 1,
            },
            Err(_) => report.failed += 1,
        }
    }
    for rel in &plan.delete {
        if let Some((doc, _)) = db.document_state(folder_id, rel)? {
            db.delete_document(doc)?;
            report.removed += 1;
        }
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ingest::chunker::CharApprox;
    use crate::rag::embedder::FakeEmbedder;
    use crate::storage::db::tests::temp_db;
    use std::fs::File;
    use std::time::{Duration, SystemTime};

    const BODY: &str = "First paragraph about alpha.\n\nSecond paragraph about beta.\n";

    fn folder(db: &Db) -> i64 {
        db.write(|c| {
            c.execute(
                "INSERT INTO folders(path, added_at, status) VALUES ('/f', 0, 'active')",
                [],
            )?;
            Ok(c.last_insert_rowid())
        })
        .unwrap()
    }

    fn count(db: &Db, sql: &str) -> i64 {
        db.read(|c| c.query_row(sql, [], |r| r.get(0))).unwrap()
    }

    fn set_mtime(path: &Path, secs: u64) {
        let t = SystemTime::UNIX_EPOCH + Duration::from_secs(secs);
        File::options()
            .write(true)
            .open(path)
            .unwrap()
            .set_modified(t)
            .unwrap();
    }

    #[test]
    fn new_skip_and_reindex() {
        let (_db_dir, db) = temp_db();
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("a.txt");
        std::fs::write(&file, BODY).unwrap();
        set_mtime(&file, 1_000);
        let folder = folder(&db);
        let reg = ParserRegistry::new();
        let cfg = ChunkerConfig::default();
        let run = || {
            process_file(
                &db,
                &reg,
                folder,
                dir.path(),
                "a.txt",
                &cfg,
                &CharApprox,
                &FakeEmbedder,
            )
            .unwrap()
        };

        let first = run();
        assert_eq!(first.change, Change::New);
        assert!(first.chunks > 0);
        assert_eq!(count(&db, "SELECT count(*) FROM doc_versions"), 1);
        assert_eq!(
            count(&db, "SELECT count(*) FROM chunks"),
            first.chunks as i64
        );
        assert_eq!(
            count(&db, "SELECT count(*) FROM chunks WHERE length(sha256) = 32"),
            first.chunks as i64
        );
        assert_eq!(
            count(&db, "SELECT count(*) FROM vec_chunks"),
            first.chunks as i64
        );
        assert_eq!(
            count(
                &db,
                "SELECT count(*) FROM documents WHERE status = 'indexed'"
            ),
            1
        );

        // Only the mtime changes: skipped, and the new stat is stored so the next run skips hashing.
        set_mtime(&file, 2_000);
        assert_eq!(run().change, Change::Skip);
        assert_eq!(count(&db, "SELECT count(*) FROM doc_versions"), 1);
        let path = ScopedPath::new(dir.path(), Path::new("a.txt")).unwrap();
        let stored = db
            .document_state(folder, "a.txt")
            .unwrap()
            .unwrap()
            .1
            .unwrap();
        assert_eq!(stored.stat, hash::stat(&path).unwrap());
        assert_eq!(
            hash::decide(Some(&stored), stored.stat, || panic!("rehashed")).unwrap(),
            Decision::Skip
        );

        // One byte changes, same size, new mtime.
        std::fs::write(&file, BODY.replace("alpha", "alphA")).unwrap();
        set_mtime(&file, 3_000);
        let third = run();
        assert_eq!(
            (third.change, third.doc_id),
            (Change::Reindex, first.doc_id)
        );
        assert_eq!(count(&db, "SELECT count(*) FROM doc_versions"), 2);
        assert_eq!(
            count(&db, "SELECT count(*) FROM chunks"),
            third.chunks as i64
        );
        assert_eq!(
            count(&db, "SELECT count(*) FROM vec_chunks"),
            third.chunks as i64
        );
        let text: String = db
            .read(|c| {
                c.query_row("SELECT text FROM chunks ORDER BY chunk_index", [], |r| {
                    r.get(0)
                })
            })
            .unwrap();
        assert!(text.contains("alphA"));
    }

    #[test]
    fn parse_failure_marks_error_and_retries_as_new() {
        let (_db_dir, db) = temp_db();
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.docx"), "not a zip").unwrap();
        let folder = folder(&db);
        let reg = ParserRegistry::new();
        let cfg = ChunkerConfig::default();
        let run = || {
            process_file(
                &db,
                &reg,
                folder,
                dir.path(),
                "a.docx",
                &cfg,
                &CharApprox,
                &FakeEmbedder,
            )
        };
        assert!(matches!(run(), Err(PipelineError::Parse(_))));
        assert_eq!(
            count(&db, "SELECT count(*) FROM documents WHERE status = 'error' AND error_code = 'parse_failed'"),
            1
        );
        // No hash was stored, so the next run still tries, on the same row.
        assert!(matches!(run(), Err(PipelineError::Parse(_))));
        assert_eq!(count(&db, "SELECT count(*) FROM documents"), 1);
    }

    struct FailEmbedder;

    impl Embedder for FailEmbedder {
        fn dim(&self) -> usize {
            384
        }
        fn model_id(&self) -> &str {
            "fail"
        }
        fn embed_passages(&self, _: &[&str]) -> Result<Vec<Vec<f32>>, crate::rag::error::RagError> {
            Err(crate::rag::error::RagError::Tokenizer)
        }
        fn embed_query(&self, _: &str) -> Result<Vec<f32>, crate::rag::error::RagError> {
            Err(crate::rag::error::RagError::Tokenizer)
        }
    }

    #[test]
    fn embed_failure_marks_error_and_writes_nothing() {
        let (_db_dir, db) = temp_db();
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.txt"), BODY).unwrap();
        let r = process_file(
            &db,
            &ParserRegistry::new(),
            folder(&db),
            dir.path(),
            "a.txt",
            &ChunkerConfig::default(),
            &CharApprox,
            &FailEmbedder,
        );
        assert!(matches!(r, Err(PipelineError::Embed(_))));
        assert_eq!(
            count(&db, "SELECT count(*) FROM documents WHERE status = 'error' AND error_code = 'embed_failed'"),
            1
        );
        assert_eq!(count(&db, "SELECT count(*) FROM chunks"), 0);
        assert_eq!(count(&db, "SELECT count(*) FROM vec_chunks"), 0);
    }

    #[test]
    fn index_folder_once_indexes_supported_visible_files() {
        let (_db_dir, db) = temp_db();
        let outside = tempfile::tempdir().unwrap();
        std::fs::write(outside.path().join("secret.txt"), "secret words").unwrap();
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("Notes");
        std::fs::create_dir_all(root.join("sub")).unwrap();
        std::fs::create_dir_all(root.join(".hidden")).unwrap();
        std::fs::write(root.join("a.txt"), BODY).unwrap();
        std::fs::write(root.join("sub/b.md"), "# Title\n\nGamma text.\n").unwrap();
        std::fs::write(root.join(".hidden/c.txt"), BODY).unwrap();
        std::fs::write(root.join(".DS_Store"), [0u8, 1, 2]).unwrap();
        std::fs::write(root.join("x.bin"), [0u8, 1, 2]).unwrap();
        std::fs::write(root.join("bad.docx"), "not a zip").unwrap();
        std::os::unix::fs::symlink(outside.path(), root.join("link")).unwrap();
        std::os::unix::fs::symlink(outside.path().join("secret.txt"), root.join("l.txt")).unwrap();
        let reg = ParserRegistry::new();
        let cfg = ChunkerConfig::default();
        let run = || index_folder_once(&db, &reg, &root, &cfg, &CharApprox, &FakeEmbedder).unwrap();

        let first = run();
        assert_eq!(
            (first.new, first.reindexed, first.skipped, first.failed),
            (2, 0, 0, 1)
        );
        let second = run();
        assert_eq!(second.folder_id, first.folder_id);
        assert_eq!(
            (second.new, second.reindexed, second.skipped, second.failed),
            (0, 0, 2, 1)
        );
        assert_eq!(count(&db, "SELECT count(*) FROM folders"), 1);
        assert_eq!(
            count(&db, "SELECT count(*) FROM folders WHERE label = 'Notes'"),
            1
        );
        let rels: Vec<String> = db
            .read(|c| {
                c.prepare("SELECT rel_path FROM documents ORDER BY rel_path")?
                    .query_map([], |r| r.get(0))?
                    .collect()
            })
            .unwrap();
        assert_eq!(rels, ["a.txt", "bad.docx", "sub/b.md"]);
        assert_eq!(
            count(&db, "SELECT count(*) FROM chunks"),
            count(&db, "SELECT count(*) FROM vec_chunks")
        );
    }

    fn rel_paths(db: &Db) -> Vec<String> {
        db.read(|c| {
            c.prepare("SELECT rel_path FROM documents ORDER BY rel_path")?
                .query_map([], |r| r.get(0))?
                .collect()
        })
        .unwrap()
    }

    #[test]
    fn index_folder_once_removes_vanished_files_after_a_complete_walk() {
        let (_db_dir, db) = temp_db();
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("Notes");
        std::fs::create_dir_all(root.join("sub")).unwrap();
        std::fs::write(root.join("a.txt"), BODY).unwrap();
        std::fs::write(root.join("b.txt"), BODY).unwrap();
        std::fs::write(root.join("sub/c.txt"), BODY).unwrap();
        let reg = ParserRegistry::new();
        let cfg = ChunkerConfig::default();
        let run = || index_folder_once(&db, &reg, &root, &cfg, &CharApprox, &FakeEmbedder).unwrap();
        assert_eq!(run().new, 3);

        std::fs::remove_file(root.join("a.txt")).unwrap();
        std::fs::rename(root.join("b.txt"), root.join(".b.txt")).unwrap();
        let report = run();
        assert_eq!((report.skipped, report.removed), (1, 2));
        assert_eq!(rel_paths(&db), ["sub/c.txt"]);
        assert_eq!(
            count(&db, "SELECT count(*) FROM chunks"),
            count(&db, "SELECT count(*) FROM vec_chunks")
        );

        // An unreadable subfolder is a walk error, so its documents stay.
        let sub = root.join("sub");
        let mode = |m| {
            std::fs::set_permissions(&sub, std::os::unix::fs::PermissionsExt::from_mode(m)).unwrap()
        };
        mode(0o000);
        let report = run();
        mode(0o755);
        assert_eq!((report.failed, report.removed), (1, 0));
        assert_eq!(rel_paths(&db), ["sub/c.txt"]);
    }

    #[test]
    fn a_changed_embedding_model_reindexes_every_document() {
        let (_db_dir, db) = temp_db();
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.txt"), BODY).unwrap();
        let reg = ParserRegistry::new();
        let cfg = ChunkerConfig::default();
        let run =
            || index_folder_once(&db, &reg, dir.path(), &cfg, &CharApprox, &FakeEmbedder).unwrap();
        assert_eq!(run().new, 1);
        assert_eq!(run().skipped, 1);

        assert!(db.sync_embedding_model("other-model").unwrap());
        assert!(!db.sync_embedding_model("other-model").unwrap());
        let report = run();
        assert_eq!((report.new, report.skipped), (1, 0));
        assert_eq!(
            db.setting("embedding_model_id").unwrap().as_deref(),
            Some("fake")
        );
        assert_eq!(count(&db, "SELECT count(*) FROM doc_versions"), 2);
        assert_eq!(run().skipped, 1);
    }

    #[test]
    fn path_outside_root_is_rejected() {
        let (_db_dir, db) = temp_db();
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("root")).unwrap();
        std::fs::write(dir.path().join("x.txt"), "x").unwrap();
        let r = process_file(
            &db,
            &ParserRegistry::new(),
            folder(&db),
            &dir.path().join("root"),
            "../x.txt",
            &ChunkerConfig::default(),
            &CharApprox,
            &FakeEmbedder,
        );
        assert!(matches!(r, Err(PipelineError::Scope(_))));
    }
}
