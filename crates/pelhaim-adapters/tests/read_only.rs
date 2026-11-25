mod common;

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use common::{Running, live_home, opts, repo_tree, run_once};
use pelhaim_adapters::observe::{ObserveClaude, ObserveCodex, ObserveOptions};
use pelhaim_adapters::{AdapterCommand, AdapterKind, Capabilities, SessionAdapter};
use pelhaim_core::{Event, RepoRef, SessionKey, Tool};
use sha2::{Digest, Sha256};

type Fingerprint = BTreeMap<PathBuf, (u64, SystemTime, Vec<u8>)>;

/// Every entry under `root`, directories included, so a created file or directory shows up.
fn fingerprint(root: &Path) -> (Fingerprint, Vec<PathBuf>) {
    let mut files = BTreeMap::new();
    let mut dirs = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            let rel = path.strip_prefix(root).unwrap().to_path_buf();
            let meta = fs::symlink_metadata(&path).unwrap();
            if meta.is_dir() {
                dirs.push(rel);
                stack.push(path);
            } else {
                let hash = Sha256::digest(fs::read(&path).unwrap()).to_vec();
                files.insert(rel, (meta.len(), meta.modified().unwrap(), hash));
            }
        }
    }
    dirs.sort();
    (files, dirs)
}

fn discovered(events: &[Event], key: &SessionKey) -> bool {
    events
        .iter()
        .any(|e| matches!(e, Event::SessionDiscovered { key: k, .. } if k == key))
}

#[tokio::test(flavor = "multi_thread")]
async fn observe_adapters_never_modify_fixture_home() {
    let claude = live_home("claude");
    let codex = live_home("codex");
    let before = (fingerprint(claude.path()), fingerprint(codex.path()));
    assert!(before.0.0.len() >= 9 && before.1.0.len() >= 2);

    let once = |home: &Path| ObserveOptions {
        once: true,
        ..opts(home)
    };
    let c = run_once(Box::new(ObserveClaude::new(once(claude.path())))).await;
    let x = run_once(Box::new(ObserveCodex::new(once(codex.path())))).await;
    // Proof the adapters really read the homes, so an unchanged home means something.
    assert!(discovered(&c, &SessionKey::claude("S1")));
    assert!(discovered(&c, &SessionKey::claude_sub("S1", "a1")));
    assert!(discovered(&x, &SessionKey::codex("C1")));
    assert!(matches!(
        c.last(),
        Some(Event::CatchUpDone { tool: Tool::Claude })
    ));
    assert!(matches!(
        x.last(),
        Some(Event::CatchUpDone { tool: Tool::Codex })
    ));

    let live = |home: &Path| ObserveOptions {
        sweep: Duration::from_millis(400),
        ..opts(home)
    };
    let mut rc = Running::start(Box::new(ObserveClaude::new(live(claude.path()))), 64);
    let mut rx = Running::start(Box::new(ObserveCodex::new(live(codex.path()))), 64);
    rc.caught_up().await;
    rx.caught_up().await;
    rc.commands.send(AdapterCommand::Refresh).await.unwrap();
    rx.commands.send(AdapterCommand::Refresh).await.unwrap();
    let (a, b) = tokio::join!(
        rc.collect_for(Duration::from_secs(3)),
        rx.collect_for(Duration::from_secs(3))
    );
    // Nothing changed on disk, so the live phase must not invent sessions or drop them.
    let churn = |e: &Event| {
        matches!(
            e,
            Event::SessionGone { .. }
                | Event::SessionReset { .. }
                | Event::SessionDiscovered { .. }
        )
    };
    assert!(!a.iter().chain(&b).any(churn), "{a:?} {b:?}");
    rc.shutdown().await;
    rx.shutdown().await;

    let after = (fingerprint(claude.path()), fingerprint(codex.path()));
    assert_eq!(before, after, "observe mode changed a watched home");
}

fn sources(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            sources(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

/// F9: every source file of the crate that reads session files, not a hand-kept list.
#[test]
fn observe_sources_contain_no_write_calls() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    sources(&src, &mut files);
    for must in [
        "lib.rs",
        "adapter.rs",
        "tail.rs",
        "watch.rs",
        "observe/mod.rs",
        "observe/claude.rs",
        "observe/codex.rs",
        "observe/discover.rs",
        "observe/version.rs",
        "observe/repo.rs",
    ] {
        assert!(files.contains(&src.join(must)), "scan missed {must}");
    }
    let bad = [
        "OpenOptions",
        "File::options",
        "File::create",
        "fs::write",
        "fs::rename",
        "fs::remove",
        "fs::copy",
        "remove_file",
        "remove_dir",
        "create_dir",
        "hard_link",
        "symlink(",
        "set_len",
        "write_all",
        "set_permissions",
        "set_modified",
        "set_times",
        "Command::new",
        "kill(",
    ];
    for file in &files {
        let text = fs::read_to_string(file).unwrap();
        for b in bad {
            assert!(
                !text.contains(b),
                "observe code must stay read-only: found {b} in {}",
                file.display()
            );
        }
    }
}

#[test]
fn capabilities_are_read_only() {
    let read_only = Capabilities {
        can_answer: false,
        can_take_over: false,
        can_spawn: false,
        read_only: true,
    };
    let o = opts(Path::new("/nonexistent"));
    let claude = ObserveClaude::new(o.clone());
    let codex = ObserveCodex::new(o);
    assert_eq!(claude.capabilities(), read_only);
    assert_eq!(codex.capabilities(), read_only);
    assert_eq!(claude.kind(), AdapterKind::ObserveClaude);
    assert_eq!(codex.kind(), AdapterKind::ObserveCodex);
}

#[test]
fn default_options_poll_every_second_and_sweep_every_ten() {
    let o = opts(Path::new("/h"));
    assert_eq!(o.poll, Duration::from_secs(1));
    assert_eq!(o.sweep, Duration::from_secs(10));
    assert!(!o.once);
}

fn repo_of(events: &[Event], key: &SessionKey) -> Option<RepoRef> {
    events.iter().rev().find_map(|e| match e {
        Event::SessionDiscovered { key: k, repo, .. } if k == key => Some(repo.clone()),
        _ => None,
    })?
}

/// §7.7 step 4: resolving cwds to repositories reads `.git` entries and
/// writes nothing, in the repositories or the homes.
#[tokio::test(flavor = "multi_thread")]
async fn repo_resolution_never_modifies_repos_or_homes() {
    let repos = tempfile::tempdir().unwrap();
    repo_tree(repos.path());
    let root = fs::canonicalize(repos.path()).unwrap();
    let worktree = root.join("SafeAscent-r7v2/crates");

    let claude = tempfile::tempdir().unwrap();
    let project = claude.path().join("projects/-w");
    fs::create_dir_all(&project).unwrap();
    let line = |sid: &str, cwd: &Path| {
        format!(
            r#"{{"type":"user","uuid":"u1","parentUuid":null,"sessionId":"{sid}","timestamp":"2026-11-24T10:00:00.000Z","cwd":"{}","version":"2.1.331","message":{{"role":"user","content":"hi"}}}}"#,
            cwd.display()
        )
    };
    fs::write(project.join("W1.jsonl"), line("W1", &worktree) + "\n").unwrap();
    fs::write(
        project.join("N1.jsonl"),
        line("N1", &root.join("loose/dir")) + "\n",
    )
    .unwrap();

    let codex = tempfile::tempdir().unwrap();
    let day = codex.path().join("sessions/2026/11/24");
    fs::create_dir_all(&day).unwrap();
    fs::write(
        day.join("rollout-2026-11-24T10-00-00-X1.jsonl"),
        format!(
            r#"{{"timestamp":"2026-11-24T10:00:00Z","type":"session_meta","payload":{{"id":"X1","timestamp":"2026-11-24T10:00:00Z","cwd":"{}","cli_version":"0.158.0"}}}}"#,
            root.join("Plain/src/deep").display()
        ) + "\n",
    )
    .unwrap();

    let before = (
        fingerprint(repos.path()),
        fingerprint(claude.path()),
        fingerprint(codex.path()),
    );
    let once = |home: &Path| ObserveOptions {
        once: true,
        ..opts(home)
    };
    let c = run_once(Box::new(ObserveClaude::new(once(claude.path())))).await;
    let x = run_once(Box::new(ObserveCodex::new(once(codex.path())))).await;
    let named = |name: &str| {
        Some(RepoRef {
            name: name.into(),
            root: root.join(name),
        })
    };
    assert_eq!(repo_of(&c, &SessionKey::claude("W1")), named("SafeAscent"));
    assert!(discovered(&c, &SessionKey::claude("N1")));
    assert_eq!(repo_of(&c, &SessionKey::claude("N1")), None);
    assert_eq!(repo_of(&x, &SessionKey::codex("X1")), named("Plain"));
    let after = (
        fingerprint(repos.path()),
        fingerprint(claude.path()),
        fingerprint(codex.path()),
    );
    assert_eq!(before, after, "repo resolution changed a repo or a home");
}
