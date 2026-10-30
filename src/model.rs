//! Leaf-free config-model primitives shared by the curator, warden, and lector apps.
//!
//! These are the parts of the window → group → tab model that carry no knowledge of any app's
//! leaf tab shape: the whole-app presentation enum [`Density`], the keybinding mode [`TabDigitKeys`],
//! the non-fatal [`Warning`], and the shared serde field defaults. The window/group container is
//! [`crate::window`]. Each app re-exports these under its own config crate so
//! `app_config::Density` etc. keep resolving, and layers its own leaf types on top.

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

/// Behaviour of the ⌘1/⌘2 menu accelerators — a whole-app keybinding mode, no per-window cascade.
///
/// - `Jump` (default, the macOS convention): ⌘1–⌘9 jump straight to the tab at that 1-based
///   position.
/// - `Cycle`: ⌘1 = next tab, ⌘2 = previous tab (aliasing ⌘⇧] / ⌘⇧[). Claiming the digit-1/2
///   chords deliberately removes the digit-1/2 *jumps*, so direct jumps start at ⌘3.
///
/// Deserializes from / serializes to the lowercase token the config file uses (`jump` / `cycle`);
/// an unrecognised value is a parse error. [`TabDigitKeys::as_str`] is that token for the apps'
/// Cycle Tabs menu toggle, which writes it back ([`crate::toggle_tab_digit_keys`]). `tab_digit_keys`
/// deliberately never reaches the chrome DTO the way density does.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TabDigitKeys {
    #[default]
    Jump,
    Cycle,
}

impl TabDigitKeys {
    /// The token used in the config file.
    pub fn as_str(self) -> &'static str {
        match self {
            TabDigitKeys::Jump => "jump",
            TabDigitKeys::Cycle => "cycle",
        }
    }

    /// Whether ⌘1/⌘2 cycle tabs. This is the bridge to shell-core's menu builder, which takes a
    /// plain `bool`: shell-core must never depend on config-core (the cores are mutually
    /// independent), so the consuming app passes this rather than the enum itself.
    pub fn is_cycle(self) -> bool {
        matches!(self, TabDigitKeys::Cycle)
    }

    /// The other mode — what the apps' Cycle Tabs menu toggle writes.
    pub fn toggled(self) -> Self {
        match self {
            TabDigitKeys::Jump => TabDigitKeys::Cycle,
            TabDigitKeys::Cycle => TabDigitKeys::Jump,
        }
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

    #[derive(Deserialize)]
    struct WrapK {
        k: TabDigitKeys,
    }

    #[test]
    fn tab_digit_keys_defaults_jump_and_round_trips_lowercase() {
        assert_eq!(TabDigitKeys::default(), TabDigitKeys::Jump);
        assert_eq!(TabDigitKeys::Jump.as_str(), "jump");
        assert_eq!(TabDigitKeys::Cycle.as_str(), "cycle");
        // `is_cycle` is the bridge the apps hand to shell-core's menu builder.
        assert!(!TabDigitKeys::Jump.is_cycle());
        assert!(TabDigitKeys::Cycle.is_cycle());
        assert_eq!(TabDigitKeys::Jump.toggled(), TabDigitKeys::Cycle);
        assert_eq!(TabDigitKeys::Cycle.toggled(), TabDigitKeys::Jump);
        // Deserializes from the lowercase token…
        let k: TabDigitKeys = toml::from_str("k = \"cycle\"\n")
            .map(|w: WrapK| w.k)
            .unwrap();
        assert_eq!(k, TabDigitKeys::Cycle);
        // …and an unknown token is a parse error, not a silent default.
        assert!(toml::from_str::<WrapK>("k = \"wiggle\"\n").is_err());
    }

    #[test]
    fn shared_defaults() {
        assert!(default_true());
        assert_eq!(default_window_width(), 1500);
        assert_eq!(default_window_height(), 1000);
    }
}
