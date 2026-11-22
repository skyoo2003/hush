//! Change detection: stat first, hash only when the stat changed.

use std::fs::File;
use std::io::Read;
use std::time::UNIX_EPOCH;

use sha2::{Digest, Sha256};

use crate::security::paths::ScopedPath;

const BLOCK: usize = 64 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FileStat {
    pub size: u64,
    /// Nanoseconds since the Unix epoch, negative before it.
    pub mtime_ns: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FileState {
    pub stat: FileStat,
    pub sha256: [u8; 32],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    Skip,
    Reindex(FileState),
    New(FileState),
}

pub fn stat(path: &ScopedPath) -> std::io::Result<FileStat> {
    let md = std::fs::metadata(path.as_path())?;
    let ns = |d: std::time::Duration| i64::try_from(d.as_nanos()).unwrap_or(i64::MAX);
    let mtime_ns = match md.modified()?.duration_since(UNIX_EPOCH) {
        Ok(d) => ns(d),
        Err(e) => -ns(e.duration()),
    };
    Ok(FileStat {
        size: md.len(),
        mtime_ns,
    })
}

pub fn sha256_file(path: &ScopedPath) -> std::io::Result<[u8; 32]> {
    let mut file = File::open(path.as_path())?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0; BLOCK];
    loop {
        match file.read(&mut buf) {
            Ok(0) => return Ok(hasher.finalize().into()),
            Ok(n) => hasher.update(&buf[..n]),
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
            Err(e) => return Err(e),
        }
    }
}

/// An unchanged stat skips without hashing; a changed stat with the same hash also skips.
pub fn decide(
    prev: Option<&FileState>,
    cur: FileStat,
    hash: impl FnOnce() -> std::io::Result<[u8; 32]>,
) -> std::io::Result<Decision> {
    match prev {
        Some(p) if p.stat == cur => Ok(Decision::Skip),
        Some(p) => {
            let sha256 = hash()?;
            Ok(if sha256 == p.sha256 {
                Decision::Skip
            } else {
                Decision::Reindex(FileState { stat: cur, sha256 })
            })
        }
        None => Ok(Decision::New(FileState {
            stat: cur,
            sha256: hash()?,
        })),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    const STAT: FileStat = FileStat {
        size: 3,
        mtime_ns: 10,
    };
    const STATE: FileState = FileState {
        stat: STAT,
        sha256: [1; 32],
    };

    #[test]
    fn equal_stat_skips_without_hashing() {
        let d = decide(Some(&STATE), STAT, || panic!("hashed")).unwrap();
        assert_eq!(d, Decision::Skip);
    }

    #[test]
    fn changed_stat_hashes() {
        let cur = FileStat {
            mtime_ns: 11,
            ..STAT
        };
        assert_eq!(
            decide(Some(&STATE), cur, || Ok([1; 32])).unwrap(),
            Decision::Skip
        );
        let new = FileState {
            stat: cur,
            sha256: [2; 32],
        };
        assert_eq!(
            decide(Some(&STATE), cur, || Ok([2; 32])).unwrap(),
            Decision::Reindex(new)
        );
        assert_eq!(
            decide(None, cur, || Ok([2; 32])).unwrap(),
            Decision::New(new)
        );
    }

    #[test]
    fn hashes_and_stats_a_file() {
        let dir = tempfile::tempdir().unwrap();
        // Larger than one block, so the loop runs more than once.
        let bytes = vec![7u8; BLOCK * 2 + 5];
        std::fs::write(dir.path().join("a.bin"), &bytes).unwrap();
        let p = ScopedPath::new(dir.path(), Path::new("a.bin")).unwrap();
        assert_eq!(
            sha256_file(&p).unwrap(),
            <[u8; 32]>::from(Sha256::digest(&bytes))
        );
        assert_eq!(stat(&p).unwrap().size, bytes.len() as u64);
    }
}
