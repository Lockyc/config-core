//! Where an app's config file lives. Leaf-agnostic: every app in the family resolves its config
//! the same way and differs only in its env-var name and its `~/.config` subdirectory, so the
//! mechanism lives here and the two strings are parameters.

use std::path::PathBuf;

/// The config to load: `$<env_var>` if set **and non-empty**, else [`default_config_path`].
///
/// The env override is what lets each app's `just run` point at its repo's `examples/config.toml`
/// so iterating never touches the developer's real config.
///
/// A **set-but-empty** var falls through to the default rather than yielding `PathBuf::from("")`,
/// whose only symptom is a confusing "cannot read config: No such file or directory". Two of the
/// three apps shipped that bug before this was shared.
pub fn resolve_config_path(env_var: &str, app_dir: &str) -> PathBuf {
    if let Ok(p) = std::env::var(env_var) {
        if !p.is_empty() {
            return PathBuf::from(p);
        }
    }
    default_config_path(app_dir)
}

/// `~/.config/<app_dir>/config.toml`.
///
/// Deliberately `~/.config` — **not** `dirs::config_dir()`, which on macOS is
/// `~/Library/Application Support` and would take every app's config out of the dotfiles
/// bare-repo workflow the user manages it with.
pub fn default_config_path(app_dir: &str) -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".config")
        .join(app_dir)
        .join("config.toml")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn env_override_wins_when_set_and_non_empty() {
        std::env::set_var("CONFIG_CORE_TEST_VAR", "/tmp/explicit.toml");
        assert_eq!(
            resolve_config_path("CONFIG_CORE_TEST_VAR", "demo"),
            std::path::PathBuf::from("/tmp/explicit.toml")
        );
        std::env::remove_var("CONFIG_CORE_TEST_VAR");
    }

    #[test]
    fn set_but_empty_env_falls_through_to_the_default() {
        // The bug this function exists to fix. `var_os(..).map(PathBuf::from)` — which curator and
        // lector both shipped — yields PathBuf::from("") here, and the only symptom the user ever
        // sees is "cannot read config: No such file or directory". warden got this right; this is
        // warden's behaviour, shared.
        std::env::set_var("CONFIG_CORE_TEST_EMPTY", "");
        assert_eq!(
            resolve_config_path("CONFIG_CORE_TEST_EMPTY", "demo"),
            default_config_path("demo")
        );
        std::env::remove_var("CONFIG_CORE_TEST_EMPTY");
    }

    #[test]
    fn unset_env_falls_through_to_the_default() {
        std::env::remove_var("CONFIG_CORE_TEST_UNSET");
        assert_eq!(
            resolve_config_path("CONFIG_CORE_TEST_UNSET", "demo"),
            default_config_path("demo")
        );
    }

    #[test]
    fn default_is_dot_config_not_macos_application_support() {
        // Deliberately ~/.config, so the config slots into the dotfiles bare-repo workflow.
        // dirs::config_dir() would give ~/Library/Application Support on macOS.
        let p = default_config_path("demo");
        assert!(p.ends_with(".config/demo/config.toml"), "{}", p.display());
        assert!(!p.to_string_lossy().contains("Application Support"));
    }
}
