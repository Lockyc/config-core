# config-core

![Built with Rust](https://img.shields.io/badge/built%20with-Rust-CE412B?logo=rust&logoColor=white)
[![License](https://img.shields.io/github/license/Lockyc/config-core)](LICENSE)

Shared TOML config primitives for the [curator](https://github.com/Lockyc/curator) and
[warden](https://github.com/Lockyc/warden) apps — the two sibling macOS consoles share the same
`window → group → tab` config *shape* and the same house style, so the domain-free pieces live
here once instead of being copied between them.

It is intentionally **not** a generic config framework. The apps' leaf tab fields genuinely
diverge (curator: `url`/`session`; warden: `dir`/`shell`/`probe`), so each app keeps its own
model, validation, and cascade resolution and calls these primitives. Only the parts that need
zero knowledge of the leaf are shared:

- **`fmt`** — a house-style TOML formatter (`format_str`, `format_file`) wrapping `taplo` with a
  fixed style: nested-table indentation, aligned `=` and trailing comments, preserved key order,
  blank-line-separated containers with tight nested tabs. `format_file` rewrites atomically and
  only when bytes change (watcher-loop safe), preserving symlinks and file mode. `fmt_cli` is the
  shared `fmt` CLI subcommand both apps delegate to, so `warden fmt` and `curator fmt` are one
  implementation (the `validate` subcommand stays per-app — it prints each app's own schema).
- **`colour`** — `#rgb` / `#rrggbb` hex parsing for the per-window accent colour (`Colour::parse`,
  `Colour::hex`).
- **`edit`** — leaf-agnostic structural insertion. `add_tab(path, window_title, group, fields)`
  appends a `[[window.tab]]` or `[[window.group.tab]]` table, atomic and comment-preserving via
  `toml_edit`. The caller supplies an ordered field list; this module knows nothing about what the
  fields mean, so it works for curator's (`url`/`session`) leaves and warden's
  (`dir`/`shell`/`probe`) leaves alike. An unknown group errors rather than silently creating one.

## Use

It's consumed as a git dependency (the apps build from source on a fresh clone, so there's no
crates.io publish):

```toml
[dependencies]
config-core = { git = "https://github.com/Lockyc/config-core" }
```

## Develop

```sh
cargo test    # unit + golden formatter tests
cargo fmt
cargo clippy
```

## License

[MIT](LICENSE) © Lachlan Collins
