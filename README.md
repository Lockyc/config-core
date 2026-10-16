# config-core

![Built with Rust](https://img.shields.io/badge/built%20with-Rust-CE412B?logo=rust&logoColor=white)
[![License](https://img.shields.io/github/license/Lockyc/config-core)](LICENSE)

Shared TOML config primitives for the [curator](https://github.com/Lockyc/curator),
[warden](https://github.com/Lockyc/warden), and [lector](https://github.com/Lockyc/lector) apps,
which share the same `window → group → tab` config shape and house style.

It is not a generic config framework: each app keeps its own leaf tab type (curator:
`url`/`session`; warden: `dir`/`shell`/`probe`; lector: `dir`), validation, and the rules for how
window and group settings pass down to each tab.
config-core provides the leaf-agnostic pieces:

- **`model`** — the leaf-free config-model primitives: `Density` (whole-app chrome sizing),
  `TabDigitKeys` (the ⌘1/⌘2 jump-vs-cycle keybinding mode), the non-fatal `Warning`, and the logic-free `Group<T>`
  container — a `name` plus a `Vec<T>` of each app's own leaf tab.
  It also holds the shared serde defaults (`default_true`, `default_window_width`/`_height`).
- **`fmt`** — a house-style TOML formatter (`format_str`, `format_file`) wrapping `taplo` with a
  fixed style: nested-table indentation, aligned `=` and trailing comments, preserved key order,
  a blank line between window/group tables but none between the `tab` tables nested inside them. `format_file` rewrites atomically and
  only when bytes change, preserving symlinks and file mode. `fmt_cli` is the `fmt` CLI subcommand
  behind `warden fmt`, `curator fmt`, and `lector fmt`.
- **`colour`** — `#rgb` / `#rrggbb` hex parsing for the per-window accent colour (`Colour::parse`,
  `Colour::hex`).
- **`edit`** — leaf-agnostic structural insertion. `add_tab(path, window_title, group, fields)`
  appends a `[[window.tab]]` or `[[window.group.tab]]` table, atomic and comment-preserving via
  `toml_edit`. The caller supplies an ordered field list for any leaf shape. An unknown group is an
  error. Field values are `config_core::toml_edit::Value` (`toml_edit` is re-exported; callers need
  no dependency of their own on it).
- **`paths`** — config-path resolution (`resolve_config_path`, `default_config_path`). The
  environment variable the app names (e.g. `WARDEN_CONFIG`) wins when set and non-empty; otherwise
  the path is the default
  (`~/.config/<app_dir>/config.toml`).
- **`seed`** — a starter config, on request only. `write_default_config(path, template)` writes
  `template` to `path` atomically if nothing is there yet, and never clobbers an existing file. The
  caller supplies the template.
- **`roots`** — project-tree discovery for a `[[window.root]]` block (warden and lector):
  `scan_root` walks a dir for git repos, `resolve_root_dir` validates a root's `{name, dir, depth}`,
  and `discover_projects` flattens roots into `DiscoveredProject`s the app maps onto its own tab.

## Use

It's consumed as a git dependency pinned to a **commit rev** (no tags, no crates.io releases):

```toml
[dependencies]
config-core = { git = "https://github.com/Lockyc/config-core", rev = "<commit>" }
```

## Develop

```sh
cargo test    # unit + golden formatter tests
cargo fmt
cargo clippy
```

## License

[MIT](LICENSE) © Lachlan Collins
