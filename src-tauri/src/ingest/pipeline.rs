//! One file -> stored chunks: scope, stat, hash, parse, chunk, write.
//! Sync on purpose: callers run it in `spawn_blocking`. This is the one ingest file that
//! calls storage.

use std::path::Path;

use super::chunker::{self, ChunkerConfig, TokenCounter};
use super::error::PipelineError;
use super::hash::{self, Decision};
use super::parser::ParserRegistry;
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
pub fn process_file(
    db: &Db,
    registry: &ParserRegistry,
    folder_id: i64,
    root: &Path,
    rel_path: &str,
    cfg: &ChunkerConfig,
    counter: &dyn TokenCounter,
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
    let needs_ocr = doc.pages.iter().any(|p| p.needs_ocr);
    db.replace_chunks(doc_id, &state, needs_ocr, &chunks)?;
    Ok(Outcome {
        doc_id,
        change,
        chunks: chunks.len(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ingest::chunker::CharApprox;
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
        let run =
            || process_file(&db, &reg, folder, dir.path(), "a.txt", &cfg, &CharApprox).unwrap();

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
        let run = || process_file(&db, &reg, folder, dir.path(), "a.docx", &cfg, &CharApprox);
        assert!(matches!(run(), Err(PipelineError::Parse(_))));
        assert_eq!(
            count(&db, "SELECT count(*) FROM documents WHERE status = 'error' AND error_code = 'parse_failed'"),
            1
        );
        // No hash was stored, so the next run still tries, on the same row.
        assert!(matches!(run(), Err(PipelineError::Parse(_))));
        assert_eq!(count(&db, "SELECT count(*) FROM documents"), 1);
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
        );
        assert!(matches!(r, Err(PipelineError::Scope(_))));
    }
}
