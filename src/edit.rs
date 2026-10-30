//! Leaf-agnostic structural edits to a config file. Each app builds its own group/tab field list
//! (curator: `title`/`url`/`session`/…; warden: `title`/`dir`/…) and calls these; this module knows
//! nothing about what the fields mean. A window's shared `title`/`colour` have their own
//! `add_window`/`edit_window`. Add/remove/set/move edit windows, groups and tabs (addressed by an
//! `At`); `set_key` / `remove_key` set or remove one root key; `toggle_tab_digit_keys` is the apps' Cycle Tabs menu toggle.
//! Comment/format preserving — only the addressed entry changes. Every call re-reads the file, so
//! an external edit between two calls is never overwritten.

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
    #[error("no tab with that key/value: {0}")]
    TabNotFound(String),
    #[error("a window titled {0:?} already exists")]
    WindowExists(String),
    #[error("{0}")]
    BadColour(#[from] crate::ColourError),
    #[error("config has a `{0}` entry that is not an array of tables")]
    Malformed(&'static str),
}

/// Address of a window, group or tab table. Windows and groups match on their trimmed `title` /
/// `name`; `tab` is a `(key, value)` string match inside the addressed tab array — the window's
/// loose `tab` array when `group` is `None`, else that group's.
#[derive(Debug, Clone, Copy)]
pub struct At<'a> {
    pub window: &'a str,
    pub group: Option<&'a str>,
    pub tab: Option<(&'a str, &'a str)>,
}

fn matches_trimmed(t: &Table, key: &str, want: &str) -> bool {
    t.get(key)
        .and_then(Item::as_str)
        .is_some_and(|s| s.trim() == want.trim())
}

fn window_index(doc: &DocumentMut, title: &str) -> Result<usize, EditError> {
    doc.get("window")
        .and_then(Item::as_array_of_tables)
        .and_then(|ws| ws.iter().position(|t| matches_trimmed(t, "title", title)))
        .ok_or_else(|| EditError::WindowNotFound(title.to_string()))
}

fn window_mut<'d>(doc: &'d mut DocumentMut, title: &str) -> Result<&'d mut Table, EditError> {
    let i = window_index(doc, title)?;
    Ok(doc["window"]
        .as_array_of_tables_mut()
        .and_then(|ws| ws.get_mut(i))
        .expect("index just found"))
}

fn group_index(win: &Table, name: &str) -> Result<usize, EditError> {
    win.get("group")
        .and_then(Item::as_array_of_tables)
        .and_then(|gs| gs.iter().position(|t| matches_trimmed(t, "name", name)))
        .ok_or_else(|| EditError::GroupNotFound(name.to_string()))
}

fn group_mut<'d>(win: &'d mut Table, name: &str) -> Result<&'d mut Table, EditError> {
    let i = group_index(win, name)?;
    Ok(win["group"]
        .as_array_of_tables_mut()
        .and_then(|gs| gs.get_mut(i))
        .expect("index just found"))
}

/// The table that owns the addressed tab array: the window, or one of its groups.
fn tab_owner_mut<'d>(
    doc: &'d mut DocumentMut,
    window: &str,
    group: Option<&str>,
) -> Result<&'d mut Table, EditError> {
    let win = window_mut(doc, window)?;
    match group {
        None => Ok(win),
        Some(g) => group_mut(win, g),
    }
}

fn tab_index(owner: &Table, key: &str, val: &str) -> Result<usize, EditError> {
    match owner.get("tab") {
        None => Err(EditError::TabNotFound(format!("{key} = {val:?}"))),
        Some(item) => item
            .as_array_of_tables()
            .ok_or(EditError::Malformed("tab"))?
            .iter()
            .position(|t| t.get(key).and_then(Item::as_str) == Some(val))
            .ok_or_else(|| EditError::TabNotFound(format!("{key} = {val:?}"))),
    }
}

fn tabs_or_create(owner: &mut Table) -> Result<&mut ArrayOfTables, EditError> {
    owner
        .entry("tab")
        .or_insert(Item::ArrayOfTables(ArrayOfTables::new()))
        .as_array_of_tables_mut()
        .ok_or(EditError::Malformed("tab"))
}

fn table_from(fields: &[(&str, Value)]) -> Table {
    let mut t = Table::new();
    for (k, v) in fields {
        t.insert(k, value(v.clone()));
    }
    t
}

fn load(path: &Path) -> Result<DocumentMut, EditError> {
    Ok(std::fs::read_to_string(path)?.parse()?)
}

fn save(path: &Path, doc: &DocumentMut) -> Result<(), EditError> {
    #[cfg(test)]
    tests::SAVES.with(|n| n.set(n.get() + 1));
    atomic_write(path, &doc.to_string())?;
    Ok(())
}

/// Remove root-table `key`; whether it was there. Atomic + comment-preserving: the comment directly
/// above the key goes with it, but a block set off from it by a blank line — the file's header, when
/// `key` comes first — stays, above whatever comes first after it.
pub fn remove_key(path: &Path, key: &str) -> Result<bool, EditError> {
    let mut doc = load(path)?;
    let first = doc.iter().next().is_some_and(|(k, _)| k == key);
    let prefix = doc
        .as_table()
        .key(key)
        .and_then(|k| k.leaf_decor().prefix())
        .and_then(|p| p.as_str())
        .map(str::to_string);
    if doc.remove(key).is_none() {
        return Ok(false);
    }
    let header = prefix
        .as_deref()
        .filter(|_| first)
        .and_then(|p| p.rfind("\n\n").map(|i| p[..i].trim_end()))
        .filter(|h| !h.is_empty());
    if let Some(header) = header {
        prepend_to_first(&mut doc, &format!("{header}\n\n"));
    }
    save(path, &doc)?;
    Ok(true)
}

/// Puts `text` (comment lines ending in one blank line) in front of whatever `doc` now starts
/// with: a root key, or the first table. The blank lines that set that item off go, so `text`'s
/// own blank line is the only one between them.
fn prepend_to_first(doc: &mut DocumentMut, text: &str) {
    let Some((mut key, item)) = doc.as_table_mut().iter_mut().next() else {
        let trailing = doc.trailing().as_str().unwrap_or("").to_string();
        doc.set_trailing(format!("{text}{}", trailing.trim_start()));
        return;
    };
    let joined = |own: Option<&str>| format!("{text}{}", own.unwrap_or("").trim_start());
    match item {
        Item::Value(_) => {
            let own = key
                .leaf_decor()
                .prefix()
                .and_then(|p| p.as_str())
                .map(str::to_string);
            key.leaf_decor_mut().set_prefix(joined(own.as_deref()));
        }
        Item::Table(t) => {
            let own = t
                .decor()
                .prefix()
                .and_then(|p| p.as_str())
                .map(str::to_string);
            t.decor_mut().set_prefix(joined(own.as_deref()));
        }
        Item::ArrayOfTables(a) => {
            if let Some(t) = a.get_mut(0) {
                let own = t
                    .decor()
                    .prefix()
                    .and_then(|p| p.as_str())
                    .map(str::to_string);
                t.decor_mut().set_prefix(joined(own.as_deref()));
            }
        }
        Item::None => {}
    }
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
    save(path, &doc)
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
    let mut doc = load(path)?;
    let owner = tab_owner_mut(&mut doc, window_title, group)?;
    tabs_or_create(owner)?.push(table_from(fields));
    save(path, &doc)
}

/// Append a `[[window]]` titled `title` (trimmed), with `colour` when given. Errors
/// `WindowExists` when the title is already taken, `BadColour` for a colour
/// [`Colour::parse`](crate::Colour::parse) refuses.
pub fn add_window(path: &Path, title: &str, colour: Option<&str>) -> Result<(), EditError> {
    let title = title.trim();
    colour.map(crate::Colour::parse).transpose()?;
    let mut doc = load(path)?;
    if window_index(&doc, title).is_ok() {
        return Err(EditError::WindowExists(title.to_string()));
    }
    let mut fields = vec![("title", Value::from(title))];
    fields.extend(colour.map(|c| ("colour", Value::from(c))));
    let new = table_from(&fields);
    doc.entry("window")
        .or_insert(Item::ArrayOfTables(ArrayOfTables::new()))
        .as_array_of_tables_mut()
        .ok_or(EditError::Malformed("window"))?
        .push(new);
    save(path, &doc)
}

/// Change the window titled `title`: `colour` (`None` keeps it, `Some(None)` clears it), then
/// `new_title`, in one write. The colour goes first, so it is addressed while the window still
/// carries `title`. Nothing to change is a no-op. Errors as [`add_window`] does for a `new_title`
/// another window holds or a bad colour.
pub fn edit_window(
    path: &Path,
    title: &str,
    new_title: Option<&str>,
    colour: Option<Option<&str>>,
) -> Result<(), EditError> {
    let at = At {
        window: title,
        group: None,
        tab: None,
    };
    if colour.is_none() && new_title.is_none() {
        return Ok(());
    }
    colour.flatten().map(crate::Colour::parse).transpose()?;
    let mut doc = load(path)?;
    if let Some(new) = new_title.map(str::trim) {
        if window_index(&doc, new).is_ok_and(|i| window_index(&doc, title).ok() != Some(i)) {
            return Err(EditError::WindowExists(new.to_string()));
        }
    }
    if let Some(colour) = colour {
        apply_field(&mut doc, at, "colour", colour.map(Value::from))?;
    }
    if let Some(new_title) = new_title {
        apply_field(&mut doc, at, "title", Some(Value::from(new_title.trim())))?;
    }
    save(path, &doc)
}

/// Append a `[[window.group]]` populated with `fields` to the named window.
pub fn add_group(path: &Path, window: &str, fields: &[(&str, Value)]) -> Result<(), EditError> {
    let mut doc = load(path)?;
    let win = window_mut(&mut doc, window)?;
    win.entry("group")
        .or_insert(Item::ArrayOfTables(ArrayOfTables::new()))
        .as_array_of_tables_mut()
        .ok_or(EditError::Malformed("group"))?
        .push(table_from(fields));
    save(path, &doc)
}

/// Remove the addressed tab, else group (with its tabs), else window (with everything in it).
pub fn remove(path: &Path, at: At) -> Result<(), EditError> {
    let mut doc = load(path)?;
    match (at.group, at.tab) {
        (g, Some((k, v))) => {
            let owner = tab_owner_mut(&mut doc, at.window, g)?;
            let i = tab_index(owner, k, v)?;
            let tabs = owner["tab"].as_array_of_tables_mut().expect("checked");
            tabs.remove(i);
            if tabs.is_empty() {
                owner.remove("tab");
            }
        }
        (Some(g), None) => {
            let win = window_mut(&mut doc, at.window)?;
            let i = group_index(win, g)?;
            let groups = win["group"].as_array_of_tables_mut().expect("checked");
            groups.remove(i);
            if groups.is_empty() {
                win.remove("group");
            }
        }
        (None, None) => {
            let i = window_index(&doc, at.window)?;
            let ws = doc["window"].as_array_of_tables_mut().expect("checked");
            ws.remove(i);
            if ws.is_empty() {
                doc.remove("window");
            }
        }
    }
    save(path, &doc)
}

/// Set `key` on the addressed table (tab, else group, else window); `None` removes it. An existing
/// value keeps its surrounding whitespace and inline comment. Setting a window's `title` renames it.
pub fn set_field(path: &Path, at: At, key: &str, new: Option<Value>) -> Result<(), EditError> {
    let mut doc = load(path)?;
    apply_field(&mut doc, at, key, new)?;
    save(path, &doc)
}

/// `set_field`'s change on an already-loaded document, so several fields can share one write.
fn apply_field(
    doc: &mut DocumentMut,
    at: At,
    key: &str,
    new: Option<Value>,
) -> Result<(), EditError> {
    let target: &mut Table = match (at.group, at.tab) {
        (g, Some((k, v))) => {
            let owner = tab_owner_mut(doc, at.window, g)?;
            let i = tab_index(owner, k, v)?;
            owner["tab"]
                .as_array_of_tables_mut()
                .and_then(|a| a.get_mut(i))
                .expect("checked")
        }
        (Some(g), None) => group_mut(window_mut(doc, at.window)?, g)?,
        (None, None) => window_mut(doc, at.window)?,
    };
    match new {
        None => {
            target.remove(key);
        }
        Some(mut v) => {
            if let Some(old) = target.get(key).and_then(Item::as_value) {
                *v.decor_mut() = old.decor().clone();
            }
            target.insert(key, Item::Value(v));
        }
    }
    Ok(())
}

/// Move the tab addressed by `from` to the end of the named window's group `to_group` (`None`: the
/// window's loose `tab` array). The table travels whole, comments included.
pub fn move_tab(
    path: &Path,
    from: At,
    to_window: &str,
    to_group: Option<&str>,
) -> Result<(), EditError> {
    let (k, v) = from
        .tab
        .ok_or_else(|| EditError::TabNotFound("no tab addressed".to_string()))?;
    let mut doc = load(path)?;
    // Resolve the destination first so a bad one leaves the source untouched.
    tab_owner_mut(&mut doc, to_window, to_group)?;
    let table = {
        let owner = tab_owner_mut(&mut doc, from.window, from.group)?;
        let i = tab_index(owner, k, v)?;
        let tabs = owner["tab"].as_array_of_tables_mut().expect("checked");
        let t = tabs.remove(i);
        if tabs.is_empty() {
            owner.remove("tab");
        }
        t
    };
    let mut table = table;
    // A parsed table remembers its old document position and would print there, under its old
    // parent; unpositioned, it prints with its new parent like any freshly pushed table.
    table.set_position(None);
    let dest = tab_owner_mut(&mut doc, to_window, to_group)?;
    tabs_or_create(dest)?.push(table);
    save(path, &doc)
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
    fn remove_key_drops_a_root_key_and_its_own_comment_but_keeps_the_header() {
        let (_d, p) = write_tmp(
            "# header\n\n# The window's name.\ntitle = \"Club\"\n\n# Its accent.\ncolour = \"#2a6f4e\"\n\n\
             # Sizing.\ndensity = \"compact\"\n\n[[window]]\ntitle = \"a\"\n",
        );
        assert!(remove_key(&p, "title").unwrap());
        assert!(remove_key(&p, "colour").unwrap());
        assert!(!remove_key(&p, "colour").unwrap(), "already gone");
        assert_eq!(
            std::fs::read_to_string(&p).unwrap(),
            "# header\n\n# Sizing.\ndensity = \"compact\"\n\n[[window]]\ntitle = \"a\"\n"
        );
    }

    #[test]
    fn remove_key_keeps_the_header_when_only_tables_follow() {
        let (_d, p) = write_tmp("# header\n\ntitle = \"x\"\n\n[[window]]\ntitle = \"a\"\n");
        assert!(remove_key(&p, "title").unwrap());
        assert_eq!(
            std::fs::read_to_string(&p).unwrap(),
            "# header\n\n[[window]]\ntitle = \"a\"\n"
        );
    }

    #[cfg(unix)]
    #[test]
    fn remove_key_keeps_the_file_mode_and_a_symlink() {
        use std::os::unix::fs::PermissionsExt;
        let (d, real) = write_tmp("a = 1\nb = 2\n");
        std::fs::set_permissions(&real, std::fs::Permissions::from_mode(0o600)).unwrap();
        let link = d.path().join("link.toml");
        std::os::unix::fs::symlink(&real, &link).unwrap();
        assert!(remove_key(&link, "a").unwrap());
        assert!(std::fs::symlink_metadata(&link)
            .unwrap()
            .file_type()
            .is_symlink());
        let mode = std::fs::metadata(&real).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
        assert_eq!(std::fs::read_to_string(&real).unwrap().trim(), "b = 2");
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
        assert!(matches!(err, EditError::Malformed("tab")));
    }

    const SAMPLE: &str = "# top\n[[window]]\ntitle = \"Comms\" # inline\n\n# loose a\n[[window.tab]]\ntitle = \"A\"\nurl = \"https://a.test/\"\n\n# loose b\n[[window.tab]]\ntitle = \"B\"\nurl = \"https://b.test/\"\n\n[[window.group]]\nname = \"Chat\"\n\n# grouped c\n[[window.group.tab]]\ntitle = \"C\"\nurl = \"https://c.test/\"\n\n[[window.group]]\nname = \"Other\"\n\n[[window]]\ntitle = \"Second\"\n";

    fn at<'a>(window: &'a str, group: Option<&'a str>, tab: Option<(&'a str, &'a str)>) -> At<'a> {
        At { window, group, tab }
    }

    fn parse(p: &std::path::Path) -> DocumentMut {
        std::fs::read_to_string(p).unwrap().parse().unwrap()
    }

    #[test]
    fn add_window_appends_and_refuses_a_taken_title() {
        let (_d, p) = write_tmp("# keep\n[[window]]\ntitle = \"Comms\"\n");
        add_window(&p, " New ", None).unwrap();
        let doc = parse(&p);
        assert_eq!(doc["window"].as_array_of_tables().unwrap().len(), 2);
        assert_eq!(doc["window"][1]["title"].as_str(), Some("New"));
        assert!(doc["window"][1].get("colour").is_none());
        assert!(std::fs::read_to_string(&p).unwrap().contains("# keep"));
        let err = add_window(&p, " Comms ", None).unwrap_err();
        assert!(matches!(err, EditError::WindowExists(_)));
        // First window into a config that has none.
        let (_d2, p2) = write_tmp("# empty\n");
        add_window(&p2, "One", Some("#abc")).unwrap();
        assert_eq!(parse(&p2)["window"][0]["title"].as_str(), Some("One"));
        assert_eq!(parse(&p2)["window"][0]["colour"].as_str(), Some("#abc"));
    }

    thread_local! {
        pub(super) static SAVES: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
    }

    #[test]
    fn edit_window_writes_the_file_once() {
        let (_d, p) = write_tmp("[[window]]\ntitle = \"Work\"\ncolour = \"#abc\"\n");
        let before = SAVES.with(|n| n.get());
        edit_window(&p, "Work", Some("Job"), Some(Some("#0f0"))).unwrap();
        assert_eq!(SAVES.with(|n| n.get()) - before, 1);
        let doc = parse(&p);
        assert_eq!(doc["window"][0]["title"].as_str(), Some("Job"));
        assert_eq!(doc["window"][0]["colour"].as_str(), Some("#0f0"));
        edit_window(&p, "Job", None, None).unwrap();
        assert_eq!(SAVES.with(|n| n.get()) - before, 1);
    }

    #[test]
    fn window_writes_refuse_a_taken_title_and_a_bad_colour() {
        let (_d, p) = write_tmp("[[window]]\ntitle = \"Work\"\n[[window]]\ntitle = \"Job\"\n");
        let before = std::fs::read_to_string(&p).unwrap();
        assert!(matches!(
            edit_window(&p, "Work", Some(" Job "), None),
            Err(EditError::WindowExists(t)) if t == "Job"
        ));
        assert!(matches!(
            edit_window(&p, "Work", None, Some(Some("red"))),
            Err(EditError::BadColour(_))
        ));
        assert!(matches!(
            add_window(&p, "New", Some("#ggg")),
            Err(EditError::BadColour(_))
        ));
        assert_eq!(std::fs::read_to_string(&p).unwrap(), before);
        // Its own title, differently spaced, is not taken.
        edit_window(&p, "Work", Some(" Work "), None).unwrap();
    }

    #[test]
    fn window_writes_land_in_the_config() {
        let (_d, p) =
            write_tmp("# keep\n[[window]]\ntitle = \"Work\"\ncolour = \"#abc\" # accent\n");
        add_window(&p, "Side", Some("#f00")).unwrap();
        assert_eq!(parse(&p)["window"][1]["colour"].as_str(), Some("#f00"));
        // Colour cleared and title changed in one edit, addressed by the old title.
        edit_window(&p, "Work", Some("Job"), Some(None)).unwrap();
        let doc = parse(&p);
        assert_eq!(doc["window"][0]["title"].as_str(), Some("Job"));
        assert!(doc["window"][0].get("colour").is_none());
        // Colour only: the title stays.
        edit_window(&p, "Job", None, Some(Some("#0f0"))).unwrap();
        assert_eq!(parse(&p)["window"][0]["colour"].as_str(), Some("#0f0"));
        assert_eq!(parse(&p)["window"][0]["title"].as_str(), Some("Job"));
        // Nothing to change is a no-op, even for a missing window.
        edit_window(&p, "Nope", None, None).unwrap();
        assert!(matches!(
            edit_window(&p, "Nope", Some("X"), None),
            Err(EditError::WindowNotFound(_))
        ));
        remove(&p, at("Side", None, None)).unwrap();
        assert_eq!(parse(&p)["window"].as_array_of_tables().unwrap().len(), 1);
        assert!(std::fs::read_to_string(&p).unwrap().contains("# keep"));
    }

    #[test]
    fn add_group_appends_to_named_window() {
        let (_d, p) = write_tmp(SAMPLE);
        add_group(&p, "Second", &[("name", "G".into())]).unwrap();
        let doc = parse(&p);
        assert_eq!(doc["window"][1]["group"][0]["name"].as_str(), Some("G"));
        let err = add_group(&p, "Nope", &[("name", "G".into())]).unwrap_err();
        assert!(matches!(err, EditError::WindowNotFound(_)));
    }

    #[test]
    fn remove_tab_by_url_keeps_neighbours_and_comments() {
        let (_d, p) = write_tmp(SAMPLE);
        remove(&p, at("Comms", None, Some(("url", "https://a.test/")))).unwrap();
        let out = std::fs::read_to_string(&p).unwrap();
        assert!(!out.contains("https://a.test/"));
        assert!(out.contains("# top") && out.contains("# inline") && out.contains("# loose b"));
        assert!(out.contains("https://b.test/") && out.contains("https://c.test/"));
    }

    #[test]
    fn remove_group_removes_its_tabs() {
        let (_d, p) = write_tmp(SAMPLE);
        remove(&p, at("Comms", Some("Chat"), None)).unwrap();
        let out = std::fs::read_to_string(&p).unwrap();
        assert!(!out.contains("Chat") && !out.contains("https://c.test/"));
        assert!(out.contains("Other") && out.contains("https://a.test/"));
    }

    #[test]
    fn remove_window_removes_the_block() {
        let (_d, p) = write_tmp(SAMPLE);
        remove(&p, at("Comms", None, None)).unwrap();
        let doc = parse(&p);
        let ws = doc["window"].as_array_of_tables().unwrap();
        assert_eq!(ws.len(), 1);
        assert_eq!(ws.get(0).unwrap()["title"].as_str(), Some("Second"));
    }

    #[test]
    fn set_field_sets_and_unsets() {
        let (_d, p) = write_tmp(SAMPLE);
        let target = at("Comms", None, Some(("title", "B")));
        set_field(&p, target, "load_on_open", Some(true.into())).unwrap();
        set_field(
            &p,
            at("Comms", None, Some(("title", "B"))),
            "url",
            Some("https://z.test/".into()),
        )
        .unwrap();
        let doc = parse(&p);
        let b = doc["window"][0]["tab"][1].as_table().unwrap();
        assert_eq!(b["load_on_open"].as_bool(), Some(true));
        assert_eq!(b["url"].as_str(), Some("https://z.test/"));
        set_field(
            &p,
            at("Comms", None, Some(("title", "B"))),
            "load_on_open",
            None,
        )
        .unwrap();
        assert!(parse(&p)["window"][0]["tab"][1]
            .get("load_on_open")
            .is_none());
        // Group-level field.
        set_field(
            &p,
            at("Comms", Some("Chat"), None),
            "collapsed",
            Some(true.into()),
        )
        .unwrap();
        assert_eq!(
            parse(&p)["window"][0]["group"][0]["collapsed"].as_bool(),
            Some(true)
        );
    }

    #[test]
    fn set_field_on_window_title_renames() {
        let (_d, p) = write_tmp(SAMPLE);
        set_field(&p, at("Comms", None, None), "title", Some("Renamed".into())).unwrap();
        let out = std::fs::read_to_string(&p).unwrap();
        assert!(out.contains("title = \"Renamed\" # inline"));
        assert!(add_tab(&p, "Renamed", None, &[("title", "X".into())]).is_ok());
    }

    #[test]
    fn move_tab_between_groups_keeps_fields() {
        let (_d, p) = write_tmp(SAMPLE);
        move_tab(
            &p,
            at("Comms", None, Some(("url", "https://a.test/"))),
            "Comms",
            Some("Other"),
        )
        .unwrap();
        let out = std::fs::read_to_string(&p).unwrap();
        let doc = parse(&p);
        let moved = doc["window"][0]["group"][1]["tab"][0].as_table().unwrap();
        assert_eq!(moved["title"].as_str(), Some("A"));
        assert_eq!(moved["url"].as_str(), Some("https://a.test/"));
        assert_eq!(
            doc["window"][0]["tab"].as_array_of_tables().unwrap().len(),
            1
        );
        assert!(out.contains("# loose a"));
        // And back to the loose array of another window.
        move_tab(
            &p,
            at("Comms", Some("Other"), Some(("title", "A"))),
            "Second",
            None,
        )
        .unwrap();
        let doc = parse(&p);
        assert_eq!(doc["window"][1]["tab"][0]["title"].as_str(), Some("A"));
    }

    #[test]
    fn unknown_tab_errors() {
        let (_d, p) = write_tmp(SAMPLE);
        let err = remove(&p, at("Comms", None, Some(("url", "https://nope/")))).unwrap_err();
        assert!(matches!(err, EditError::TabNotFound(_)));
        let err = set_field(
            &p,
            at("Comms", None, Some(("url", "https://nope/"))),
            "x",
            None,
        )
        .unwrap_err();
        assert!(matches!(err, EditError::TabNotFound(_)));
        let err = move_tab(
            &p,
            at("Comms", None, Some(("url", "https://nope/"))),
            "Second",
            None,
        )
        .unwrap_err();
        assert!(matches!(err, EditError::TabNotFound(_)));
        let err = remove(&p, at("Comms", Some("Ghost"), None)).unwrap_err();
        assert!(matches!(err, EditError::GroupNotFound(_)));
    }

    #[test]
    fn edit_after_external_change_preserves_it() {
        let (_d, p) = write_tmp("[[window]]\ntitle = \"W\"\n");
        add_tab(&p, "W", None, &[("title", "A".into())]).unwrap();
        let mut s = std::fs::read_to_string(&p).unwrap();
        s.push_str("\n[[window]]\ntitle = \"B\"\n");
        std::fs::write(&p, s).unwrap();
        add_tab(&p, "W", None, &[("title", "C".into())]).unwrap();
        let out = std::fs::read_to_string(&p).unwrap();
        assert!(
            out.contains("title = \"B\"")
                && out.contains("title = \"C\"")
                && out.contains("title = \"A\"")
        );
    }

    #[test]
    fn non_array_window_or_group_key_is_malformed() {
        let (_d, p) = write_tmp("window = \"oops\"\n");
        let err = add_window(&p, "X", None).unwrap_err();
        assert!(matches!(err, EditError::Malformed("window")));
        let (_d, p) = write_tmp("[[window]]\ntitle = \"W\"\ngroup = \"oops\"\n");
        let err = add_group(&p, "W", &[("name", "G".into())]).unwrap_err();
        assert!(matches!(err, EditError::Malformed("group")));
    }

    /// Pins toml_edit printing an unpositioned table under its new parent (after that parent's last
    /// descendant): a toml_edit upgrade that changes this misfiles tabs silently.
    #[test]
    fn moved_and_added_tabs_land_under_their_parent() {
        let (_d, p) = write_tmp(
            "[[window]]\ntitle = \"W1\"\n[[window.group]]\nname = \"G1\"\n[[window.group.tab]]\ntitle = \"a\"\n[[window.group]]\nname = \"G2\"\n[[window.group.tab]]\ntitle = \"b\"\n\n[[window]]\ntitle = \"W2\"\n[[window.group]]\nname = \"G3\"\n[[window.group.tab]]\ntitle = \"c\"\n[[window.group]]\nname = \"G4\"\n[[window.group.tab]]\ntitle = \"d\"\n",
        );
        move_tab(
            &p,
            at("W2", Some("G4"), Some(("title", "d"))),
            "W1",
            Some("G1"),
        )
        .unwrap();
        add_tab(&p, "W1", Some("G1"), &[("title", "e".into())]).unwrap();
        let doc = parse(&p);
        let titles = |w: usize, g: usize| -> Vec<String> {
            doc["window"][w]["group"][g]
                .get("tab")
                .and_then(Item::as_array_of_tables)
                .map(|ts| {
                    ts.iter()
                        .map(|t| t["title"].as_str().unwrap().to_string())
                        .collect()
                })
                .unwrap_or_default()
        };
        assert_eq!(titles(0, 0), ["a", "d", "e"]);
        assert_eq!(titles(0, 1), ["b"]);
        assert_eq!(titles(1, 0), ["c"]);
        assert!(titles(1, 1).is_empty());
        assert_eq!(doc["window"][1]["group"][1]["name"].as_str(), Some("G4"));
    }
}
