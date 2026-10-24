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

Seven modules, all leaf-agnostic. Each module's docblocks carry the reasons for its behaviour.

- `model` — leaf-free model primitives (`Density`, `TabDigitKeys`, `Warning`, the logic-free
  `Group<T>`, shared serde defaults). All three apps re-export them.
- `roots` — project-tree discovery (`scan_root`, `resolve_root_dir`, `discover_projects`,
  `tree_path`). Consumed by warden and lector.
- `fmt` — the house TOML formatter (`format_str`, `format_file`) and the shared `fmt` CLI
  subcommand (`fmt_cli`) all three apps delegate to. `validate` stays per-app: it prints each
  app's own leaf schema.
- `colour` — `Colour::parse` / `Colour::hex` for accent colours.
- `edit` — `add_tab`, a comment-preserving `[[…tab]]` append taking a caller-ordered field list.
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

**Do not** grow this into a generic config framework or genericize a window/group/tab model over a
leaf **trait**. The apps' leaves diverge (curator: `url`/`session`; warden: `dir`/`shell`/`probe`;
lector: `dir`, a local doc-repo path) *and their divergence is multi-level* — each app also decorates
`Config` and `WindowConfig` (curator's `session`/`allow_insecure`, lector's `theme`/`roots`, warden's
`probe_interval`/`tab_digit_keys`), so each app owns its own model, validation, and cascade
resolution. Only add a primitive here when it is **leaf-free** (no app's tab-leaf type appears in its
signature) and shared by **two or more** apps. `roots` is one precedent
(`scan_root`/`RootDir`/`discover_projects` — consumed by warden + lector but not curator, so
leaf-freeness, not "all three", is the gate); `model` is the other (`Density`/`Warning` across all
three, `Group<T>`/`default_*` across curator + lector). No core→core edges: shell-core's menu takes
a bool for `TabDigitKeys` rather than depending on this crate.

The gate is **leaf-freeness**, and a *logic-free* generic container passes it: `Group<T>` is a `name`
plus `Vec<T>` — it names each app's leaf generically but touches no leaf meaning, so it belongs here.
What stays forbidden is genericizing the *model* over a leaf **trait** (a `Tab: TabLike` the
window/group machinery dispatches through) — that would smuggle leaf logic behind a type param and is
the abstraction this crate exists to avoid. Rule of thumb: a container that only *holds* leaves is
fine; a trait the model *calls into* is not. `fmt`'s "tab" leaf detection keys on the literal `tab`
table name, which all three apps use (`[[window.tab]]`, `[[window.group.tab]]`).

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
