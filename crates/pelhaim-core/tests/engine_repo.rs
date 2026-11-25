//! §7.7 step 4 (owner, 2026-11-24): an untagged session lands on the line of
//! the repository its cwd lies in.

use std::path::PathBuf;

use pelhaim_core::testkit::*;
use pelhaim_core::{
    AlertKind, AssignTarget, Command, Engine, EngineConfig, Event, Input, LineKey, ProjectCfg,
    RepoRef, RestoredState, SessionKey, Snapshot, TagResolution, Tool,
};

fn t0() -> pelhaim_core::Ts {
    ts("2026-11-24T10:00:00Z")
}

fn repo(name: &str) -> Option<RepoRef> {
    Some(RepoRef {
        name: name.into(),
        root: PathBuf::from(format!("/r/{name}")),
    })
}

fn found(e: &mut Engine, key: &SessionKey, parent: Option<&SessionKey>, repo: Option<RepoRef>) {
    e.apply(Input::Event(Event::SessionDiscovered {
        key: key.clone(),
        tool: Tool::Claude,
        parent: parent.cloned(),
        path: PathBuf::from(format!("/fixtures/{}.jsonl", key.0)),
        cwd: Some("/somewhere".into()),
        repo,
        version: None,
        started: t0(),
    }));
}

fn line_of(s: &Snapshot, k: &SessionKey) -> LineKey {
    let t = s.trains.values().find(|t| &t.session == k).expect("train");
    s.tracks[&t.track].line.clone()
}

fn tag_of(s: &Snapshot, k: &SessionKey) -> TagResolution {
    s.trains
        .values()
        .find(|t| &t.session == k)
        .expect("train")
        .tag
        .clone()
}

fn line_names(s: &Snapshot) -> Vec<(LineKey, String)> {
    s.lines
        .iter()
        .map(|l| (l.key.clone(), l.name.clone()))
        .collect()
}

fn open_assigns(s: &Snapshot) -> Vec<String> {
    s.alerts
        .iter()
        .filter(|a| a.is_open() && matches!(a.kind, AlertKind::AssignProject { .. }))
        .map(|a| a.id.0.clone())
        .collect()
}

fn project(slug: &str) -> LineKey {
    LineKey::Project(slug.into())
}

#[test]
fn untagged_repo_session_auto_creates_a_line_named_after_the_repo() {
    let mut e = engine(&[]);
    let k = SessionKey::claude("S1");
    found(&mut e, &k, None, repo("SafeAscent"));
    tag(&mut e, &k, "Fix the flaky risk test");
    let s = e.snapshot();
    assert_eq!(line_of(&s, &k), project("safeascent"));
    assert_eq!(tag_of(&s, &k), TagResolution::Repo("safeascent".into()));
    assert!(
        open_assigns(&s).is_empty(),
        "repo-placed sessions never ask"
    );
    assert_eq!(
        line_names(&s),
        vec![
            (LineKey::Dispatch, "DISPATCH".into()),
            (project("safeascent"), "SafeAscent".into()),
            (LineKey::Unsorted, "UNSORTED".into()),
        ]
    );
}

#[test]
fn no_repo_and_no_tag_is_unsorted_with_an_assign_alert() {
    let mut e = engine(&[]);
    let k = SessionKey::claude("S1");
    found(&mut e, &k, None, None);
    tag(&mut e, &k, "Plan the week");
    let s = e.snapshot();
    assert_eq!(line_of(&s, &k), LineKey::Unsorted);
    assert_eq!(open_assigns(&s), ["assign:claude:S1"]);
}

#[test]
fn repo_matches_a_configured_slug_without_an_extra_line() {
    let mut e = engine(&["safeascent"]);
    let k = SessionKey::claude("S1");
    found(&mut e, &k, None, repo("SafeAscent"));
    tag(&mut e, &k, "hi");
    let s = e.snapshot();
    assert_eq!(line_of(&s, &k), project("safeascent"));
    assert_eq!(s.lines.len(), 3);
    assert_eq!(s.lines[1].name, "SAFEASCENT", "the configured name wins");
}

#[test]
fn repo_matches_a_configured_path_before_its_name() {
    let cfg = EngineConfig {
        projects: vec![ProjectCfg {
            slug: "sa".into(),
            name: "Safe".into(),
            order: 1,
            path: Some(PathBuf::from("/r/SafeAscent")),
            color: None,
        }],
        ..EngineConfig::default()
    };
    let mut e = Engine::new(cfg, RestoredState::default());
    let k = SessionKey::claude("S1");
    found(&mut e, &k, None, repo("SafeAscent"));
    tag(&mut e, &k, "hi");
    let s = e.snapshot();
    assert_eq!(line_of(&s, &k), project("sa"));
    assert_eq!(s.lines.len(), 3, "no auto line beside the configured one");
}

#[test]
fn a_tag_overrides_the_repo() {
    let mut e = engine(&["ethica"]);
    let k = SessionKey::claude("S1");
    found(&mut e, &k, None, repo("SafeAscent"));
    tag(&mut e, &k, "pelhaim-project: ethica\nRun the judge");
    let s = e.snapshot();
    assert_eq!(line_of(&s, &k), project("ethica"));
    assert_eq!(tag_of(&s, &k), TagResolution::Tagged("ethica".into()));
    assert!(!s.lines.iter().any(|l| l.key == project("safeascent")));
}

#[test]
fn an_owner_assignment_overrides_the_repo() {
    let mut e = engine(&["ethica"]);
    let k = SessionKey::claude("S1");
    found(&mut e, &k, None, repo("SafeAscent"));
    tag(&mut e, &k, "hi");
    e.apply(Input::Command(Command::Assign {
        session: k.clone(),
        target: AssignTarget::Project("ethica".into()),
        at: t0(),
    }));
    assert_eq!(line_of(&e.snapshot(), &k), project("ethica"));
}

#[test]
fn an_unconfigured_tag_gets_its_own_line_and_the_repo_names_it() {
    let mut e = engine(&[]);
    let a = SessionKey::claude("A");
    found(&mut e, &a, None, None);
    tag(&mut e, &a, "pelhaim-project: safeascent\nGo");
    let s = e.snapshot();
    assert_eq!(line_of(&s, &a), project("safeascent"));
    assert_eq!(tag_of(&s, &a), TagResolution::Tagged("safeascent".into()));
    assert!(open_assigns(&s).is_empty());
    assert_eq!(s.lines[1].name, "safeascent");

    let b = SessionKey::claude("B");
    found(&mut e, &b, None, repo("SafeAscent"));
    let s = e.snapshot();
    assert_eq!(line_of(&s, &b), project("safeascent"));
    assert_eq!(s.lines.len(), 3, "tag and repo share one line");
    assert_eq!(s.lines[1].name, "SafeAscent");
}

#[test]
fn a_later_cwd_moves_the_train_and_expires_its_assign_alert() {
    let mut e = engine(&[]);
    let k = SessionKey::claude("S1");
    found(&mut e, &k, None, None);
    tag(&mut e, &k, "hi");
    assert_eq!(open_assigns(&e.snapshot()).len(), 1);
    found(&mut e, &k, None, repo("cabn"));
    let s = e.snapshot();
    assert_eq!(line_of(&s, &k), project("cabn"));
    assert!(open_assigns(&s).is_empty());
}

#[test]
fn a_repo_subagent_of_the_orchestrator_registers_and_does_not_inherit_dispatch() {
    let mut e = engine(&[]);
    let o = SessionKey::claude("O");
    found(&mut e, &o, None, None);
    tag(&mut e, &o, "pelhaim-project: dispatch\nPlan");
    let in_repo = SessionKey::claude_sub("O", "a1");
    let bare = SessionKey::claude_sub("O", "a2");
    spawn(&mut e, &o, "toolu_1", "worktree work", "", t0());
    spawn(&mut e, &o, "toolu_2", "home dir work", "", t0());
    found(&mut e, &in_repo, Some(&o), repo("loosy-goose"));
    found(&mut e, &bare, Some(&o), None);
    link(&mut e, &in_repo, &o, "toolu_1");
    link(&mut e, &bare, &o, "toolu_2");
    let s = e.snapshot();
    assert_eq!(line_of(&s, &o), LineKey::Dispatch);
    assert_eq!(line_of(&s, &in_repo), project("loosy-goose"));
    assert_eq!(line_of(&s, &bare), LineKey::Unsorted);
    assert_eq!(open_assigns(&s), ["assign:claude:O/a2"]);
}

#[test]
fn auto_lines_are_assign_options() {
    let mut e = engine(&["ethica"]);
    let placed = SessionKey::claude("P");
    found(&mut e, &placed, None, repo("cabn"));
    let lost = SessionKey::claude("L");
    found(&mut e, &lost, None, None);
    tag(&mut e, &lost, "hi");
    let s = e.snapshot();
    let AlertKind::AssignProject { options, .. } = &s.alerts[0].kind else {
        panic!("expected an Assign alert");
    };
    let targets: Vec<&AssignTarget> = options.iter().map(|o| &o.target).collect();
    assert_eq!(
        targets,
        [
            &AssignTarget::Project("ethica".into()),
            &AssignTarget::Project("cabn".into()),
            &AssignTarget::Dispatch,
        ]
    );
}

#[test]
fn auto_projects_off_ignores_the_repo() {
    let mut e = engine_tags_only(&["safeascent"]);
    let k = SessionKey::claude("S1");
    found(&mut e, &k, None, repo("SafeAscent"));
    tag(&mut e, &k, "hi");
    let s = e.snapshot();
    assert_eq!(line_of(&s, &k), LineKey::Unsorted);
    assert_eq!(tag_of(&s, &k), TagResolution::Untagged);
    assert_eq!(open_assigns(&s).len(), 1);
}

#[test]
fn a_decision_on_an_auto_line_carries_the_line_name() {
    let mut e = engine(&["ethica"]);
    let k = SessionKey::claude("S1");
    found(&mut e, &k, None, repo("SafeAscent"));
    tag(&mut e, &k, "hi");
    ask(
        &mut e,
        &k,
        "toolu_q",
        vec![question("Stage", "Go?", &["Yes", "No"])],
        t0(),
    );
    answered(&mut e, &k, "toolu_q", "Yes", t0() + secs(5));
    let log = |e: &mut Engine, project: Option<&str>| {
        let id = e.snapshot().alerts[0].id.clone();
        e.apply(Input::Command(Command::LogDecision {
            alert: id,
            project: project.map(str::to_string),
            at: t0() + secs(9),
        }));
        e.snapshot().decisions_recent[0].clone()
    };
    let d = log(&mut e, None);
    assert_eq!(d.project, "safeascent");
    assert_eq!(d.project_name.as_deref(), Some("SafeAscent"));

    let mut e = engine(&["ethica"]);
    found(&mut e, &k, None, repo("ethica"));
    tag(&mut e, &k, "hi");
    ask(
        &mut e,
        &k,
        "toolu_q",
        vec![question("Stage", "Go?", &["Yes", "No"])],
        t0(),
    );
    answered(&mut e, &k, "toolu_q", "Yes", t0() + secs(5));
    let d = log(&mut e, None);
    assert_eq!(d.project, "ethica");
    assert_eq!(d.project_name, None, "a configured project needs no name");
}
