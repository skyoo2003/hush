// SPDX-License-Identifier: GPL-3.0-or-later
//! Stress run and crash test over the corpus from `scripts/make-stress-corpus.swift`.
//!
//! `cargo run --release --example stress_bench -- <corpus> <work> [mode]`, under `/usr/bin/time -l`
//! for peak RSS. Needs the default embedding model in `models/` (`scripts/fetch-models.sh`).
//!
//! - `full` (default): fresh DB in `<work>`, index the corpus, print time, outcomes, DB size,
//!   integrity_check, and search latency.
//! - `crash <seconds>`: start `full` as a child, SIGKILL it after `<seconds>`, then `resume`.
//! - `check`: integrity_check and the FTS5 content check on the DB in `<work>`.
//! - `resume`: reopen the DB in `<work>`, run integrity_check, count unfinished documents, rescan,
//!   and fail unless every document ends indexed or failed with a reason and integrity_check passes.
//!
//! The DB key lives in `<work>/key` for this run only: a bench key, never the keychain.

use std::error::Error;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use locite_lib::ingest::chunker::ChunkerConfig;
use locite_lib::ingest::parser::ParserRegistry;
use locite_lib::ingest::pipeline::register_folder;
use locite_lib::ingest::queue::{IndexQueue, QueueConfig, Services};
use locite_lib::rag::embedder::{BertEmbedder, MODELS};
use locite_lib::rag::hybrid::{NoopReranker, SearchMode, SearchService};
use locite_lib::rag::vector_store::SearchFilter;
use locite_lib::storage::db::Db;
use locite_lib::storage::keychain::DbKey;
use tokio_util::sync::CancellationToken;

type R<T> = Result<T, Box<dyn Error>>;

const ROOT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/..");
const QUERIES: &[&str] = &[
    "warranty disclaimer",
    "patent license grant",
    "redistribution of source code",
    "liability for damages",
    "attribution notice requirements",
    "termination of rights",
    "personal data processing obligations",
    "service level credits",
    "confidential information disclosure",
    "open source code reuse across agencies",
];
const SEARCHES: usize = 300;

fn main() {
    if let Err(e) = run() {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}

fn run() -> R<()> {
    let args: Vec<String> = std::env::args().collect();
    let (corpus, work) = match (args.get(1), args.get(2)) {
        (Some(c), Some(w)) => (PathBuf::from(c), PathBuf::from(w)),
        _ => return Err("usage: stress_bench <corpus> <work> [full|crash <s>|resume]".into()),
    };
    match args.get(3).map(String::as_str).unwrap_or("full") {
        "full" => full(&corpus, &work),
        "resume" => resume(&work),
        "check" => integrity(&*open(&work, false)?),
        "crash" => {
            let secs: u64 = args.get(4).ok_or("crash needs <seconds>")?.parse()?;
            crash(&corpus, &work, secs)
        }
        m => Err(format!("unknown mode {m}").into()),
    }
}

fn open(work: &Path, fresh: bool) -> R<Arc<Db>> {
    let key_file = work.join("key");
    let key = if fresh {
        if work.exists() {
            std::fs::remove_dir_all(work)?;
        }
        std::fs::create_dir_all(work)?;
        let k = DbKey::generate()?;
        std::fs::write(&key_file, k.as_bytes())?;
        k
    } else {
        DbKey::from_slice(&std::fs::read(&key_file)?)?
    };
    Ok(Arc::new(Db::open(&work.join("stress.db"), &key)?))
}

fn start(db: &Arc<Db>, embedder: &Arc<BertEmbedder>) -> R<IndexQueue> {
    Ok(IndexQueue::start(
        Services {
            db: db.clone(),
            registry: ParserRegistry::full(
                &Path::new(env!("CARGO_MANIFEST_DIR")).join("vendor/pdfium/lib/libpdfium.dylib"),
            ),
            chunker: ChunkerConfig::default(),
            counter: embedder.clone(),
            embedder: embedder.clone(),
        },
        QueueConfig::default(),
        CancellationToken::new(),
        |_| {},
    )?)
}

fn embedder() -> R<Arc<BertEmbedder>> {
    Ok(Arc::new(BertEmbedder::load(
        &Path::new(ROOT).join("models"),
        MODELS[0].id,
    )?))
}

fn index(db: &Arc<Db>, emb: &Arc<BertEmbedder>, id: i64, root: &Path) -> R<f64> {
    let q = start(db, emb)?;
    let t = Instant::now();
    q.rescan(id, root)?;
    if !q.wait_idle(Duration::from_secs(4 * 3600)) {
        return Err("indexing timed out".into());
    }
    Ok(t.elapsed().as_secs_f64())
}

fn full(corpus: &Path, work: &Path) -> R<()> {
    let db = open(work, true)?;
    let emb = embedder()?;
    let (id, root) = register_folder(&db, corpus)?;
    println!("indexing {} files", std::fs::read_dir(corpus)?.count());
    let secs = index(&db, &emb, id, &root)?;
    println!("indexing: {secs:.1} s");
    report(&db)?;
    integrity(&db)?;
    db.write(|c| c.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |_| Ok(())))?;
    let size: u64 = std::fs::read_dir(work)?
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().starts_with("stress.db"))
        .map(|e| e.metadata().map(|m| m.len()).unwrap_or(0))
        .sum();
    println!("db size: {:.1} MB", size as f64 / 1e6);
    latency(&db, &emb)
}

fn report(db: &Db) -> R<()> {
    let rows: Vec<(String, Option<String>, i64)> = db.read(|c| {
        let mut s = c.prepare(
            "SELECT status, error_code, count(*) FROM documents GROUP BY 1, 2 ORDER BY 1, 2",
        )?;
        let rows = s.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?;
        rows.collect()
    })?;
    for (status, code, n) in rows {
        println!("documents {status} {}: {n}", code.unwrap_or_default());
    }
    let chunks: i64 = db.read(|c| c.query_row("SELECT count(*) FROM chunks", [], |r| r.get(0)))?;
    println!("chunks: {chunks}");
    Ok(())
}

fn integrity(db: &Db) -> R<()> {
    // On the writer: a reader whose only FTS5 use was an earlier integrity_check reports
    // "fts5: corruption found" after other connections write, though the data is intact and a
    // fresh connection or a MATCH on that reader sees it whole (docs/benchmarks.md).
    let r: String = db.write(|c| c.query_row("PRAGMA integrity_check", [], |r| r.get(0)))?;
    println!("integrity_check: {r}");
    // FTS5's own check with rank 1 also compares the index with the `chunks` content.
    let fts = db
        .write(|c| {
            c.execute(
                "INSERT INTO chunks_fts(chunks_fts, rank) VALUES ('integrity-check', 1)",
                [],
            )
        })
        .map(|_| "ok".to_owned())
        .unwrap_or_else(|e| e.to_string());
    println!("fts5 integrity-check: {fts}");
    if r != "ok" || fts != "ok" {
        return Err("integrity_check failed".into());
    }
    Ok(())
}

/// Documents a crash left half done: a pending row, or a row with no hash yet.
fn unfinished(db: &Db) -> R<i64> {
    Ok(db.read(|c| {
        c.query_row(
            "SELECT count(*) FROM documents WHERE status = 'pending' OR (status = 'indexed' AND sha256 IS NULL)",
            [],
            |r| r.get(0),
        )
    })?)
}

fn latency(db: &Db, emb: &Arc<BertEmbedder>) -> R<()> {
    let search = SearchService::new(emb.clone(), Box::new(NoopReranker));
    for q in &QUERIES[..3] {
        search.search(db, q, SearchMode::Hybrid, &SearchFilter::default(), 10)?;
    }
    let mut lat = Vec::with_capacity(SEARCHES);
    for i in 0..SEARCHES {
        let s = Instant::now();
        search.search(
            db,
            QUERIES[i % QUERIES.len()],
            SearchMode::Hybrid,
            &SearchFilter::default(),
            10,
        )?;
        lat.push(s.elapsed());
    }
    lat.sort();
    let pct = |p: f64| {
        let i = ((lat.len() as f64 * p).ceil() as usize).clamp(1, lat.len()) - 1;
        lat[i].as_secs_f64() * 1000.0
    };
    println!(
        "search ({SEARCHES}, idle): p50 {:.0} ms, p95 {:.0} ms, p99 {:.0} ms, max {:.0} ms",
        pct(0.5),
        pct(0.95),
        pct(0.99),
        pct(1.0)
    );
    Ok(())
}

fn crash(corpus: &Path, work: &Path, secs: u64) -> R<()> {
    let mut child = std::process::Command::new(std::env::current_exe()?)
        .arg(corpus)
        .arg(work)
        .arg("full")
        .spawn()?;
    std::thread::sleep(Duration::from_secs(secs));
    if child.try_wait()?.is_some() {
        return Err("indexing finished before the kill; use fewer seconds".into());
    }
    child.kill()?; // SIGKILL: no destructor, no checkpoint, no clean close.
    child.wait()?;
    println!("killed indexing after {secs} s");
    resume(work)
}

fn resume(work: &Path) -> R<()> {
    let db = open(work, false)?;
    integrity(&db)?;
    let (id, root): (i64, String) = db.read(|c| {
        c.query_row("SELECT id, path FROM folders", [], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
    })?;
    println!("after the crash:");
    report(&db)?;
    println!("unfinished: {}", unfinished(&db)?);
    let emb = embedder()?;
    let secs = index(&db, &emb, id, Path::new(&root))?;
    println!("resumed indexing: {secs:.1} s");
    report(&db)?;
    let left = unfinished(&db)?;
    println!("unfinished after resume: {left}");
    integrity(&db)?;
    let orphans: i64 = db.read(|c| {
        c.query_row(
            "SELECT (SELECT count(*) FROM chunks WHERE doc_id NOT IN (SELECT id FROM documents))
                  + (SELECT count(*) FROM documents d WHERE d.status = 'indexed' AND d.size > 0
                     AND NOT EXISTS (SELECT 1 FROM chunks WHERE doc_id = d.id))",
            [],
            |r| r.get(0),
        )
    })?;
    println!("orphan chunks + indexed documents without chunks: {orphans}");
    if left != 0 || orphans != 0 {
        return Err("resume left unfinished or inconsistent documents".into());
    }
    Ok(())
}
