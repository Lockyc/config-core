//! Leaf-agnostic structural edits to a config file. Each app builds its own field list (curator:
//! `title`/`url`/`session`/…; warden: `title`/`dir`/…) and calls `add_tab`; this module knows
//! nothing about what the fields mean. `set_key` sets one root key; `toggle_tab_digit_keys` is the
//! apps' Cycle Tabs menu toggle. Comment/format preserving — only the touched entry changes.

use crate::io::atomic_write;
use crate::model::TabDigitKeys;
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
    #[error("config has a `tab` entry that is not an array of tables")]
    MalformedTab,
}

/// Set root-table `key` to `v`, replacing its value or adding it above the first table. Atomic +
/// comment-preserving. A missing file is an `Io` error: the app seeds its starter config first, so
/// the starter's documentation is never replaced by a one-key file.
pub fn set_key(path: &Path, key: &str, v: Value) -> Result<(), EditError> {
    let src = std::fs::read_to_string(path)?;
    let mut doc: DocumentMut = src.parse()?;
    let has_root_keys = doc.iter().any(|(_, item)| item.is_value());
    match doc.get_mut(key).and_then(Item::as_value_mut) {
        // Keeps the entry's own decor (a trailing `# why` comment) by swapping only the value.
        Some(old) => {
            let decor = old.decor().clone();
            *old = v;
            *old.decor_mut() = decor;
        }
        None if has_root_keys => {
            doc.insert(key, value(v));
        }
        // The root's first key. toml_edit hangs the file's comments on whichever table renders
        // first (or on the document's trailer), so the line is placed in the text instead.
        None => {
            let mut line = DocumentMut::new();
            line.insert(key, value(v));
            let out = insert_first_key(&src, &line.to_string());
            out.parse::<DocumentMut>()?;
            atomic_write(path, &out)?;
            return Ok(());
        }
    }
    atomic_write(path, &doc.to_string())?;
    Ok(())
}

/// `src` (a file with no root keys) with `line` added after its header comment block — up to the
/// last blank line before the first table, so a comment directly above that table stays its own —
/// and set off from the table by a blank line.
fn insert_first_key(src: &str, line: &str) -> String {
    let lines: Vec<&str> = src.split_inclusive('\n').collect();
    let Some(first_table) = lines.iter().position(|l| l.trim_start().starts_with('[')) else {
        if src.trim().is_empty() {
            return line.to_string();
        }
        let sep = if src.ends_with('\n') { "\n" } else { "\n\n" };
        return format!("{src}{sep}{line}");
    };
    let head = &lines[..first_table];
    let split = head
        .iter()
        .rposition(|l| l.trim().is_empty())
        .map_or(0, |i| i + 1);
    format!(
        "{}{line}\n{}",
        lines[..split].concat(),
        lines[split..].concat()
    )
}

/// Flips `tab_digit_keys` from what the file says (absent is `jump`) and returns the new mode.
/// Reading the file, not the app's loaded config, keeps two clicks inside one reload from both
/// writing the same value.
pub fn toggle_tab_digit_keys(path: &Path) -> Result<TabDigitKeys, EditError> {
    let doc: DocumentMut = std::fs::read_to_string(path)?.parse()?;
    let now = match doc.get("tab_digit_keys").and_then(Item::as_str) {
        Some("cycle") => TabDigitKeys::Cycle,
        _ => TabDigitKeys::Jump,
    };
    let next = now.toggled();
    set_key(path, "tab_digit_keys", next.as_str().into())?;
    Ok(next)
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
            .ok_or(EditError::MalformedTab)?,
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
                .ok_or(EditError::MalformedTab)?
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
    fn set_key_replaces_a_root_key_keeping_comments() {
        let (_d, p) =
            write_tmp("# c\ntab_digit_keys = \"jump\" # why\n\n[[window]]\ntitle = \"a\"\n");
        set_key(&p, "tab_digit_keys", "cycle".into()).unwrap();
        let out = std::fs::read_to_string(&p).unwrap();
        assert!(out.starts_with("# c\n"), "{out}");
        assert!(out.contains("tab_digit_keys = \"cycle\""), "{out}");
        assert!(out.contains("[[window]]\ntitle = \"a\""), "{out}");
    }

    #[test]
    fn set_key_adds_a_missing_key_above_the_tables() {
        let (_d, p) = write_tmp("title = \"x\"\n\n[[window]]\ntitle = \"a\"\n");
        set_key(&p, "tab_digit_keys", "cycle".into()).unwrap();
        let out = std::fs::read_to_string(&p).unwrap();
        let key = out.find("tab_digit_keys").expect("key written");
        assert!(key < out.find("[[window]]").unwrap(), "{out}");
        let t: toml::Table = toml::from_str(&out).unwrap();
        assert_eq!(t["tab_digit_keys"].as_str(), Some("cycle"));
    }

    #[test]
    fn set_key_keeps_a_header_comment_above_a_new_first_key() {
        // With no root keys, toml_edit hangs the file's header comment on the first table.
        let (_d, p) = write_tmp("# header\n\n[[window]]\ntitle = \"a\"\n");
        set_key(&p, "tab_digit_keys", "cycle".into()).unwrap();
        let out = std::fs::read_to_string(&p).unwrap();
        assert_eq!(
            out,
            "# header\n\ntab_digit_keys = \"cycle\"\n\n[[window]]\ntitle = \"a\"\n"
        );
    }

    /// A new first root key goes after the file's header block and before the comment that
    /// belongs to the first table, whatever shape the file has.
    #[test]
    fn set_key_places_a_new_first_key_in_every_layout() {
        let cases = [
            // Comments only.
            ("# header\n# more\n", "# header\n# more\n\nk = 1\n"),
            // An implicit first table.
            (
                "# header\n\n[window.a]\nx = 1\n",
                "# header\n\nk = 1\n\n[window.a]\nx = 1\n",
            ),
            // The first table carries its own comment.
            (
                "# header\n\n# Work window\n[[window]]\ntitle = \"a\"\n",
                "# header\n\nk = 1\n\n# Work window\n[[window]]\ntitle = \"a\"\n",
            ),
            // No comments at all.
            (
                "[[window]]\ntitle = \"a\"\n",
                "k = 1\n\n[[window]]\ntitle = \"a\"\n",
            ),
            // Empty file.
            ("", "k = 1\n"),
        ];
        for (before, after) in cases {
            let (_d, p) = write_tmp(before);
            set_key(&p, "k", 1i64.into()).unwrap();
            assert_eq!(
                std::fs::read_to_string(&p).unwrap(),
                after,
                "from {before:?}"
            );
        }
    }

    #[test]
    fn toggle_flips_what_the_file_says() {
        let (_d, p) = write_tmp("tab_digit_keys = \"cycle\"\n");
        assert_eq!(toggle_tab_digit_keys(&p).unwrap(), TabDigitKeys::Jump);
        // Two quick toggles land back where they started, whatever the app has loaded.
        assert_eq!(toggle_tab_digit_keys(&p).unwrap(), TabDigitKeys::Cycle);
        let out = std::fs::read_to_string(&p).unwrap();
        assert_eq!(out, "tab_digit_keys = \"cycle\"\n");
        // Absent is the default, `jump`.
        let (_d, p) = write_tmp("# c\n");
        assert_eq!(toggle_tab_digit_keys(&p).unwrap(), TabDigitKeys::Cycle);
    }

    #[test]
    fn set_key_on_a_missing_file_is_not_found() {
        let dir = tempfile::tempdir().unwrap();
        match set_key(&dir.path().join("none.toml"), "k", true.into()) {
            Err(EditError::Io(e)) => assert_eq!(e.kind(), std::io::ErrorKind::NotFound),
            other => panic!("{other:?}"),
        }
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

    #[test]
    fn appends_tab_into_named_group() {
        let (_d, p) = write_tmp(
            "[[window]]\ntitle = \"Comms\"\n[[window.group]]\nname = \"Chat\"\n[[window.group.tab]]\ntitle = \"A\"\nurl = \"https://a.test/\"\n",
        );
        add_tab(
            &p,
            "Comms",
            Some("Chat"),
            &[("title", "B".into()), ("url", "https://b.test/".into())],
        )
        .unwrap();
        let out = std::fs::read_to_string(&p).unwrap();
        let cfg: DocumentMut = out.parse().unwrap();
        let tabs = cfg["window"][0]["group"][0]["tab"]
            .as_array_of_tables()
            .unwrap();
        assert_eq!(tabs.len(), 2);
        assert_eq!(tabs.get(1).unwrap()["title"].as_str(), Some("B"));
    }

    #[test]
    fn unknown_window_errors() {
        let (_d, p) = write_tmp("[[window]]\ntitle = \"Comms\"\n");
        let err = add_tab(
            &p,
            "Nope",
            None,
            &[("title", "X".into()), ("url", "https://x.test/".into())],
        )
        .unwrap_err();
        assert!(matches!(err, EditError::WindowNotFound(_)));
    }

    #[test]
    fn unknown_group_errors() {
        let (_d, p) = write_tmp("[[window]]\ntitle = \"Comms\"\n");
        let err = add_tab(
            &p,
            "Comms",
            Some("Ghost"),
            &[("title", "X".into()), ("url", "https://x.test/".into())],
        )
        .unwrap_err();
        assert!(matches!(err, EditError::GroupNotFound(_)));
    }

    #[test]
    fn preserves_surrounding_comments_and_field_order() {
        let (_d, p) = write_tmp(
            "# top comment\n[[window]]\ntitle = \"Comms\" # inline\n[[window.tab]]\ntitle = \"Existing\"\nurl = \"https://e.test/\"\n",
        );
        add_tab(
            &p,
            "Comms",
            None,
            &[
                ("title", "New".into()),
                ("url", "https://n.test/".into()),
                ("load_on_open", true.into()),
            ],
        )
        .unwrap();
        let out = std::fs::read_to_string(&p).unwrap();
        // Untouched content survives verbatim.
        assert!(out.contains("# top comment"));
        assert!(out.contains("title = \"Comms\" # inline"));
        assert!(out.contains("title = \"Existing\""));
        // New table present with fields in the given order (title before url before load_on_open).
        let ti = out.find("title = \"New\"").unwrap();
        let ui = out.find("url = \"https://n.test/\"").unwrap();
        let li = out.find("load_on_open = true").unwrap();
        assert!(ti < ui && ui < li);
    }

    #[test]
    fn non_array_tab_key_errors() {
        let (_d, p) = write_tmp("[[window]]\ntitle = \"W\"\ntab = \"oops\"\n");
        let err = add_tab(
            &p,
            "W",
            None,
            &[("title", "X".into()), ("url", "https://x.test/".into())],
        )
        .unwrap_err();
        assert!(matches!(err, EditError::MalformedTab));
    }
}
