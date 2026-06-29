# config-core

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
  only when bytes change (watcher-loop safe), preserving symlinks and file mode.
- **`colour`** — `#rgb` / `#rrggbb` hex parsing for the per-window accent colour (`Colour::parse`,
  `Colour::hex`).

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
