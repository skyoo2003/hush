//! Folder walk -> what to index, reindex, and delete, compared with the DB by stat.
//! Reads only directory entries and metadata, never file content.

use std::collections::HashMap;
use std::path::Path;

use ignore::gitignore::{Gitignore, GitignoreBuilder};
use ignore::WalkBuilder;

use super::error::PipelineError;
use super::hash;
use super::parser::ParserRegistry;
use crate::storage::db::Db;

/// Always excluded, in gitignore syntax. A trailing `/` matches directories only.
pub const DEFAULT_EXCLUDES: &[&str] = &[
    "*.tmp",
    "~$*",
    ".DS_Store",
    "*.part",
    "node_modules/",
    ".git/",
    "target/",
    "build/",
    "dist/",
    "*.app/",
];

/// Settings key for the user's exclude patterns: gitignore lines, one per line.
pub const EXCLUDES_SETTING: &str = "exclude_patterns";

const MAX_DEPTH: usize = 32;

/// Relative paths under the folder root, ready to become index jobs.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct ScanPlan {
    /// Not in the DB, or never indexed (pending, error, or cleared by a model change).
    pub new: Vec<String>,
    /// Indexed, but the stat changed. The hash decides whether it really reindexes.
    pub reindex: Vec<String>,
    /// In the DB but not seen. Empty unless `complete`.
    pub delete: Vec<String>,
    pub unchanged: usize,
    /// Entries the walk could not read.
    pub failed: usize,
    /// No read errors, so every unseen document is really gone.
    pub complete: bool,
}

/// Walks `root` (canonical) without following symlinks, skipping hidden entries, excluded
/// patterns, and unsupported types, then diffs the files against `folder_id`'s documents.
/// Documents are only listed for deletion after a walk with no read errors, so an
/// unreadable subfolder never wipes its documents.
pub fn scan(
    db: &Db,
    registry: &ParserRegistry,
    folder_id: i64,
    root: &Path,
) -> Result<ScanPlan, PipelineError> {
    let excludes = filter(db, root)?.excludes;
    let mut known: HashMap<String, Option<hash::FileStat>> = db
        .folder_documents(folder_id)?
        .into_iter()
        .map(|(_, rel, stat)| (rel, stat))
        .collect();
    let mut plan = ScanPlan {
        complete: true,
        ..ScanPlan::default()
    };
    let walk = WalkBuilder::new(root)
        .standard_filters(false)
        .hidden(true)
        .follow_links(false)
        .max_depth(Some(MAX_DEPTH))
        .filter_entry(move |e| {
            let is_dir = e.file_type().is_some_and(|t| t.is_dir());
            !excludes.matched(e.path(), is_dir).is_ignore()
        })
        .build();
    for entry in walk {
        let mut fail = || {
            plan.failed += 1;
            plan.complete = false;
        };
        let Ok(entry) = entry else {
            fail();
            continue;
        };
        // Symlinks are not followed, so a link is never `is_file` here.
        if !entry.file_type().is_some_and(|t| t.is_file()) {
            continue;
        }
        if !supported(registry, entry.path()) {
            continue;
        }
        let rel = entry
            .path()
            .strip_prefix(root)
            .ok()
            .and_then(|r| r.to_str());
        let stat = entry.metadata().ok().and_then(|md| hash::stat_of(&md).ok());
        let (Some(rel), Some(stat)) = (rel, stat) else {
            fail();
            continue;
        };
        match known.remove(rel) {
            None | Some(None) => plan.new.push(rel.to_owned()),
            Some(Some(prev)) if prev == stat => plan.unchanged += 1,
            Some(Some(_)) => plan.reindex.push(rel.to_owned()),
        }
    }
    if plan.complete {
        plan.delete = known.into_keys().collect();
    }
    Ok(plan)
}

/// The rules `scan` applies to one path, for callers that see single paths (the watcher).
pub struct Filter {
    excludes: Gitignore,
}

/// Reads the user's exclude patterns for `root` (canonical).
pub fn filter(db: &Db, root: &Path) -> Result<Filter, PipelineError> {
    Ok(Filter {
        excludes: excludes(root, &db.setting(EXCLUDES_SETTING)?.unwrap_or_default())?,
    })
}

impl Filter {
    /// Whether `scan` would list `path`, a regular file under `root`: no hidden component,
    /// not excluded itself or through a parent directory, and a supported type.
    pub fn admits_file(&self, registry: &ParserRegistry, root: &Path, path: &Path) -> bool {
        self.visible(root, path, false) && supported(registry, path)
    }

    /// Whether `scan` would walk into `path`, a directory under `root`.
    pub fn admits_dir(&self, root: &Path, path: &Path) -> bool {
        self.visible(root, path, true)
    }

    fn visible(&self, root: &Path, path: &Path, is_dir: bool) -> bool {
        let Ok(rel) = path.strip_prefix(root) else {
            return false;
        };
        let hidden = rel
            .components()
            .any(|c| c.as_os_str().as_encoded_bytes().starts_with(b"."));
        !hidden
            && !self
                .excludes
                .matched_path_or_any_parents(path, is_dir)
                .is_ignore()
    }
}

fn supported(registry: &ParserRegistry, path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| registry.supports(&e.to_ascii_lowercase()))
}

/// Default excludes plus the user's lines. A user line that is not a valid glob is skipped,
/// so one typo never disables the defaults.
fn excludes(root: &Path, user: &str) -> Result<Gitignore, PipelineError> {
    let mut b = GitignoreBuilder::new(root);
    for line in DEFAULT_EXCLUDES {
        b.add_line(None, line).map_err(PipelineError::Exclude)?;
    }
    for line in user.lines() {
        let _ = b.add_line(None, line);
    }
    b.build().map_err(PipelineError::Exclude)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ingest::chunker::{CharApprox, ChunkerConfig};
    use crate::ingest::pipeline::{process_file, register_folder};
    use crate::rag::embedder::FakeEmbedder;
    use crate::storage::db::tests::temp_db;
    use std::os::unix::fs::symlink;

    fn sorted(mut v: Vec<String>) -> Vec<String> {
        v.sort();
        v
    }

    /// Every regular file under `root`, relative, with nothing filtered.
    fn walkdir_files(root: &Path) -> Vec<String> {
        WalkBuilder::new(root)
            .standard_filters(false)
            .build()
            .flatten()
            .filter(|e| e.file_type().is_some_and(|t| t.is_file()))
            .map(|e| {
                e.path()
                    .strip_prefix(root)
                    .unwrap()
                    .to_str()
                    .unwrap()
                    .to_owned()
            })
            .collect()
    }

    #[test]
    fn symlinks_are_never_followed() {
        let (_db_dir, db) = temp_db();
        let outside = tempfile::tempdir().unwrap();
        std::fs::write(outside.path().join("secret.txt"), "secret").unwrap();
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("Notes");
        std::fs::create_dir_all(root.join("a/b")).unwrap();
        std::fs::write(root.join("a/b/doc.txt"), "x").unwrap();
        // A cycle back to an ancestor, a link out of the folder, and a file link out of it.
        symlink(&root, root.join("a/b/loop")).unwrap();
        symlink(outside.path(), root.join("out")).unwrap();
        symlink(outside.path().join("secret.txt"), root.join("s.txt")).unwrap();
        let (folder, root) = register_folder(&db, &root).unwrap();

        let plan = scan(&db, &ParserRegistry::new(), folder, &root).unwrap();
        assert_eq!(plan.new, ["a/b/doc.txt"]);
        assert!(plan.complete);
    }

    #[test]
    fn default_and_user_excludes() {
        let (_db_dir, db) = temp_db();
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("Notes");
        for d in [
            "node_modules",
            "build",
            "sub/target",
            "My.app",
            "keep",
            "drafts",
        ] {
            std::fs::create_dir_all(root.join(d)).unwrap();
        }
        for f in [
            "a.txt",
            "x.tmp",
            "~$lock.docx",
            "dl.txt.part",
            ".hidden.txt",
            "node_modules/n.txt",
            "build/b.txt",
            "sub/target/t.txt",
            "My.app/m.txt",
            "keep/k.md",
            "drafts/d.txt",
            "keep/secret.md",
        ] {
            std::fs::write(root.join(f), "x").unwrap();
        }
        let (folder, root) = register_folder(&db, &root).unwrap();
        db.set_setting(EXCLUDES_SETTING, "drafts/\nsecret.*\n[invalid")
            .unwrap();

        let reg = ParserRegistry::new();
        let plan = scan(&db, &reg, folder, &root).unwrap();
        assert_eq!(sorted(plan.new), ["a.txt", "keep/k.md"]);
        // The watcher's filter agrees with the walk on every file.
        let filter = filter(&db, &root).unwrap();
        let mut admitted: Vec<String> = walkdir_files(&root)
            .into_iter()
            .filter(|p| filter.admits_file(&reg, &root, &root.join(p)))
            .collect();
        admitted.sort();
        assert_eq!(admitted, ["a.txt", "keep/k.md"]);
        assert!(filter.admits_dir(&root, &root.join("keep")));
        for d in ["drafts", "sub/target", "My.app", ".git"] {
            assert!(!filter.admits_dir(&root, &root.join(d)), "{d}");
        }
    }

    #[test]
    fn plan_diffs_against_the_db() {
        let (_db_dir, db) = temp_db();
        let dir = tempfile::tempdir().unwrap();
        for f in ["same.txt", "edit.txt", "gone.txt"] {
            std::fs::write(dir.path().join(f), "one two").unwrap();
        }
        let (folder, root) = register_folder(&db, dir.path()).unwrap();
        let reg = ParserRegistry::new();
        for f in ["same.txt", "edit.txt", "gone.txt"] {
            let cfg = ChunkerConfig::default();
            process_file(
                &db,
                &reg,
                folder,
                &root,
                f,
                &cfg,
                &CharApprox,
                &FakeEmbedder,
            )
            .unwrap();
        }
        std::fs::write(root.join("edit.txt"), "one two three").unwrap();
        std::fs::remove_file(root.join("gone.txt")).unwrap();
        std::fs::write(root.join("fresh.txt"), "x").unwrap();

        let plan = scan(&db, &reg, folder, &root).unwrap();
        assert_eq!(
            plan,
            ScanPlan {
                new: vec!["fresh.txt".into()],
                reindex: vec!["edit.txt".into()],
                delete: vec!["gone.txt".into()],
                unchanged: 1,
                failed: 0,
                complete: true,
            }
        );
    }
}
