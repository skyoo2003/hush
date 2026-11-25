// SPDX-License-Identifier: GPL-3.0-or-later
//! Absolute rule 5 end to end: index, watch, fail, search, and panic over files whose names
//! and text carry a marker, through the app's real log setup and crash hook, then export
//! diagnostics. The marker and the folder's path must appear in no log, crash report, or zip
//! entry. Its own test binary, because the log subscriber and the panic hook are process-wide.

use std::io::Read;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use locite_lib::diagnostics::{self, AppState, CRASH_DIR};
use locite_lib::ingest::chunker::{ChunkerConfig, TokenCounter};
use locite_lib::ingest::parser::ParserRegistry;
use locite_lib::ingest::queue::{IndexQueue, QueueConfig, Services};
use locite_lib::ingest::watcher::{self, FolderWatcher};
use locite_lib::rag::embedder::FakeEmbedder;
use locite_lib::rag::hybrid::{NoopReranker, SearchMode, SearchService};
use locite_lib::rag::vector_store::SearchFilter;
use locite_lib::storage::db::Db;
use locite_lib::storage::keychain::DbKey;
use tokio_util::sync::CancellationToken;

const MARK: &str = "zqxprivacymarker";

struct Chars;
impl TokenCounter for Chars {
    fn count(&self, text: &str) -> usize {
        text.chars().count().div_ceil(4)
    }
}

/// Every file under `dir`, as (relative name, bytes).
fn files(dir: &Path) -> Vec<(String, Vec<u8>)> {
    let mut out = Vec::new();
    for e in std::fs::read_dir(dir).unwrap() {
        let p = e.unwrap().path();
        if p.is_dir() {
            out.extend(files(&p));
        } else {
            out.push((p.display().to_string(), std::fs::read(&p).unwrap()));
        }
    }
    out
}

fn assert_clean(what: &str, bytes: &[u8], secrets: &[&str]) {
    let text = String::from_utf8_lossy(bytes).to_lowercase();
    for s in secrets {
        assert!(
            !text.contains(&s.to_lowercase()),
            "{what} contains {s}:\n{text}"
        );
    }
}

#[test]
fn no_document_text_name_or_path_reaches_logs_crash_reports_or_diagnostics() {
    let logs = tempfile::tempdir().unwrap();
    locite_lib::init_tracing(logs.path()).unwrap();
    diagnostics::install_panic_hook(logs.path().join(CRASH_DIR));

    let docs = tempfile::tempdir().unwrap();
    let root = docs.path().join(format!("{MARK}-folder"));
    std::fs::create_dir(&root).unwrap();
    let body = format!("{MARK} launch plan moved to March. {MARK} budget notes.");
    let fixtures: &[(&str, &[u8])] = &[
        ("plan.txt", body.as_bytes()),
        ("notes.md", body.as_bytes()),
        (
            "broken.docx",
            b"PK\x03\x04 not really a zip zqxprivacymarker",
        ),
        ("fake.pdf", b"%PDF-1.7 zqxprivacymarker truncated"),
        ("empty.pdf", b""),
        ("bad.png", b"\x89PNG\r\n\x1a\n zqxprivacymarker"),
        ("locked.txt", body.as_bytes()),
    ];
    for (name, bytes) in fixtures {
        std::fs::write(root.join(format!("{MARK}-{name}")), bytes).unwrap();
    }
    let locked = root.join(format!("{MARK}-locked.txt"));
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o000)).unwrap();

    let data = tempfile::tempdir().unwrap();
    let db =
        Arc::new(Db::open(&data.path().join("audit.db"), &DbKey::generate().unwrap()).unwrap());
    let pdfium = Path::new(env!("CARGO_MANIFEST_DIR")).join("vendor/pdfium/lib/libpdfium.dylib");
    let queue = Arc::new(
        IndexQueue::start(
            Services {
                db: db.clone(),
                registry: ParserRegistry::full(&pdfium),
                chunker: ChunkerConfig::default(),
                counter: Arc::new(Chars),
                embedder: Arc::new(FakeEmbedder),
            },
            QueueConfig::default(),
            CancellationToken::new(),
            |_| {},
        )
        .unwrap(),
    );
    let w = FolderWatcher::start(db.clone(), queue.clone()).unwrap();
    watcher::add_folder(&db, Some(&w), &root).unwrap();
    assert!(queue.wait_idle(Duration::from_secs(120)));
    // A change the watcher sees, and a file that vanishes.
    std::fs::write(
        root.join(format!("{MARK}-plan.txt")),
        format!("{body} edited"),
    )
    .unwrap();
    std::fs::remove_file(root.join(format!("{MARK}-notes.md"))).unwrap();
    std::thread::sleep(Duration::from_secs(3));
    assert!(queue.wait_idle(Duration::from_secs(120)));

    let search = SearchService::new(Arc::new(FakeEmbedder), Box::new(NoopReranker));
    let hits = search
        .search(
            &db,
            &format!("{MARK} launch"),
            SearchMode::Hybrid,
            &SearchFilter::default(),
            10,
        )
        .unwrap();
    assert!(!hits.hits.is_empty(), "the fixture was indexed");

    // A panic message can quote anything the code formats into it, here a path and text.
    let path = root.join(format!("{MARK}-plan.txt"));
    let panicked = std::thread::Builder::new()
        .name("audit".into())
        .spawn(move || -> () { panic!("could not open {}: {MARK} launch plan", path.display()) })
        .unwrap()
        .join();
    assert!(panicked.is_err());

    let zip = data.path().join("diagnostics.zip");
    diagnostics::export(
        &zip,
        logs.path(),
        &AppState {
            db: Some(&db),
            air_gap: true,
            installed_models: vec![],
        },
    )
    .unwrap();
    drop(w);
    queue.cancel();
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o600)).unwrap();

    let root_str = root.display().to_string();
    let secrets = [MARK, root_str.as_str(), "launch plan", "budget notes"];
    let written = files(logs.path());
    let log_text: String = written
        .iter()
        .map(|(_, b)| String::from_utf8_lossy(b).into_owned())
        .collect();
    // The audit means something only if the failures and the panic were logged.
    for want in [
        "index job failed",
        "folder status",
        "panic; crash report saved",
    ] {
        assert!(log_text.contains(want), "no `{want}` in logs:\n{log_text}");
    }
    let crashes = written
        .iter()
        .filter(|(n, _)| n.contains(CRASH_DIR))
        .count();
    assert_eq!(crashes, 1);
    for (name, bytes) in &written {
        assert_clean(name, bytes, &secrets);
    }

    let mut archive = zip::ZipArchive::new(std::fs::File::open(&zip).unwrap()).unwrap();
    assert!(archive.len() >= 3, "summary, a log, and the crash report");
    for i in 0..archive.len() {
        let mut f = archive.by_index(i).unwrap();
        let name = f.name().unwrap().into_owned();
        assert_clean("zip entry name", name.as_bytes(), &secrets);
        let mut bytes = Vec::new();
        f.read_to_end(&mut bytes).unwrap();
        assert_clean(&name, &bytes, &secrets);
    }
}
