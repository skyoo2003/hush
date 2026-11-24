// SPDX-License-Identifier: GPL-3.0-or-later
//! Chunk vectors (vec_chunks) and keyword search (chunks_fts) with one shared metadata filter.
//! Calls block; run them in `spawn_blocking` or a worker.

use std::collections::HashMap;
use std::path::Path;

use rusqlite::types::ToSql;

use super::error::RagError;
use crate::ingest::chunker::Chunk;
use crate::storage::db::{ChunkId, Db, DocId};
use crate::storage::documents::insert_chunks;

/// Embedding width; tied to `vec_chunks ... float[384]` in migration 0001.
pub const DIM: usize = 384;
/// sqlite-vec's largest accepted `k` (SQLITE_VEC_VEC0_K_MAX).
const MAX_K: usize = 4096;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "lowercase")]
pub enum KindFilter {
    Text,
    Pdf,
    #[serde(rename = "doc")]
    Docx,
    Image,
}

impl KindFilter {
    /// Stored `documents.ext` values for this kind; matches the parsers' `supports`.
    pub fn exts(self) -> &'static [&'static str] {
        match self {
            Self::Text => &["txt", "md", "markdown"],
            Self::Pdf => &["pdf"],
            Self::Docx => &["docx"],
            Self::Image => &["png", "jpg", "jpeg"],
        }
    }
}

/// Metadata constraints shared by `knn` and `keyword`. The query-operator parser and the
/// filter-chip UI convert into this. An empty vec or `None` means no constraint; fields are
/// ANDed. `exts` are lowercase without a dot, as stored in `documents.ext`. `modified_*`
/// compare `documents.mtime` (ns): after is inclusive, before is exclusive.
#[derive(Debug, Default, Clone)]
pub struct SearchFilter {
    pub folders: Vec<i64>,
    pub exts: Vec<String>,
    pub kinds: Vec<KindFilter>,
    pub modified_after: Option<i64>,
    pub modified_before: Option<i64>,
    /// Phrases a chunk must contain, each matched as an FTS5 phrase (same tokens, in order).
    pub phrases: Vec<String>,
}

/// Shared WHERE fragment over `documents d`. Values are bound, never spliced in.
macro_rules! filter_sql {
    () => {
        "(:folders IS NULL OR d.folder_id IN (SELECT value FROM json_each(:folders)))
         AND (:exts IS NULL OR d.ext IN (SELECT value FROM json_each(:exts)))
         AND (:kind_exts IS NULL OR d.ext IN (SELECT value FROM json_each(:kind_exts)))
         AND (:after IS NULL OR d.mtime >= :after)
         AND (:before IS NULL OR d.mtime < :before)
         AND (:phrases IS NULL OR c.id IN (SELECT rowid FROM chunks_fts WHERE chunks_fts MATCH :phrases))"
    };
}

const KNN_SQL: &str = concat!(
    "WITH knn AS (SELECT chunk_id, distance FROM vec_chunks WHERE embedding MATCH :q AND k = :fetch)
     SELECT knn.chunk_id, knn.distance FROM knn
     JOIN chunks c ON c.id = knn.chunk_id
     JOIN documents d ON d.id = c.doc_id
     WHERE ",
    filter_sql!(),
    " ORDER BY knn.distance LIMIT :k"
);

const KEYWORD_SQL: &str = concat!(
    "SELECT chunks_fts.rowid, bm25(chunks_fts) AS s FROM chunks_fts
     JOIN chunks c ON c.id = chunks_fts.rowid
     JOIN documents d ON d.id = c.doc_id
     WHERE chunks_fts MATCH :q AND ",
    filter_sql!(),
    " ORDER BY s LIMIT :k"
);

/// What a search result shows about one chunk. Carries no absolute path.
#[derive(Debug, Clone)]
pub struct ChunkDetail {
    pub doc_id: DocId,
    pub text: String,
    pub page: Option<u32>,
    pub section_path: Option<String>,
    pub rel_path: String,
    /// The folder's label, else the last component of its path.
    pub folder_label: String,
    /// `documents.mtime` in ns.
    pub mtime: Option<i64>,
}

struct FilterParams {
    folders: Option<String>,
    exts: Option<String>,
    kind_exts: Option<String>,
    after: Option<i64>,
    before: Option<i64>,
    phrases: Option<String>,
}

fn json_list<T: serde::Serialize>(v: &[T]) -> rusqlite::Result<Option<String>> {
    if v.is_empty() {
        return Ok(None);
    }
    serde_json::to_string(v)
        .map(Some)
        .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))
}

impl FilterParams {
    fn new(f: &SearchFilter) -> rusqlite::Result<Self> {
        let kind_exts: Vec<&str> = f.kinds.iter().flat_map(|k| k.exts()).copied().collect();
        Ok(Self {
            folders: json_list(&f.folders)?,
            exts: json_list(&f.exts)?,
            kind_exts: json_list(&kind_exts)?,
            after: f.modified_after,
            before: f.modified_before,
            phrases: phrase_query(&f.phrases),
        })
    }

    fn named(&self) -> Vec<(&'static str, &dyn ToSql)> {
        vec![
            (":folders", &self.folders),
            (":exts", &self.exts),
            (":kind_exts", &self.kind_exts),
            (":after", &self.after),
            (":before", &self.before),
            (":phrases", &self.phrases),
        ]
    }
}

/// One vector of width `DIM` per chunk.
pub(crate) fn shape_ok(chunks: &[Chunk], embeddings: &[Vec<f32>]) -> bool {
    embeddings.len() == chunks.len() && embeddings.iter().all(|e| e.len() == DIM)
}

pub(crate) fn to_blob(v: &[f32]) -> Vec<u8> {
    v.iter().flat_map(|x| x.to_le_bytes()).collect()
}

fn sql_int(n: usize) -> i64 {
    i64::try_from(n).unwrap_or(i64::MAX)
}

/// Each whitespace token as a quoted FTS5 string, so operators and syntax match literally.
/// Tokens are ORed so bm25 ranks partial matches (eval: Hybrid Recall@5 0.88 -> 0.92). Tokens without an alphanumeric char are dropped.
fn fts_query(query: &str) -> String {
    query
        .split_whitespace()
        .filter(|t| t.chars().any(char::is_alphanumeric))
        .map(|t| format!("\"{}\"", t.replace('"', "\"\"")))
        .collect::<Vec<_>>()
        .join(" OR ")
}

/// Each phrase as a quoted FTS5 string, ANDed. Phrases without an alphanumeric char are dropped.
fn phrase_query(phrases: &[String]) -> Option<String> {
    let q = phrases
        .iter()
        .filter(|p| p.chars().any(char::is_alphanumeric))
        .map(|p| format!("\"{}\"", p.replace('"', "\"\"")))
        .collect::<Vec<_>>()
        .join(" AND ");
    (!q.is_empty()).then_some(q)
}

impl Db {
    /// Replaces the doc's chunks and their vectors in one transaction; any error rolls it back.
    pub fn upsert_document_chunks(
        &self,
        doc: DocId,
        chunks: &[Chunk],
        embeddings: &[Vec<f32>],
    ) -> Result<(), RagError> {
        if !shape_ok(chunks, embeddings) {
            return Err(RagError::EmbeddingShape);
        }
        self.write(|c| {
            let tx = c.transaction()?;
            // The chunks_ad trigger clears the matching fts and vec rows.
            tx.execute("DELETE FROM chunks WHERE doc_id = ?1", [doc.0])?;
            insert_chunks(&tx, doc, chunks, embeddings)?;
            tx.commit()
        })?;
        Ok(())
    }

    /// Up to `k` nearest chunks by L2 distance (lower = closer) that pass `filter`.
    pub fn knn(
        &self,
        query: &[f32],
        k: usize,
        filter: &SearchFilter,
    ) -> Result<Vec<(ChunkId, f32)>, RagError> {
        if query.len() != DIM {
            return Err(RagError::EmbeddingShape);
        }
        if k == 0 {
            return Ok(Vec::new());
        }
        let q = to_blob(query);
        let k_sql = sql_int(k);
        // shortcut: the filter runs after a raw KNN of at most MAX_K rows, so a filter that keeps
        // fewer than k of the MAX_K nearest returns short; upgrade to vec0 partition/metadata
        // columns (new migration) if narrow filters on large libraries come back short.
        let mut fetch = k.saturating_mul(4).min(MAX_K);
        let mut total: Option<usize> = None;
        Ok(self.read(|c| {
            let fp = FilterParams::new(filter)?;
            let mut stmt = c.prepare_cached(KNN_SQL)?;
            loop {
                let fetch_sql = sql_int(fetch);
                let mut p = fp.named();
                p.extend([
                    (":q", &q as &dyn ToSql),
                    (":fetch", &fetch_sql),
                    (":k", &k_sql),
                ]);
                let rows = stmt
                    .query_map(p.as_slice(), |r| {
                        Ok((ChunkId(r.get(0)?), r.get::<_, f64>(1)? as f32))
                    })?
                    .collect::<rusqlite::Result<Vec<_>>>()?;
                if rows.len() >= k || fetch >= MAX_K {
                    return Ok(rows);
                }
                // chunks bounds vec_chunks (one vector per chunk), and its count is cheap.
                let total = match total {
                    Some(t) => t,
                    None => {
                        let n: i64 =
                            c.query_row("SELECT count(*) FROM chunks", [], |r| r.get(0))?;
                        *total.insert(usize::try_from(n).unwrap_or(usize::MAX))
                    }
                };
                if fetch >= total {
                    return Ok(rows);
                }
                fetch = fetch.saturating_mul(4).min(MAX_K);
            }
        })?)
    }

    /// Up to `k` chunks matching any whitespace token of `query` literally, by bm25
    /// (lower = better). FTS syntax in `query` is never interpreted.
    pub fn keyword(
        &self,
        query: &str,
        k: usize,
        filter: &SearchFilter,
    ) -> Result<Vec<(ChunkId, f64)>, RagError> {
        let q = fts_query(query);
        if q.is_empty() || k == 0 {
            return Ok(Vec::new());
        }
        let k_sql = sql_int(k);
        Ok(self.read(|c| {
            let fp = FilterParams::new(filter)?;
            let mut p = fp.named();
            p.extend([(":q", &q as &dyn ToSql), (":k", &k_sql)]);
            let mut stmt = c.prepare_cached(KEYWORD_SQL)?;
            let rows = stmt
                .query_map(p.as_slice(), |r| Ok((ChunkId(r.get(0)?), r.get(1)?)))?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            Ok(rows)
        })?)
    }

    /// Details for `ids` in one query. Ids with no chunk row are absent from the map.
    pub fn chunk_details(
        &self,
        ids: &[ChunkId],
    ) -> Result<HashMap<ChunkId, ChunkDetail>, RagError> {
        let raw: Vec<i64> = ids.iter().map(|c| c.0).collect();
        let rows = self.read(|c| {
            let ids = json_list(&raw)?;
            c.prepare_cached(
                "SELECT c.id, c.doc_id, c.text, c.page, c.section_path, d.rel_path, f.label,
                        f.path, d.mtime
                 FROM chunks c
                 JOIN documents d ON d.id = c.doc_id
                 JOIN folders f ON f.id = d.folder_id
                 WHERE c.id IN (SELECT value FROM json_each(?1))",
            )?
            .query_map([ids], |r| {
                let label: Option<String> = r.get(6)?;
                let path: String = r.get(7)?;
                let folder_label = label.unwrap_or_else(|| {
                    Path::new(&path)
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_default()
                });
                Ok((
                    ChunkId(r.get(0)?),
                    ChunkDetail {
                        doc_id: DocId(r.get(1)?),
                        text: r.get(2)?,
                        page: r.get(3)?,
                        section_path: r.get(4)?,
                        rel_path: r.get(5)?,
                        folder_label,
                        mtime: r.get(8)?,
                    },
                ))
            })?
            .collect::<rusqlite::Result<HashMap<_, _>>>()
        })?;
        Ok(rows)
    }

    /// Plain mean of the doc's chunk vectors; `None` when it has none.
    pub fn doc_vector(&self, doc: DocId) -> Result<Option<Vec<f32>>, RagError> {
        let blobs: Vec<Vec<u8>> = self.read(|c| {
            c.prepare(
                "SELECT v.embedding FROM chunks c JOIN vec_chunks v ON v.chunk_id = c.id
                 WHERE c.doc_id = ?1",
            )?
            .query_map([doc.0], |r| r.get(0))?
            .collect()
        })?;
        if blobs.is_empty() {
            return Ok(None);
        }
        let mut sum = vec![0f32; DIM];
        for b in &blobs {
            if b.len() != DIM * 4 {
                return Err(RagError::EmbeddingShape);
            }
            for (s, x) in sum.iter_mut().zip(b.as_chunks::<4>().0) {
                *s += f32::from_le_bytes(*x);
            }
        }
        let n = blobs.len() as f32;
        Ok(Some(sum.into_iter().map(|s| s / n).collect()))
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use rusqlite::params;

    use super::*;
    use crate::rag::embedder::{Embedder, FakeEmbedder};
    use crate::storage::db::tests::temp_db;

    fn folder(db: &Db, path: &str) -> i64 {
        db.write(|c| {
            c.execute(
                "INSERT INTO folders(path, added_at, status) VALUES (?1, 0, 'active')",
                [path],
            )?;
            Ok(c.last_insert_rowid())
        })
        .unwrap()
    }

    fn doc(db: &Db, folder: i64, rel: &str, ext: &str, mtime: i64) -> DocId {
        db.write(|c| {
            c.execute(
                "INSERT INTO documents(folder_id, rel_path, ext, mtime, status)
                 VALUES (?1, ?2, ?3, ?4, 'indexed')",
                params![folder, rel, ext, mtime],
            )?;
            Ok(DocId(c.last_insert_rowid()))
        })
        .unwrap()
    }

    fn chunks(doc: DocId, texts: &[&str]) -> Vec<Chunk> {
        texts
            .iter()
            .enumerate()
            .map(|(i, t)| Chunk {
                doc_id: doc,
                chunk_index: i as u32,
                page: None,
                section_path: None,
                start_offset: 0,
                end_offset: t.chars().count() as u32,
                bbox: None,
                text: (*t).to_string(),
                sha256: [0; 32],
            })
            .collect()
    }

    fn embed(texts: &[&str]) -> Vec<Vec<f32>> {
        FakeEmbedder.embed_passages(texts).unwrap()
    }

    fn upsert(db: &Db, d: DocId, texts: &[&str]) {
        db.upsert_document_chunks(d, &chunks(d, texts), &embed(texts))
            .unwrap();
    }

    fn count(db: &Db, sql: &str) -> i64 {
        db.read(|c| c.query_row(sql, [], |r| r.get(0))).unwrap()
    }

    /// (chunks, vec_chunks, chunks_fts) row counts.
    fn counts(db: &Db) -> (i64, i64, i64) {
        (
            count(db, "SELECT count(*) FROM chunks"),
            count(db, "SELECT count(*) FROM vec_chunks"),
            count(db, "SELECT count(*) FROM chunks_fts"),
        )
    }

    fn fts_hits(db: &Db, q: &str) -> i64 {
        db.read(|c| {
            c.query_row(
                "SELECT count(*) FROM chunks_fts WHERE chunks_fts MATCH ?1",
                [q],
                |r| r.get(0),
            )
        })
        .unwrap()
    }

    fn doc_of(db: &Db, id: ChunkId) -> DocId {
        DocId(
            db.read(|c| {
                c.query_row("SELECT doc_id FROM chunks WHERE id = ?1", [id.0], |r| {
                    r.get(0)
                })
            })
            .unwrap(),
        )
    }

    fn query(text: &str) -> Vec<f32> {
        FakeEmbedder.embed_query(text).unwrap()
    }

    #[test]
    fn reupsert_leaves_exactly_the_new_chunks() {
        let (_dir, db) = temp_db();
        let f = folder(&db, "/f");
        let d = doc(&db, f, "a.txt", "txt", 1);
        let other = doc(&db, f, "b.txt", "txt", 1);
        upsert(&db, other, &["otherdoc"]);

        upsert(&db, d, &["alpha one", "alpha two", "alpha three"]);
        upsert(&db, d, &["alpha one", "alpha two", "alpha three"]);
        assert_eq!(counts(&db), (4, 4, 4));
        assert_eq!(fts_hits(&db, "alpha"), 3);

        upsert(&db, d, &["bravo one", "bravo two"]);
        assert_eq!(counts(&db), (3, 3, 3));
        assert_eq!(fts_hits(&db, "alpha"), 0);
        assert_eq!(fts_hits(&db, "bravo"), 2);
        assert_eq!(fts_hits(&db, "otherdoc"), 1);
        assert_eq!(
            count(
                &db,
                "SELECT count(*) FROM vec_chunks v JOIN chunks c ON c.id = v.chunk_id"
            ),
            3
        );
        db.write(|c| {
            c.execute(
                "INSERT INTO chunks_fts(chunks_fts, rank) VALUES ('integrity-check', 1)",
                [],
            )
        })
        .unwrap();
    }

    #[test]
    fn delete_document_and_empty_upsert_clear_all_three_tables() {
        let (_dir, db) = temp_db();
        let f = folder(&db, "/f");
        let d = doc(&db, f, "a.txt", "txt", 1);
        upsert(&db, d, &["alpha one", "alpha two"]);
        db.upsert_document_chunks(d, &[], &[]).unwrap();
        assert_eq!(counts(&db), (0, 0, 0));
        assert_eq!(fts_hits(&db, "alpha"), 0);

        upsert(&db, d, &["alpha one", "alpha two"]);
        db.delete_document(d).unwrap();
        assert_eq!(counts(&db), (0, 0, 0));
        assert_eq!(fts_hits(&db, "alpha"), 0);
    }

    #[test]
    fn failed_upsert_rolls_back() {
        let (_dir, db) = temp_db();
        let f = folder(&db, "/f");
        let d = doc(&db, f, "a.txt", "txt", 1);
        upsert(&db, d, &["alpha one", "alpha two"]);

        let mut dup = chunks(d, &["bravo one", "bravo two"]);
        dup[1].chunk_index = 0;
        assert!(matches!(
            db.upsert_document_chunks(d, &dup, &embed(&["bravo one", "bravo two"])),
            Err(RagError::Storage(_))
        ));
        let wrong_count = db.upsert_document_chunks(d, &chunks(d, &["x"]), &[]);
        assert!(matches!(wrong_count, Err(RagError::EmbeddingShape)));
        let wrong_dim = db.upsert_document_chunks(d, &chunks(d, &["x"]), &[vec![0.0; 3]]);
        assert!(matches!(wrong_dim, Err(RagError::EmbeddingShape)));

        assert_eq!(counts(&db), (2, 2, 2));
        assert_eq!(fts_hits(&db, "alpha"), 2);
        assert_eq!(fts_hits(&db, "bravo"), 0);
        assert_eq!(
            db.knn(&query("alpha one"), 5, &SearchFilter::default())
                .unwrap()
                .len(),
            2
        );
    }

    #[test]
    fn keyword_treats_fts_syntax_literally() {
        let (_dir, db) = temp_db();
        let f = folder(&db, "/f");
        let d = doc(&db, f, "a.txt", "txt", 1);
        upsert(&db, d, &["we met near the river", "word list here"]);
        let all = SearchFilter::default();
        for q in [
            "\"",
            "*",
            "NEAR",
            "-",
            ":",
            "NEAR(a b)",
            "foo:bar",
            "\"unbalanced",
            "a - b",
            "***",
        ] {
            assert!(db.keyword(q, 5, &all).is_ok(), "query {q:?} failed");
        }
        assert!(db.keyword("***", 5, &all).unwrap().is_empty());
        let near = db.keyword("NEAR", 5, &all).unwrap();
        assert_eq!(near.len(), 1);
        let near_text: String = db
            .read(|c| {
                c.query_row(
                    "SELECT text FROM chunks WHERE id = ?1",
                    [near[0].0 .0],
                    |r| r.get(0),
                )
            })
            .unwrap();
        assert!(near_text.contains("near"));
        // `-word` is a literal token, not an exclusion.
        assert_eq!(db.keyword("here -word", 5, &all).unwrap().len(), 1);
        assert_eq!(db.keyword("met river", 5, &all).unwrap().len(), 1);
        assert_eq!(db.keyword("met list", 5, &all).unwrap().len(), 2);
        assert!(db.keyword("absent zzz", 5, &all).unwrap().is_empty());
        assert!(db.keyword("river", 0, &all).unwrap().is_empty());
    }

    #[test]
    fn filters_apply_to_knn_and_keyword() {
        let (_dir, db) = temp_db();
        let (f1, f2) = (folder(&db, "/f1"), folder(&db, "/f2"));
        let d1 = doc(&db, f1, "1.txt", "txt", 100);
        let d2 = doc(&db, f1, "2.pdf", "pdf", 200);
        let d3 = doc(&db, f2, "3.md", "md", 300);
        let d4 = doc(&db, f2, "4.txt", "txt", 400);
        for (d, w) in [(d1, "one"), (d2, "two"), (d3, "three"), (d4, "four")] {
            upsert(&db, d, &[&format!("shared {w}")]);
        }
        let q = query("shared");
        let docs = |f: SearchFilter| -> BTreeSet<i64> {
            let by_knn: BTreeSet<i64> = db
                .knn(&q, 10, &f)
                .unwrap()
                .into_iter()
                .map(|(c, _)| doc_of(&db, c).0)
                .collect();
            let by_kw: BTreeSet<i64> = db
                .keyword("shared", 10, &f)
                .unwrap()
                .into_iter()
                .map(|(c, _)| doc_of(&db, c).0)
                .collect();
            assert_eq!(by_knn, by_kw);
            by_knn
        };
        let set = |ds: &[DocId]| ds.iter().map(|d| d.0).collect::<BTreeSet<_>>();
        let exts = |e: &[&str]| e.iter().map(|s| s.to_string()).collect::<Vec<_>>();

        assert_eq!(docs(SearchFilter::default()), set(&[d1, d2, d3, d4]));
        assert_eq!(
            docs(SearchFilter {
                folders: vec![f1],
                ..Default::default()
            }),
            set(&[d1, d2])
        );
        assert_eq!(
            docs(SearchFilter {
                exts: exts(&["txt"]),
                ..Default::default()
            }),
            set(&[d1, d4])
        );
        assert_eq!(
            docs(SearchFilter {
                kinds: vec![KindFilter::Text],
                ..Default::default()
            }),
            set(&[d1, d3, d4])
        );
        assert_eq!(
            docs(SearchFilter {
                exts: exts(&["md"]),
                kinds: vec![KindFilter::Pdf],
                ..Default::default()
            }),
            set(&[])
        );
        assert_eq!(
            docs(SearchFilter {
                exts: exts(&["txt", "pdf"]),
                kinds: vec![KindFilter::Text],
                ..Default::default()
            }),
            set(&[d1, d4])
        );
        assert_eq!(
            docs(SearchFilter {
                modified_after: Some(200),
                modified_before: Some(400),
                ..Default::default()
            }),
            set(&[d2, d3])
        );
        assert_eq!(
            docs(SearchFilter {
                folders: vec![f2],
                exts: exts(&["txt", "md"]),
                kinds: vec![KindFilter::Text],
                modified_after: Some(300),
                modified_before: Some(400),
                phrases: vec!["Shared  THREE".into()],
            }),
            set(&[d3])
        );
        let phrases = |p: &[&str]| SearchFilter {
            phrases: exts(p),
            ..Default::default()
        };
        assert_eq!(docs(phrases(&["shared two"])), set(&[d2]));
        assert_eq!(docs(phrases(&["two shared"])), set(&[]));
        assert_eq!(docs(phrases(&["shared", "four"])), set(&[d4]));
        // FTS syntax inside a phrase is literal, and a phrase of only symbols adds no constraint.
        assert_eq!(docs(phrases(&["shared\" OR \"one"])), set(&[]));
        assert_eq!(docs(phrases(&["*"])), set(&[d1, d2, d3, d4]));
    }

    #[test]
    fn knn_widens_until_the_filter_has_k_rows() {
        let (_dir, db) = temp_db();
        let (f1, f2) = (folder(&db, "/f1"), folder(&db, "/f2"));
        let near = doc(&db, f1, "near.txt", "txt", 1);
        let far = doc(&db, f2, "far.txt", "txt", 1);
        let texts: Vec<String> = (0..30).map(|i| format!("alpha beta n{i}")).collect();
        let refs: Vec<&str> = texts.iter().map(String::as_str).collect();
        upsert(&db, near, &refs);
        upsert(&db, far, &["zulu yankee xray"]);

        let q = query("alpha beta");
        let only_far = SearchFilter {
            folders: vec![f2],
            ..Default::default()
        };
        let hits = db.knn(&q, 1, &only_far).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(doc_of(&db, hits[0].0), far);
        // The far chunk is not among the first fetch of 4.
        let unfiltered = db.knn(&q, 4, &SearchFilter::default()).unwrap();
        assert!(unfiltered.iter().all(|(c, _)| doc_of(&db, *c) == near));
        assert!(db.knn(&q, 0, &only_far).unwrap().is_empty());
        assert!(matches!(
            db.knn(&[0.0; 3], 1, &only_far),
            Err(RagError::EmbeddingShape)
        ));
    }

    #[test]
    fn sqlite_vec_max_k_is_4096() {
        let (_dir, db) = temp_db();
        let f = folder(&db, "/f");
        upsert(&db, doc(&db, f, "a.txt", "txt", 1), &["alpha"]);
        let raw = |k: usize| {
            db.read(|c| {
                c.query_row(
                    "SELECT count(*) FROM vec_chunks WHERE embedding MATCH ?1 AND k = ?2",
                    params![to_blob(&query("alpha")), sql_int(k)],
                    |r| r.get::<_, i64>(0),
                )
            })
        };
        assert_eq!(raw(MAX_K).unwrap(), 1);
        assert!(raw(MAX_K + 1).is_err());
    }

    #[test]
    fn doc_vector_ranks_its_own_doc_first() {
        let (_dir, db) = temp_db();
        let f = folder(&db, "/f");
        let docs = [
            ("a.txt", ["apple banana cherry", "banana smoothie recipe"]),
            ("b.txt", ["engine piston crankshaft", "piston ring wear"]),
            (
                "c.txt",
                ["violin cello orchestra", "orchestra concert hall"],
            ),
        ];
        let ids: Vec<DocId> = docs
            .iter()
            .map(|(rel, texts)| {
                let d = doc(&db, f, rel, "txt", 1);
                upsert(&db, d, texts);
                d
            })
            .collect();
        for d in ids {
            let v = db.doc_vector(d).unwrap().unwrap();
            let top = db.knn(&v, 1, &SearchFilter::default()).unwrap();
            assert_eq!(doc_of(&db, top[0].0), d);
        }
        assert!(db.doc_vector(DocId(9999)).unwrap().is_none());
    }
}
