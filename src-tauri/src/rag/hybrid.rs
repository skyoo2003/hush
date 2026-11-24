// SPDX-License-Identifier: GPL-3.0-or-later
//! Query -> ranked, cited hits: vector KNN and keyword search fused with RRF, then rerank,
//! per-document cap, and snippets. Calls block; run them in `spawn_blocking` or a worker.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use serde::Serialize;
use unicode_segmentation::UnicodeSegmentation;

use super::embedder::Embedder;
use super::error::RagError;
use super::query_parser::QueryFilter;
use super::vector_store::{ChunkDetail, SearchFilter};
use crate::storage::db::{ChunkId, Db};

/// Candidates fetched from each retriever.
const POOL: usize = 50;
const RRF_K: f32 = 60.0;
const PER_DOC: usize = 3;
const SNIPPET_CHARS: usize = 300;
const STOPWORDS: &[&str] = &[
    "a", "an", "and", "are", "as", "at", "be", "by", "can", "do", "does", "for", "from", "has",
    "have", "how", "i", "if", "in", "is", "it", "its", "my", "of", "on", "or", "that", "the",
    "their", "this", "to", "was", "were", "what", "when", "where", "which", "who", "why", "will",
    "with", "you",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SearchMode {
    /// KNN only; exists for eval comparison.
    VectorOnly,
    #[default]
    Hybrid,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "snake_case")]
pub enum MatchKind {
    Vector,
    Keyword,
    Both,
}

/// One cited result. Paths are relative to the folder; no absolute path is ever included.
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct SearchHit {
    #[cfg_attr(test, ts(type = "number"))]
    pub chunk_id: i64,
    #[cfg_attr(test, ts(type = "number"))]
    pub doc_id: i64,
    pub file_name: String,
    pub rel_path: String,
    pub folder_label: String,
    pub page: Option<u32>,
    pub section_path: Option<String>,
    pub snippet: String,
    /// `[start, end)` of query-term matches in `snippet`, in UTF-16 code units.
    pub highlights: Vec<(usize, usize)>,
    /// RRF score in Hybrid mode (rank-only, not comparable across queries); cosine
    /// similarity in VectorOnly mode.
    pub score: f32,
    pub match_kind: MatchKind,
    /// File mtime in milliseconds since the Unix epoch.
    #[cfg_attr(test, ts(type = "number | null"))]
    pub modified: Option<i64>,
}

#[derive(Debug, Clone, Default, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct SearchResults {
    pub hits: Vec<SearchHit>,
    /// Best cosine similarity among the KNN candidates; 0.0 when there are none.
    pub top_score: f32,
    /// Cosine similarities of all KNN candidates, descending. The relevance signal for an
    /// insufficient-evidence check; RRF scores cannot serve it.
    pub scores: Vec<f32>,
    /// Operators parsed from the query, for the read-only chips. Set by the `search` command.
    pub filters: Vec<QueryFilter>,
}

pub struct Candidate {
    pub chunk_id: ChunkId,
    pub score: f32,
    pub match_kind: MatchKind,
    pub detail: ChunkDetail,
}

pub trait Reranker: Send + Sync {
    /// Reorders, rescores, or drops `candidates` in place; best first.
    fn rerank(&self, query: &str, candidates: &mut Vec<Candidate>) -> Result<(), RagError>;
}

/// Keeps the fused order.
pub struct NoopReranker;

impl Reranker for NoopReranker {
    fn rerank(&self, _: &str, _: &mut Vec<Candidate>) -> Result<(), RagError> {
        Ok(())
    }
}

pub struct SearchService {
    embedder: Arc<dyn Embedder>,
    reranker: Box<dyn Reranker>,
}

impl SearchService {
    pub fn new(embedder: Arc<dyn Embedder>, reranker: Box<dyn Reranker>) -> Self {
        Self { embedder, reranker }
    }

    pub fn search(
        &self,
        db: &Db,
        query: &str,
        mode: SearchMode,
        filter: &SearchFilter,
        limit: usize,
    ) -> Result<SearchResults, RagError> {
        if query.trim().is_empty() || limit == 0 {
            return Ok(SearchResults::default());
        }
        let terms = query_terms(query);
        let knn = db.knn(&self.embedder.embed_query(query)?, POOL, filter)?;
        let mut scores: Vec<f32> = knn.iter().map(|&(_, d)| cosine(d)).collect();
        scores.sort_by(|a, b| b.total_cmp(a));
        let ranked = match mode {
            SearchMode::VectorOnly => knn
                .iter()
                .map(|&(id, d)| (id, cosine(d), MatchKind::Vector))
                .collect(),
            SearchMode::Hybrid => {
                // Stopwords dropped: with ORed tokens, "of" alone would match nearly every chunk.
                let keyword = db.keyword(&terms.join(" "), POOL, filter)?;
                rrf(
                    &knn.iter().map(|c| c.0).collect::<Vec<_>>(),
                    &keyword.iter().map(|c| c.0).collect::<Vec<_>>(),
                )
            }
        };
        let ids: Vec<ChunkId> = ranked.iter().map(|r| r.0).collect();
        let mut details = db.chunk_details(&ids)?;
        let mut candidates: Vec<Candidate> = ranked
            .into_iter()
            .filter_map(|(chunk_id, score, match_kind)| {
                Some(Candidate {
                    chunk_id,
                    score,
                    match_kind,
                    detail: details.remove(&chunk_id)?,
                })
            })
            .collect();
        self.reranker.rerank(query, &mut candidates)?;

        let mut per_doc: HashMap<i64, usize> = HashMap::new();
        let hits = candidates
            .into_iter()
            .filter(|c| {
                let n = per_doc.entry(c.detail.doc_id.0).or_insert(0);
                *n += 1;
                *n <= PER_DOC
            })
            .take(limit)
            .map(|c| hit(c, &terms))
            .collect();
        Ok(SearchResults {
            hits,
            top_score: scores.first().copied().unwrap_or(0.0),
            scores,
            filters: Vec::new(),
        })
    }
}

/// Vectors are L2-normalized, so |a - b|² = 2 - 2cos.
fn cosine(l2: f32) -> f32 {
    1.0 - l2 * l2 / 2.0
}

/// Reciprocal rank fusion: score = sum of 1 / (60 + rank), rank 1-based. Best first; ties by id.
fn rrf(vector: &[ChunkId], keyword: &[ChunkId]) -> Vec<(ChunkId, f32, MatchKind)> {
    let mut fused: HashMap<ChunkId, (f32, bool, bool)> = HashMap::new();
    for (is_vector, list) in [(true, vector), (false, keyword)] {
        for (rank, id) in list.iter().enumerate() {
            let e = fused.entry(*id).or_default();
            e.0 += 1.0 / (RRF_K + rank as f32 + 1.0);
            if is_vector {
                e.1 = true;
            } else {
                e.2 = true;
            }
        }
    }
    let mut out: Vec<_> = fused
        .into_iter()
        .map(|(id, (score, v, k))| {
            let kind = match (v, k) {
                (true, true) => MatchKind::Both,
                (true, false) => MatchKind::Vector,
                _ => MatchKind::Keyword,
            };
            (id, score, kind)
        })
        .collect();
    out.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0 .0.cmp(&b.0 .0)));
    out
}

fn hit(c: Candidate, terms: &[String]) -> SearchHit {
    let d = c.detail;
    let (snippet, highlights) = snippet(&d.text, terms);
    SearchHit {
        chunk_id: c.chunk_id.0,
        doc_id: d.doc_id.0,
        file_name: file_name(&d.rel_path).to_string(),
        rel_path: d.rel_path,
        folder_label: d.folder_label,
        page: d.page,
        section_path: d.section_path,
        snippet,
        highlights,
        score: c.score,
        match_kind: c.match_kind,
        modified: d.mtime.map(|ns| ns / 1_000_000),
    }
}

fn file_name(rel_path: &str) -> &str {
    rel_path.rsplit('/').next().unwrap_or_default()
}

/// `[file name, p.X] "excerpt"`, with the section path in place of a missing page. The excerpt is
/// the chunk's start, whitespace collapsed, cut at `SNIPPET_CHARS` chars.
pub fn citation(d: &ChunkDetail) -> String {
    let name = file_name(&d.rel_path);
    let head = match (d.page, &d.section_path) {
        (Some(p), _) => format!("{name}, p.{p}"),
        (None, Some(s)) => format!("{name}, {s}"),
        (None, None) => name.to_string(),
    };
    let text = d.text.split_whitespace().collect::<Vec<_>>().join(" ");
    let excerpt = match text.char_indices().nth(SNIPPET_CHARS) {
        Some((cut, _)) => format!("{}…", text[..cut].trim_end()),
        None => text,
    };
    format!("[{head}] \"{excerpt}\"")
}

/// Byte ranges of maximal alphanumeric runs.
fn words(s: &str) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut start = None;
    for (i, ch) in s.char_indices() {
        match (ch.is_alphanumeric(), start) {
            (true, None) => start = Some(i),
            (false, Some(st)) => {
                out.push((st, i));
                start = None;
            }
            _ => {}
        }
    }
    if let Some(st) = start {
        out.push((st, s.len()));
    }
    out
}

/// Distinct lowercase query words, stopwords removed, in query order.
fn query_terms(query: &str) -> Vec<String> {
    let mut seen = HashSet::new();
    words(query)
        .into_iter()
        .map(|(a, b)| query[a..b].to_lowercase())
        .filter(|w| !STOPWORDS.contains(&w.as_str()) && seen.insert(w.clone()))
        .collect()
}

fn is_term(word: &str, terms: &[String]) -> bool {
    terms.contains(&word.to_lowercase())
}

/// At most `SNIPPET_CHARS` chars centered on the sentence with the most distinct query terms
/// (earliest on ties), plus UTF-16 ranges of the term matches inside it.
fn snippet(text: &str, terms: &[String]) -> (String, Vec<(usize, usize)>) {
    let mut best: Option<(usize, std::ops::Range<usize>)> = None;
    let mut pos = 0;
    for s in text.split_sentence_bounds() {
        let range = pos..pos + s.len();
        pos += s.len();
        let found: HashSet<String> = words(s)
            .into_iter()
            .map(|(a, b)| s[a..b].to_lowercase())
            .filter(|w| terms.contains(w))
            .collect();
        if best.as_ref().is_none_or(|(n, _)| found.len() > *n) {
            best = Some((found.len(), range));
        }
    }
    let sentence = best.map_or(0..0, |b| b.1);
    // Char index -> byte offset, with the end as the last entry.
    let bounds: Vec<usize> = text
        .char_indices()
        .map(|(i, _)| i)
        .chain([text.len()])
        .collect();
    let char_at = |byte: usize| bounds.partition_point(|&b| b < byte);
    let total = bounds.len() - 1;
    let (mut fs, mut fe) = (char_at(sentence.start), char_at(sentence.end));
    if fe - fs > SNIPPET_CHARS {
        // Too long to show whole: center on its first term match instead.
        let s = &text[sentence.clone()];
        if let Some((a, b)) = words(s)
            .into_iter()
            .find(|&(a, b)| is_term(&s[a..b], terms))
        {
            (fs, fe) = (char_at(sentence.start + a), char_at(sentence.start + b));
        } else {
            fe = fs;
        }
    }
    let mid = (fs + fe) / 2;
    let start = mid
        .saturating_sub(SNIPPET_CHARS / 2)
        .min(total.saturating_sub(SNIPPET_CHARS));
    let end = (start + SNIPPET_CHARS).min(total);
    let snippet = text[bounds[start]..bounds[end]].trim().to_string();
    let utf16 = |byte: usize| snippet[..byte].encode_utf16().count();
    let highlights = words(&snippet)
        .into_iter()
        .filter(|&(a, b)| is_term(&snippet[a..b], terms))
        .map(|(a, b)| (utf16(a), utf16(b)))
        .collect();
    (snippet, highlights)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ingest::chunker::{CharApprox, Chunk, ChunkerConfig};
    use crate::ingest::parser::ParserRegistry;
    use crate::ingest::pipeline::index_folder_once;
    use crate::rag::embedder::FakeEmbedder;
    use crate::rag::vector_store::DIM;
    use crate::storage::db::tests::temp_db;
    use crate::storage::db::DocId;

    fn detail(page: Option<u32>, section: Option<&str>, text: &str) -> ChunkDetail {
        ChunkDetail {
            doc_id: DocId(1),
            text: text.into(),
            page,
            section_path: section.map(Into::into),
            rel_path: "legal/terms.pdf".into(),
            folder_label: "Contracts".into(),
            mtime: None,
        }
    }

    #[test]
    fn citation_names_the_file_and_page_or_section() {
        let text = "Either party may\n  terminate.";
        assert_eq!(
            citation(&detail(Some(3), Some("Termination"), text)),
            "[terms.pdf, p.3] \"Either party may terminate.\""
        );
        assert_eq!(
            citation(&detail(None, Some("Termination > Notice"), text)),
            "[terms.pdf, Termination > Notice] \"Either party may terminate.\""
        );
        assert_eq!(
            citation(&detail(None, None, text)),
            "[terms.pdf] \"Either party may terminate.\""
        );
    }

    #[test]
    fn citation_cuts_a_long_excerpt_at_a_char_boundary() {
        let c = citation(&detail(Some(1), None, &"가".repeat(SNIPPET_CHARS + 5)));
        assert_eq!(
            c,
            format!("[terms.pdf, p.1] \"{}…\"", "가".repeat(SNIPPET_CHARS))
        );
    }

    fn service(e: impl Embedder + 'static) -> SearchService {
        SearchService::new(Arc::new(e), Box::new(NoopReranker))
    }

    fn add_doc(db: &Db, folder: i64, rel: &str, texts: &[&str], e: &dyn Embedder) -> DocId {
        let d = db.insert_pending_document(folder, rel, "txt").unwrap();
        let chunks: Vec<Chunk> = texts
            .iter()
            .enumerate()
            .map(|(i, t)| Chunk {
                doc_id: d,
                chunk_index: i as u32,
                page: None,
                section_path: None,
                start_offset: 0,
                end_offset: t.chars().count() as u32,
                bbox: None,
                text: (*t).to_string(),
                sha256: [0; 32],
            })
            .collect();
        db.upsert_document_chunks(d, &chunks, &e.embed_passages(texts).unwrap())
            .unwrap();
        d
    }

    fn search(
        s: &SearchService,
        db: &Db,
        q: &str,
        mode: SearchMode,
        limit: usize,
    ) -> SearchResults {
        s.search(db, q, mode, &SearchFilter::default(), limit)
            .unwrap()
    }

    /// Queries embed to axis 0; passages containing "rareword" to axis 1, others to axis 0.
    struct Stub;

    impl Stub {
        fn axis(i: usize) -> Vec<f32> {
            let mut v = vec![0.0; DIM];
            v[i] = 1.0;
            v
        }
    }

    impl Embedder for Stub {
        fn dim(&self) -> usize {
            DIM
        }
        fn model_id(&self) -> &str {
            "stub"
        }
        fn embed_passages(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>, RagError> {
            Ok(texts
                .iter()
                .map(|t| Self::axis(usize::from(t.contains("rareword"))))
                .collect())
        }
        fn embed_query(&self, _: &str) -> Result<Vec<f32>, RagError> {
            Ok(Self::axis(0))
        }
    }

    #[test]
    fn rrf_orders_by_fused_rank_and_tags_match_kind() {
        let ids = |v: &[i64]| v.iter().map(|&i| ChunkId(i)).collect::<Vec<_>>();
        let fused = rrf(&ids(&[1, 2, 3]), &ids(&[3, 4]));
        let order: Vec<(i64, MatchKind)> = fused.iter().map(|f| (f.0 .0, f.2)).collect();
        assert_eq!(
            order,
            [
                (3, MatchKind::Both),
                (1, MatchKind::Vector),
                (2, MatchKind::Vector),
                (4, MatchKind::Keyword),
            ]
        );
        assert!((fused[0].1 - (1.0 / 63.0 + 1.0 / 61.0)).abs() < 1e-6);
        assert!((fused[1].1 - 1.0 / 61.0).abs() < 1e-6);
    }

    #[test]
    fn hybrid_marks_both_and_reports_cosine_scores() {
        let (_dir, db) = temp_db();
        let f = db.upsert_folder("/f", "f").unwrap();
        add_doc(&db, f, "a.txt", &["apple banana"], &FakeEmbedder);
        add_doc(&db, f, "b.txt", &["engine piston"], &FakeEmbedder);
        let s = service(FakeEmbedder);
        let r = search(&s, &db, "apple banana", SearchMode::Hybrid, 10);
        assert_eq!(r.hits[0].rel_path, "a.txt");
        assert_eq!(r.hits[0].match_kind, MatchKind::Both);
        assert_eq!(r.hits[1].match_kind, MatchKind::Vector);
        assert!((r.top_score - 1.0).abs() < 1e-4);
        assert_eq!(r.scores.len(), 2);
        assert!(r.scores[0] >= r.scores[1]);
        let v = search(&s, &db, "apple banana", SearchMode::VectorOnly, 10);
        assert!((v.hits[0].score - 1.0).abs() < 1e-4);
        let empty = search(&s, &db, "  ", SearchMode::Hybrid, 10);
        assert!(empty.hits.is_empty() && empty.scores.is_empty() && empty.top_score == 0.0);
    }

    #[test]
    fn caps_hits_per_document() {
        let (_dir, db) = temp_db();
        let f = db.upsert_folder("/f", "f").unwrap();
        let texts: Vec<String> = (0..6).map(|i| format!("alpha beta n{i}")).collect();
        let refs: Vec<&str> = texts.iter().map(String::as_str).collect();
        add_doc(&db, f, "many.txt", &refs, &FakeEmbedder);
        add_doc(&db, f, "one.txt", &["alpha gamma"], &FakeEmbedder);
        let s = service(FakeEmbedder);
        for mode in [SearchMode::Hybrid, SearchMode::VectorOnly] {
            let r = search(&s, &db, "alpha beta", mode, 10);
            let many = r.hits.iter().filter(|h| h.rel_path == "many.txt").count();
            assert_eq!(many, PER_DOC);
            assert_eq!(r.hits.len(), PER_DOC + 1);
        }
        assert_eq!(
            search(&s, &db, "alpha beta", SearchMode::Hybrid, 2)
                .hits
                .len(),
            2
        );
    }

    #[test]
    fn keyword_only_match_surfaces_in_hybrid_only() {
        let (_dir, db) = temp_db();
        let f = db.upsert_folder("/f", "f").unwrap();
        // More than POOL chunks sit on the query's axis, so KNN never reaches the rare one.
        for i in 0..20 {
            add_doc(
                &db,
                f,
                &format!("{i}.txt"),
                &["common words", "common text", "common more"],
                &Stub,
            );
        }
        add_doc(&db, f, "rare.txt", &["the rareword is here"], &Stub);
        let s = service(Stub);
        let hybrid = search(&s, &db, "rareword", SearchMode::Hybrid, 50);
        let rare = hybrid
            .hits
            .iter()
            .find(|h| h.rel_path == "rare.txt")
            .unwrap();
        assert_eq!(rare.match_kind, MatchKind::Keyword);
        let vector = search(&s, &db, "rareword", SearchMode::VectorOnly, 50);
        assert!(vector.hits.iter().all(|h| h.rel_path != "rare.txt"));
    }

    #[test]
    fn snippet_picks_the_best_sentence() {
        let terms = query_terms("What is the apache license warranty?");
        assert_eq!(terms, ["apache", "license", "warranty"]);
        let text = "The license is short. Apache gives no warranty under this license. Bye.";
        let (s, h) = snippet(text, &terms);
        assert_eq!(s, text);
        let long = format!(
            "{} The license is short. {} Apache gives no warranty under this license. {}",
            "filler words here. ".repeat(40),
            "more filler text. ".repeat(40),
            "tail sentence. ".repeat(40)
        );
        let (s, h2) = snippet(&long, &terms);
        assert!(s.chars().count() <= SNIPPET_CHARS);
        assert!(s.contains("Apache gives no warranty under this license."));
        assert_eq!(s, s.trim());
        let marked: Vec<String> = h2
            .iter()
            .map(|&(a, b)| String::from_utf16(&s.encode_utf16().collect::<Vec<_>>()[a..b]).unwrap())
            .collect();
        assert!(marked.iter().any(|m| m == "Apache"));
        assert!(marked.iter().all(|m| terms.contains(&m.to_lowercase())));
        assert!(!h.is_empty());
        // One huge sentence: the window centers on the first term.
        let huge = format!("{}needle{}", "x ".repeat(400), " y".repeat(400));
        let (s, _) = snippet(&huge, &query_terms("needle"));
        assert!(s.contains("needle") && s.chars().count() <= SNIPPET_CHARS);
    }

    #[test]
    fn highlights_are_utf16_offsets() {
        let text = "Über 😀 straße: the Café note.";
        let (s, h) = snippet(text, &query_terms("straße café"));
        let units: Vec<u16> = s.encode_utf16().collect();
        let marked: Vec<String> = h
            .iter()
            .map(|&(a, b)| String::from_utf16(&units[a..b]).unwrap())
            .collect();
        assert_eq!(marked, ["straße", "Café"]);
        // 😀 is two UTF-16 units, so the byte and char offsets would both be wrong here.
        assert_eq!(h[0], (8, 14));
    }

    #[test]
    fn hits_never_carry_the_absolute_folder_path() {
        let (_dir, db) = temp_db();
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("Research");
        std::fs::create_dir_all(root.join("sub")).unwrap();
        std::fs::write(
            root.join("sub/notes.txt"),
            "Quarterly revenue grew. Costs fell.",
        )
        .unwrap();
        index_folder_once(
            &db,
            &ParserRegistry::new(),
            &root,
            &ChunkerConfig::default(),
            &CharApprox,
            &FakeEmbedder,
        )
        .unwrap();
        let r = search(
            &service(FakeEmbedder),
            &db,
            "revenue",
            SearchMode::Hybrid,
            10,
        );
        let h = &r.hits[0];
        assert_eq!(
            (
                h.rel_path.as_str(),
                h.file_name.as_str(),
                h.folder_label.as_str()
            ),
            ("sub/notes.txt", "notes.txt", "Research")
        );
        assert!(h.modified.is_some_and(|ms| ms > 1_000_000_000_000));
        let json = serde_json::to_string(&r).unwrap();
        let abs = root.canonicalize().unwrap();
        assert!(!json.contains(abs.to_str().unwrap()));
        assert!(!json.contains(dir.path().to_str().unwrap()));
    }
}
