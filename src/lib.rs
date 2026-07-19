//! Domain-free TOML config primitives shared by the curator, warden, and lector apps.
//!
//! All three share the same window → group → tab config *shape* and the same need to keep the
//! file tidy, but their leaf tab fields diverge (curator: `url`/`session`; warden:
//! `dir`/`shell`/`probe`; lector: `dir`). This crate carries only the parts that need no
//! knowledge of any app's leaf — the house-style formatter ([`fmt`]), hex colour parsing
//! ([`colour`]), structural config edits ([`edit`]), config-path resolution ([`paths`]), and
//! starter-config seeding ([`seed`]). Each app keeps its own model, validation, and cascade
//! resolution and calls these primitives, so there's no leaf abstraction to fight.
//!
//! # Modules
//!
//! - [`fmt`] — house-style TOML formatter (`format_str`, `format_file`). Atomic, diff-guarded,
//!   watcher-loop safe. [`fmt_cli`] is the shared `fmt` CLI subcommand all three apps delegate to
//!   (the `validate` subcommand stays per-app — it prints each app's own leaf schema).
//! - [`colour`] — `#rgb`/`#rrggbb` hex accent-colour parsing ([`Colour`], [`ColourError`]).
//! - [`edit`] — leaf-agnostic structural insertion. [`add_tab`] appends a `[[window.tab]]` or
//!   `[[window.group.tab]]` table from an ordered field list, atomic and comment-preserving via
//!   `toml_edit`. The caller supplies the fields; this module knows nothing about what they mean.
//! - [`paths`] — where an app's config file lives. [`resolve_config_path`] checks the app's env
//!   override (set and non-empty) before falling back to [`default_config_path`]
//!   (`~/.config/<app_dir>/config.toml`).
//! - [`seed`] — writes a starter config when an app has none. [`write_default_config`] never
//!   fires automatically (no launch hook, no first-run marker) and never clobbers an existing
//!   file — an app calls it only in response to a user action.
pub mod colour;
pub mod edit;
pub mod fmt;
mod io;
pub mod paths;
pub mod roots;
pub mod seed;

pub use colour::{Colour, ColourError};
pub use edit::{add_tab, EditError};
pub use fmt::{fmt_cli, format_file, format_str};
pub use paths::{default_config_path, resolve_config_path};
pub use roots::{
    discover_projects, resolve_root_dir, scan_root, tree_path, DiscoveredProject, RootDir,
    RootError, DEFAULT_ROOT_DEPTH,
};
pub use seed::{write_default_config, SeedError};

// Re-exported so consumers can name the field-value type `add_tab` takes
// (`config_core::toml_edit::Value`) without declaring their own `toml_edit` dependency — which
// would risk a version skew against the one this crate's API is built on.
pub use toml_edit;
