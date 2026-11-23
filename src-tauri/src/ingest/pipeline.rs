//! One file -> stored chunks: scope, stat, hash, parse, chunk, embed, write.
//! Sync on purpose: callers run it in `spawn_blocking`. This is the one ingest file that
//! calls storage.

use std::path::Path;

use super::chunker::{self, ChunkerConfig, TokenCounter};
use super::error::PipelineError;
use super::hash::{self, Decision};
use super::parser::ParserRegistry;
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

/// Idempotent: a crash after the pending row leaves its sha256 NULL, so the next run treats
/// the file as new.
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
    // shortcut: the file is read twice (hash, then parse), so an edit in between indexes content newer than the stored hash; hash the parser's bytes if that matters.
    let decision = hash::decide(prev.as_ref(), stat, || hash::sha256_file(&path))
        .map_err(PipelineError::Io)?;
    let (state, change) = match decision {
        Decision::Skip => {
            if prev.is_some_and(|p| p.stat != stat) {
                db.update_stat(doc_id, stat)?;
            }
            return Ok(Outcome {
                doc_id,
                change: Change::Skip,
                chunks: 0,
            });
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
    let chunks = chunker::chunk(doc_id, &doc, cfg, counter);
    let texts: Vec<&str> = chunks.iter().map(|c| c.text.as_str()).collect();
    let embeddings = match embedder.embed_passages(&texts) {
        Ok(e) => e,
        Err(e) => {
            db.mark_error(doc_id, "embed_failed")?;
            return Err(PipelineError::Embed(e));
        }
    };
    let needs_ocr = doc.pages.iter().any(|p| p.needs_ocr);
    db.replace_chunks(doc_id, &state, needs_ocr, &chunks, &embeddings)?;
    Ok(Outcome {
        doc_id,
        change,
        chunks: chunks.len(),
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IndexReport {
    pub folder_id: i64,
    pub new: usize,
    pub reindexed: usize,
    pub skipped: usize,
    /// Files whose `process_file` failed, plus entries that could not be read.
    pub failed: usize,
}

/// Registers `root` and indexes every supported file under it once. Hidden entries are
/// skipped and symlinks are not followed. Per-file errors are counted, not returned.
// shortcut: no watcher and no removal of vanished files; add both with the folder watcher.
pub fn index_folder_once(
    db: &Db,
    registry: &ParserRegistry,
    root: &Path,
    cfg: &ChunkerConfig,
    counter: &dyn TokenCounter,
    embedder: &dyn Embedder,
) -> Result<IndexReport, PipelineError> {
    let root = root.canonicalize().map_err(PipelineError::Io)?;
    let not_utf8 = || PipelineError::Io(std::io::ErrorKind::InvalidData.into());
    let path = root.to_str().ok_or_else(not_utf8)?;
    let label = root.file_name().and_then(|n| n.to_str()).unwrap_or(path);
    let mut report = IndexReport {
        folder_id: db.upsert_folder(path, label)?,
        new: 0,
        reindexed: 0,
        skipped: 0,
        failed: 0,
    };
    let mut dirs = vec![root.clone()];
    while let Some(dir) = dirs.pop() {
        let entries = match std::fs::read_dir(&dir) {
            Ok(e) => e,
            Err(e) if dir == root => return Err(PipelineError::Io(e)),
            Err(_) => {
                report.failed += 1;
                continue;
            }
        };
        for entry in entries {
            let Ok((entry, kind)) = entry.and_then(|e| e.file_type().map(|t| (e, t))) else {
                report.failed += 1;
                continue;
            };
            if entry.file_name().as_encoded_bytes().starts_with(b".") {
                continue;
            }
            let path = entry.path();
            // `file_type` does not follow symlinks, so a link is neither a dir nor a file here.
            if kind.is_dir() {
                dirs.push(path);
                continue;
            }
            let ext = path
                .extension()
                .and_then(|e| e.to_str())
                .map(str::to_ascii_lowercase);
            if !kind.is_file() || !ext.is_some_and(|e| registry.supports(&e)) {
                continue;
            }
            let Some(rel) = path.strip_prefix(&root).ok().and_then(|r| r.to_str()) else {
                report.failed += 1;
                continue;
            };
            match process_file(
                db,
                registry,
                report.folder_id,
                &root,
                rel,
                cfg,
                counter,
                embedder,
            ) {
                Ok(o) => match o.change {
                    Change::New => report.new += 1,
                    Change::Reindex => report.reindexed += 1,
                    Change::Skip => report.skipped += 1,
                },
                Err(_) => report.failed += 1,
            }
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
