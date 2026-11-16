//! The login session a webview opens in, and the cascade every app resolves it through.
//!
//! An app lets `session` be set at any level of its config: on a tab, a group, a window and the
//! whole app. A tab opens in the most specific level that sets one, else [`DEFAULT_SESSION`]. A
//! blank value (`session = ""`, or only whitespace) is unset and falls through to the next level,
//! so no store is ever keyed on an empty string.
//!
//! The levels themselves are app-owned keys (each app's tab leaf and window/group extras), since
//! not every app opens logins; this module holds only the rule that reads them.

/// The session a tab opens in when no level of its config sets one. Every tab left at the
/// default shares one store, so a sign-in carries across them.
pub const DEFAULT_SESSION: &str = "default";

/// `s` with surrounding whitespace removed, or `None` when it is unset or blank.
pub fn normalize(s: Option<&str>) -> Option<&str> {
    s.map(str::trim).filter(|t| !t.is_empty())
}

/// The session the first level of `chain` that sets one names, most specific level first (tab,
/// group, window, app); [`DEFAULT_SESSION`] when none does.
pub fn resolve<'a>(chain: impl IntoIterator<Item = Option<&'a str>>) -> String {
    chain
        .into_iter()
        .find_map(normalize)
        .unwrap_or(DEFAULT_SESSION)
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_most_specific_level_wins() {
        assert_eq!(resolve([Some("tab"), Some("group"), Some("window")]), "tab");
        assert_eq!(resolve([None, Some("group"), Some("window")]), "group");
        assert_eq!(resolve([None, None, Some("window"), Some("app")]), "window");
        assert_eq!(resolve([None, None, None, Some("app")]), "app");
    }

    #[test]
    fn nothing_set_is_the_default() {
        assert_eq!(resolve([None, None]), DEFAULT_SESSION);
        assert_eq!(resolve(std::iter::empty()), DEFAULT_SESSION);
    }

    #[test]
    fn blank_falls_through_and_values_are_trimmed() {
        assert_eq!(resolve([Some(""), Some("  "), Some(" work ")]), "work");
        assert_eq!(resolve([Some("\t")]), DEFAULT_SESSION);
        assert_eq!(normalize(Some(" a ")), Some("a"));
        assert_eq!(normalize(Some(" ")), None);
    }
}
