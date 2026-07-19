//! Project-tree discovery shared by warden and lector. Pure over a directory tree (no AppKit/
//! Tauri) so it unit-tests against temp dirs: stop at every `.git` (dir or file — worktrees use a
//! file), never descend into a git root, skip hidden dirs, do not follow symlinks. `depth` counts
//! levels below the root dir. Leaf-free — no app tab type appears here (config-core charter).

use std::path::{Path, PathBuf};

/// True if `dir` is a git root (`.git` dir or file present).
fn is_git_root(dir: &Path) -> bool {
    let dot = dir.join(".git");
    dot.exists()
}

/// Recursive worker: push git roots found at or below `dir`. `remaining` is the depth
/// budget below `dir` (0 = may match `dir` itself but not descend).
fn walk(dir: &Path, remaining: u32, out: &mut Vec<PathBuf>) {
    if is_git_root(dir) {
        out.push(dir.to_path_buf());
        return; // never descend into a git root — no sub-repos
    }
    if remaining == 0 {
        return;
    }
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return, // unreadable dir → skip silently
    };
    for entry in entries.flatten() {
        let path = entry.path();
        // Directories only; skip symlinks (cycle/noise) and hidden/dot dirs.
        let ft = match entry.file_type() {
            Ok(ft) => ft,
            Err(_) => continue,
        };
        if !ft.is_dir() || ft.is_symlink() {
            continue;
        }
        if entry.file_name().to_string_lossy().starts_with('.') {
            continue;
        }
        walk(&path, remaining - 1, out);
    }
}

/// Absolute git-root project paths beneath `dir`, deterministic (sorted) order.
pub fn scan_root(dir: &Path, max_depth: u32) -> Vec<PathBuf> {
    let mut out = Vec::new();
    // The root dir itself doesn't count as a project even if it's a repo; start at its
    // children so `max_depth` counts levels *below* `dir`.
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            let ft = match entry.file_type() {
                Ok(ft) => ft,
                Err(_) => continue,
            };
            if !ft.is_dir() || ft.is_symlink() {
                continue;
            }
            if entry.file_name().to_string_lossy().starts_with('.') {
                continue;
            }
            walk(&path, max_depth.saturating_sub(1), &mut out);
        }
    }
    out.sort();
    out
}

/// Folder segments strictly between `root_dir` and `project` (project name excluded).
pub fn tree_path(root_dir: &Path, project: &Path) -> Vec<String> {
    let rel = match project.strip_prefix(root_dir) {
        Ok(r) => r,
        Err(_) => return Vec::new(),
    };
    let mut segs: Vec<String> = rel
        .components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect();
    segs.pop(); // drop the project's own dir name
    segs
}

/// Default project-tree scan depth below a root's `dir` when `depth` is omitted — a safety floor
/// against runaway walks on a deep or huge tree.
pub const DEFAULT_ROOT_DEPTH: u32 = 6;

/// A validated project-tree root: a scanned dir + its section name + scan depth. Leaf-free — the
/// app attaches its own per-root fields (warden: shell/cmd/probe/kill; lector: none) separately.
#[derive(Debug, Clone, PartialEq)]
pub struct RootDir {
    pub name: String,
    pub dir: PathBuf,
    pub depth: u32,
}

/// Why a raw root failed shared validation. The app maps these onto its own error type with the
/// enclosing window's context (config-core has no window concept).
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum RootError {
    #[error("root has an empty dir")]
    EmptyDir,
    #[error("root has an empty name")]
    EmptyName,
    #[error("root has invalid depth {0} (must be >= 1)")]
    ZeroDepth(u32),
}

fn expand_tilde(s: &str) -> PathBuf {
    let t = s.trim();
    if t == "~" {
        return dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    }
    if let Some(rest) = t.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(rest);
        }
    }
    PathBuf::from(t)
}

fn basename(dir: &Path) -> String {
    dir.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| dir.to_string_lossy().into_owned())
}

/// Validate + resolve a raw root's shared fields. `name` defaults to `basename(dir)`; an explicit
/// empty name is an error (a typo, not "use the default"). `dir` is tilde-expanded (not
/// canonicalized — a missing dir must still resolve; the caller warns). `depth` defaults to
/// [`DEFAULT_ROOT_DEPTH`]; `0` is an error. Stored name is trimmed so a trailing-space typo
/// collides in the app's section namespace.
pub fn resolve_root_dir(
    name: Option<&str>,
    dir: &str,
    depth: Option<u32>,
) -> Result<RootDir, RootError> {
    if dir.trim().is_empty() {
        return Err(RootError::EmptyDir);
    }
    let path = expand_tilde(dir);
    let name = match name {
        Some(n) if n.trim().is_empty() => return Err(RootError::EmptyName),
        Some(n) => n.trim().to_string(),
        None => basename(&path),
    };
    let depth = depth.unwrap_or(DEFAULT_ROOT_DEPTH);
    if depth == 0 {
        return Err(RootError::ZeroDepth(0));
    }
    Ok(RootDir {
        name,
        dir: path,
        depth,
    })
}

/// One discovered project handed back to the app: its path, the folder segments between the root
/// dir and it (for chrome-core's folder-tree nesting), and the section (root) name it belongs to.
#[derive(Debug, Clone, PartialEq)]
pub struct DiscoveredProject {
    pub path: PathBuf,
    pub tree_path: Vec<String>,
    pub section: String,
}

/// Flatten a window's roots into discovered projects, roots in order, sorted within each root. No
/// cross-root dedup: a project reachable via two roots appears once per root, and the consuming
/// app collapses duplicates by its own tab identity (first occurrence wins).
pub fn discover_projects(roots: &[RootDir]) -> Vec<DiscoveredProject> {
    let mut out = Vec::new();
    for root in roots {
        for path in scan_root(&root.dir, root.depth) {
            let tp = tree_path(&root.dir, &path);
            out.push(DiscoveredProject {
                path,
                tree_path: tp,
                section: root.name.clone(),
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn tmp(name: &str) -> PathBuf {
        let base =
            std::env::temp_dir().join(format!("config-core-scan-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(&base).unwrap();
        base
    }
    fn git(dir: &Path) {
        fs::create_dir_all(dir).unwrap();
        fs::create_dir_all(dir.join(".git")).unwrap();
    }

    #[test]
    fn finds_git_roots_and_stops_at_them() {
        let base = tmp("stop");
        git(&base.join("gh/lockyc/warden"));
        // a nested repo inside a git root must NOT be discovered separately
        git(&base.join("gh/lockyc/warden/vendor/sub"));
        git(&base.join("gh/other/proj"));
        fs::create_dir_all(&base.join("gh/empty")).unwrap(); // no repo → nothing
        let mut got = scan_root(&base, 6);
        got.sort();
        assert_eq!(
            got,
            vec![base.join("gh/lockyc/warden"), base.join("gh/other/proj")]
        );
    }

    #[test]
    fn respects_depth_and_skips_hidden() {
        let base = tmp("depth");
        git(&base.join("a/b/c/deep")); // depth 4 below base
        git(&base.join(".hidden/repo")); // hidden dir skipped
        assert!(scan_root(&base, 2).is_empty()); // too shallow to reach it
        assert_eq!(scan_root(&base, 6), vec![base.join("a/b/c/deep")]);
    }

    #[test]
    fn git_file_worktree_counts_as_root() {
        let base = tmp("wt");
        let wt = base.join("worktree");
        fs::create_dir_all(&wt).unwrap();
        fs::write(wt.join(".git"), "gitdir: /somewhere\n").unwrap();
        assert_eq!(scan_root(&base, 6), vec![wt]);
    }

    #[test]
    fn tree_path_is_segments_between_root_and_project() {
        let root = PathBuf::from("/r/Developer");
        assert_eq!(
            tree_path(&root, &PathBuf::from("/r/Developer/gh/lockyc/warden")),
            vec!["gh".to_string(), "lockyc".to_string()]
        );
        assert!(tree_path(&root, &PathBuf::from("/r/Developer/loose")).is_empty());
    }

    #[test]
    fn resolve_defaults_name_to_basename_and_depth_to_default() {
        let r = resolve_root_dir(None, "~/Developer", None).unwrap();
        assert_eq!(r.name, "Developer");
        assert_eq!(r.depth, DEFAULT_ROOT_DEPTH);
        assert!(r.dir.is_absolute(), "tilde must expand");
    }

    #[test]
    fn resolve_trims_explicit_name_and_keeps_depth() {
        let r = resolve_root_dir(Some("  Dev  "), "/tmp/x", Some(3)).unwrap();
        assert_eq!(r.name, "Dev");
        assert_eq!(r.depth, 3);
    }

    #[test]
    fn resolve_rejects_empty_dir_name_and_zero_depth() {
        assert_eq!(resolve_root_dir(None, "  ", None), Err(RootError::EmptyDir));
        assert_eq!(
            resolve_root_dir(Some(" "), "/tmp/x", None),
            Err(RootError::EmptyName)
        );
        assert_eq!(
            resolve_root_dir(None, "/tmp/x", Some(0)),
            Err(RootError::ZeroDepth(0))
        );
    }

    #[test]
    fn discover_maps_each_project_with_treepath_and_section() {
        let base = tmp("discover");
        git(&base.join("gh/lockyc/lector"));
        git(&base.join("solo"));
        let root = RootDir {
            name: "Dev".into(),
            dir: base.clone(),
            depth: 6,
        };
        let got = discover_projects(std::slice::from_ref(&root));
        // sorted within a root: "gh/..." before "solo"
        assert_eq!(got.len(), 2);
        assert_eq!(got[0].path, base.join("gh/lockyc/lector"));
        assert_eq!(
            got[0].tree_path,
            vec!["gh".to_string(), "lockyc".to_string()]
        );
        assert_eq!(got[0].section, "Dev");
        assert!(got[1].tree_path.is_empty()); // solo sits directly under the root
        assert_eq!(got[1].section, "Dev");
    }

    #[test]
    fn discover_preserves_root_order_and_emits_overlap_twice() {
        // Cross-root dedup is the APP's job; discover emits a shared project once per root.
        let base = tmp("discover-order");
        git(&base.join("proj"));
        let a = RootDir {
            name: "A".into(),
            dir: base.clone(),
            depth: 6,
        };
        let b = RootDir {
            name: "B".into(),
            dir: base.clone(),
            depth: 6,
        };
        let got = discover_projects(&[a, b]);
        assert_eq!(got.len(), 2);
        assert_eq!(got[0].section, "A");
        assert_eq!(got[1].section, "B");
    }
}
