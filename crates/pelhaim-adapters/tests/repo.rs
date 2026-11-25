mod common;

use std::fs;
use std::path::{Path, PathBuf};

use common::repo_tree;
use pelhaim_adapters::observe::{RepoResolver, find_repo};
use pelhaim_core::{Event, RepoRef, SessionKey, Tool, Ts};

fn tree() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    repo_tree(dir.path());
    let root = fs::canonicalize(dir.path()).unwrap();
    (dir, root)
}

fn at(root: &Path, name: &str) -> Option<RepoRef> {
    Some(RepoRef {
        name: name.rsplit('/').next().unwrap().into(),
        root: root.join(name),
    })
}

#[test]
fn plain_repo_from_its_root_and_below() {
    let (_d, root) = tree();
    assert_eq!(find_repo(&root.join("Plain")), at(&root, "Plain"));
    assert_eq!(find_repo(&root.join("Plain/src/deep")), at(&root, "Plain"));
}

#[test]
fn linked_worktree_resolves_to_the_main_repo() {
    let (_d, root) = tree();
    assert_eq!(
        find_repo(&root.join("SafeAscent-r7v2/crates")),
        at(&root, "SafeAscent")
    );
}

#[test]
fn relative_gitdir_and_commondir_resolve_too() {
    let (_d, root) = tree();
    fs::write(
        root.join("SafeAscent-r7v2/.git"),
        "gitdir: ../SafeAscent/.git/worktrees/r7v2\n",
    )
    .unwrap();
    assert_eq!(
        find_repo(&root.join("SafeAscent-r7v2")),
        at(&root, "SafeAscent")
    );
}

#[test]
fn nested_repo_wins_inside_it_and_outer_outside() {
    let (_d, root) = tree();
    assert_eq!(
        find_repo(&root.join("Outer/vendor/Inner/lib")),
        at(&root, "Outer/vendor/Inner")
    );
    assert_eq!(find_repo(&root.join("Outer/docs")), at(&root, "Outer"));
    assert_eq!(find_repo(&root.join("Outer/vendor")), at(&root, "Outer"));
}

#[test]
fn no_repo_is_none() {
    let (_d, root) = tree();
    assert_eq!(find_repo(&root.join("loose/dir")), None);
    assert_eq!(find_repo(&root.join("loose/gone/never")), None);
    assert_eq!(find_repo(Path::new("relative/path")), None);
}

#[cfg(unix)]
#[test]
fn symlinked_cwd_resolves_through_the_link() {
    let (_d, root) = tree();
    std::os::unix::fs::symlink(root.join("SafeAscent-r7v2/crates"), root.join("loose/link"))
        .unwrap();
    assert_eq!(find_repo(&root.join("loose/link")), at(&root, "SafeAscent"));
}

#[test]
fn a_gitfile_without_commondir_names_its_own_checkout() {
    let (_d, root) = tree();
    fs::create_dir_all(root.join("Outer/.git/modules/sub")).unwrap();
    fs::create_dir_all(root.join("Outer/sub")).unwrap();
    fs::write(root.join("Outer/sub/.git"), "gitdir: ../.git/modules/sub\n").unwrap();
    assert_eq!(find_repo(&root.join("Outer/sub")), at(&root, "Outer/sub"));
    fs::write(root.join("Outer/sub/.git"), "not a pointer").unwrap();
    assert_eq!(find_repo(&root.join("Outer/sub")), at(&root, "Outer/sub"));
}

#[test]
fn a_worktree_of_a_bare_repo_is_named_without_dot_git() {
    let (_d, root) = tree();
    fs::create_dir_all(root.join("ethica.git/worktrees/w")).unwrap();
    fs::write(root.join("ethica.git/worktrees/w/commondir"), "../..").unwrap();
    fs::create_dir_all(root.join("ethica-w")).unwrap();
    fs::write(
        root.join("ethica-w/.git"),
        "gitdir: ../ethica.git/worktrees/w",
    )
    .unwrap();
    assert_eq!(
        find_repo(&root.join("ethica-w")),
        Some(RepoRef {
            name: "ethica".into(),
            root: root.join("ethica.git"),
        })
    );
}

#[test]
fn resolver_caches_per_directory_and_fills_discoveries() {
    let (_d, root) = tree();
    let cwd = root.join("Plain/src").display().to_string();
    let mut r = RepoResolver::new();
    assert_eq!(r.resolve(&cwd), at(&root, "Plain"));
    fs::remove_dir_all(root.join("Plain/.git")).unwrap();
    assert_eq!(r.resolve(&cwd), at(&root, "Plain"), "cached");
    assert_eq!(RepoResolver::new().resolve(&cwd), None);

    let discovered = |cwd: Option<String>| Event::SessionDiscovered {
        key: SessionKey::claude("S1"),
        tool: Tool::Claude,
        parent: None,
        path: PathBuf::from("/x.jsonl"),
        cwd,
        repo: None,
        version: None,
        started: Ts::UNIX_EPOCH,
    };
    let mut events = vec![discovered(Some(cwd)), discovered(None)];
    r.fill(&mut events);
    let repos: Vec<Option<RepoRef>> = events
        .into_iter()
        .map(|e| match e {
            Event::SessionDiscovered { repo, .. } => repo,
            _ => unreachable!(),
        })
        .collect();
    assert_eq!(repos, [at(&root, "Plain"), None]);
}
