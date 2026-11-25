//! §7.7 step 4 (2026-11-24): a session's cwd to its main repository, by reading
//! `.git` entries only. No git process runs, and nothing is written.

use std::collections::HashMap;
use std::fs::{self, File};
use std::io::Read;
use std::path::{Component, Path, PathBuf};

use pelhaim_core::{Event, RepoRef};

/// `.git` files and `commondir` are one short line; anything longer is not one.
const SMALL_FILE_MAX: u64 = 4096;
const CACHE_MAX: usize = 4096;

/// Per-directory cache of `find_repo`, so each cwd is walked once per run.
#[derive(Default)]
pub struct RepoResolver {
    cache: HashMap<String, Option<RepoRef>>,
}

impl RepoResolver {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn resolve(&mut self, cwd: &str) -> Option<RepoRef> {
        if let Some(hit) = self.cache.get(cwd) {
            return hit.clone();
        }
        if self.cache.len() >= CACHE_MAX {
            self.cache.clear();
        }
        let found = find_repo(Path::new(cwd));
        self.cache.insert(cwd.to_string(), found.clone());
        found
    }

    /// Sets `repo` on every `SessionDiscovered` from its `cwd`.
    pub fn fill(&mut self, events: &mut [Event]) {
        for e in events {
            if let Event::SessionDiscovered { cwd, repo, .. } = e {
                *repo = cwd.as_deref().and_then(|c| self.resolve(c));
            }
        }
    }
}

/// The nearest `.git` at or above `cwd`, symlinks resolved first. A `.git`
/// file (a linked worktree) is followed through `gitdir:` and `commondir` to
/// the repository it was added from.
pub fn find_repo(cwd: &Path) -> Option<RepoRef> {
    if !cwd.is_absolute() {
        return None;
    }
    let start = fs::canonicalize(cwd).unwrap_or_else(|_| normalize(cwd));
    for dir in start.ancestors() {
        let Ok(meta) = fs::metadata(dir.join(".git")) else {
            continue;
        };
        if meta.is_dir() {
            return named(dir);
        }
        if meta.is_file() {
            return linked(dir).or_else(|| named(dir));
        }
    }
    None
}

fn named(root: &Path) -> Option<RepoRef> {
    Some(RepoRef {
        name: root.file_name()?.to_str()?.to_string(),
        root: root.to_path_buf(),
    })
}

/// `None` (so the checkout's own directory names it) for a submodule, whose
/// git dir has no `commondir`, or for a `.git` file that is not a pointer.
fn linked(dir: &Path) -> Option<RepoRef> {
    let text = read_small(&dir.join(".git"))?;
    let gitdir = text.lines().next()?.strip_prefix("gitdir:")?.trim();
    let gitdir = dir.join(gitdir);
    let common = read_small(&gitdir.join("commondir"))?;
    let common = gitdir.join(common.lines().next()?.trim());
    let common = fs::canonicalize(&common).unwrap_or_else(|_| normalize(&common));
    if common.file_name().is_some_and(|n| n == ".git") {
        return named(common.parent()?);
    }
    // A bare repository: `<name>.git` is itself the root.
    let name = common.file_name()?.to_str()?;
    Some(RepoRef {
        name: name.strip_suffix(".git").unwrap_or(name).to_string(),
        root: common.clone(),
    })
}

fn read_small(path: &Path) -> Option<String> {
    let mut text = String::new();
    File::open(path)
        .ok()?
        .take(SMALL_FILE_MAX)
        .read_to_string(&mut text)
        .ok()?;
    Some(text)
}

fn normalize(p: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in p.components() {
        match c {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other),
        }
    }
    out
}
