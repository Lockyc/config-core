# config-core — agent notes

A small Rust library crate of **domain-free TOML config primitives** shared by three sibling
macOS apps: [curator](https://github.com/Lockyc/curator) (webview console),
[warden](https://github.com/Lockyc/warden) (terminal console), and
[lector](https://github.com/Lockyc/lector) (local-docs console). All three share the same
`window → group → tab` config shape and house style; this crate holds the parts that need no
knowledge of any app's leaf tab fields.

## Scope — deliberately narrow

Five modules, all leaf-agnostic:

- `fmt` — `format_str(&str) -> String` (pure) and `format_file(&Path) -> io::Result<bool>` (atomic,
  diff-guarded, symlink/mode-preserving). Wraps `taplo` with a fixed house style; `separate_sections`
  post-processes blank lines (containers separated, `[[…tab]]` leaves tight). The golden tests pin
  real-config output so a `taplo` bump can't silently change formatting. `format_file` returns
  `Ok(false)` when nothing changed — that no-op-on-clean property is what makes it safe to drive
  from a file watcher (format-on-save) without looping. `fmt_cli(check, &path) -> i32` is the shared
  `fmt` **CLI subcommand** all three apps delegate to (read → reject non-TOML → format/`--check`, with
  identical messages + exit codes); the caller passes its own resolved default config path. `validate`
  is *not* here — it prints each app's own leaf schema, which this crate deliberately doesn't know.
- `colour` — `Colour::parse` / `Colour::hex` for `#rgb`/`#rrggbb` accent colours.
- `edit` — `add_tab(path, window_title, group, &[(&str, toml_edit::Value)])` appends a
  `[[window.tab]]`/`[[window.group.tab]]` table (atomic, comment/format-preserving via
  `toml_edit`; reuses the shared `atomic_write`). **Leaf-agnostic:** the caller passes an ordered
  field list, so it works for curator's (`url`/`session`) and warden's (`dir`/`shell`/`probe`)
  leaves alike (lector doesn't consume this module — it re-exports only `fmt`/`colour`). New-group
  creation is intentionally *not* built — an unknown `group` errors (`EditError::GroupNotFound`);
  the `Option<&str>` parameter is the seam for adding it later. A pre-existing non-array `tab` key
  errors as `EditError::MalformedTab`. `toml_edit` is re-exported (`config_core::toml_edit`) so a
  consumer can name the field-value type without pinning its own `toml_edit` dependency (which
  would risk a version skew against this crate's API).
- `paths` — `resolve_config_path(env_var, app_dir) -> PathBuf` and
  `default_config_path(app_dir) -> PathBuf`. Every app resolves its config the same way — the
  named env var if set **and non-empty**, else `~/.config/<app_dir>/config.toml` — and differs only
  in its env-var name and `~/.config` subdirectory, both passed in by the caller. Deliberately
  `~/.config`, not `dirs::config_dir()` (macOS: `~/Library/Application Support`), so the config
  stays in the dotfiles bare-repo workflow the user manages it with. A set-but-empty env var falls
  through to the default rather than yielding `PathBuf::from("")`, whose only symptom is a
  confusing "cannot read config: No such file or directory" — warden had this right; curator and
  lector didn't, until this was shared.
- `seed` — `write_default_config(path, template) -> Result<bool, SeedError>`: write `template` to
  `path` if nothing is there yet, atomically, never clobbering an existing file (`Ok(false)` = a
  file already existed and was left alone). **Never fires automatically** — no launch hook, no
  first-run marker; an app calls it only when the user clicks a "Create a starter config" button
  (shell-core's home surface). The mechanism (where, atomically, never-clobber) is shared; the
  template string is the caller's own leaf schema, passed in.

**`resolve_config_path` and `write_default_config` are path/filesystem primitives, not TOML-content
primitives like the original three modules — and that's exactly the scope this crate's own bar
already covers, not an expansion of it.** They qualify on the same test as everything else here:
identical and leaf-free in all three apps. Every app resolves its config path the same way and
differs only in an env-var name and a `~/.config` subdirectory (both parameters); every app seeds a
starter config the same way (atomically, never clobbering) and differs only in the template
contents (also a parameter). Neither knows a single leaf field.

**Footgun: `atomic_write` and `atomic_create` are not interchangeable — `atomic_write` requires the
target to already exist.** `io::atomic_write` opens with `std::fs::canonicalize(path)`, which
**fails** (`ErrorKind::NotFound`) when nothing is at `path` yet — that's deliberate: canonicalizing
the *target* is what keeps a rewrite landing on a dotfiles-symlinked config in place rather than
replacing the link with a plain file, and `format_file`/`edit::add_tab` both depend on that
resolution. Seeding a config where none exists is the opposite case, so `io::atomic_create` is a
**sibling**, not a relaxed `atomic_write`: it canonicalizes the *parent* directory instead, so a
`~/.config` symlinked out of a dotfiles repo still receives the real file through the link. A test
(`atomic_write_still_refuses_a_nonexistent_target`) pins that `atomic_write` keeps refusing a
missing target — don't "simplify" the two into one function; that would either break the
symlinked-config rewrite case or make `atomic_write` silently start creating files where an
existing one was expected.

**Do not** grow this into a generic config framework or genericize a window/group/tab model over a
leaf trait. The apps' leaves diverge (curator: `url`/`session`; warden: `dir`/`shell`/`probe`;
lector: `dir`, a local doc-repo path) and each app owns its own model, validation, and cascade
resolution. Only add a primitive here when it is genuinely identical and leaf-free in *all three*
apps. `fmt`'s "tab" leaf detection keys on the literal `tab` table name, which all three apps use
(`[[window.tab]]`, `[[window.group.tab]]`).

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

`cargo test` (unit + golden formatter tests), `cargo fmt`, `cargo clippy` — all green before a
push. No CI; the gate is local.
