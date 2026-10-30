//! The `[[window]]` → `[[window.group]]` → `[[…tab]]` container every app shares.
//!
//! [`Window<T, X, G>`] and [`Group<T, G>`] carry the keys common to every app; each app supplies
//! its leaf tab `T`, its window extras `X`, and its group extras `G` (or [`NoExtras`]). The
//! container never interprets `T`, `X` or `G` — it only holds them.

use std::fmt;

use serde::de::{self, DeserializeOwned, MapAccess, Visitor};
use serde::{Deserialize, Deserializer};
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
        Ok(Window {
            title: take(&mut t, "title")?.ok_or_else(|| de::Error::missing_field("title"))?,
            width: take(&mut t, "width")?.unwrap_or_else(default_window_width),
            height: take(&mut t, "height")?.unwrap_or_else(default_window_height),
            colour: take(&mut t, "colour")?,
            open_on_start: take(&mut t, "open_on_start")?.unwrap_or(true),
            groups: take(&mut t, "group")?.unwrap_or_default(),
            extra: remainder(t)?,
        })
    }
}

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
            name: take(&mut t, "name")?.ok_or_else(|| de::Error::missing_field("name"))?,
            tabs: take(&mut t, "tab")?.unwrap_or_default(),
            extra: remainder(t)?,
        })
    }
}

/// Removes `key` from `t` and deserializes it, prefixing any error with the key: a value
/// deserialized out of a detached `toml::Value` has no span, so the key is the only locator.
fn take<V: DeserializeOwned, E: de::Error>(t: &mut Table, key: &str) -> Result<Option<V>, E> {
    t.remove(key)
        .map(|v| {
            v.try_into()
                .map_err(|e: toml::de::Error| E::custom(format_args!("`{key}`: {}", e.message())))
        })
        .transpose()
}

/// Deserializes what is left of `t` after the common keys were taken.
fn remainder<V: DeserializeOwned, E: de::Error>(t: Table) -> Result<V, E> {
    toml::Value::Table(t)
        .try_into()
        .map_err(|e: toml::de::Error| E::custom(e.message()))
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
}
