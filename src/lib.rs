//! Domain-free TOML config primitives shared by the curator and warden apps.
//!
//! Both apps share the same window → group → tab config *shape* and the same need to keep the
//! file tidy, but their leaf tab fields diverge (curator: `url`/`session`; warden:
//! `dir`/`shell`/`probe`). This crate carries only the parts that need no knowledge of either
//! app's leaf — the house-style formatter ([`fmt`]), hex colour parsing ([`colour`]), and
//! structural config edits ([`edit`]). Each app keeps its own model, validation, and cascade
//! resolution and calls these primitives, so there's no leaf abstraction to fight.
//!
//! # Modules
//!
//! - [`fmt`] — house-style TOML formatter (`format_str`, `format_file`). Atomic, diff-guarded,
//!   watcher-loop safe. [`fmt_cli`] is the shared `fmt` CLI subcommand both apps delegate to (the
//!   `validate` subcommand stays per-app — it prints each app's own leaf schema).
//! - [`colour`] — `#rgb`/`#rrggbb` hex accent-colour parsing ([`Colour`], [`ColourError`]).
//! - [`edit`] — leaf-agnostic structural insertion. [`add_tab`] appends a `[[window.tab]]` or
//!   `[[window.group.tab]]` table from an ordered field list, atomic and comment-preserving via
//!   `toml_edit`. The caller supplies the fields; this module knows nothing about what they mean.
pub mod colour;
pub mod edit;
pub mod fmt;
mod io;

pub use colour::{Colour, ColourError};
pub use edit::{add_tab, EditError};
pub use fmt::{fmt_cli, format_file, format_str};

// Re-exported so consumers can name the field-value type `add_tab` takes
// (`config_core::toml_edit::Value`) without declaring their own `toml_edit` dependency — which
// would risk a version skew against the one this crate's API is built on.
pub use toml_edit;
