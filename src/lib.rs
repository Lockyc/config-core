//! Domain-free TOML config primitives shared by the curator and warden apps.
//!
//! Both apps share the same window → group → tab config *shape* and the same need to keep the
//! file tidy, but their leaf tab fields diverge (curator: `url`/`session`; warden:
//! `dir`/`shell`/`probe`). This crate carries only the parts that need no knowledge of either
//! app's leaf — the house-style formatter ([`fmt`]) and hex colour parsing ([`colour`]). Each
//! app keeps its own model, validation, and cascade resolution and calls these primitives, so
//! there's no leaf abstraction to fight.
pub mod colour;
pub mod edit;
pub mod fmt;
mod io;

pub use colour::{Colour, ColourError};
pub use edit::{add_tab, EditError};
pub use fmt::{format_file, format_str};
