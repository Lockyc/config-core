//! Write a starter config when an app has none.
//!
//! **Never fires automatically.** There is no launch hook and no first-run marker — an app calls
//! this only when the user asks (a menu item, or the home surface's button). Nothing is written to
//! a home directory without a click.
//!
//! The template is the app's own (its leaf schema is not this crate's business); the mechanism —
//! where, atomically, never clobbering — is shared.

use crate::io::atomic_create;
use std::path::Path;

#[derive(Debug, thiserror::Error)]
pub enum SeedError {
    #[error("cannot write config: {0}")]
    Io(#[from] std::io::Error),
}

/// Write `template` to `path` if nothing is there. Creates parent dirs. **Never clobbers.**
///
/// `Ok(true)` = written. `Ok(false)` = a file already existed and was left untouched — the caller
/// should say so rather than report success, since the user asked for a new config and got their
/// old one.
pub fn write_default_config(path: &Path, template: &str) -> Result<bool, SeedError> {
    if path.exists() {
        return Ok(false);
    }
    atomic_create(path, template)?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_the_template_when_nothing_is_there() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        assert!(write_default_config(&path, "dark_mode = true\n").unwrap());
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "dark_mode = true\n"
        );
    }

    #[test]
    fn never_clobbers_an_existing_config() {
        // The whole point: a user's real config must survive a stray click on "Create a starter
        // config". Returning Ok(false) lets the caller say so rather than silently doing nothing.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(&path, "mine = true\n").unwrap();

        assert!(!write_default_config(&path, "dark_mode = true\n").unwrap());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "mine = true\n");
    }

    #[test]
    fn creates_the_parent_directory() {
        // ~/.config/<app>/ won't exist on a fresh machine.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("lector").join("config.toml");
        assert!(write_default_config(&path, "x = 1\n").unwrap());
        assert!(path.is_file());
    }

    #[test]
    fn the_written_template_is_valid_toml() {
        // A template that doesn't parse would seed a config the app then refuses to load —
        // the exact stranding this feature exists to end.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        let template = "dark_mode = true\n\n[[window]]\n  title = \"Docs\"\n";
        write_default_config(&path, template).unwrap();
        let src = std::fs::read_to_string(&path).unwrap();
        assert!(src.parse::<toml_edit::DocumentMut>().is_ok());
    }
}
