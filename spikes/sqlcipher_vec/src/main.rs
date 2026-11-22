//! Throwaway spike: SQLCipher (rusqlite bundled) + sqlite-vec (static, auto_extension) + FTS5.
//! Run: cargo run --release            (checks 1-3, then KNN bench at 1k/100k/300k)
//!      cargo run --release -- quick   (checks 1-3 only)
use rusqlite::{ffi, params, Connection};
use std::path::{Path, PathBuf};
use std::time::Instant;

const DIM: usize = 384;
const KEY: &str = "spike-correct-key";
const MARKER: &str = "CONFIDENTIAL-MARKER";

fn register_vec() {
    // SAFETY: the sqlite-vec crate declares sqlite3_vec_init as `fn()`, but the C symbol has the
    // extension entry-point ABI that sqlite3_auto_extension expects. Called once before any open.
    unsafe {
        ffi::sqlite3_auto_extension(Some(std::mem::transmute::<
            *const (),
            unsafe extern "C" fn(
                *mut ffi::sqlite3,
                *mut *mut std::os::raw::c_char,
                *const ffi::sqlite3_api_routines,
            ) -> i32,
        >(sqlite_vec::sqlite3_vec_init as *const ())));
    }
}

struct Rng(u64);
impl Rng {
    fn next_f32(&mut self) -> f32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 >> 40) as f32 / (1u64 << 24) as f32 - 0.5
    }
    fn unit_vec(&mut self) -> Vec<f32> {
        let mut v: Vec<f32> = (0..DIM).map(|_| self.next_f32()).collect();
        let n = v.iter().map(|x| x * x).sum::<f32>().sqrt();
        v.iter_mut().for_each(|x| *x /= n);
        v
    }
}

fn blob(v: &[f32]) -> Vec<u8> {
    v.iter().flat_map(|x| x.to_le_bytes()).collect()
}

fn open(path: &Path, key: Option<&str>) -> rusqlite::Result<Connection> {
    let c = Connection::open(path)?;
    if let Some(k) = key {
        c.pragma_update(None, "key", k)?;
    }
    Ok(c)
}

fn fresh(path: &Path) {
    for s in ["", "-wal", "-shm"] {
        let _ = std::fs::remove_file(format!("{}{s}", path.display()));
    }
}

fn create_and_fill(c: &Connection, n: usize, with_fts: bool) {
    c.pragma_update(None, "journal_mode", "WAL").unwrap();
    c.execute_batch(&format!(
        "CREATE VIRTUAL TABLE vec USING vec0(embedding float[{DIM}]);"
    ))
    .unwrap();
    if with_fts {
        c.execute_batch("CREATE VIRTUAL TABLE docs USING fts5(body);")
            .unwrap();
    }
    let mut rng = Rng(0x9E3779B97F4A7C15);
    c.execute_batch("BEGIN").unwrap();
    {
        let mut iv = c
            .prepare("INSERT INTO vec(rowid, embedding) VALUES (?1, ?2)")
            .unwrap();
        let mut id = c
            .prepare("INSERT INTO docs(rowid, body) VALUES (?1, ?2)")
            .ok();
        for i in 1..=n as i64 {
            iv.execute(params![i, blob(&rng.unit_vec())]).unwrap();
            if let Some(s) = id.as_mut() {
                s.execute(params![
                    i,
                    format!(
                        "document {i} about topic{} {MARKER} secret body text",
                        i % 10
                    )
                ])
                .unwrap();
            }
        }
    }
    c.execute_batch("COMMIT").unwrap();
}

fn knn(c: &Connection, q: &[f32]) -> Vec<(i64, f64)> {
    let mut s = c
        .prepare_cached(
            "SELECT rowid, distance FROM vec WHERE embedding MATCH ?1 AND k = 10 ORDER BY distance",
        )
        .unwrap();
    s.query_map([blob(q)], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect()
}

/// Case-insensitive search for the marker and for FTS5 tokens it splits into.
fn leaks(path: &Path) -> Vec<String> {
    let mut hits = vec![];
    for s in ["", "-wal", "-shm"] {
        let p = format!("{}{s}", path.display());
        let Ok(bytes) = std::fs::read(&p) else {
            continue;
        };
        let lower = bytes.to_ascii_lowercase();
        for needle in [
            "confidential-marker",
            "confidential",
            "secret body",
            "sqlite format 3",
        ] {
            if lower.windows(needle.len()).any(|w| w == needle.as_bytes()) {
                hits.push(format!("{p} ({} bytes): {needle}", bytes.len()));
            }
        }
    }
    hits
}

fn checks(dir: &Path) {
    let db = dir.join("check.db");
    fresh(&db);
    let c = open(&db, Some(KEY)).unwrap();
    let cipher: String = c
        .query_row("PRAGMA cipher_version", [], |r| r.get(0))
        .unwrap();
    let sqlite: String = c
        .query_row("SELECT sqlite_version()", [], |r| r.get(0))
        .unwrap();
    let vecv: String = c
        .query_row("SELECT vec_version()", [], |r| r.get(0))
        .unwrap();
    let fts5: i64 = c
        .query_row("SELECT sqlite_compileoption_used('ENABLE_FTS5')", [], |r| {
            r.get(0)
        })
        .unwrap();
    println!("sqlcipher {cipher} / sqlite {sqlite} / sqlite-vec {vecv} / ENABLE_FTS5={fts5}");
    assert!(!cipher.is_empty() && fts5 == 1);

    // Check 1: create, insert 1,000 each, query.
    create_and_fill(&c, 1000, true);
    let q = Rng(0x9E3779B97F4A7C15).unit_vec(); // == embedding of rowid 1
    let hits = knn(&c, &q);
    println!("[1] KNN rows={} top={:?}", hits.len(), hits[0]);
    assert_eq!(hits.len(), 10);
    assert_eq!(hits[0].0, 1);
    assert!(hits[0].1 < 1e-5);
    assert!(hits.windows(2).all(|w| w[0].1 <= w[1].1));
    let m: i64 = c
        .query_row(
            "SELECT count(*) FROM docs WHERE docs MATCH 'topic3'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let m2: i64 = c
        .query_row(
            "SELECT count(*) FROM docs WHERE docs MATCH 'document AND \"17\"'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    println!("[1] FTS5 MATCH 'topic3' rows={m}, 'document AND 17' rows={m2}");
    assert_eq!(m, 100);
    assert_eq!(m2, 1);

    // Check 3 (live): connection still open, data sits in the uncheckpointed WAL.
    let wal = std::fs::metadata(format!("{}-wal", db.display()))
        .map(|m| m.len())
        .unwrap_or(0);
    let live = leaks(&db);
    println!("[3] live (WAL {wal} bytes) leaks={live:?}");
    assert!(wal > 0 && live.is_empty());
    drop(c);
    let closed = leaks(&db);
    println!("[3] after close leaks={closed:?}");
    assert!(closed.is_empty());

    // Check 2: wrong key / no key must fail on first read; right key still works.
    for (label, key) in [("wrong key", Some("wrong-key")), ("no key", None)] {
        let r = open(&db, key).and_then(|c| {
            c.query_row("SELECT count(*) FROM sqlite_master", [], |r| {
                r.get::<_, i64>(0)
            })
        });
        println!("[2] {label}: {r:?}");
        assert!(r.is_err());
    }
    let n: i64 = open(&db, Some(KEY))
        .unwrap()
        .query_row("SELECT count(*) FROM docs", [], |r| r.get(0))
        .unwrap();
    assert_eq!(n, 1000);

    // Negative control: the scanner must find the marker in an unencrypted copy.
    let plain = dir.join("plain.db");
    fresh(&plain);
    let pc = open(&plain, None).unwrap();
    create_and_fill(&pc, 1000, true);
    let ctl = leaks(&plain);
    println!("[3] control (plaintext, open) leaks={}", ctl.len());
    assert!(!ctl.is_empty());
    drop(pc);
    fresh(&plain);
    fresh(&db);
    println!("checks 1-3 PASS");
}

fn pct(sorted: &[f64], p: f64) -> f64 {
    sorted[((sorted.len() - 1) as f64 * p).round() as usize]
}

fn bench(dir: &Path) {
    const ROUNDS: usize = 5;
    const Q: usize = 20;
    println!("\nN,config,insert_s,db_mb,round_p50_ms(x{ROUNDS}),p50_ms,p95_ms,max_ms");
    for n in [1_000usize, 100_000, 300_000] {
        // (label, key, cache_size pragma). -1048576 = 1 GiB page cache.
        for (label, key, cache) in [
            ("enc-default-cache", Some(KEY), None),
            ("enc-1GiB-cache", Some(KEY), Some(-1_048_576)),
            ("plaintext", None, None),
        ] {
            let db = dir.join(format!("bench-{n}-{label}.db"));
            fresh(&db);
            let c = open(&db, key).unwrap();
            let t = Instant::now();
            create_and_fill(&c, n, false);
            let insert_s = t.elapsed().as_secs_f64();
            c.pragma_update(None, "wal_checkpoint", "TRUNCATE").unwrap();
            drop(c);
            let mb = std::fs::metadata(&db).unwrap().len() as f64 / 1e6;
            let c = open(&db, key).unwrap(); // fresh connection: empty page cache
            if let Some(cs) = cache {
                c.pragma_update(None, "cache_size", cs).unwrap();
            }
            let mut rng = Rng(42);
            for _ in 0..3 {
                assert_eq!(knn(&c, &rng.unit_vec()).len(), 10); // warmup
            }
            let mut all = vec![];
            let mut round_p50 = vec![];
            for _ in 0..ROUNDS {
                let mut r = vec![];
                for _ in 0..Q {
                    let q = rng.unit_vec();
                    let t = Instant::now();
                    let hits = knn(&c, &q);
                    r.push(t.elapsed().as_secs_f64() * 1e3);
                    assert_eq!(hits.len(), 10);
                }
                r.sort_by(f64::total_cmp);
                round_p50.push(format!("{:.2}", pct(&r, 0.5)));
                all.extend(r);
            }
            all.sort_by(f64::total_cmp);
            println!(
                "{n},{label},{insert_s:.1},{mb:.0},{},{:.2},{:.2},{:.2}",
                round_p50.join("/"),
                pct(&all, 0.5),
                pct(&all, 0.95),
                all[all.len() - 1]
            );
            drop(c);
            fresh(&db);
        }
    }
}

fn main() {
    register_vec();
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/spike-data");
    std::fs::create_dir_all(&dir).unwrap();
    checks(&dir);
    if std::env::args().nth(1).as_deref() != Some("quick") {
        bench(&dir);
    }
}
