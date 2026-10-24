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
/// whose only symptom is a confusing "cannot read config: No such file or directory".
pub fn resolve_config_path(env_var: &str, app_dir: &str) -> PathBuf {
    resolve_from(std::env::var(env_var).ok(), app_dir)
}

/// The pure half of [`resolve_config_path`], given the env var's value. Tests call this rather
/// than set process env, which races every other test thread reading `HOME`.
fn resolve_from(value: Option<String>, app_dir: &str) -> PathBuf {
    match value {
        Some(p) if !p.is_empty() => PathBuf::from(p),
        _ => default_config_path(app_dir),
    }
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
        assert_eq!(
            resolve_from(Some("/tmp/explicit.toml".into()), "demo"),
            std::path::PathBuf::from("/tmp/explicit.toml")
        );
    }

    #[test]
    fn set_but_empty_env_falls_through_to_the_default() {
        // `var_os(..).map(PathBuf::from)` would yield PathBuf::from("") here, whose only symptom is
        // "cannot read config: No such file or directory".
        assert_eq!(
            resolve_from(Some(String::new()), "demo"),
            default_config_path("demo")
        );
    }

    #[test]
    fn unset_env_falls_through_to_the_default() {
        assert_eq!(resolve_from(None, "demo"), default_config_path("demo"));
        // The env-reading wrapper, against a var no test ever sets.
        assert_eq!(
            resolve_config_path("CONFIG_CORE_TEST_NEVER_SET", "demo"),
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
