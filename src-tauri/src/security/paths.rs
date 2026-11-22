//! Paths the app may read: inside a registered folder after every symlink is resolved.

use std::path::{Path, PathBuf};

use super::error::SecurityError;

/// A canonical path inside its root. No Debug: paths must not reach logs.
pub struct ScopedPath(PathBuf);

impl ScopedPath {
    /// `candidate` may be relative to `root` or absolute. Both must exist.
    pub fn new(root: &Path, candidate: &Path) -> Result<Self, SecurityError> {
        // shortcut: check-then-open, so a symlink swapped after this call is not caught; open with O_NOFOLLOW per component if that matters.
        let root = root.canonicalize().map_err(SecurityError::Unresolvable)?;
        // `join` keeps an absolute candidate as-is, so absolute paths take the same check.
        let path = root
            .join(candidate)
            .canonicalize()
            .map_err(SecurityError::Unresolvable)?;
        if path.starts_with(&root) {
            Ok(Self(path))
        } else {
            Err(SecurityError::OutsideScope)
        }
    }

    pub fn as_path(&self) -> &Path {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::os::unix::fs::symlink;

    /// `<tmp>/root/{a.txt, sub/b.txt}` and `<tmp>/outside/secret.txt`.
    fn tree() -> (tempfile::TempDir, PathBuf, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path().canonicalize().unwrap();
        let (root, outside) = (base.join("root"), base.join("outside"));
        fs::create_dir_all(root.join("sub")).unwrap();
        fs::create_dir_all(&outside).unwrap();
        fs::write(root.join("a.txt"), "a").unwrap();
        fs::write(root.join("sub/b.txt"), "b").unwrap();
        fs::write(outside.join("secret.txt"), "s").unwrap();
        (dir, root, outside)
    }

    fn outside_scope(r: Result<ScopedPath, SecurityError>) -> bool {
        matches!(r, Err(SecurityError::OutsideScope))
    }

    #[test]
    fn relative_inside_is_allowed() {
        let (_d, root, _) = tree();
        let p = ScopedPath::new(&root, Path::new("sub/../sub/b.txt")).unwrap();
        assert_eq!(p.as_path(), root.join("sub/b.txt"));
    }

    #[test]
    fn dot_dot_escape_is_rejected() {
        let (_d, root, _) = tree();
        assert!(outside_scope(ScopedPath::new(
            &root,
            Path::new("../outside/secret.txt")
        )));
        assert!(outside_scope(ScopedPath::new(
            &root,
            Path::new("sub/../../outside")
        )));
    }

    #[test]
    fn absolute_paths_are_checked_against_root() {
        let (_d, root, outside) = tree();
        assert!(outside_scope(ScopedPath::new(
            &root,
            &outside.join("secret.txt")
        )));
        assert!(outside_scope(ScopedPath::new(&root, Path::new("/etc"))));
        assert!(ScopedPath::new(&root, &root.join("a.txt")).is_ok());
    }

    #[test]
    fn sibling_with_root_as_prefix_is_rejected() {
        let (_d, root, _) = tree();
        let sibling = root.with_file_name("root2");
        fs::create_dir(&sibling).unwrap();
        assert!(outside_scope(ScopedPath::new(&root, &sibling)));
    }

    #[test]
    fn symlink_out_of_root_is_rejected() {
        let (_d, root, outside) = tree();
        symlink(outside.join("secret.txt"), root.join("file_link")).unwrap();
        symlink(&outside, root.join("dir_link")).unwrap();
        assert!(outside_scope(ScopedPath::new(
            &root,
            Path::new("file_link")
        )));
        assert!(outside_scope(ScopedPath::new(
            &root,
            Path::new("dir_link/secret.txt")
        )));
    }

    #[test]
    fn symlink_within_root_is_allowed() {
        let (_d, root, _) = tree();
        symlink(root.join("sub/b.txt"), root.join("inner_link")).unwrap();
        let p = ScopedPath::new(&root, Path::new("inner_link")).unwrap();
        assert_eq!(p.as_path(), root.join("sub/b.txt"));
    }

    #[test]
    fn symlink_cycle_and_missing_path_are_unresolvable() {
        let (_d, root, _) = tree();
        symlink(root.join("loop_b"), root.join("loop_a")).unwrap();
        symlink(root.join("loop_a"), root.join("loop_b")).unwrap();
        for c in ["loop_a", "missing.txt"] {
            assert!(matches!(
                ScopedPath::new(&root, Path::new(c)),
                Err(SecurityError::Unresolvable(_))
            ));
        }
    }

    #[test]
    fn root_that_is_a_symlink_is_resolved_first() {
        let (d, root, _) = tree();
        let alias = d.path().join("alias");
        symlink(&root, &alias).unwrap();
        let p = ScopedPath::new(&alias, Path::new("a.txt")).unwrap();
        assert_eq!(p.as_path(), root.join("a.txt"));
    }
}
