// SPDX-License-Identifier: GPL-3.0-or-later
//! Retrieval eval: `cargo run --example eval [-- <corpus_dir> <queries.jsonl>]`.
//! Defaults to `eval/corpus` and `eval/queries.jsonl`. Indexes the corpus into a throwaway
//! encrypted DB with the default embedding model (run `scripts/fetch-models.sh` first), then
//! prints Recall@5 and MRR@10 per search mode and the top_score split for no-answer queries.

use std::collections::BTreeMap;
use std::error::Error;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use locite_lib::ingest::chunker::ChunkerConfig;
use locite_lib::ingest::parser::ParserRegistry;
use locite_lib::ingest::pipeline::index_folder_once;
use locite_lib::rag::embedder::{BertEmbedder, MODELS};
use locite_lib::rag::hybrid::{NoopReranker, SearchMode, SearchResults, SearchService};
use locite_lib::rag::vector_store::SearchFilter;
use locite_lib::storage::db::Db;
use locite_lib::storage::keychain::DbKey;

const ROOT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/..");
const LIMIT: usize = 10;

#[derive(serde::Deserialize, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
#[serde(rename_all = "snake_case")]
enum Kind {
    ProperNoun,
    Clause,
    Numeric,
    Concept,
    NoAnswer,
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Relevant {
    file: String,
    anchor: String,
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Query {
    id: String,
    query: String,
    kind: Kind,
    relevant: Vec<Relevant>,
}

fn main() {
    if let Err(e) = run() {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}

fn norm(s: &str) -> String {
    s.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

fn load_queries(path: &Path, corpus: &Path) -> Result<Vec<Query>, Box<dyn Error>> {
    let mut out = Vec::new();
    for (i, line) in std::fs::read_to_string(path)?.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let q: Query =
            serde_json::from_str(line).map_err(|e| format!("queries line {}: {e}", i + 1))?;
        if (q.kind == Kind::NoAnswer) != q.relevant.is_empty() {
            return Err(format!("{}: relevant must be empty iff kind is no_answer", q.id).into());
        }
        if let Some(r) = q.relevant.iter().find(|r| !corpus.join(&r.file).is_file()) {
            return Err(format!("{}: {} is not in the corpus", q.id, r.file).into());
        }
        out.push(q);
    }
    if out.is_empty() {
        return Err("no queries".into());
    }
    Ok(out)
}

/// Per query: the fraction of `relevant` found in the top 5, and 1/rank of the first
/// relevant hit within the top 10.
fn score(db: &Db, q: &Query, r: &SearchResults) -> Result<(f64, f64), Box<dyn Error>> {
    let mut found = vec![false; q.relevant.len()];
    let mut rr = 0.0;
    for (rank, hit) in r.hits.iter().enumerate() {
        let text: String = db.read(|c| {
            c.query_row(
                "SELECT text FROM chunks WHERE id = ?1",
                [hit.chunk_id],
                |r| r.get(0),
            )
        })?;
        let text = norm(&text);
        for (i, rel) in q.relevant.iter().enumerate() {
            if hit.rel_path == rel.file && text.contains(&norm(&rel.anchor)) {
                if rank < 5 {
                    found[i] = true;
                }
                if rr == 0.0 {
                    rr = 1.0 / (rank + 1) as f64;
                }
            }
        }
    }
    let recall = found.iter().filter(|f| **f).count() as f64 / found.len() as f64;
    Ok((recall, rr))
}

fn dist(name: &str, v: &mut [f32]) {
    if v.is_empty() {
        println!("  {name}: none");
        return;
    }
    v.sort_by(f32::total_cmp);
    let list: Vec<String> = v.iter().map(|s| format!("{s:.3}")).collect();
    println!(
        "  {name} (n={}): min {:.3}  p50 {:.3}  max {:.3}\n    [{}]",
        v.len(),
        v[0],
        v[v.len() / 2],
        v[v.len() - 1],
        list.join(", ")
    );
}

fn run() -> Result<(), Box<dyn Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (corpus, queries) = match args.as_slice() {
        [] => (
            Path::new(ROOT).join("eval/corpus"),
            Path::new(ROOT).join("eval/queries.jsonl"),
        ),
        [c, q] => (PathBuf::from(c), PathBuf::from(q)),
        _ => return Err("usage: eval [<corpus_dir> <queries.jsonl>]".into()),
    };
    let queries = load_queries(&queries, &corpus)?;

    let tmp = tempfile::tempdir()?;
    let db = Db::open(&tmp.path().join("eval.db"), &DbKey::generate()?)?;
    let embedder = Arc::new(BertEmbedder::load(
        &Path::new(ROOT).join("models"),
        MODELS[0].id,
    )?);
    let report = index_folder_once(
        &db,
        &ParserRegistry::new(),
        &corpus,
        &ChunkerConfig::default(),
        &*embedder,
        &*embedder,
    )?;
    println!("indexed: {report:?}");
    if report.failed > 0 {
        return Err("some corpus files failed to index".into());
    }
    let service = SearchService::new(embedder, Box::new(NoopReranker));

    let answerable = queries.iter().filter(|q| q.kind != Kind::NoAnswer).count();
    for mode in [SearchMode::VectorOnly, SearchMode::Hybrid] {
        let (mut recall, mut mrr) = (0.0, 0.0);
        let mut by_kind: BTreeMap<Kind, (f64, usize)> = BTreeMap::new();
        let (mut top_none, mut top_ans) = (Vec::new(), Vec::new());
        let mut misses = Vec::new();
        for q in &queries {
            let r = service.search(&db, &q.query, mode, &SearchFilter::default(), LIMIT)?;
            if q.kind == Kind::NoAnswer {
                top_none.push(r.top_score);
                continue;
            }
            top_ans.push(r.top_score);
            let (rc, rr) = score(&db, q, &r)?;
            recall += rc;
            mrr += rr;
            let k = by_kind.entry(q.kind).or_default();
            k.0 += rc;
            k.1 += 1;
            if rc < 1.0 {
                let first = if rr > 0.0 {
                    format!("first relevant at rank {}", (1.0 / rr).round())
                } else {
                    "no relevant hit in top 10".into()
                };
                misses.push(format!("  {} {mode:?}: recall@5 {rc:.2}, {first}", q.id));
            }
        }
        let n = answerable.max(1) as f64;
        println!("\n== {mode:?} ==");
        println!(
            "Recall@5 {:.3}  MRR@10 {:.3}  (answerable n={answerable})",
            recall / n,
            mrr / n
        );
        for (kind, (sum, count)) in &by_kind {
            println!(
                "  {kind:?}: Recall@5 {:.3} (n={count})",
                sum / *count as f64
            );
        }
        if misses.is_empty() {
            println!("misses: none");
        } else {
            println!("misses:\n{}", misses.join("\n"));
        }
        if mode == SearchMode::Hybrid {
            println!("top_score:");
            dist("no_answer", &mut top_none);
            dist("answerable", &mut top_ans);
        }
    }
    Ok(())
}
