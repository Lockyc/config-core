---
type: architecture
links:
  - rel: see-also
    to: README.md
    note: human-facing counterpart — same crate, README for contributors, this file for agents
---

# config-core — agent notes

A small Rust library crate of **domain-free TOML config primitives** shared by three sibling
macOS apps: [curator](https://github.com/Lockyc/curator) (webview console),
[warden](https://github.com/Lockyc/warden) (terminal console), and
[lector](https://github.com/Lockyc/lector) (local-docs console). All three share the same
`window → group → tab` config shape and house style; this crate holds the parts that need no
knowledge of any app's leaf tab fields.

## Scope — deliberately narrow

Eight modules, all leaf-agnostic. Each module's docblocks carry the reasons for its behaviour.

- `model` — leaf-free model primitives (`Density`, `TabDigitKeys`, `Warning`, shared serde
  defaults). All three apps re-export them.
- `window` — the `Window<T, X, G>` / `Group<T, G>` container, generic over each app's leaf `T` and
  extras `X`/`G` (`NoExtras` when an app has none).
- `roots` — project-tree discovery (`scan_root`, `resolve_root_dir`, `discover_projects`,
  `tree_path`). Consumed by warden and lector.
- `fmt` — the house TOML formatter (`format_str`, `format_file`) and the shared `fmt` CLI
  subcommand (`fmt_cli`) all three apps delegate to. `validate` stays per-app: it prints each
  app's own leaf schema.
- `colour` — `Colour::parse` / `Colour::hex` for accent colours.
- `edit` — `add_tab` plus `add_window`/`edit_window`/`add_group`/`remove`/`set_field`/`move_tab` (addressed by `At`): comment-preserving, re-reading the file per call.
  Consumed by curator and warden. `toml_edit` is re-exported (`config_core::toml_edit`) so a
  consumer can name the field-value type without pinning its own `toml_edit`, which would risk a
  version skew against this crate's API.
- `paths` — `resolve_config_path` / `default_config_path`: every app's config location, differing
  only in the env-var name and `~/.config` subdirectory passed in.
- `seed` — `write_default_config`: an atomic, never-clobbering starter config, written only when
  the user asks (shell-core's home surface). The template is the caller's.

`paths` and `seed` are filesystem primitives rather than TOML-content ones, and qualify on the same
test as everything else here: identical and leaf-free in all three apps.

`io::atomic_write` and `io::atomic_create` are deliberately two functions; see `io.rs` and
`atomic_write_still_refuses_a_nonexistent_target`.

**config-core owns the window → group → tab container** — `Window<T, X, G>` and `Group<T, G>`
(`window`) — and window-level validation; **each app owns its leaf `T` and its extras `X`/`G`**
(curator's `url`/`session`, warden's `dir`/`shell`/`probe`, lector's `dir`; per-window and per-group
decorations such as a window `session` or a group `url`). The container holds the common keys and
hands every other key to the app's extras, which `deny_unknown_fields`, so an unknown key is still
reported by name. Leaf meaning stays in the app.

Otherwise, add a primitive here only when it is **leaf-free** (no app's tab-leaf *meaning* appears in
it; holding `T` generically is fine) and shared by **two or more** apps. `roots`
(`scan_root`/`RootDir`/`discover_projects` — warden + lector, not curator) shows that leaf-freeness,
not "all three", is the gate. Don't grow this into a generic config framework. No core→core edges:
shell-core's menu takes a bool for `TabDigitKeys` rather than depending on this crate. `fmt`'s "tab"
leaf detection keys on the literal `tab` table name, which every app uses (`[[window.tab]]`,
`[[window.group.tab]]`).

## Consumed as a git dependency

All three apps build from source on a fresh clone, so there's no crates.io publish — they depend
via `config-core = { git = "https://github.com/Lockyc/config-core" }`. Cargo fetches it at build
time, so the apps' install-from-source flows keep working untouched. A breaking change here needs
all three apps re-pointed/re-tested.

## Branching

**Main-only.** There's no release cadence — the apps pin a git rev — so a `dev` trunk buys
nothing. Commit straight to `main` (code and docs alike) and push. Integration is by direct
commit/merge on `main`; **don't open pull requests.**

## Test/build

`just gate` (`cargo fmt --check`, `cargo clippy -D warnings`, `cargo test`) is the only gate.
