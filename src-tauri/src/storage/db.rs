//! The single SQLCipher DB: one writer connection plus a small reader pool, WAL mode.
//! All calls block; async callers run them in `spawn_blocking`.

use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, MutexGuard, Once, TryLockError};

use rusqlite::{ffi, Connection};

use super::error::StorageError;
use super::keychain::{DbKey, KeyProvider};

/// Index `i` upgrades the schema from `user_version` i to i + 1.
const MIGRATIONS: &[&str] = &[include_str!("migrations/0001_init.sql")];
const READERS: usize = 4;
/// Page cache budget, in KiB. ADR-001: KNN is ~10x faster once the vector table fits in the cache.
/// Each reader may grow to this size, and `soft_heap_limit` holds all connections together near it.
// shortcut: fixed 1 GiB (~600k vectors, ADR-001 estimate); derive from physical RAM if 8 GB Macs swap.
const CACHE_BUDGET_KIB: i64 = 1 << 20;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DocId(pub i64);

pub struct Db {
    writer: Mutex<Connection>,
    readers: Vec<Mutex<Connection>>,
    next_reader: AtomicUsize,
}

/// Opens the DB with the stored key, creating both on first run.
/// A stored key that does not open the DB is an error, never a reason to make a new key.
pub fn open_or_create(path: &Path, keys: &dyn KeyProvider) -> Result<Db, StorageError> {
    match keys.load()? {
        Some(key) => Db::open(path, &key),
        None if path.try_exists()? => Err(StorageError::KeyMissing),
        None => {
            let key = DbKey::generate()?;
            // Key first: a key without a DB is harmless, a DB without its key is unrecoverable.
            keys.store(&key)?;
            Db::open(path, &key)
        }
    }
}

impl Db {
    pub fn open(path: &Path, key: &DbKey) -> Result<Self, StorageError> {
        register_sqlite_vec();
        let mut writer = connect(path, key)?;
        writer.pragma_update(None, "journal_mode", "WAL")?;
        migrate(&mut writer)?;
        let readers = (0..READERS)
            .map(|_| {
                let c = connect(path, key)?;
                c.pragma_update(None, "query_only", true)?;
                c.pragma_update(None, "cache_size", -CACHE_BUDGET_KIB)?;
                c.pragma_update(None, "soft_heap_limit", CACHE_BUDGET_KIB * 1024)?;
                Ok(Mutex::new(c))
            })
            .collect::<Result<_, StorageError>>()?;
        Ok(Self {
            writer: Mutex::new(writer),
            readers,
            next_reader: AtomicUsize::new(0),
        })
    }

    pub fn read<T>(
        &self,
        f: impl FnOnce(&Connection) -> rusqlite::Result<T>,
    ) -> Result<T, StorageError> {
        // Lowest free reader first, so a lone caller keeps hitting reader 0 and its warm page cache.
        let free = self.readers.iter().find_map(|m| match m.try_lock() {
            Ok(g) => Some(g),
            Err(TryLockError::Poisoned(e)) => Some(e.into_inner()),
            Err(TryLockError::WouldBlock) => None,
        });
        let conn = free.unwrap_or_else(|| {
            let i = self.next_reader.fetch_add(1, Ordering::Relaxed) % self.readers.len();
            lock(&self.readers[i])
        });
        Ok(f(&conn)?)
    }

    pub fn write<T>(
        &self,
        f: impl FnOnce(&mut Connection) -> rusqlite::Result<T>,
    ) -> Result<T, StorageError> {
        Ok(f(&mut lock(&self.writer))?)
    }

    /// One statement, so one transaction: chunks and doc_versions cascade, and the
    /// `chunks_ad` trigger removes the matching chunks_fts and vec_chunks rows.
    pub fn delete_document(&self, id: DocId) -> Result<(), StorageError> {
        self.write(|c| c.execute("DELETE FROM documents WHERE id = ?1", [id.0]))?;
        Ok(())
    }

    pub fn setting(&self, key: &str) -> Result<Option<String>, StorageError> {
        self.read(|c| {
            match c.query_row("SELECT value FROM settings WHERE key = ?1", [key], |r| {
                r.get(0)
            }) {
                Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
                v => v.map(Some),
            }
        })
    }

    pub fn set_setting(&self, key: &str, value: &str) -> Result<(), StorageError> {
        self.write(|c| {
            c.execute(
                "INSERT INTO settings(key, value) VALUES (?1, ?2)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                [key, value],
            )
        })?;
        Ok(())
    }
}

fn lock(m: &Mutex<Connection>) -> MutexGuard<'_, Connection> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

fn connect(path: &Path, key: &DbKey) -> Result<Connection, StorageError> {
    let c = Connection::open(path)?;
    c.execute_batch(&key.pragma())?;
    c.execute_batch("PRAGMA cipher_memory_security = ON;")?;
    // The first read decrypts page 1, so a wrong key fails here and not later.
    c.query_row("SELECT count(*) FROM sqlite_master", [], |_| Ok(()))
        .map_err(|e| match e.sqlite_error_code() {
            Some(rusqlite::ErrorCode::NotADatabase) => StorageError::WrongKey,
            _ => e.into(),
        })?;
    // temp_store: rule 4, no plaintext temp files.
    c.execute_batch("PRAGMA foreign_keys = ON; PRAGMA temp_store = MEMORY;")?;
    Ok(c)
}

fn migrate(c: &mut Connection) -> Result<(), StorageError> {
    let current: i64 = c.pragma_query_value(None, "user_version", |r| r.get(0))?;
    let latest = MIGRATIONS.len() as i64;
    if current > latest {
        return Err(StorageError::SchemaTooNew(current));
    }
    for (version, sql) in (current..).zip(&MIGRATIONS[current as usize..]) {
        let tx = c.transaction()?;
        tx.execute_batch(sql)?;
        tx.pragma_update(None, "user_version", version + 1)?;
        tx.commit()?;
    }
    Ok(())
}

fn register_sqlite_vec() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        // SAFETY: the sqlite-vec crate declares sqlite3_vec_init as `fn()`, but the C symbol has the
        // extension entry-point ABI that sqlite3_auto_extension expects. Registered once, before any open.
        unsafe {
            ffi::sqlite3_auto_extension(Some(std::mem::transmute::<
                *const (),
                unsafe extern "C" fn(
                    *mut ffi::sqlite3,
                    *mut *mut std::os::raw::c_char,
                    *const ffi::sqlite3_api_routines,
                ) -> i32,
            >(
                sqlite_vec::sqlite3_vec_init as *const ()
            )));
        }
    });
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::storage::keychain::InMemoryKeyProvider;
    use rusqlite::params;
    use std::path::PathBuf;
    use tempfile::TempDir;

    const MARKER: &str = "Confidential-Marker";

    fn key(b: u8) -> DbKey {
        DbKey::from_slice(&[b; 32]).unwrap()
    }

    fn db_path() -> (TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.db");
        (dir, path)
    }

    pub(crate) fn temp_db() -> (TempDir, Db) {
        let (dir, path) = db_path();
        let db = Db::open(&path, &key(1)).unwrap();
        (dir, db)
    }

    /// Inserts a document with one chunk (with vector) and one version row. Returns the doc id.
    fn insert_doc(db: &Db, rel_path: &str, text: &str) -> DocId {
        db.write(|c| {
            c.execute(
                "INSERT OR IGNORE INTO folders(path, added_at, status) VALUES ('/f', 0, 'active')",
                [],
            )?;
            c.execute(
                "INSERT INTO documents(folder_id, rel_path, ext, status)
                 SELECT id, ?1, 'txt', 'indexed' FROM folders WHERE path = '/f'",
                [rel_path],
            )?;
            let doc = c.last_insert_rowid();
            c.execute(
                "INSERT INTO doc_versions(doc_id, sha256, size, mtime, seen_at) VALUES (?1, x'00', 1, 1, 1)",
                [doc],
            )?;
            c.execute(
                "INSERT INTO chunks(doc_id, chunk_index, start_offset, end_offset, text)
                 VALUES (?1, 0, 0, ?2, ?3)",
                params![doc, text.len() as i64, text],
            )?;
            let chunk = c.last_insert_rowid();
            let embedding: Vec<u8> = (0..384).flat_map(|i| (i as f32).to_le_bytes()).collect();
            c.execute(
                "INSERT INTO vec_chunks(chunk_id, embedding) VALUES (?1, ?2)",
                params![chunk, embedding],
            )?;
            Ok(DocId(doc))
        })
        .unwrap()
    }

    fn count(db: &Db, sql: &str) -> i64 {
        db.read(|c| c.query_row(sql, [], |r| r.get(0))).unwrap()
    }

    #[test]
    fn same_key_reopens_other_key_fails() {
        let (_dir, path) = db_path();
        insert_doc(&Db::open(&path, &key(1)).unwrap(), "a.txt", MARKER);

        let db = Db::open(&path, &key(1)).unwrap();
        let text: String = db
            .read(|c| c.query_row("SELECT text FROM chunks", [], |r| r.get(0)))
            .unwrap();
        assert_eq!(text, MARKER);
        drop(db);

        assert!(matches!(
            Db::open(&path, &key(2)),
            Err(StorageError::WrongKey)
        ));
    }

    #[test]
    fn no_plaintext_in_db_wal_or_shm() {
        let (_dir, path) = db_path();
        let db = Db::open(&path, &key(1)).unwrap();
        insert_doc(&db, "a.txt", &format!("{MARKER} secret body"));
        let needles = [
            "confidential-marker",
            "confidential",
            "secret body",
            "sqlite format 3",
        ];
        let scan = || {
            for suffix in ["", "-wal", "-shm"] {
                let Ok(bytes) = std::fs::read(format!("{}{suffix}", path.display())) else {
                    continue;
                };
                let lower = bytes.to_ascii_lowercase();
                for n in needles {
                    assert!(
                        !lower.windows(n.len()).any(|w| w == n.as_bytes()),
                        "{n:?} found in db{suffix}"
                    );
                }
            }
        };
        // WAL not yet checkpointed: the chunk lives only in -wal here.
        assert!(
            std::fs::metadata(format!("{}-wal", path.display()))
                .unwrap()
                .len()
                > 0
        );
        scan();
        drop(db);
        scan();
    }

    #[test]
    fn connection_pragmas() {
        let (_dir, db) = temp_db();
        let w = |sql: &str| -> i64 { db.write(|c| c.query_row(sql, [], |r| r.get(0))).unwrap() };
        assert_eq!(w("PRAGMA foreign_keys"), 1);
        assert_eq!(w("PRAGMA temp_store"), 2);
        // SQLITE_TEMP_STORE=0 at compile time would force temp files whatever the pragma says.
        let opts: Vec<String> = db
            .read(|c| {
                c.prepare("PRAGMA compile_options")?
                    .query_map([], |r| r.get(0))?
                    .collect()
            })
            .unwrap();
        assert!(!opts.iter().any(|o| o == "TEMP_STORE=0"), "{opts:?}");
        assert_eq!(count(&db, "PRAGMA cache_size"), -CACHE_BUDGET_KIB);
        assert_eq!(
            count(&db, "PRAGMA soft_heap_limit"),
            CACHE_BUDGET_KIB * 1024
        );
        let mem: String = db
            .write(|c| c.query_row("PRAGMA cipher_memory_security", [], |r| r.get(0)))
            .unwrap();
        assert_eq!(mem, "1");
        let mode: String = db
            .read(|c| c.query_row("PRAGMA journal_mode", [], |r| r.get(0)))
            .unwrap();
        assert_eq!(mode, "wal");
        assert_eq!(count(&db, "PRAGMA foreign_keys"), 1);
        assert!(db.read(|c| c.execute("DELETE FROM settings", [])).is_err());
    }

    fn reader_id(db: &Db) -> usize {
        db.read(|c| Ok(std::ptr::from_ref(c) as usize)).unwrap()
    }

    #[test]
    fn sequential_reads_reuse_one_reader() {
        let (_dir, db) = temp_db();
        let first = reader_id(&db);
        for _ in 0..READERS * 2 {
            assert_eq!(reader_id(&db), first);
        }
    }

    #[test]
    fn busy_reader_does_not_block_others() {
        use std::sync::mpsc::channel;
        use std::time::Duration;
        let (_dir, db) = temp_db();
        let db = &db;
        let (release, held) = channel::<()>();
        let (entered, wait_entered) = channel();
        let (done, wait_done) = channel();
        std::thread::scope(|s| {
            s.spawn(move || {
                db.read(|_| {
                    entered.send(()).unwrap();
                    held.recv().unwrap();
                    Ok(())
                })
                .unwrap()
            });
            wait_entered.recv().unwrap();
            s.spawn(move || {
                // Round-robin would land on the held reader within READERS calls and wait.
                for _ in 0..READERS * 2 {
                    reader_id(db);
                }
                done.send(()).unwrap();
            });
            let r = wait_done.recv_timeout(Duration::from_secs(5));
            release.send(()).unwrap();
            assert!(r.is_ok(), "reads waited on the busy reader");
        });
    }

    #[test]
    fn migrations_run_once_and_newer_schema_is_rejected() {
        let (_dir, path) = db_path();
        let db = Db::open(&path, &key(1)).unwrap();
        assert_eq!(count(&db, "PRAGMA user_version"), MIGRATIONS.len() as i64);
        drop(db);
        let db = Db::open(&path, &key(1)).unwrap();
        db.write(|c| c.pragma_update(None, "user_version", 99))
            .unwrap();
        drop(db);
        assert!(matches!(
            Db::open(&path, &key(1)),
            Err(StorageError::SchemaTooNew(99))
        ));
    }

    #[test]
    fn delete_document_clears_chunks_vectors_fts_and_versions() {
        let (_dir, db) = temp_db();
        let gone = insert_doc(&db, "a.txt", "alpha");
        insert_doc(&db, "b.txt", "bravo");

        db.delete_document(gone).unwrap();

        assert_eq!(count(&db, "SELECT count(*) FROM documents"), 1);
        assert_eq!(count(&db, "SELECT count(*) FROM chunks"), 1);
        assert_eq!(count(&db, "SELECT count(*) FROM doc_versions"), 1);
        assert_eq!(count(&db, "SELECT count(*) FROM vec_chunks"), 1);
        let fts = |q: &str| {
            db.read(|c| {
                c.query_row(
                    "SELECT count(*) FROM chunks_fts WHERE chunks_fts MATCH ?1",
                    [q],
                    |r| r.get::<_, i64>(0),
                )
            })
            .unwrap()
        };
        assert_eq!(fts("alpha"), 0);
        assert_eq!(fts("bravo"), 1);
        db.write(|c| {
            c.execute(
                "INSERT INTO chunks_fts(chunks_fts, rank) VALUES ('integrity-check', 1)",
                [],
            )
        })
        .unwrap();
    }

    #[test]
    fn settings_round_trip() {
        let (_dir, db) = temp_db();
        assert_eq!(db.setting("k").unwrap(), None);
        db.set_setting("k", "1").unwrap();
        db.set_setting("k", "2").unwrap();
        assert_eq!(db.setting("k").unwrap().as_deref(), Some("2"));
    }

    #[test]
    fn first_run_creates_and_stores_key() {
        let (_dir, path) = db_path();
        let keys = InMemoryKeyProvider::default();
        insert_doc(&open_or_create(&path, &keys).unwrap(), "a.txt", "x");
        assert!(keys.load().unwrap().is_some());
        let db = open_or_create(&path, &keys).unwrap();
        assert_eq!(count(&db, "SELECT count(*) FROM documents"), 1);
    }

    #[test]
    fn stored_key_that_fails_is_an_error_not_a_new_key() {
        let (_dir, path) = db_path();
        drop(open_or_create(&path, &InMemoryKeyProvider::default()).unwrap());

        let other = InMemoryKeyProvider::default();
        other.store(&key(9)).unwrap();
        assert!(matches!(
            open_or_create(&path, &other),
            Err(StorageError::WrongKey)
        ));
        assert_eq!(other.load().unwrap().unwrap().as_bytes(), &[9; 32]);

        let empty = InMemoryKeyProvider::default();
        assert!(matches!(
            open_or_create(&path, &empty),
            Err(StorageError::KeyMissing)
        ));
        assert!(empty.load().unwrap().is_none());
    }
}
