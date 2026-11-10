# config-core

![Platform](https://img.shields.io/badge/platform-macOS-555)
![Rust](https://img.shields.io/badge/rust-CE412B?logo=rust&logoColor=white)
[![License](https://img.shields.io/github/license/lockyc/config-core)](LICENSE)

The TOML config layer four macOS apps share — [warden](https://github.com/lockyc/warden)
(terminals), [curator](https://github.com/lockyc/curator) (browser tabs),
[lector](https://github.com/lockyc/lector) (local doc sites) and
[flotilla](https://github.com/lockyc/flotilla) (document workspaces): the
`window → group → tab` shape, its validation, edits that keep your comments, and the `fmt`
formatter behind each app's `<app> fmt`.

It is not a generic config framework: each app keeps its own leaf tab type (curator:
`url`/`session`; warden: `dir`/`shell`/`probe`; lector: `dir`; flotilla: `url`), validation, and
the rules for how window and group settings pass down to each tab.

## Consumers

warden, curator, lector and flotilla, each pinned to a **commit rev** (no tags, no crates.io
releases).

## Use

```toml
[dependencies]
config-core = { git = "https://github.com/lockyc/config-core", rev = "<commit>" }
```

A config in the shared shape, with the app's own keys on its tabs:

```toml
[[window]]
title  = "work"
colour = "#0f8a8a"

  [[window.group]]
  name = "services"

    [[window.group.tab]]
    title = "api"
    url   = "https://api.example.com/"
```

Load it with your leaf tab type, check the shared keys, and format it in house style:

```rust
use config_core::{format_file, resolve_config_path, validate_windows, Window};
use serde::Deserialize;

#[derive(Deserialize)]
struct Tab { title: String, url: String }

#[derive(Deserialize)]
struct Config { #[serde(rename = "window")] windows: Vec<Window<Tab>> }

let path = resolve_config_path("MYAPP_CONFIG", "myapp");   // $MYAPP_CONFIG, else ~/.config/myapp/config.toml
let config: Config = toml::from_str(&std::fs::read_to_string(&path)?)?;
validate_windows(&config.windows)?;                        // titles, sizes, colours, group names
format_file(&path)?;                                       // rewrites only when bytes change
```

A key outside the common set lands in the window's or group's extras type (`NoExtras` when an app
has none), so an unknown key is reported by name, inside the window, group and tab it sits in.

## What it provides

- **`window`** — `Window<T, X, G>` / `Group<T, G>` with the common keys, `validate_windows`, and
  `retitle_window` for a rename made in place.
- **`edit`** — comment-preserving, atomic edits of windows, groups, tabs and keys via `toml_edit`
  (re-exported, so callers need no dependency of their own).
- **`fmt`** — the house-style formatter (`format_str`, `format_file`) and `fmt_cli`, the `fmt`
  subcommand behind `warden fmt`, `curator fmt` and `lector fmt`.
- **`roots`** — finding every git repo under a `[[window.root]]` directory (warden and lector).
- **`paths`**, **`seed`**, **`colour`**, **`model`** — config-path resolution, a starter config
  written only when nothing is there, `#rgb` / `#rrggbb` colours, and the shared `Density`,
  `TabDigitKeys` and `Warning` types.

Every function and its contract: the crate's rustdoc (`cargo doc --open`).

## Status

In use by all four apps.

## Related

[chrome-core](https://github.com/lockyc/chrome-core) (the sidebar) and
[shell-core](https://github.com/lockyc/shell-core) (app shell and release tooling) are the other
two cores the apps share.

## Development

```sh
git clone https://github.com/lockyc/config-core && cd config-core
just gate     # cargo fmt --check, clippy -D warnings, tests
cargo test    # the quick loop: unit + golden formatter tests
```

Conventions: [CLAUDE.md](CLAUDE.md).

## License

MIT — see [LICENSE](LICENSE).
