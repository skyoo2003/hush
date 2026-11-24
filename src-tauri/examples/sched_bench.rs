// SPDX-License-Identifier: GPL-3.0-or-later
//! Search latency while indexing: `cargo run --release --example sched_bench [-- <seconds>]`.
//! Builds synthetic documents from the license texts in `eval/corpus`, indexes a 1,000-document
//! seed folder, then for each plan indexes a fresh 1,000-document folder while searching every
//! 150 ms for the window (default 60 s), and prints search p50/p95/p99. Needs the default
//! embedding model in `models/` (run `scripts/fetch-models.sh`).

use std::error::Error;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use locite_lib::ingest::chunker::ChunkerConfig;
use locite_lib::ingest::parser::ParserRegistry;
use locite_lib::ingest::pipeline::register_folder;
use locite_lib::ingest::queue::{IndexQueue, QueueConfig, Services};
use locite_lib::ingest::scheduler::{self, LowPowerPolicy, Plan, Reason};
use locite_lib::rag::embedder::{BertEmbedder, MODELS};
use locite_lib::rag::hybrid::{NoopReranker, SearchMode, SearchService};
use locite_lib::rag::vector_store::SearchFilter;
use locite_lib::storage::db::Db;
use locite_lib::storage::keychain::DbKey;
use tokio_util::sync::CancellationToken;

const ROOT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/..");
const DOCS: usize = 1000;
const SEARCH_EVERY: Duration = Duration::from_millis(150);
const QUERIES: &[&str] = &[
    "warranty disclaimer",
    "patent license grant",
    "redistribution of source code",
    "liability for damages",
    "attribution notice requirements",
    "termination of rights",
    "modified versions must be marked",
    "contributor agreement",
    "trademark use not permitted",
    "copyleft obligations for derivative works",
];

fn main() {
    if let Err(e) = run() {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}

/// Deterministic sentences shuffled from the eval corpus: no real user documents.
fn write_corpus(dir: &Path, salt: u64) -> Result<(), Box<dyn Error>> {
    let mut sentences = Vec::new();
    for e in std::fs::read_dir(Path::new(ROOT).join("eval/corpus"))? {
        let text = std::fs::read_to_string(e?.path())?;
        sentences.extend(
            text.split(". ")
                .map(|s| s.split_whitespace().collect::<Vec<_>>().join(" "))
                .filter(|s| s.len() > 40),
        );
    }
    std::fs::create_dir_all(dir)?;
    let mut x = 0x9E37_79B9_7F4A_7C15_u64 ^ salt;
    for i in 0..DOCS {
        let mut body = String::new();
        // About 3 KB, so a document is a few chunks.
        while body.len() < 3000 {
            x = x
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            body += &sentences[(x >> 33) as usize % sentences.len()];
            body += ". ";
        }
        std::fs::write(dir.join(format!("doc{i:04}.txt")), body)?;
    }
    Ok(())
}

fn pct(sorted: &[Duration], p: f64) -> f64 {
    let i = ((sorted.len() as f64 * p).ceil() as usize).clamp(1, sorted.len()) - 1;
    sorted[i].as_secs_f64() * 1000.0
}

fn run() -> Result<(), Box<dyn Error>> {
    let window = Duration::from_secs(
        std::env::args()
            .nth(1)
            .map(|s| s.parse())
            .transpose()?
            .unwrap_or(60),
    );
    let tmp = tempfile::tempdir()?;
    let db = Arc::new(Db::open(&tmp.path().join("bench.db"), &DbKey::generate()?)?);
    let embedder = Arc::new(BertEmbedder::load(
        &Path::new(ROOT).join("models"),
        MODELS[0].id,
    )?);
    let search = SearchService::new(embedder.clone(), Box::new(NoopReranker));
    let start_queue = || {
        IndexQueue::start(
            Services {
                db: db.clone(),
                registry: ParserRegistry::new(),
                chunker: ChunkerConfig::default(),
                counter: embedder.clone(),
                embedder: embedder.clone(),
            },
            QueueConfig::default(),
            CancellationToken::new(),
            |_| {},
        )
    };
    let folder = |name: &str, salt| -> Result<_, Box<dyn Error>> {
        let dir = tmp.path().join(name);
        write_corpus(&dir, salt)?;
        Ok(register_folder(&db, &dir)?)
    };

    let q = start_queue()?;
    let max = q.budget().max();
    println!(
        "cores {}, parse workers spawned {max}, model {}",
        std::thread::available_parallelism()?,
        MODELS[0].id
    );
    let (id, root) = folder("seed", 0)?;
    let t = Instant::now();
    q.rescan(id, &root)?;
    if !q.wait_idle(Duration::from_secs(3600)) {
        return Err("seed indexing timed out".into());
    }
    let secs = t.elapsed().as_secs_f64();
    println!(
        "seed: {} documents indexed in {secs:.1} s ({:.1} docs/s, full speed)",
        q.progress().done,
        f64::from(q.progress().done) / secs
    );
    drop(q);

    let full = Plan {
        parse_workers: max,
        embedding: true,
    };
    let runs = [
        ("scheduler OFF (all workers, embedding on)", full),
        (
            "scheduler ON, Normal",
            scheduler::plan(Reason::Default, LowPowerPolicy::Pause, max),
        ),
        (
            "scheduler ON, Interactive",
            scheduler::plan(Reason::Searching, LowPowerPolicy::Pause, max),
        ),
    ];
    println!(
        "| Run | Plan | Searches | p50 ms | p95 ms | p99 ms | max ms | Docs indexed in window |"
    );
    println!("|---|---|---|---|---|---|---|---|");
    for (i, (name, plan)) in runs.into_iter().enumerate() {
        let q = start_queue()?;
        scheduler::apply(&q, plan);
        let (id, root) = folder(&format!("run{i}"), i as u64 + 1)?;
        // Warm the model and caches; not counted.
        for text in &QUERIES[..3] {
            search.search(&db, text, SearchMode::Hybrid, &SearchFilter::default(), 10)?;
        }
        q.rescan(id, &root)?;
        let t = Instant::now();
        let mut lat = Vec::new();
        while t.elapsed() < window {
            let text = QUERIES[lat.len() % QUERIES.len()];
            let s = Instant::now();
            search.search(&db, text, SearchMode::Hybrid, &SearchFilter::default(), 10)?;
            lat.push(s.elapsed());
            std::thread::sleep(SEARCH_EVERY.saturating_sub(s.elapsed()));
        }
        let done = q.progress().done;
        q.cancel();
        drop(q);
        lat.sort();
        println!(
            "| {name} | {} parse, embedding {} | {} | {:.0} | {:.0} | {:.0} | {:.0} | {done} |",
            plan.parse_workers,
            if plan.embedding { "on" } else { "off" },
            lat.len(),
            pct(&lat, 0.50),
            pct(&lat, 0.95),
            pct(&lat, 0.99),
            pct(&lat, 1.0),
        );
    }
    Ok(())
}
