//! Leaf-agnostic structural edits to a config file. Each app builds its own group/tab field list
//! (curator: `title`/`url`/`session`/…; warden: `title`/`dir`/…) and calls these; this module knows
//! nothing about what the fields mean. A window's shared `title`/`colour` have their own
//! `add_window`/`edit_window`. Add/remove/set/move edit windows, groups and tabs (addressed by an
//! `At`); `set_key` sets one root key (an app's menu toggle writing its setting). Comment/format
//! preserving — only the addressed entry changes. Every call re-reads the file, so an external
//! edit between two calls is never overwritten.

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
    #[error("no tab with that key/value: {0}")]
    TabNotFound(String),
    #[error("a window titled {0:?} already exists")]
    WindowExists(String),
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
    atomic_write(path, &doc.to_string())?;
    Ok(())
}

/// Set root-table `key` to `v`, replacing its value or adding it above the first table. Atomic +
/// comment-preserving. A missing file is an `Io` error: the app seeds its starter config first, so
/// the starter's documentation is never replaced by a one-key file.
pub fn set_key(path: &Path, key: &str, v: Value) -> Result<(), EditError> {
    let mut doc = load(path)?;
    match doc.get_mut(key).and_then(Item::as_value_mut) {
        // Keeps the entry's own decor (a trailing `# why` comment) by swapping only the value.
        Some(old) => {
            let decor = old.decor().clone();
            *old = v;
            *old.decor_mut() = decor;
        }
        None => {
            let first_key = !doc.iter().any(|(_, item)| item.is_value());
            doc.insert(key, value(v));
            // The file's header comment hangs on the first table while the root has no keys; it
            // moves onto the new key so it stays the file's header.
            if first_key {
                if let Some(header) = take_first_table_prefix(&mut doc) {
                    if let Some(mut k) = doc.key_mut(key) {
                        k.leaf_decor_mut().set_prefix(header);
                    }
                }
            }
        }
    }
    save(path, &doc)
}

/// Takes the first table's leading comment, leaving one blank line in its place.
fn take_first_table_prefix(doc: &mut DocumentMut) -> Option<String> {
    let decor = doc.iter_mut().find_map(|(_, item)| match item {
        Item::Table(t) => Some(t.decor_mut()),
        Item::ArrayOfTables(a) => a.get_mut(0).map(|t| t.decor_mut()),
        _ => None,
    })?;
    let header = decor.prefix()?.as_str()?.to_string();
    if header.trim().is_empty() {
        return None;
    }
    decor.set_prefix("\n");
    Some(header)
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
/// `WindowExists` when the title is already taken.
pub fn add_window(path: &Path, title: &str, colour: Option<&str>) -> Result<(), EditError> {
    let title = title.trim();
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
/// `new_title`. The colour goes first, so it is addressed while the window still carries `title`.
/// Nothing to change is a no-op.
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
    if let Some(colour) = colour {
        set_field(path, at, "colour", colour.map(Value::from))?;
    }
    if let Some(new_title) = new_title {
        set_field(path, at, "title", Some(Value::from(new_title.trim())))?;
    }
    Ok(())
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
    let target: &mut Table = match (at.group, at.tab) {
        (g, Some((k, v))) => {
            let owner = tab_owner_mut(&mut doc, at.window, g)?;
            let i = tab_index(owner, k, v)?;
            owner["tab"]
                .as_array_of_tables_mut()
                .and_then(|a| a.get_mut(i))
                .expect("checked")
        }
        (Some(g), None) => group_mut(window_mut(&mut doc, at.window)?, g)?,
        (None, None) => window_mut(&mut doc, at.window)?,
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
    save(path, &doc)
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
