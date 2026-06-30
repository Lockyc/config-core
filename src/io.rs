//! Shared atomic, identity-preserving file write. Resolves symlinks so a linked config (e.g. one
//! symlinked from a dotfiles repo) is rewritten in place, and carries the target's mode onto the
//! temp file so `persist` doesn't leave the tempfile's owner-only 0600 default. Used by both the
//! formatter (`fmt::format_file`) and the editor (`edit::add_tab`).

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
