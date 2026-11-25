use pelhaim_core::testkit::*;
use pelhaim_core::{
    AlertEvent, AlertId, AlertKind, AlertState, AnsweredBy, AssignOption, AssignTarget, Effect,
    Engine, EngineConfig, Event, LineKey, RestoredState, SessionKey, Snapshot, TagResolution,
    TrainState, format_train_number,
};

fn t0() -> pelhaim_core::Ts {
    ts("2026-11-23T10:00:00Z")
}

fn key(sid: &str) -> SessionKey {
    SessionKey::claude(sid)
}

fn train_state(s: &Snapshot, k: &SessionKey) -> TrainState {
    s.trains
        .values()
        .find(|t| &t.session == k)
        .unwrap_or_else(|| panic!("no train for {k:?}"))
        .state
        .clone()
}

fn alert_effects(fx: &[Effect]) -> Vec<AlertEvent> {
    fx.iter()
        .filter_map(|f| match f {
            Effect::Alert(a) => Some(a.clone()),
            _ => None,
        })
        .collect()
}

fn tagged_session(e: &mut Engine, k: &SessionKey, slug: &str, at: pelhaim_core::Ts) {
    discover_in(e, k, None, at);
    tag(e, k, &format!("pelhaim-project: {slug}\nwork"));
    prompt(e, k, "work", at);
}

fn stage_c1() -> Vec<pelhaim_core::Question> {
    vec![question("Stage C1", "Proceed to C1?", &["Proceed", "Hold"])]
}

#[test]
fn ask_user_question_raises_read_only_alert_and_yellow_train() {
    let mut e = engine(&["safeascent"]);
    let k = key("S1");
    tagged_session(&mut e, &k, "safeascent", t0());
    tool_open(&mut e, &k, "toolu_q", "AskUserQuestion", t0() + secs(5));
    let fx = ask(&mut e, &k, "toolu_q", stage_c1(), t0() + secs(5));

    let snap = e.snapshot();
    assert_eq!(snap.alerts.len(), 1);
    let a = &snap.alerts[0];
    let id = AlertId("q:claude:S1:toolu_q".into());
    assert_eq!(a.id, id);
    assert_eq!(
        a.kind,
        AlertKind::Question {
            questions: stage_c1()
        }
    );
    assert_eq!(a.kind.tab(), "Question");
    assert!(!a.answerable);
    assert_eq!(a.state, AlertState::Open);
    assert_eq!(a.line, LineKey::Project("safeascent".into()));
    assert_eq!(a.created, t0() + secs(5));
    assert_eq!(train_state(&snap, &k), TrainState::Waiting(id));
    assert_eq!(alert_effects(&fx), vec![AlertEvent::Raised(a.clone())]);
    assert!(
        snap.lines
            .iter()
            .find(|l| l.key == LineKey::Project("safeascent".into()))
            .unwrap()
            .has_open_alert
    );

    // A train waiting on the owner neither stalls nor goes "probably waiting".
    tick(&mut e, t0() + secs(30 * 60));
    let snap = e.snapshot();
    assert!(matches!(train_state(&snap, &k), TrainState::Waiting(_)));
    assert_eq!(snap.alerts.len(), 1);
}

#[test]
fn terminal_answer_closes_alert() {
    let mut e = engine(&["safeascent"]);
    let k = key("S1");
    tagged_session(&mut e, &k, "safeascent", t0());
    tool_open(&mut e, &k, "toolu_q", "AskUserQuestion", t0() + secs(5));
    ask(&mut e, &k, "toolu_q", stage_c1(), t0() + secs(5));
    let fx = answered(&mut e, &k, "toolu_q", "Proceed", t0() + secs(40));

    let snap = e.snapshot();
    let a = &snap.alerts[0];
    assert_eq!(
        a.state,
        AlertState::Answered {
            answer: "Proceed".into(),
            by: AnsweredBy::Terminal,
            at: t0() + secs(40),
        }
    );
    assert!(!a.is_open());
    assert_eq!(train_state(&snap, &k), TrainState::Working);
    assert_eq!(alert_effects(&fx), vec![AlertEvent::Answered(a.clone())]);
    assert!(snap.lines.iter().all(|l| !l.has_open_alert));
}

#[test]
fn open_tool_call_becomes_probably_waiting_after_20s() {
    let mut e = engine(&["safeascent"]);
    let k = key("S1");
    tagged_session(&mut e, &k, "safeascent", t0());
    tool_open(&mut e, &k, "toolu_b", "Bash", t0());

    tick(&mut e, t0() + secs(19));
    assert_eq!(train_state(&e.snapshot(), &k), TrainState::Working);

    let fx = tick(&mut e, t0() + secs(21));
    let snap = e.snapshot();
    assert_eq!(train_state(&snap, &k), TrainState::ProbablyWaiting);
    assert!(snap.alerts.is_empty());
    assert!(alert_effects(&fx).is_empty());

    tool_close(&mut e, &k, "toolu_b", t0() + secs(25));
    assert_eq!(train_state(&e.snapshot(), &k), TrainState::Working);
}

#[test]
fn excluded_tools_never_trigger_probably_waiting() {
    for name in ["AskUserQuestion", "Agent", "Task"] {
        let mut e = engine(&["safeascent"]);
        let k = key("S1");
        tagged_session(&mut e, &k, "safeascent", t0());
        tool_open(&mut e, &k, "toolu_x", name, t0());
        tick(&mut e, t0() + secs(60));
        assert_eq!(
            train_state(&e.snapshot(), &k),
            TrainState::Working,
            "{name}"
        );
    }
}

#[test]
fn agent_call_with_live_subagent_is_not_probably_waiting() {
    let mut e = engine(&["ethica"]);
    let p = key("P1");
    let c = SessionKey::claude_sub("P1", "a1");
    tagged_session(&mut e, &p, "ethica", t0());
    spawn(&mut e, &p, "toolu_A", "child work", "", t0());
    discover_in(&mut e, &c, Some(&p), t0() + secs(1));
    link(&mut e, &c, &p, "toolu_A");
    activity(&mut e, &c, t0() + secs(300));
    tick(&mut e, t0() + secs(330));

    let snap = e.snapshot();
    assert_eq!(train_state(&snap, &p), TrainState::Working);
    assert_eq!(train_state(&snap, &c), TrainState::Working);
    assert!(snap.alerts.iter().all(|a| a.kind.tab() != "Stalled"));
}

#[test]
fn berthed_parent_waiting_on_children_is_not_probably_waiting() {
    let mut e = engine(&["ethica"]);
    let p = key("P1");
    let c = SessionKey::claude_sub("P1", "a1");
    tagged_session(&mut e, &p, "ethica", t0());
    // A call left open when the turn ended (interrupted), plus a background launch.
    tool_open(&mut e, &p, "toolu_orphan", "Bash", t0());
    spawn(&mut e, &p, "toolu_A", "child work", "", t0() + secs(1));
    discover_in(&mut e, &c, Some(&p), t0() + secs(2));
    link(&mut e, &c, &p, "toolu_A");
    background_launched(&mut e, &p, "toolu_A", t0() + secs(2));
    turn_end(&mut e, &p, None, t0() + secs(3));
    activity(&mut e, &c, t0() + secs(5));

    tick(&mut e, t0() + secs(40));
    let snap = e.snapshot();
    let parent = snap.trains.values().find(|t| t.session == p).unwrap();
    assert!(parent.position.berthed);
    assert_eq!(parent.state, TrainState::Working);
}

#[test]
fn subagent_writes_keep_parent_from_stalling() {
    let mut e = engine(&["ethica"]);
    let p = key("P1");
    let c = SessionKey::claude_sub("P1", "a1");
    tagged_session(&mut e, &p, "ethica", t0());
    spawn(&mut e, &p, "toolu_A", "child work", "", t0());
    discover_in(&mut e, &c, Some(&p), t0() + secs(1));
    link(&mut e, &c, &p, "toolu_A");
    for m in 1..=30 {
        activity(&mut e, &c, t0() + secs(60 * m));
        tick(&mut e, t0() + secs(60 * m + 30));
    }
    let snap = e.snapshot();
    assert_eq!(train_state(&snap, &p), TrainState::Working);
    assert!(snap.alerts.iter().all(|a| a.kind != AlertKind::Stalled));
}

fn background_parent(e: &mut Engine, p: &SessionKey, c: &SessionKey) {
    tagged_session(e, p, "ethica", t0());
    spawn(e, p, "toolu_A", "child work", "", t0());
    discover_in(e, c, Some(p), t0() + secs(1));
    link(e, c, p, "toolu_A");
    background_launched(e, p, "toolu_A", t0() + secs(1));
    turn_end(e, p, None, t0() + secs(2));
}

#[test]
fn stalled_child_does_not_keep_parent_green() {
    let mut e = engine(&["ethica"]);
    let (p, c) = (key("P1"), SessionKey::claude_sub("P1", "a1"));
    background_parent(&mut e, &p, &c);
    assert_eq!(train_state(&e.snapshot(), &p), TrainState::Working);

    tick(&mut e, t0() + secs(11 * 60));
    let snap = e.snapshot();
    assert_eq!(train_state(&snap, &c), TrainState::Stalled);
    assert_eq!(train_state(&snap, &p), TrainState::Idle);
    let pid = snap.trains.values().find(|t| t.session == p).unwrap().id;
    assert_eq!(e.effective_last_write(pid), Some(t0() + secs(2)));

    // The child coming back turns the parent green again.
    activity(&mut e, &c, t0() + secs(12 * 60));
    let snap = e.snapshot();
    assert_eq!(train_state(&snap, &c), TrainState::Working);
    assert_eq!(train_state(&snap, &p), TrainState::Working);
}

#[test]
fn failed_child_does_not_keep_parent_green() {
    let mut e = engine(&["ethica"]);
    let (p, c) = (key("P1"), SessionKey::claude_sub("P1", "a1"));
    background_parent(&mut e, &p, &c);
    e.apply(ev(Event::Failed {
        key: c.clone(),
        reason: "api error".into(),
        at: t0() + secs(30),
    }));
    let snap = e.snapshot();
    assert_eq!(train_state(&snap, &p), TrainState::Idle);
    let pid = snap.trains.values().find(|t| t.session == p).unwrap().id;
    assert_eq!(e.effective_last_write(pid), Some(t0() + secs(2)));
}

#[test]
fn stall_raises_trying_to_contact_and_recovers() {
    let mut e = engine(&["safeascent"]);
    let k = key("S1");
    tagged_session(&mut e, &k, "safeascent", t0());

    tick(&mut e, t0() + secs(10 * 60 - 1));
    assert_eq!(train_state(&e.snapshot(), &k), TrainState::Working);

    let fx = tick(&mut e, t0() + secs(10 * 60));
    let snap = e.snapshot();
    assert_eq!(train_state(&snap, &k), TrainState::Stalled);
    let a = snap.alerts[0].clone();
    assert_eq!(a.id, AlertId("stalled:claude:S1:0".into()));
    assert_eq!(a.kind, AlertKind::Stalled);
    assert_eq!(a.kind.tab(), "Stalled");
    assert!(!a.answerable);
    assert_eq!(alert_effects(&fx), vec![AlertEvent::Raised(a.clone())]);
    // The popup body is "Trying to contact train <number>…", built from the train's number.
    let number = snap.trains[&a.train.unwrap()].number;
    assert_eq!(format_train_number(number), "1000");

    let fx = activity(&mut e, &k, t0() + secs(12 * 60));
    let snap = e.snapshot();
    assert_eq!(train_state(&snap, &k), TrainState::Working);
    assert!(snap.alerts.is_empty());
    let mut expired = a;
    expired.state = AlertState::Expired;
    assert_eq!(alert_effects(&fx), vec![AlertEvent::Updated(expired)]);

    // F18: the recovering event's own rule then sets the state.
    tick(&mut e, t0() + secs(30 * 60));
    assert_eq!(train_state(&e.snapshot(), &k), TrainState::Stalled);
    turn_end(&mut e, &k, None, t0() + secs(31 * 60));
    let snap = e.snapshot();
    assert_eq!(train_state(&snap, &k), TrainState::Idle);
    assert!(snap.alerts.is_empty());
}

#[test]
fn untagged_raises_assign_with_orchestrator_option_last() {
    let mut e = engine(&["safeascent", "ethica"]);
    let k = key("U1");
    discover_in(&mut e, &k, None, t0());
    tag(&mut e, &k, "just do the thing");

    let snap = e.snapshot();
    assert_eq!(snap.alerts.len(), 1);
    let a = &snap.alerts[0];
    assert_eq!(a.id, AlertId("assign:claude:U1".into()));
    assert_eq!(a.kind.tab(), "Assign");
    assert!(a.answerable);
    assert!(a.kind.remote_answerable());
    assert_eq!(a.line, LineKey::Unsorted);
    assert_eq!(
        a.kind,
        AlertKind::AssignProject {
            session: k.clone(),
            options: vec![
                AssignOption {
                    label: "SAFEASCENT".into(),
                    target: AssignTarget::Project("safeascent".into()),
                },
                AssignOption {
                    label: "ETHICA".into(),
                    target: AssignTarget::Project("ethica".into()),
                },
                AssignOption {
                    label: "This is the orchestrator".into(),
                    target: AssignTarget::Dispatch,
                },
            ],
            prefill: None,
        }
    );
}

#[test]
fn tagged_session_has_no_assign_alert() {
    let mut e = engine(&["safeascent"]);
    let k = key("S1");
    let mut fx = e.apply(ev(Event::SessionDiscovered {
        key: k.clone(),
        tool: pelhaim_core::Tool::Claude,
        parent: None,
        path: "/fixtures/S1.jsonl".into(),
        cwd: None,
        repo: None,
        version: None,
        started: t0(),
    }));
    assert!(e.snapshot().alerts.is_empty());
    fx.extend(tag(&mut e, &k, "pelhaim-project: safeascent\nhi"));
    fx.extend(prompt(&mut e, &k, "hi", t0()));
    assert!(e.snapshot().alerts.is_empty());
    assert!(alert_effects(&fx).is_empty());
}

#[test]
fn untagged_session_gets_assign_only_once_tag_state_is_seen() {
    let mut e = engine(&["safeascent"]);
    let k = key("U1");
    discover_in(&mut e, &k, None, t0());
    let fx = prompt(&mut e, &k, "hi", t0());
    assert!(e.snapshot().alerts.is_empty());
    assert!(alert_effects(&fx).is_empty());

    let fx = tag(&mut e, &k, "no tag here");
    let snap = e.snapshot();
    assert_eq!(snap.alerts.len(), 1);
    assert_eq!(snap.alerts[0].id, AlertId("assign:claude:U1".into()));
    assert_eq!(
        alert_effects(&fx),
        vec![AlertEvent::Raised(snap.alerts[0].clone())]
    );
}

#[test]
fn subagent_gets_no_assign_alert_until_linked() {
    let mut e = engine(&["ethica"]);
    let p = key("P1");
    let (untagged_child, tagged_child) = (
        SessionKey::claude_sub("P1", "a1"),
        SessionKey::claude_sub("P1", "a2"),
    );
    tagged_session(&mut e, &p, "ethica", t0());
    let assign_ids = |e: &Engine| {
        e.snapshot()
            .alerts
            .iter()
            .filter(|a| a.kind.tab() == "Assign")
            .map(|a| a.id.0.clone())
            .collect::<Vec<_>>()
    };

    // Discovered before the link, and even with its own first message seen.
    for c in [&untagged_child, &tagged_child] {
        discover_in(&mut e, c, Some(&p), t0() + secs(1));
        tag(&mut e, c, "do the work");
    }
    assert!(assign_ids(&e).is_empty());

    spawn(
        &mut e,
        &p,
        "toolu_t",
        "tagged",
        "pelhaim-project: ethica",
        t0() + secs(2),
    );
    link(&mut e, &tagged_child, &p, "toolu_t");
    assert!(assign_ids(&e).is_empty());

    spawn(&mut e, &p, "toolu_u", "untagged", "", t0() + secs(4));
    link(&mut e, &untagged_child, &p, "toolu_u");
    assert_eq!(assign_ids(&e), ["assign:claude:P1/a1"]);
}

#[test]
fn mis_tagged_prefills_closest_slug() {
    let mut e = engine_tags_only(&["safeascent", "ethica"]);
    let k = key("M1");
    discover_in(&mut e, &k, None, t0());
    tag(&mut e, &k, "pelhaim-project: ethika\nhi");
    let snap = e.snapshot();
    assert_eq!(
        snap.trains.values().next().unwrap().tag,
        TagResolution::MisTagged("ethika".into())
    );
    match &snap.alerts[0].kind {
        AlertKind::AssignProject { prefill, .. } => assert_eq!(*prefill, Some(1)),
        k => panic!("expected AssignProject, got {k:?}"),
    }
}

#[test]
fn result_alerts_only_when_enabled() {
    let mut off = engine(&["safeascent"]);
    let k = key("S1");
    tagged_session(&mut off, &k, "safeascent", t0());
    turn_end(&mut off, &k, Some("All done."), t0() + secs(5));
    assert!(off.snapshot().alerts.is_empty());

    let mut on = Engine::new(
        EngineConfig {
            result_alerts: true,
            ..cfg(&["safeascent"])
        },
        RestoredState::default(),
    );
    tagged_session(&mut on, &k, "safeascent", t0());
    let long = "x".repeat(700);
    let fx = turn_end(&mut on, &k, Some(&long), t0() + secs(5));
    let snap = on.snapshot();
    let a = &snap.alerts[0];
    assert_eq!(a.id, AlertId("result:claude:S1:1".into()));
    assert_eq!(a.kind.tab(), "Result");
    assert!(!a.kind.remote_answerable());
    match &a.kind {
        AlertKind::Result { summary } => {
            assert_eq!(summary.chars().count(), 600);
            assert!(summary.ends_with('…'));
        }
        k => panic!("expected Result, got {k:?}"),
    }
    assert_eq!(alert_effects(&fx), vec![AlertEvent::Raised(a.clone())]);

    prompt(&mut on, &k, "again", t0() + secs(10));
    turn_end(&mut on, &k, None, t0() + secs(20));
    assert_eq!(on.snapshot().alerts.len(), 1);
}

#[test]
fn session_gone_expires_alerts() {
    let mut e = engine(&["safeascent"]);
    let k = key("S1");
    tagged_session(&mut e, &k, "safeascent", t0());
    ask(&mut e, &k, "toolu_q", stage_c1(), t0() + secs(5));

    // A reset and replay rebuilds exactly one alert.
    e.apply(ev(Event::SessionReset { key: k.clone() }));
    tag(&mut e, &k, "pelhaim-project: safeascent\nwork");
    prompt(&mut e, &k, "work", t0());
    ask(&mut e, &k, "toolu_q", stage_c1(), t0() + secs(5));
    let snap = e.snapshot();
    assert_eq!(snap.alerts.len(), 1);
    assert!(matches!(train_state(&snap, &k), TrainState::Waiting(_)));

    let fx = e.apply(ev(Event::SessionGone { key: k.clone() }));
    assert!(e.snapshot().alerts.is_empty());
    assert!(matches!(
        alert_effects(&fx).as_slice(),
        [AlertEvent::Updated(a)] if a.state == AlertState::Expired && a.id.0 == "q:claude:S1:toolu_q"
    ));
}

#[test]
fn tracks_and_lines_with_open_alert_sort_first() {
    let mut e = engine(&["alpha", "beta"]);
    let (b1, b2, a1) = (key("B1"), key("B2"), key("A1"));
    tagged_session(&mut e, &b1, "beta", t0());
    ask(&mut e, &b1, "toolu_q", stage_c1(), t0() + secs(1));
    tagged_session(&mut e, &b2, "beta", t0() + secs(2));
    tagged_session(&mut e, &a1, "alpha", t0() + secs(3));

    let names = |s: &Snapshot| s.lines.iter().map(|l| l.name.clone()).collect::<Vec<_>>();
    let beta_tracks = |s: &Snapshot| {
        let l = s.lines.iter().find(|l| l.name == "BETA").unwrap();
        l.tracks
            .iter()
            .map(|tr| s.tracks[tr].session.clone())
            .collect::<Vec<_>>()
    };
    let snap = e.snapshot();
    assert_eq!(names(&snap), ["DISPATCH", "BETA", "ALPHA", "UNSORTED"]);
    // B1 waits on the owner, B2 is working and more recent; the open alert wins.
    assert_eq!(beta_tracks(&snap), [b1.clone(), b2.clone()]);

    // Dismissed to the tray is still unresolved, so it still floats.
    let id = snap.alerts[0].id.clone();
    e.apply(pelhaim_core::Input::Command(
        pelhaim_core::Command::Dismiss {
            alert: id,
            at: t0() + secs(4),
        },
    ));
    assert_eq!(
        names(&e.snapshot()),
        ["DISPATCH", "BETA", "ALPHA", "UNSORTED"]
    );

    answered(&mut e, &b1, "toolu_q", "Proceed", t0() + secs(5));
    turn_end(&mut e, &b1, None, t0() + secs(6));
    let snap = e.snapshot();
    assert_eq!(names(&snap), ["DISPATCH", "ALPHA", "BETA", "UNSORTED"]);
    assert_eq!(beta_tracks(&snap), [b2, b1]);
}

#[test]
fn snapshot_alerts_in_created_order() {
    let mut e = engine(&["alpha"]);
    let (x, y) = (key("X1"), key("Y1"));
    tagged_session(&mut e, &x, "alpha", t0());
    tagged_session(&mut e, &y, "alpha", t0());
    ask(&mut e, &x, "q2", stage_c1(), t0() + secs(20));
    ask(&mut e, &y, "q1", stage_c1(), t0() + secs(10));
    let ids: Vec<String> = e.snapshot().alerts.iter().map(|a| a.id.0.clone()).collect();
    assert_eq!(ids, ["q:claude:Y1:q1", "q:claude:X1:q2"]);
}

#[test]
fn sample_board_matches_its_description() {
    let s = sample_board();
    let by_label = |n: u16| s.trains.values().find(|t| t.number == n).unwrap();
    let names: Vec<&str> = s.lines.iter().map(|l| l.name.as_str()).collect();
    assert_eq!(names[0], "DISPATCH");
    assert!(names.contains(&"STRINGCAN"));
    let stringcan = s.lines.iter().find(|l| l.name == "STRINGCAN").unwrap();
    assert!(!stringcan.active);

    let s1 = by_label(1423);
    assert_eq!(
        s.tracks[&s1.track].line,
        LineKey::Project("safeascent".into())
    );
    let q = s
        .alerts
        .iter()
        .find(|a| a.kind.tab() == "Question")
        .unwrap();
    assert_eq!(s1.state, TrainState::Waiting(q.id.clone()));
    match &q.kind {
        AlertKind::Question { questions } => {
            assert_eq!(questions[0].header, "Stage C1");
            assert_eq!(questions[0].question, "Proceed to C1?");
            assert_eq!(questions[0].options.len(), 2);
        }
        _ => unreachable!(),
    }

    let dispatch = s.lines.iter().find(|l| l.key == LineKey::Dispatch).unwrap();
    assert!(
        dispatch.tracks.is_empty(),
        "the orchestrator draws no train"
    );
    assert!(dispatch.active);
    assert_eq!(dispatch.running, 1);
    let subs: Vec<_> = s
        .trains
        .values()
        .filter(|t| t.parent.is_some() && t.session.0.starts_with("claude:D1/"))
        .collect();
    assert_eq!(subs.len(), 2);
    let on = |slug: &str| {
        subs.iter()
            .find(|t| s.tracks[&t.track].line == LineKey::Project(slug.into()))
            .unwrap()
            .state
            .clone()
    };
    assert_eq!(on("ethica"), TrainState::Working);
    assert_eq!(on("cabn"), TrainState::Done);
    assert_eq!(
        s.forks.len(),
        1,
        "only S1's fork; forks off DISPATCH are not drawn"
    );

    let state_on = |slug: &str| {
        s.trains
            .values()
            .find(|t| {
                t.parent.is_none() && s.tracks[&t.track].line == LineKey::Project(slug.into())
            })
            .unwrap()
            .state
            .clone()
    };
    assert_eq!(state_on("ethica"), TrainState::Stalled);
    assert_eq!(
        state_on("loosy-goose"),
        TrainState::Failed("aborted".into())
    );
    assert!(s.alerts.iter().any(|a| a.kind.tab() == "Stalled"));
    assert!(s.alerts.iter().any(|a| a.kind.tab() == "Assign"));
    let unsorted = s.lines.iter().find(|l| l.key == LineKey::Unsorted).unwrap();
    assert_eq!(unsorted.tracks.len(), 1);

    let sa = s.lines.iter().find(|l| l.name == "SAFEASCENT").unwrap();
    assert_eq!(sa.open_decisions, 2);
    assert!(s.keep_awake.held);
    assert_eq!(s.keep_awake.count, 3);
    assert_eq!(s.keep_awake.mechanism.as_deref(), Some("caffeinate"));
}
