# config-core — agent notes

A small Rust library crate of **domain-free TOML config primitives** shared by two sibling macOS
apps, [curator](https://github.com/Lockyc/curator) (webview console) and
[warden](https://github.com/Lockyc/warden) (terminal console). Both share the same
`window → group → tab` config shape and house style; this crate holds the parts that need no
knowledge of either app's leaf tab fields.

## Scope — deliberately narrow

Three modules, all leaf-agnostic:

- `fmt` — `format_str(&str) -> String` (pure) and `format_file(&Path) -> io::Result<bool>` (atomic,
  diff-guarded, symlink/mode-preserving). Wraps `taplo` with a fixed house style; `separate_sections`
  post-processes blank lines (containers separated, `[[…tab]]` leaves tight). The golden tests pin
  real-config output so a `taplo` bump can't silently change formatting. `format_file` returns
  `Ok(false)` when nothing changed — that no-op-on-clean property is what makes it safe to drive
  from a file watcher (format-on-save) without looping.
- `colour` — `Colour::parse` / `Colour::hex` for `#rgb`/`#rrggbb` accent colours.
- `edit` — `add_tab(path, window_title, group, &[(&str, toml_edit::Value)])` appends a
  `[[window.tab]]`/`[[window.group.tab]]` table (atomic, comment/format-preserving via
  `toml_edit`; reuses the shared `atomic_write`). **Leaf-agnostic:** the caller passes an ordered
  field list, so it works for curator's (`url`/`session`) and warden's (`dir`/`shell`/`probe`)
  leaves alike. New-group creation is intentionally *not* built — an unknown `group` errors
  (`EditError::GroupNotFound`); the `Option<&str>` parameter is the seam for adding it later.
  A pre-existing non-array `tab` key errors as `EditError::MalformedTab`.

**Do not** grow this into a generic config framework or genericize a window/group/tab model over a
leaf trait. The apps' leaves diverge (curator: `url`/`session`; warden: `dir`/`shell`/`probe`) and
each app owns its own model, validation, and cascade resolution. Only add a primitive here when it
is genuinely identical and leaf-free in *both* apps. `fmt`'s "tab" leaf detection keys on the
literal `tab` table name, which both apps use (`[[window.tab]]`, `[[window.group.tab]]`).

## Consumed as a git dependency

Both apps build from source on a fresh clone, so there's no crates.io publish — they depend via
`config-core = { git = "https://github.com/Lockyc/config-core" }`. Cargo fetches it at build time,
so the apps' install-from-source flows keep working untouched. A breaking change here needs both
apps re-pointed/re-tested.

## Test/build

`cargo test` (unit + golden formatter tests), `cargo fmt`, `cargo clippy` — all green before a
push. No CI; the gate is local.
