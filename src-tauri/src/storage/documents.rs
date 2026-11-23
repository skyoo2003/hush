//! Document and chunk rows for the ingest pipeline. All of its SQL lives here.

use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{params, OptionalExtension};

use super::db::{Db, DocId};
use super::error::StorageError;
use crate::ingest::chunker::Chunk;
use crate::ingest::hash::{FileStat, FileState};

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX))
        .unwrap_or(0)
}

fn size_sql(size: u64) -> i64 {
    i64::try_from(size).unwrap_or(i64::MAX)
}

/// Inserts `chunks` for `doc` inside the caller's transaction. Returns the new row ids in order.
pub(crate) fn insert_chunks(
    tx: &rusqlite::Transaction,
    doc: DocId,
    chunks: &[Chunk],
) -> rusqlite::Result<Vec<i64>> {
    let mut insert = tx.prepare(
        "INSERT INTO chunks(doc_id, chunk_index, page, section_path, start_offset,
                            end_offset, bbox_json, text, sha256)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
    )?;
    chunks
        .iter()
        .map(|ch| {
            let bbox = ch
                .bbox
                .as_ref()
                .map(serde_json::to_string)
                .transpose()
                .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;
            insert.execute(params![
                doc.0,
                ch.chunk_index,
                ch.page,
                ch.section_path,
                ch.start_offset,
                ch.end_offset,
                bbox,
                ch.text,
                &ch.sha256[..],
            ])?;
            Ok(tx.last_insert_rowid())
        })
        .collect()
}

impl Db {
    /// The document's id and its last indexed state; the state is `None` until the first index.
    pub fn document_state(
        &self,
        folder_id: i64,
        rel_path: &str,
    ) -> Result<Option<(DocId, Option<FileState>)>, StorageError> {
        let row = self.read(|c| {
            c.query_row(
                "SELECT id, sha256, size, mtime FROM documents WHERE folder_id = ?1 AND rel_path = ?2",
                params![folder_id, rel_path],
                |r| {
                    Ok((
                        r.get::<_, i64>(0)?,
                        r.get::<_, Option<Vec<u8>>>(1)?,
                        r.get::<_, Option<i64>>(2)?,
                        r.get::<_, Option<i64>>(3)?,
                    ))
                },
            )
            .optional()
        })?;
        Ok(row.map(|(id, sha, size, mtime)| {
            let state = match (sha.and_then(|s| <[u8; 32]>::try_from(s).ok()), size, mtime) {
                (Some(sha256), Some(size), Some(mtime_ns)) => Some(FileState {
                    // A negative size never matches a real stat, so the file is rehashed.
                    stat: FileStat {
                        size: u64::try_from(size).unwrap_or(u64::MAX),
                        mtime_ns,
                    },
                    sha256,
                }),
                _ => None,
            };
            (DocId(id), state)
        }))
    }

    pub fn insert_pending_document(
        &self,
        folder_id: i64,
        rel_path: &str,
        ext: &str,
    ) -> Result<DocId, StorageError> {
        self.write(|c| {
            c.execute(
                "INSERT INTO documents(folder_id, rel_path, ext, status) VALUES (?1, ?2, ?3, 'pending')",
                params![folder_id, rel_path, ext],
            )?;
            Ok(DocId(c.last_insert_rowid()))
        })
    }

    /// Records a new stat for unchanged content, so the next scan skips hashing.
    pub fn update_stat(&self, doc: DocId, stat: FileStat) -> Result<(), StorageError> {
        self.write(|c| {
            c.execute(
                "UPDATE documents SET size = ?2, mtime = ?3 WHERE id = ?1",
                params![doc.0, size_sql(stat.size), stat.mtime_ns],
            )
        })?;
        Ok(())
    }

    /// One transaction: the doc's chunks are replaced, the doc is marked indexed, and one
    /// version row is added.
    pub fn replace_chunks(
        &self,
        doc: DocId,
        state: &FileState,
        needs_ocr: bool,
        chunks: &[Chunk],
    ) -> Result<(), StorageError> {
        let (size, mtime, now) = (size_sql(state.stat.size), state.stat.mtime_ns, now());
        self.write(|c| {
            let tx = c.transaction()?;
            tx.execute("DELETE FROM chunks WHERE doc_id = ?1", [doc.0])?;
            insert_chunks(&tx, doc, chunks)?;
            tx.execute(
                "UPDATE documents SET sha256 = ?2, size = ?3, mtime = ?4, status = 'indexed',
                        error_code = NULL, indexed_at = ?5, needs_ocr = ?6
                 WHERE id = ?1",
                params![doc.0, &state.sha256[..], size, mtime, now, needs_ocr],
            )?;
            tx.execute(
                "INSERT INTO doc_versions(doc_id, sha256, size, mtime, seen_at) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![doc.0, &state.sha256[..], size, mtime, now],
            )?;
            tx.commit()
        })
    }

    /// `code` is a fixed identifier such as `parse_failed`, never a message with document data.
    pub fn mark_error(&self, doc: DocId, code: &str) -> Result<(), StorageError> {
        self.write(|c| {
            c.execute(
                "UPDATE documents SET status = 'error', error_code = ?2 WHERE id = ?1",
                params![doc.0, code],
            )
        })?;
        Ok(())
    }
}
