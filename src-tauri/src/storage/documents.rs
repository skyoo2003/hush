//! Document and chunk rows for the ingest pipeline. All of its SQL lives here.

use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{params, OptionalExtension};
use serde::Serialize;

use super::db::{Db, DocId};
use super::error::StorageError;
use crate::ingest::chunker::Chunk;
use crate::ingest::hash::{FileStat, FileState};
use crate::rag::vector_store::{shape_ok, to_blob};

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX))
        .unwrap_or(0)
}

fn size_sql(size: u64) -> i64 {
    i64::try_from(size).unwrap_or(i64::MAX)
}

/// Inserts `chunks` for `doc` and their vectors inside the caller's transaction.
/// Callers check `shape_ok` first; `zip` would drop unmatched rows silently.
pub(crate) fn insert_chunks(
    tx: &rusqlite::Transaction,
    doc: DocId,
    chunks: &[Chunk],
    embeddings: &[Vec<f32>],
) -> rusqlite::Result<()> {
    let mut insert_vec =
        tx.prepare("INSERT INTO vec_chunks(chunk_id, embedding) VALUES (?1, ?2)")?;
    let mut insert = tx.prepare(
        "INSERT INTO chunks(doc_id, chunk_index, page, section_path, start_offset,
                            end_offset, bbox_json, text, sha256)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
    )?;
    for (ch, e) in chunks.iter().zip(embeddings) {
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
        insert_vec.execute(params![tx.last_insert_rowid(), to_blob(e)])?;
    }
    Ok(())
}

/// Stored in `folders.status`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "snake_case")]
pub enum FolderStatus {
    Active,
    /// The folder is missing, e.g. its drive is unplugged. Its documents are kept.
    Unavailable,
}

impl FolderStatus {
    fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Unavailable => "unavailable",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct FolderInfo {
    #[cfg_attr(test, ts(type = "number"))]
    pub id: i64,
    /// Canonical path, shown only to the user who chose it.
    pub path: String,
    pub label: String,
    pub status: FolderStatus,
    pub documents: u32,
    /// Documents whose last index attempt failed.
    pub failed: u32,
}

const FOLDER_SELECT: &str = "SELECT f.id, f.path, coalesce(f.label, f.path), f.status,
        (SELECT count(*) FROM documents d WHERE d.folder_id = f.id),
        (SELECT count(*) FROM documents d WHERE d.folder_id = f.id AND d.status = 'error')
    FROM folders f";

fn folder_row(r: &rusqlite::Row) -> rusqlite::Result<FolderInfo> {
    let status: String = r.get(3)?;
    Ok(FolderInfo {
        id: r.get(0)?,
        path: r.get(1)?,
        label: r.get(2)?,
        // Anything but 'unavailable' was written as active.
        status: if status == FolderStatus::Unavailable.as_str() {
            FolderStatus::Unavailable
        } else {
            FolderStatus::Active
        },
        documents: r.get(4)?,
        failed: r.get(5)?,
    })
}

/// Settings key for the model that produced the stored vectors.
const EMBEDDING_MODEL_KEY: &str = "embedding_model_id";

impl Db {
    /// Every document row in `folder_id` with its relative path and its last indexed stat.
    /// The stat is `None` until the document has been indexed once.
    pub fn folder_documents(
        &self,
        folder_id: i64,
    ) -> Result<Vec<(DocId, String, Option<FileStat>)>, StorageError> {
        self.read(|c| {
            c.prepare(
                "SELECT id, rel_path, size, mtime, sha256 IS NOT NULL FROM documents WHERE folder_id = ?1",
            )?
            .query_map([folder_id], |r| {
                let stat = match (r.get::<_, Option<i64>>(2)?, r.get::<_, Option<i64>>(3)?, r.get(4)?) {
                    (Some(size), Some(mtime_ns), true) => Some(FileStat {
                        // A negative size never matches a real stat, so the file is rehashed.
                        size: u64::try_from(size).unwrap_or(u64::MAX),
                        mtime_ns,
                    }),
                    _ => None,
                };
                Ok((DocId(r.get(0)?), r.get(1)?, stat))
            })?
            .collect()
        })
    }

    /// Every folder, in the order it was added.
    pub fn folders(&self) -> Result<Vec<FolderInfo>, StorageError> {
        self.read(|c| {
            c.prepare(&format!("{FOLDER_SELECT} ORDER BY f.id"))?
                .query_map([], folder_row)?
                .collect()
        })
    }

    pub fn folder(&self, id: i64) -> Result<Option<FolderInfo>, StorageError> {
        self.read(|c| {
            c.query_row(
                &format!("{FOLDER_SELECT} WHERE f.id = ?1"),
                [id],
                folder_row,
            )
            .optional()
        })
    }

    pub fn set_folder_status(&self, id: i64, status: FolderStatus) -> Result<(), StorageError> {
        self.write(|c| {
            c.execute(
                "UPDATE folders SET status = ?2 WHERE id = ?1",
                params![id, status.as_str()],
            )
        })?;
        Ok(())
    }

    /// One transaction: the folder's documents (their chunks, vectors, FTS rows, and
    /// versions go by cascade and trigger), then the folder.
    pub fn delete_folder(&self, id: i64) -> Result<(), StorageError> {
        self.write(|c| {
            let tx = c.transaction()?;
            tx.execute("DELETE FROM documents WHERE folder_id = ?1", [id])?;
            tx.execute("DELETE FROM folders WHERE id = ?1", [id])?;
            tx.commit()
        })
    }

    /// Records `model_id` as the model behind the stored vectors. When it differs from the
    /// stored one, every document's sha256 is cleared in the same transaction, so the next
    /// scan reindexes it with the new model. Returns whether the model changed.
    pub fn sync_embedding_model(&self, model_id: &str) -> Result<bool, StorageError> {
        if self.setting(EMBEDDING_MODEL_KEY)?.as_deref() == Some(model_id) {
            return Ok(false);
        }
        self.write(|c| {
            let tx = c.transaction()?;
            tx.execute("UPDATE documents SET sha256 = NULL", [])?;
            tx.execute(
                "INSERT INTO settings(key, value) VALUES (?1, ?2)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                [EMBEDDING_MODEL_KEY, model_id],
            )?;
            tx.commit()
        })?;
        Ok(true)
    }

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

    /// One transaction: the doc's chunks and their vectors are replaced, the doc is marked
    /// indexed, and one version row is added. A shape mismatch writes nothing.
    pub fn replace_chunks(
        &self,
        doc: DocId,
        state: &FileState,
        needs_ocr: bool,
        chunks: &[Chunk],
        embeddings: &[Vec<f32>],
    ) -> Result<(), StorageError> {
        if !shape_ok(chunks, embeddings) {
            return Err(StorageError::EmbeddingShape);
        }
        let (size, mtime, now) = (size_sql(state.stat.size), state.stat.mtime_ns, now());
        self.write(|c| {
            let tx = c.transaction()?;
            tx.execute("DELETE FROM chunks WHERE doc_id = ?1", [doc.0])?;
            insert_chunks(&tx, doc, chunks, embeddings)?;
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

    /// Registers `path` as an active folder, or reactivates it. Returns its id.
    pub fn upsert_folder(&self, path: &str, label: &str) -> Result<i64, StorageError> {
        self.write(|c| {
            c.query_row(
                "INSERT INTO folders(path, label, added_at, status) VALUES (?1, ?2, ?3, 'active')
                 ON CONFLICT(path) DO UPDATE SET status = 'active' RETURNING id",
                params![path, label, now()],
                |r| r.get(0),
            )
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
