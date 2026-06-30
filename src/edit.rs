//! Leaf-agnostic structural edits to a config file. Each app builds its own field list (curator:
//! `title`/`url`/`session`/…; warden: `title`/`dir`/…) and calls `add_tab`; this module knows
//! nothing about what the fields mean. Comment/format preserving — only the new table is added.

use crate::io::atomic_write;
use std::path::Path;
use toml_edit::{value, ArrayOfTables, DocumentMut, Item, Table, Value};

#[derive(Debug, thiserror::Error)]
pub enum EditError {
    #[error("cannot read/write config: {0}")]
    Io(#[from] std::io::Error),
    #[error("invalid TOML: {0}")]
    Toml(#[from] toml_edit::TomlError),
    #[error("no window titled {0:?}")]
    WindowNotFound(String),
    #[error("window has no group named {0:?}")]
    GroupNotFound(String),
}

/// Append a `[[window.tab]]` (group `None`) or `[[window.group.tab]]` (group `Some(name)`) table
/// to the window whose `title` matches `window_title`, populated with `fields` in order. Atomic +
/// comment-preserving. Errors if the window (or named group) doesn't exist.
pub fn add_tab(
    path: &Path,
    window_title: &str,
    group: Option<&str>,
    fields: &[(&str, Value)],
) -> Result<(), EditError> {
    let src = std::fs::read_to_string(path)?;
    let mut doc: DocumentMut = src.parse()?;

    let windows = doc
        .get_mut("window")
        .and_then(Item::as_array_of_tables_mut)
        .ok_or_else(|| EditError::WindowNotFound(window_title.to_string()))?;
    let win = windows
        .iter_mut()
        .find(|t| t.get("title").and_then(|v| v.as_str()) == Some(window_title))
        .ok_or_else(|| EditError::WindowNotFound(window_title.to_string()))?;

    // Build the new tab table from the ordered fields (leaf-agnostic).
    let mut tab = Table::new();
    for (k, v) in fields {
        tab.insert(k, value(v.clone()));
    }

    let tabs = match group {
        None => win
            .entry("tab")
            .or_insert(Item::ArrayOfTables(ArrayOfTables::new()))
            .as_array_of_tables_mut()
            .expect("`tab` is an array of tables"),
        Some(g) => {
            let groups = win
                .get_mut("group")
                .and_then(Item::as_array_of_tables_mut)
                .ok_or_else(|| EditError::GroupNotFound(g.to_string()))?;
            let grp = groups
                .iter_mut()
                .find(|t| t.get("name").and_then(|v| v.as_str()) == Some(g))
                .ok_or_else(|| EditError::GroupNotFound(g.to_string()))?;
            grp.entry("tab")
                .or_insert(Item::ArrayOfTables(ArrayOfTables::new()))
                .as_array_of_tables_mut()
                .expect("`tab` is an array of tables")
        }
    };
    tabs.push(tab);

    atomic_write(path, &doc.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn write_tmp(body: &str) -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("config.toml");
        let mut f = std::fs::File::create(&p).unwrap();
        f.write_all(body.as_bytes()).unwrap();
        (dir, p)
    }

    #[test]
    fn appends_loose_tab_to_named_window() {
        let (_d, p) = write_tmp("[[window]]\ntitle = \"Comms\"\n");
        add_tab(
            &p,
            "Comms",
            None,
            &[
                ("title", "Gmail".into()),
                ("url", "https://mail.google.com/".into()),
            ],
        )
        .unwrap();
        let out = std::fs::read_to_string(&p).unwrap();
        let cfg: DocumentMut = out.parse().unwrap();
        let tabs = cfg["window"][0]["tab"].as_array_of_tables().unwrap();
        assert_eq!(tabs.len(), 1);
        assert_eq!(tabs.get(0).unwrap()["title"].as_str(), Some("Gmail"));
        assert_eq!(
            tabs.get(0).unwrap()["url"].as_str(),
            Some("https://mail.google.com/")
        );
    }
}
