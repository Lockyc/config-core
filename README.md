# config-core

[![CI](https://github.com/lockyc/config-core/actions/workflows/ci.yml/badge.svg)](https://github.com/lockyc/config-core/actions/workflows/ci.yml)
![Built with Rust](https://img.shields.io/badge/built%20with-Rust-CE412B?logo=rust&logoColor=white)
[![License](https://img.shields.io/github/license/Lockyc/config-core)](LICENSE)

Shared TOML config primitives for the [curator](https://github.com/Lockyc/curator),
[warden](https://github.com/Lockyc/warden), and [lector](https://github.com/Lockyc/lector) apps —
the three sibling macOS consoles share the same `window → group → tab` config *shape* and the
same house style, so the domain-free pieces live here once instead of being copied between them.

It is intentionally **not** a generic config framework. The apps' models diverge at every level —
the leaf tab fields (curator: `url`/`session`; warden: `dir`/`shell`/`probe`; lector: `dir`), and
the `Config`/`WindowConfig` decorations around them — so each app keeps its own leaf type,
validation, and cascade resolution and calls these primitives. Only the parts that need zero
knowledge of the leaf are shared:

- **`model`** — the leaf-free config-model primitives: `Density` (whole-app chrome sizing),
  `TabDigitKeys` (the ⌘1/⌘2 jump-vs-cycle keybinding mode), the non-fatal `Warning`, and the logic-free `Group<T>`
  container — a `name` plus a `Vec<T>` of each app's own leaf tab, holding no leaf meaning itself.
  Plus the shared serde defaults (`default_true`, `default_window_width`/`_height`).
- **`fmt`** — a house-style TOML formatter (`format_str`, `format_file`) wrapping `taplo` with a
  fixed style: nested-table indentation, aligned `=` and trailing comments, preserved key order,
  blank-line-separated containers with tight nested tabs. `format_file` rewrites atomically and
  only when bytes change (watcher-loop safe), preserving symlinks and file mode. `fmt_cli` is the
  shared `fmt` CLI subcommand all three apps delegate to, so `warden fmt`, `curator fmt`, and
  `lector fmt` are one implementation (the `validate` subcommand stays per-app — it prints each
  app's own schema).
- **`colour`** — `#rgb` / `#rrggbb` hex parsing for the per-window accent colour (`Colour::parse`,
  `Colour::hex`).
- **`edit`** — leaf-agnostic structural insertion. `add_tab(path, window_title, group, fields)`
  appends a `[[window.tab]]` or `[[window.group.tab]]` table, atomic and comment-preserving via
  `toml_edit`. The caller supplies an ordered field list; this module knows nothing about what the
  fields mean, so it works for curator's (`url`/`session`), warden's (`dir`/`shell`/`probe`) and
  lector's (`dir`) leaves alike. An unknown group errors rather than silently creating one. Field
  values are `config_core::toml_edit::Value` — the re-exported `toml_edit`, so a caller needs no
  version-skewed dependency of its own.
- **`paths`** — config-path resolution (`resolve_config_path`, `default_config_path`). The named
  env var wins when set and non-empty; a set-but-empty var falls through to the default
  (`~/.config/<app_dir>/config.toml`) instead of the confusing "cannot read config" error an empty
  path would otherwise cause.
- **`seed`** — a starter config, on request only. `write_default_config(path, template)` writes
  `template` to `path` atomically if nothing is there yet, and never clobbers an existing file. It
  never fires automatically (no launch hook, no first-run marker) — an app calls it only when the
  user asks for one. The mechanism is shared; the template is the caller's own leaf schema.
- **`roots`** — project-tree discovery for a `[[window.root]]` block (warden and lector):
  `scan_root` walks a dir for git repos, `resolve_root_dir` validates a root's `{name, dir, depth}`,
  and `discover_projects` flattens roots into `DiscoveredProject`s the app maps onto its own tab.

## Use

It's consumed as a git dependency pinned to a **commit rev** — that rev is this crate's only version
identity (no tags, no crates.io publish):

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
