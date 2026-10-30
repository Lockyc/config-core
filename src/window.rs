//! The `[[window]]` → `[[window.group]]` → `[[…tab]]` container every app shares.
//!
//! [`Window<T, X, G>`] and [`Group<T, G>`] carry the keys common to every app; each app supplies
//! its leaf tab `T`, its window extras `X`, and its group extras `G` (or [`NoExtras`]). The
//! container never interprets `T`, `X` or `G` — it only holds them.

use std::collections::HashSet;
use std::fmt;

use serde::de::{self, DeserializeOwned, Error as _, MapAccess, Visitor};
use serde::{Deserialize, Deserializer};
use thiserror::Error;
use toml::Table;

use crate::model::{default_window_height, default_window_width};

/// Extras for an app that adds no keys of its own. Deserializes only from an empty table: any key
/// is an unknown-field error naming it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct NoExtras;

impl<'de> Deserialize<'de> for NoExtras {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = NoExtras;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("a table with no further keys")
            }
            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<NoExtras, A::Error> {
                match map.next_key::<String>()? {
                    Some(key) => Err(de::Error::unknown_field(&key, &[])),
                    None => Ok(NoExtras),
                }
            }
        }
        deserializer.deserialize_map(V)
    }
}

/// One `[[window]]`: the common keys, its groups of `T` tabs, and the app's window extras `X`.
///
/// `Deserialize` is hand-written rather than derived with `#[serde(flatten)] extra: X`: serde's
/// `flatten` disables `deny_unknown_fields` on the outer struct and buffers the remainder so an
/// inner `deny_unknown_fields` no longer names the stray key. Instead the window is read as a
/// table, the common keys are taken out, and the remainder alone is deserialized into `X`, whose
/// own `deny_unknown_fields` reports any key nobody claimed.
#[derive(Debug, Clone, PartialEq)]
pub struct Window<T, X = NoExtras, G = NoExtras> {
    pub title: String,
    pub width: u32,
    pub height: u32,
    pub colour: Option<String>,
    pub open_on_start: bool,
    pub groups: Vec<Group<T, G>>,
    pub extra: X,
}

impl<'de, T, X, G> Deserialize<'de> for Window<T, X, G>
where
    T: DeserializeOwned,
    X: DeserializeOwned,
    G: DeserializeOwned,
{
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let mut t = Table::deserialize(deserializer)?;
        let title: String = take(&mut t, "title")
            .map_err(D::Error::custom)?
            .ok_or_else(|| de::Error::missing_field("title"))?;
        let at = |e: String| D::Error::custom(format_args!("window `{}`: {e}", title.trim()));
        let width = take(&mut t, "width").map_err(at)?;
        let height = take(&mut t, "height").map_err(at)?;
        let colour = take(&mut t, "colour").map_err(at)?;
        let open_on_start = take(&mut t, "open_on_start").map_err(at)?;
        let groups = take_list(&mut t, "group", "name").map_err(at)?;
        let extra = remainder(t, WINDOW_KEYS).map_err(at)?;
        Ok(Window {
            title,
            width: width.unwrap_or_else(default_window_width),
            height: height.unwrap_or_else(default_window_height),
            colour,
            open_on_start: open_on_start.unwrap_or(true),
            groups,
            extra,
        })
    }
}

/// The keys [`Window`] claims before its extras see the rest.
const WINDOW_KEYS: &[&str] = &[
    "title",
    "width",
    "height",
    "colour",
    "open_on_start",
    "group",
];

/// The keys [`Group`] claims before its extras see the rest.
const GROUP_KEYS: &[&str] = &["name", "tab"];

/// One `[[window.group]]`: a `name`, its `T` tabs, and the app's group extras `G`. Deserialized
/// the same way as [`Window`], for the same reason.
#[derive(Debug, Clone, PartialEq)]
pub struct Group<T, G = NoExtras> {
    pub name: String,
    pub tabs: Vec<T>,
    pub extra: G,
}

impl<'de, T, G> Deserialize<'de> for Group<T, G>
where
    T: DeserializeOwned,
    G: DeserializeOwned,
{
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let mut t = Table::deserialize(deserializer)?;
        Ok(Group {
            name: take(&mut t, "name")
                .map_err(D::Error::custom)?
                .ok_or_else(|| de::Error::missing_field("name"))?,
            tabs: take_list(&mut t, "tab", "title").map_err(D::Error::custom)?,
            extra: remainder(t, GROUP_KEYS).map_err(D::Error::custom)?,
        })
    }
}

/// Removes `key` from `t` and deserializes it, prefixing any error with the key. A value
/// deserialized out of a detached `toml::Value` has no span, so every error carries its locator in
/// its message: the key here, the window title, group name or tab title around it.
fn take<V: DeserializeOwned>(t: &mut Table, key: &str) -> Result<Option<V>, String> {
    t.remove(key)
        .map(|v| {
            v.try_into()
                .map_err(|e: toml::de::Error| format!("`{key}`: {}", e.message()))
        })
        .transpose()
}

/// Removes the array `key` (`group` or `tab`) from `t` and deserializes each element, prefixing an
/// error with the element's `named` string (``group `Chat`: ``) or else its position
/// (`group #2: `).
fn take_list<V: DeserializeOwned>(t: &mut Table, key: &str, named: &str) -> Result<Vec<V>, String> {
    match t.remove(key) {
        None => Ok(Vec::new()),
        Some(toml::Value::Array(items)) => items
            .into_iter()
            .enumerate()
            .map(|(i, item)| {
                let at = match item.get(named).and_then(toml::Value::as_str) {
                    Some(name) => format!("{key} `{}`", name.trim()),
                    None => format!("{key} #{}", i + 1),
                };
                item.try_into()
                    .map_err(|e: toml::de::Error| format!("{at}: {}", e.message()))
            })
            .collect(),
        Some(other) => other
            .try_into()
            .map_err(|e: toml::de::Error| format!("`{key}`: {}", e.message())),
    }
}

/// Deserializes what is left of `t` after the container's `claimed` keys were taken. An unknown
/// key's error lists the claimed keys ahead of the ones the extras know: the extras alone would
/// name only theirs.
fn remainder<V: DeserializeOwned>(t: Table, claimed: &[&str]) -> Result<V, String> {
    toml::Value::Table(t)
        .try_into()
        .map_err(|e: toml::de::Error| with_claimed_keys(e.message(), claimed))
}

/// serde's unknown-field message (``unknown field `x`, expected one of `a`, `b` `` / `expected
/// `a`` / `there are no fields`) with `claimed` listed first.
fn with_claimed_keys(message: &str, claimed: &[&str]) -> String {
    let Some((field, tail)) = message
        .strip_prefix("unknown field `")
        .and_then(|rest| rest.split_once('`'))
    else {
        return message.to_string();
    };
    let extras = tail
        .strip_prefix(", expected one of ")
        .or_else(|| tail.strip_prefix(", expected "));
    let keys: Vec<String> = claimed
        .iter()
        .map(|k| format!("`{k}`"))
        .chain(extras.map(str::to_string))
        .collect();
    format!(
        "unknown field `{field}`, expected one of {}",
        keys.join(", ")
    )
}

/// Why a window list failed [`validate_windows`]. Titles and group names are compared and reported
/// trimmed.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum WindowError {
    #[error("window #{} has an empty title", index + 1)]
    EmptyTitle { index: usize },
    #[error("duplicate window title `{0}`")]
    DuplicateTitle(String),
    #[error("window `{0}` has a zero width or height")]
    ZeroSize(String),
    #[error("window `{window}` has an invalid colour `{value}`")]
    BadColour { window: String, value: String },
    #[error("window `{0}` has a group with an empty name")]
    EmptyGroupName(String),
    #[error("window `{window}` has more than one group named `{group}`")]
    DuplicateGroupName { window: String, group: String },
}

/// Checks the keys every app shares: titles non-empty and unique, non-zero sizes, parseable
/// colours, group names non-empty and unique within their window.
pub fn validate_windows<T, X, G>(windows: &[Window<T, X, G>]) -> Result<(), WindowError> {
    let mut titles = HashSet::new();
    for (index, w) in windows.iter().enumerate() {
        let title = w.title.trim();
        if title.is_empty() {
            return Err(WindowError::EmptyTitle { index });
        }
        if !titles.insert(title) {
            return Err(WindowError::DuplicateTitle(title.to_string()));
        }
        if w.width == 0 || w.height == 0 {
            return Err(WindowError::ZeroSize(title.to_string()));
        }
        if let Some(value) = &w.colour {
            if crate::Colour::parse(value).is_err() {
                return Err(WindowError::BadColour {
                    window: title.to_string(),
                    value: value.clone(),
                });
            }
        }
        let mut names = HashSet::new();
        for g in &w.groups {
            let name = g.name.trim();
            if name.is_empty() {
                return Err(WindowError::EmptyGroupName(title.to_string()));
            }
            if !names.insert(name) {
                return Err(WindowError::DuplicateGroupName {
                    window: title.to_string(),
                    group: name.to_string(),
                });
            }
        }
    }
    Ok(())
}

/// Carry a window's in-place change from `old` to `title` and `colour` into an already-loaded
/// `windows` ahead of the config reload that will echo it: patch the window titled `old`, else the
/// one already titled `title`. Titles compare trimmed; neither listed patches nothing.
pub fn retitle_window<T, X, G>(
    windows: &mut [Window<T, X, G>],
    old: &str,
    title: &str,
    colour: Option<String>,
) {
    let (old, title) = (old.trim(), title.trim());
    let at = |t: &str| windows.iter().position(|w| w.title.trim() == t);
    let idx = at(old).or_else(|| at(title));
    if let Some(w) = idx.map(|i| &mut windows[i]) {
        w.title = title.to_string();
        w.colour = colour;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone, Deserialize, PartialEq)]
    #[serde(deny_unknown_fields)]
    struct Leaf {
        title: String,
    }

    #[derive(Debug, Clone, Deserialize, PartialEq)]
    #[serde(deny_unknown_fields)]
    struct WinExtras {
        session: Option<String>,
    }

    #[derive(Debug, Clone, Deserialize, PartialEq)]
    #[serde(deny_unknown_fields)]
    struct GroupExtras {
        url: Option<String>,
    }

    /// How every app parses: `Window` nested in its own `Config` under `[[window]]`.
    #[derive(Debug, Deserialize)]
    #[serde(bound(deserialize = "X: DeserializeOwned, G: DeserializeOwned"))]
    struct Config<X = NoExtras, G = NoExtras> {
        #[serde(rename = "window")]
        windows: Vec<Window<Leaf, X, G>>,
    }

    fn parse<X, G>(src: &str) -> Result<Config<X, G>, toml::de::Error>
    where
        X: serde::de::DeserializeOwned,
        G: serde::de::DeserializeOwned,
    {
        toml::from_str(src)
    }

    #[test]
    fn retitle_window_patches_the_renamed_or_recoloured_window() {
        let mut c: Config = parse(
            "[[window]]\ntitle = \"Docs\"\n[[window]]\ntitle = \"Side\"\ncolour = \"#abc\"\n",
        )
        .unwrap();
        retitle_window(&mut c.windows, " Docs ", "Notes", None);
        assert_eq!(c.windows[0].title, "Notes");
        // A colour change patches the window by its own title.
        retitle_window(&mut c.windows, "Side", " Side ", Some("#f00".into()));
        assert_eq!(c.windows[1].colour.as_deref(), Some("#f00"));
        assert_eq!(c.windows[1].title, "Side");
        // The reload already carried the rename: the window under its new title is patched.
        retitle_window(&mut c.windows, "Docs", "Notes", Some("#0f0".into()));
        assert_eq!(c.windows[0].colour.as_deref(), Some("#0f0"));
        // Neither title listed: nothing is patched.
        retitle_window(&mut c.windows, "Gone", "Other", None);
        assert_eq!(
            c.windows
                .iter()
                .map(|w| w.title.as_str())
                .collect::<Vec<_>>(),
            ["Notes", "Side"]
        );
    }

    #[test]
    fn errors_name_the_window_and_list_every_window_key() {
        let err = |second: &str| {
            let src = format!("[[window]]\ntitle = \"A\"\n[[window]]\ntitle = \"B\"\n{second}");
            parse::<WinExtras, GroupExtras>(&src)
                .unwrap_err()
                .to_string()
        };
        let e = err("color = \"#abc\"\n");
        assert!(e.contains("window `B`: unknown field `color`"), "{e}");
        for key in [
            "title",
            "width",
            "height",
            "colour",
            "open_on_start",
            "group",
            "session",
        ] {
            assert!(e.contains(&format!("`{key}`")), "{key} missing from {e}");
        }
        let e = err("width = \"wide\"\n");
        assert!(e.contains("window `B`: `width`"), "{e}");
        let e = err("[[window.group]]\nname = \"G\"\nbogus = 1\n");
        assert!(
            e.contains("window `B`: group `G`: unknown field `bogus`"),
            "{e}"
        );
        assert!(e.contains("`name`") && e.contains("`url`"), "{e}");
        let e = err("[[window.group]]\nname = \"G\"\n[[window.group.tab]]\ntitle = \"t\"\nx = 1\n");
        assert!(
            e.contains("window `B`: group `G`: tab `t`: unknown field `x`"),
            "{e}"
        );
        let e = err("[[window.group]]\nname = \"G\"\n[[window.group.tab]]\nx = 1\n");
        assert!(e.contains("window `B`: group `G`: tab #1:"), "{e}");
        let e = err("[[window.group]]\nurl = \"u\"\n");
        assert!(
            e.contains("window `B`: group #1: missing field `name`"),
            "{e}"
        );
        // No extras: every shared key, and the stray one.
        let e = parse::<NoExtras, NoExtras>("[[window]]\ntitle = \"A\"\ncolor = 1\n")
            .unwrap_err()
            .to_string();
        assert!(
            e.contains("window `A`: unknown field `color`, expected one of `title`"),
            "{e}"
        );
    }

    #[test]
    fn window_defaults_apply() {
        let c: Config = parse("[[window]]\ntitle = \"A\"\n").unwrap();
        let w = &c.windows[0];
        assert_eq!(w.title, "A");
        assert_eq!(w.width, 1500);
        assert_eq!(w.height, 1000);
        assert!(w.open_on_start);
        assert_eq!(w.colour, None);
        assert!(w.groups.is_empty());
        assert_eq!(w.extra, NoExtras);
    }

    #[test]
    fn window_common_keys_parse() {
        let c: Config = parse(
            "[[window]]\ntitle = \"A\"\nwidth = 800\nheight = 600\ncolour = \"#abc\"\n\
             open_on_start = false\n[[window.group]]\nname = \"G\"\n\
             [[window.group.tab]]\ntitle = \"t\"\n",
        )
        .unwrap();
        let w = &c.windows[0];
        assert_eq!((w.width, w.height), (800, 600));
        assert_eq!(w.colour.as_deref(), Some("#abc"));
        assert!(!w.open_on_start);
        assert_eq!(w.groups[0].name, "G");
        assert_eq!(w.groups[0].tabs[0].title, "t");
    }

    #[test]
    fn window_extras_receive_the_remainder() {
        let c: Config<WinExtras> = parse("[[window]]\ntitle = \"A\"\nsession = \"x\"\n").unwrap();
        assert_eq!(c.windows[0].extra.session.as_deref(), Some("x"));
        assert_eq!(c.windows[0].title, "A");
    }

    #[test]
    fn unknown_window_key_names_the_key() {
        let err = parse::<WinExtras, NoExtras>("[[window]]\ntitle = \"A\"\nbogus = 1\n")
            .unwrap_err()
            .to_string();
        assert!(err.contains("bogus"), "{err}");
    }

    #[test]
    fn unknown_window_key_with_no_extras_errors() {
        let err = parse::<NoExtras, NoExtras>("[[window]]\ntitle = \"A\"\nbogus = 1\n")
            .unwrap_err()
            .to_string();
        assert!(err.contains("bogus"), "{err}");
    }

    #[test]
    fn missing_title_is_an_error() {
        let err = parse::<NoExtras, NoExtras>("[[window]]\nwidth = 10\n")
            .unwrap_err()
            .to_string();
        assert!(err.contains("title"), "{err}");
    }

    #[test]
    fn mistyped_common_key_names_the_key() {
        let err = parse::<NoExtras, NoExtras>("[[window]]\ntitle = \"A\"\nwidth = \"wide\"\n")
            .unwrap_err()
            .to_string();
        assert!(err.contains("width"), "{err}");
    }

    #[test]
    fn group_extras_receive_the_remainder() {
        let c: Config<NoExtras, GroupExtras> = parse(
            "[[window]]\ntitle = \"A\"\n[[window.group]]\nname = \"G\"\nurl = \"https://x.test\"\n",
        )
        .unwrap();
        let g = &c.windows[0].groups[0];
        assert_eq!(g.name, "G");
        assert_eq!(g.extra.url.as_deref(), Some("https://x.test"));
    }

    #[test]
    fn unknown_group_key_names_the_key() {
        let err = parse::<NoExtras, GroupExtras>(
            "[[window]]\ntitle = \"A\"\n[[window.group]]\nname = \"G\"\nbogus = 1\n",
        )
        .unwrap_err()
        .to_string();
        assert!(err.contains("bogus"), "{err}");
    }

    #[test]
    fn group_is_generic_over_leaf_and_denies_unknown_keys() {
        let g: Group<Leaf> = toml::from_str(
            "name = \"Chat\"\n[[tab]]\ntitle = \"Gmail\"\n[[tab]]\ntitle = \"Slack\"\n",
        )
        .unwrap();
        assert_eq!(g.name, "Chat");
        assert_eq!(g.tabs.len(), 2);
        assert_eq!(g.tabs[0].title, "Gmail");
        // An empty group defaults its tab list.
        let g: Group<Leaf> = toml::from_str("name = \"Empty\"\n").unwrap();
        assert!(g.tabs.is_empty());
        // A stray group-level key is rejected.
        assert!(toml::from_str::<Group<Leaf>>("name = \"X\"\nbogus = 1\n").is_err());
    }

    fn win(title: &str) -> Window<Leaf> {
        Window {
            title: title.into(),
            width: 100,
            height: 100,
            colour: None,
            open_on_start: true,
            groups: vec![],
            extra: NoExtras,
        }
    }

    fn grp(name: &str) -> Group<Leaf> {
        Group {
            name: name.into(),
            tabs: vec![],
            extra: NoExtras,
        }
    }

    #[test]
    fn empty_title_errors() {
        assert_eq!(
            validate_windows(&[win("A"), win("  ")]),
            Err(WindowError::EmptyTitle { index: 1 })
        );
    }

    #[test]
    fn titles_differing_only_by_whitespace_collide() {
        assert_eq!(
            validate_windows(&[win("A"), win(" A ")]),
            Err(WindowError::DuplicateTitle("A".into()))
        );
    }

    #[test]
    fn zero_width_errors() {
        let mut w = win("A");
        w.width = 0;
        assert_eq!(
            validate_windows(&[w]),
            Err(WindowError::ZeroSize("A".into()))
        );
    }

    #[test]
    fn zero_height_errors() {
        let mut w = win("A");
        w.height = 0;
        assert_eq!(
            validate_windows(&[w]),
            Err(WindowError::ZeroSize("A".into()))
        );
    }

    #[test]
    fn bad_colour_errors() {
        let mut w = win("A");
        w.colour = Some("red".into());
        assert_eq!(
            validate_windows(&[w]),
            Err(WindowError::BadColour {
                window: "A".into(),
                value: "red".into()
            })
        );
    }

    #[test]
    fn empty_group_name_errors() {
        let mut w = win("A");
        w.groups = vec![grp(" ")];
        assert_eq!(
            validate_windows(&[w]),
            Err(WindowError::EmptyGroupName("A".into()))
        );
    }

    #[test]
    fn duplicate_group_name_within_a_window_errors() {
        let mut w = win("A");
        w.groups = vec![grp("G"), grp("G ")];
        assert_eq!(
            validate_windows(&[w]),
            Err(WindowError::DuplicateGroupName {
                window: "A".into(),
                group: "G".into()
            })
        );
    }

    #[test]
    fn same_group_name_in_two_windows_is_fine() {
        let mut a = win("A");
        a.groups = vec![grp("G")];
        let mut b = win("B");
        b.groups = vec![grp("G")];
        assert_eq!(validate_windows(&[a, b]), Ok(()));
    }

    #[test]
    fn valid_config_passes() {
        let mut a = win("A");
        a.colour = Some("#abc".into());
        a.groups = vec![grp("G"), grp("H")];
        assert_eq!(validate_windows(&[a, win("B")]), Ok(()));
    }
}
