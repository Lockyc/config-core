//! Model-based sequence tests over the edit API: random runs of edits against a reference model of
//! the window → group → tab tree and the root keys, starting from generated file layouts. After
//! every step the file must load as a valid window list equal to the model, keep every comment the
//! model still owns, hold no double blank line, survive `format_str` with its value intact, and have
//! been written once on success and never on a refusal.

use super::tests::SAVES;
use super::*;
use crate::window::{validate_windows, Window};
use crate::{format_str, Colour, NoExtras};
use proptest::prelude::*;
use proptest::test_runner::Config;
use proptest_state_machine::{prop_state_machine, ReferenceStateMachine, StateMachineTest};
use serde::Deserialize;
use std::collections::BTreeSet;

#[derive(Clone, Debug, PartialEq)]
struct MTab {
    title: String,
    comment: Option<u32>,
}

#[derive(Clone, Debug, PartialEq)]
struct MGroup {
    name: String,
    tabs: Vec<MTab>,
}

#[derive(Clone, Debug, PartialEq)]
struct MWin {
    title: String,
    colour: Option<String>,
    groups: Vec<MGroup>,
    tabs: Vec<MTab>,
    /// A comment line directly above the `[[window]]` header.
    comment: Option<u32>,
    /// A trailing comment on the `title` line.
    trailing: Option<u32>,
}

#[derive(Clone, Debug, PartialEq)]
struct MRoot {
    key: String,
    value: String,
    comment: Option<u32>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Outcome {
    /// The call succeeded and wrote the file, through `save` when `counted`.
    Saved { counted: bool },
    /// The call succeeded and left the file alone.
    Unchanged,
    /// The call returned an error and left the file alone.
    Refused,
}

#[derive(Clone, Debug)]
struct Model {
    /// A `# header` block set off by a blank line at the top of the file.
    header: bool,
    roots: Vec<MRoot>,
    windows: Vec<MWin>,
    last: Outcome,
}

#[derive(Clone, Debug)]
enum Op {
    AddWindow {
        title: String,
        colour: Option<String>,
    },
    EditWindow {
        title: String,
        new_title: Option<String>,
        colour: Option<Option<String>>,
    },
    Rename {
        title: String,
        new: String,
    },
    RemoveWindow {
        title: String,
    },
    AddGroup {
        window: String,
        name: String,
    },
    AddTab {
        window: String,
        group: Option<String>,
        title: String,
    },
    RemoveTab {
        window: String,
        group: Option<String>,
        title: String,
    },
    MoveTab {
        window: String,
        group: Option<String>,
        title: String,
        to_window: String,
        to_group: Option<String>,
    },
    SetKey {
        key: String,
        value: String,
    },
    RemoveKey {
        key: String,
    },
    Toggle,
}

const TITLES: &[&str] = &["A", "B", " A", "B ", "Work", "", "  "];
const COLOURS: &[&str] = &["#abc", "#0f8a8a", "zzz", "abc", "#12345", ""];
const TAB_TITLES: &[&str] = &["t1", "t2", "t3"];
const GROUP_NAMES: &[&str] = &["g1", "g2", "g3"];
const ROOT_KEYS: &[&str] = &["shell", "density", "tab_digit_keys"];
const ROOT_VALUES: &[&str] = &["fish", "compact", "jump", "cycle"];

impl Model {
    fn win(&self, title: &str) -> Option<usize> {
        self.windows.iter().position(|w| w.title == title.trim())
    }

    fn owner(&self, window: &str, group: Option<&str>) -> Option<(usize, Option<usize>)> {
        let w = self.win(window)?;
        match group {
            None => Some((w, None)),
            Some(g) => {
                let gi = self.windows[w]
                    .groups
                    .iter()
                    .position(|x| x.name == g.trim())?;
                Some((w, Some(gi)))
            }
        }
    }

    fn tabs_mut(&mut self, (w, g): (usize, Option<usize>)) -> &mut Vec<MTab> {
        match g {
            None => &mut self.windows[w].tabs,
            Some(g) => &mut self.windows[w].groups[g].tabs,
        }
    }

    fn root(&self, key: &str) -> Option<usize> {
        self.roots.iter().position(|r| r.key == key)
    }

    /// Every comment tag the file must hold: the header and each one a live item carries.
    fn comments(&self) -> BTreeSet<String> {
        let mut out = BTreeSet::new();
        if self.header {
            out.insert("header".to_string());
        }
        for r in &self.roots {
            out.extend(r.comment.map(|c| format!("c{c}")));
        }
        for w in &self.windows {
            out.extend(w.comment.map(|c| format!("c{c}")));
            out.extend(w.trailing.map(|c| format!("c{c}")));
            for t in w.tabs.iter().chain(w.groups.iter().flat_map(|g| &g.tabs)) {
                out.extend(t.comment.map(|c| format!("c{c}")));
            }
        }
        out
    }

    /// The file this layout is written as: header block, commented root keys, then windows with
    /// a comment above, a trailing comment on the title, loose tabs and groups.
    fn render(&self) -> String {
        let mut s = String::new();
        if self.header {
            s.push_str("# header\n# second header line\n\n");
        }
        for r in &self.roots {
            if let Some(c) = r.comment {
                s.push_str(&format!("# c{c}\n"));
            }
            s.push_str(&format!("{} = {:?}\n", r.key, r.value));
        }
        for w in &self.windows {
            if !s.is_empty() && !s.ends_with("\n\n") {
                s.push('\n');
            }
            if let Some(c) = w.comment {
                s.push_str(&format!("# c{c}\n"));
            }
            s.push_str("[[window]]\n");
            s.push_str(&format!("title = {:?}", w.title));
            if let Some(c) = w.trailing {
                s.push_str(&format!(" # c{c}"));
            }
            s.push('\n');
            if let Some(c) = &w.colour {
                s.push_str(&format!("colour = {c:?}\n"));
            }
            for t in &w.tabs {
                if let Some(c) = t.comment {
                    s.push_str(&format!("# c{c}\n"));
                }
                s.push_str(&format!("[[window.tab]]\ntitle = {:?}\n", t.title));
            }
            for g in &w.groups {
                s.push_str(&format!("\n[[window.group]]\nname = {:?}\n", g.name));
                for t in &g.tabs {
                    if let Some(c) = t.comment {
                        s.push_str(&format!("# c{c}\n"));
                    }
                    s.push_str(&format!("[[window.group.tab]]\ntitle = {:?}\n", t.title));
                }
            }
        }
        s
    }
}

fn pick(xs: &'static [&'static str]) -> impl Strategy<Value = String> {
    proptest::sample::select(xs).prop_map(str::to_string)
}

/// A tab drawn from the pool, with a comment above it when `commented`.
fn layout_tab(id: u32, title: String, commented: bool) -> MTab {
    MTab {
        title,
        comment: commented.then_some(id),
    }
}

/// Generated layouts: a header block or none, zero to two commented root keys, zero to three
/// windows with distinct valid titles, each with an optional comment above, a trailing comment
/// on its title, a valid colour, loose tabs and a group of tabs.
fn layout() -> BoxedStrategy<Model> {
    let roots = proptest::sample::subsequence(ROOT_KEYS, 0..=2).prop_flat_map(|keys| {
        let n = keys.len();
        (
            Just(keys),
            proptest::collection::vec(any::<bool>(), n),
            proptest::collection::vec(proptest::sample::select(&["jump", "cycle"][..]), n),
        )
    });
    let windows =
        proptest::sample::subsequence(&["A", "B", "Work"][..], 0..=3).prop_flat_map(|titles| {
            let n = titles.len();
            (
                Just(titles),
                proptest::collection::vec(
                    (
                        any::<bool>(),
                        any::<bool>(),
                        proptest::option::of(pick(&COLOURS[..2])),
                        proptest::collection::vec((pick(TAB_TITLES), any::<bool>()), 0..=2),
                        proptest::option::of(proptest::collection::vec(
                            (pick(TAB_TITLES), any::<bool>()),
                            0..=2,
                        )),
                    ),
                    n,
                ),
            )
        });
    (any::<bool>(), roots, windows)
        .prop_map(|(header, (keys, commented, values), (titles, shapes))| {
            let mut id = 0;
            let mut next = || {
                id += 1;
                id
            };
            let roots = keys
                .iter()
                .zip(commented)
                .zip(values)
                .map(|((k, c), v)| MRoot {
                    key: k.to_string(),
                    value: v.to_string(),
                    comment: c.then(&mut next),
                })
                .collect();
            let windows = titles
                .iter()
                .zip(shapes)
                .map(|(t, (above, trailing, colour, tabs, group))| MWin {
                    title: t.to_string(),
                    colour,
                    comment: above.then(&mut next),
                    trailing: trailing.then(&mut next),
                    tabs: tabs
                        .into_iter()
                        .map(|(t, c)| layout_tab(next(), t, c))
                        .collect(),
                    groups: group
                        .into_iter()
                        .map(|tabs| MGroup {
                            name: "g1".to_string(),
                            tabs: tabs
                                .into_iter()
                                .map(|(t, c)| layout_tab(next(), t, c))
                                .collect(),
                        })
                        .collect(),
                })
                .collect();
            Model {
                header,
                roots,
                windows,
                last: Outcome::Unchanged,
            }
        })
        .boxed()
}

struct EditMachine;

impl ReferenceStateMachine for EditMachine {
    type State = Model;
    type Transition = Op;

    fn init_state() -> BoxedStrategy<Model> {
        layout()
    }

    fn transitions(_: &Model) -> BoxedStrategy<Op> {
        let title = || pick(TITLES);
        let group = || proptest::option::of(pick(GROUP_NAMES));
        prop_oneof![
            (title(), proptest::option::of(pick(COLOURS)))
                .prop_map(|(title, colour)| Op::AddWindow { title, colour }),
            (
                title(),
                proptest::option::of(title()),
                proptest::option::of(proptest::option::of(pick(COLOURS)))
            )
                .prop_map(|(title, new_title, colour)| Op::EditWindow {
                    title,
                    new_title,
                    colour
                }),
            (title(), title()).prop_map(|(title, new)| Op::Rename { title, new }),
            title().prop_map(|title| Op::RemoveWindow { title }),
            (title(), pick(GROUP_NAMES)).prop_map(|(window, name)| Op::AddGroup { window, name }),
            (title(), group(), pick(TAB_TITLES)).prop_map(|(window, group, title)| Op::AddTab {
                window,
                group,
                title
            }),
            (title(), group(), pick(TAB_TITLES)).prop_map(|(window, group, title)| {
                Op::RemoveTab {
                    window,
                    group,
                    title,
                }
            }),
            (title(), group(), pick(TAB_TITLES), title(), group()).prop_map(
                |(window, group, title, to_window, to_group)| Op::MoveTab {
                    window,
                    group,
                    title,
                    to_window,
                    to_group,
                }
            ),
            (pick(ROOT_KEYS), pick(ROOT_VALUES)).prop_map(|(key, value)| Op::SetKey { key, value }),
            pick(ROOT_KEYS).prop_map(|key| Op::RemoveKey { key }),
            Just(Op::Toggle),
        ]
        .boxed()
    }

    fn apply(mut s: Model, op: &Op) -> Model {
        let colour_ok = |c: &str| Colour::parse(c).is_ok();
        s.last = match op {
            Op::AddWindow { title, colour } => {
                let t = title.trim();
                if t.is_empty()
                    || colour.as_deref().is_some_and(|c| !colour_ok(c))
                    || s.win(t).is_some()
                {
                    Outcome::Refused
                } else {
                    s.windows.push(MWin {
                        title: t.to_string(),
                        colour: colour.clone(),
                        groups: vec![],
                        tabs: vec![],
                        comment: None,
                        trailing: None,
                    });
                    Outcome::Saved { counted: true }
                }
            }
            Op::EditWindow {
                title,
                new_title,
                colour,
            } => {
                if new_title.is_none() && colour.is_none() {
                    Outcome::Unchanged
                } else if colour
                    .as_ref()
                    .and_then(|c| c.as_deref())
                    .is_some_and(|c| !colour_ok(c))
                {
                    Outcome::Refused
                } else {
                    match s.win(title) {
                        None => Outcome::Refused,
                        Some(w) => {
                            let nt = new_title.as_deref().map(str::trim);
                            if nt.is_some_and(|t| t.is_empty() || s.win(t).is_some_and(|o| o != w))
                            {
                                Outcome::Refused
                            } else {
                                if let Some(c) = colour {
                                    s.windows[w].colour = c.clone();
                                }
                                if let Some(t) = nt {
                                    s.windows[w].title = t.to_string();
                                }
                                Outcome::Saved { counted: true }
                            }
                        }
                    }
                }
            }
            Op::Rename { title, new } => {
                let t = new.trim();
                match s.win(title) {
                    Some(w) if !t.is_empty() && s.win(t).is_none_or(|o| o == w) => {
                        s.windows[w].title = t.to_string();
                        Outcome::Saved { counted: true }
                    }
                    _ => Outcome::Refused,
                }
            }
            Op::RemoveWindow { title } => match s.win(title) {
                Some(w) => {
                    s.windows.remove(w);
                    Outcome::Saved { counted: true }
                }
                None => Outcome::Refused,
            },
            Op::AddGroup { window, name } => match s.win(window) {
                Some(w) if s.windows[w].groups.iter().all(|g| &g.name != name) => {
                    s.windows[w].groups.push(MGroup {
                        name: name.clone(),
                        tabs: vec![],
                    });
                    Outcome::Saved { counted: true }
                }
                _ => Outcome::Refused,
            },
            Op::AddTab {
                window,
                group,
                title,
            } => match s.owner(window, group.as_deref()) {
                Some(o) => {
                    s.tabs_mut(o).push(MTab {
                        title: title.clone(),
                        comment: None,
                    });
                    Outcome::Saved { counted: true }
                }
                None => Outcome::Refused,
            },
            Op::RemoveTab {
                window,
                group,
                title,
            } => match s.owner(window, group.as_deref()) {
                Some(o) => {
                    let tabs = s.tabs_mut(o);
                    match tabs.iter().position(|t| &t.title == title) {
                        Some(i) => {
                            tabs.remove(i);
                            Outcome::Saved { counted: true }
                        }
                        None => Outcome::Refused,
                    }
                }
                None => Outcome::Refused,
            },
            Op::MoveTab {
                window,
                group,
                title,
                to_window,
                to_group,
            } => match (
                s.owner(window, group.as_deref()),
                s.owner(to_window, to_group.as_deref()),
            ) {
                (Some(from), Some(to)) => {
                    let tabs = s.tabs_mut(from);
                    match tabs.iter().position(|t| &t.title == title) {
                        Some(i) => {
                            let t = tabs.remove(i);
                            s.tabs_mut(to).push(t);
                            Outcome::Saved { counted: true }
                        }
                        None => Outcome::Refused,
                    }
                }
                _ => Outcome::Refused,
            },
            Op::SetKey { key, value } => set_root(&mut s, key, value),
            Op::Toggle => {
                let next = match s.root("tab_digit_keys").map(|i| s.roots[i].value.as_str()) {
                    Some("cycle") => "jump",
                    _ => "cycle",
                };
                set_root(&mut s, "tab_digit_keys", next)
            }
            Op::RemoveKey { key } => match s.root(key) {
                Some(i) => {
                    s.roots.remove(i);
                    Outcome::Saved { counted: true }
                }
                None => Outcome::Unchanged,
            },
        };
        s
    }
}

/// `set_key` on the model. A file's first root key is placed in the text and written without
/// going through `save`, so it is not counted.
fn set_root(s: &mut Model, key: &str, value: &str) -> Outcome {
    let counted = !s.roots.is_empty();
    match s.root(key) {
        Some(i) => s.roots[i].value = value.to_string(),
        None => s.roots.push(MRoot {
            key: key.to_string(),
            value: value.to_string(),
            comment: None,
        }),
    }
    Outcome::Saved { counted }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Leaf {
    title: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct WinTabs {
    #[serde(default)]
    tab: Vec<Leaf>,
}

struct Sut {
    _dir: tempfile::TempDir,
    path: std::path::PathBuf,
}

impl Sut {
    fn text(&self) -> String {
        std::fs::read_to_string(&self.path).unwrap()
    }
}

fn at<'a>(window: &'a str, group: Option<&'a str>, tab: Option<(&'a str, &'a str)>) -> At<'a> {
    At { window, group, tab }
}

fn run(path: &Path, op: &Op) -> Result<(), EditError> {
    match op {
        Op::AddWindow { title, colour } => add_window(path, title, colour.as_deref()),
        Op::EditWindow {
            title,
            new_title,
            colour,
        } => edit_window(
            path,
            title,
            new_title.as_deref(),
            colour.as_ref().map(|c| c.as_deref()),
        ),
        Op::Rename { title, new } => set_field(
            path,
            at(title, None, None),
            "title",
            Some(Value::from(new.as_str())),
        ),
        Op::RemoveWindow { title } => remove(path, at(title, None, None)),
        Op::AddGroup { window, name } => {
            add_group(path, window, &[("name", Value::from(name.as_str()))])
        }
        Op::AddTab {
            window,
            group,
            title,
        } => add_tab(
            path,
            window,
            group.as_deref(),
            &[("title", Value::from(title.as_str()))],
        ),
        Op::RemoveTab {
            window,
            group,
            title,
        } => remove(
            path,
            at(window, group.as_deref(), Some(("title", title.as_str()))),
        ),
        Op::MoveTab {
            window,
            group,
            title,
            to_window,
            to_group,
        } => move_tab(
            path,
            at(window, group.as_deref(), Some(("title", title.as_str()))),
            to_window,
            to_group.as_deref(),
        ),
        Op::SetKey { key, value } => set_key(path, key, Value::from(value.as_str())),
        Op::RemoveKey { key } => remove_key(path, key).map(|_| ()),
        Op::Toggle => toggle_tab_digit_keys(path).map(|_| ()),
    }
}

impl StateMachineTest for EditMachine {
    type SystemUnderTest = Sut;
    type Reference = EditMachine;

    fn init_test(model: &Model) -> Sut {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(&path, model.render()).unwrap();
        let sut = Sut { _dir: dir, path };
        Self::check_invariants(&sut, model);
        sut
    }

    fn apply(sut: Sut, model: &Model, op: Op) -> Sut {
        let before = sut.text();
        let saves = SAVES.with(|n| n.get());
        let result = run(&sut.path, &op);
        let wrote = SAVES.with(|n| n.get()) - saves;
        match model.last {
            Outcome::Saved { counted } => {
                assert!(result.is_ok(), "{op:?} refused: {result:?}");
                assert_eq!(wrote, u32::from(counted), "{op:?} saved {wrote} times");
            }
            Outcome::Unchanged | Outcome::Refused => {
                assert_eq!(
                    result.is_ok(),
                    model.last == Outcome::Unchanged,
                    "{op:?}: {result:?}"
                );
                assert_eq!(wrote, 0, "{op:?} saved");
                assert_eq!(sut.text(), before, "{op:?} changed the file");
            }
        }
        sut
    }

    fn check_invariants(sut: &Sut, model: &Model) {
        let text = sut.text();
        let mut root: toml::Table =
            toml::from_str(&text).unwrap_or_else(|e| panic!("unparseable:\n{text}\n{e}"));
        let windows: Vec<Window<Leaf, WinTabs, NoExtras>> = match root.remove("window") {
            None => vec![],
            Some(v) => v
                .try_into()
                .unwrap_or_else(|e| panic!("windows refused:\n{text}\n{e}")),
        };
        validate_windows(&windows).unwrap_or_else(|e| panic!("invalid:\n{text}\n{e}"));
        let got: Vec<_> = windows
            .iter()
            .map(|w| {
                (
                    w.title.clone(),
                    w.colour.clone(),
                    w.extra
                        .tab
                        .iter()
                        .map(|t| t.title.clone())
                        .collect::<Vec<_>>(),
                    w.groups
                        .iter()
                        .map(|g| {
                            (
                                g.name.clone(),
                                g.tabs.iter().map(|t| t.title.clone()).collect::<Vec<_>>(),
                            )
                        })
                        .collect::<Vec<_>>(),
                )
            })
            .collect();
        let want: Vec<_> = model
            .windows
            .iter()
            .map(|w| {
                (
                    w.title.clone(),
                    w.colour.clone(),
                    w.tabs.iter().map(|t| t.title.clone()).collect::<Vec<_>>(),
                    w.groups
                        .iter()
                        .map(|g| {
                            (
                                g.name.clone(),
                                g.tabs.iter().map(|t| t.title.clone()).collect::<Vec<_>>(),
                            )
                        })
                        .collect::<Vec<_>>(),
                )
            })
            .collect();
        assert_eq!(got, want, "windows differ from the model:\n{text}");
        let roots: std::collections::BTreeMap<String, String> = root
            .iter()
            .map(|(k, v)| (k.clone(), v.as_str().unwrap_or("?").to_string()))
            .collect();
        let want_roots = model
            .roots
            .iter()
            .map(|r| (r.key.clone(), r.value.clone()))
            .collect();
        assert_eq!(roots, want_roots, "root keys differ:\n{text}");

        let comments: BTreeSet<String> = text
            .lines()
            .filter_map(|l| l.split_once("# ").map(|(_, c)| c.trim().to_string()))
            .filter(|c| c != "second header line")
            .collect();
        assert_eq!(comments, model.comments(), "comments differ:\n{text}");
        if model.header {
            assert!(
                text.starts_with("# header\n# second header line\n\n"),
                "header moved:\n{text}"
            );
        }
        assert!(!text.contains("\n\n\n"), "a double blank line:\n{text}");

        let formatted = format_str(&text);
        let value = |s: &str| toml::from_str::<toml::Table>(s).unwrap();
        assert_eq!(
            value(&formatted),
            value(&text),
            "format_str changed a value:\n{text}"
        );
    }
}

prop_state_machine! {
    #![proptest_config(Config {
        cases: std::env::var("PROPTEST_CASES").ok().and_then(|n| n.parse().ok()).unwrap_or(64),
        .. Config::default()
    })]

    #[test]
    fn edit_sequences_match_the_model(sequential 1..24 => EditMachine);
}
