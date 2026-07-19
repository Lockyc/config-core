//! Leaf-free config-model primitives shared by the curator, warden, and lector apps.
//!
//! These are the parts of the window → group → tab model that carry no knowledge of any app's
//! leaf tab shape: the whole-app presentation enums ([`Density`], [`OpenOnLaunch`]), the
//! non-fatal [`Warning`], the logic-free [`Group<T>`] container (generic over each app's own
//! `Tab`), and the shared serde field defaults. Each app re-exports these under its own config
//! crate so `app_config::Density` etc. keep resolving, and layers its own leaf types on top.

use serde::{Deserialize, Serialize};

/// UI density — a whole-app presentation mode that scales the chrome's type and spacing as a
/// unit. The crate only carries the choice; each app's chrome owns the actual sizes (it maps
/// this to a `data-density` attribute → CSS variables).
///
/// - `Comfortable` (default): the standard sizing.
/// - `Compact`: proportionally condensed type + spacing for denser tab lists.
///
/// Deserializes from / serializes to the lowercase token the chrome reads (`comfortable` /
/// `compact`); an unrecognised value is a parse error. [`Density::as_str`] returns that same
/// token for apps (warden) that build the attribute by hand rather than through serde.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Density {
    #[default]
    Comfortable,
    Compact,
}

impl Density {
    /// The token the chrome's `data-density` attribute uses.
    pub fn as_str(self) -> &'static str {
        match self {
            Density::Comfortable => "comfortable",
            Density::Compact => "compact",
        }
    }
}

/// What to open when a window launches. The default (`false` / unset) opens the first
/// `load_on_open` (loaded) tab, else the blank background — the first tab isn't always loaded, so
/// it isn't forced. `true` opens the first tab even if it isn't loaded; a string opens the tab
/// whose `title` matches (falling back to the first).
#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum OpenOnLaunch {
    Toggle(bool),
    Tab(String),
}
impl Default for OpenOnLaunch {
    fn default() -> Self {
        OpenOnLaunch::Toggle(false)
    }
}

/// A non-fatal config issue surfaced to the user (logged on load, printed by `<app> validate`)
/// without rejecting the config — e.g. a URL/dir repeated within a window, or a dir that is
/// missing or not a directory.
#[derive(Debug, Clone, PartialEq)]
pub struct Warning {
    pub window: String,
    pub message: String,
}

/// A named `[[window.group]]` of tabs — the logic-free container shared by curator and lector,
/// generic over each app's own leaf `Tab`. Carries only presentation (the section `name`) and
/// its tabs; all leaf meaning lives in `T`.
#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
// Pin the deserialize bound: without it, serde's auto-inference for a generic struct with a
// field-level `#[serde(default)]` spuriously demands `T: Default`, which the apps' leaf `Tab`
// types don't derive. `Vec<T>: Default` holds unconditionally, so `T: Deserialize` is all we need.
#[serde(bound(deserialize = "T: serde::Deserialize<'de>"))]
pub struct Group<T> {
    pub name: String,
    #[serde(default, rename = "tab")]
    pub tabs: Vec<T>,
}

/// serde `default` for a `bool` field that defaults to `true` (serde's own bool default is
/// `false`). Named so `#[serde(default = "config_core::default_true")]` reads intent-first.
pub fn default_true() -> bool {
    true
}
/// serde `default` for a window's `width` — the shared 1500px starting size.
pub fn default_window_width() -> u32 {
    1500
}
/// serde `default` for a window's `height` — the shared 1000px starting size.
pub fn default_window_height() -> u32 {
    1000
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone, Deserialize, PartialEq)]
    #[serde(deny_unknown_fields)]
    struct Leaf {
        title: String,
    }

    #[test]
    fn density_defaults_comfortable_and_round_trips_lowercase() {
        assert_eq!(Density::default(), Density::Comfortable);
        assert_eq!(Density::Compact.as_str(), "compact");
        // Deserializes from the lowercase token…
        let d: Density = toml::from_str("d = \"compact\"\n")
            .map(|w: Wrap| w.d)
            .unwrap();
        assert_eq!(d, Density::Compact);
        // …and an unknown token is a parse error.
        assert!(toml::from_str::<Wrap>("d = \"roomy\"\n").is_err());
    }

    #[derive(Deserialize)]
    struct Wrap {
        d: Density,
    }

    #[test]
    fn open_on_launch_toggle_and_tab() {
        assert_eq!(OpenOnLaunch::default(), OpenOnLaunch::Toggle(false));
        #[derive(Deserialize)]
        struct W {
            o: OpenOnLaunch,
        }
        let w: W = toml::from_str("o = true\n").unwrap();
        assert_eq!(w.o, OpenOnLaunch::Toggle(true));
        let w: W = toml::from_str("o = \"Mail\"\n").unwrap();
        assert_eq!(w.o, OpenOnLaunch::Tab("Mail".into()));
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
        // A stray group-level key is rejected (deny_unknown_fields).
        assert!(toml::from_str::<Group<Leaf>>("name = \"X\"\nbogus = 1\n").is_err());
    }

    #[test]
    fn shared_defaults() {
        assert!(default_true());
        assert_eq!(default_window_width(), 1500);
        assert_eq!(default_window_height(), 1000);
    }
}
