//! Shared atomic, identity-preserving file write (`atomic_write`) and atomic create (`atomic_create`).
//! `atomic_write` resolves symlinks so a linked config (e.g. one symlinked from a dotfiles repo) is
//! rewritten in place, and carries the target's mode onto the temp file so `persist` doesn't leave
//! the tempfile's owner-only 0600 default. Used by both the formatter (`fmt::format_file`) and the
//! editor (`edit::add_tab`).

use std::io::Write;
use std::path::Path;

pub(crate) fn atomic_write(path: &Path, contents: &str) -> std::io::Result<()> {
    let target = std::fs::canonicalize(path)?;
    let dir = target.parent().unwrap_or_else(|| Path::new("."));
    let mut tmp = tempfile::NamedTempFile::new_in(dir)?;
    tmp.write_all(contents.as_bytes())?;
    tmp.flush()?;
    if let Ok(meta) = std::fs::metadata(&target) {
        let _ = tmp.as_file().set_permissions(meta.permissions());
    }
    tmp.persist(&target).map_err(|e| e.error)?;
    Ok(())
}

/// Atomically create `path`, creating parent dirs as needed. Unlike [`atomic_write`], the target
/// need not exist — so the **parent** is canonicalized instead of the target, keeping the write
/// inside a symlinked config dir (a `~/.config` linked out of a dotfiles repo) rather than landing
/// beside the link.
///
/// A new file gets the tempfile's default mode; there is no existing file whose permissions could
/// be carried over. Callers wanting create-if-absent semantics should check existence first — this
/// function overwrites.
pub(crate) fn atomic_create(path: &Path, contents: &str) -> std::io::Result<()> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(parent)?;
    let dir = std::fs::canonicalize(parent)?;
    let name = path.file_name().ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::InvalidInput, "path has no file name")
    })?;
    let mut tmp = tempfile::NamedTempFile::new_in(&dir)?;
    tmp.write_all(contents.as_bytes())?;
    tmp.flush()?;
    tmp.persist(dir.join(name)).map_err(|e| e.error)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn atomic_create_writes_a_new_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("new.toml");
        atomic_create(&path, "a = 1\n").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "a = 1\n");
    }

    #[test]
    fn atomic_create_creates_missing_parent_dirs() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("deep").join("nested").join("c.toml");
        atomic_create(&path, "x = 1\n").unwrap();
        assert!(path.is_file());
    }

    #[cfg(unix)]
    #[test]
    fn atomic_create_lands_through_a_symlinked_parent() {
        // A ~/.config symlinked out of a dotfiles repo must still receive the real file, not have
        // the link replaced. atomic_write canonicalizes the target; for a file that doesn't exist
        // yet, only the parent can be canonicalized — this pins that it is.
        let dir = tempfile::tempdir().unwrap();
        let real = dir.path().join("real-config-dir");
        std::fs::create_dir(&real).unwrap();
        let link = dir.path().join("linked");
        std::os::unix::fs::symlink(&real, &link).unwrap();

        atomic_create(&link.join("c.toml"), "y = 2\n").unwrap();

        // The file must exist at the REAL location, reached through the link.
        assert_eq!(
            std::fs::read_to_string(real.join("c.toml")).unwrap(),
            "y = 2\n"
        );
    }

    #[test]
    fn atomic_write_still_refuses_a_nonexistent_target() {
        // Guards the boundary between the two functions: atomic_write REQUIRES pre-existence
        // (it canonicalizes the target), which is what keeps a format/edit from silently creating
        // a file where an existing one was meant. Don't "fix" this by relaxing it — that is what
        // atomic_create is for.
        let dir = tempfile::tempdir().unwrap();
        let err = atomic_write(&dir.path().join("nope.toml"), "a = 1\n").unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::NotFound);
    }
}
