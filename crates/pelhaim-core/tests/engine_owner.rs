use std::collections::{BTreeMap, BTreeSet};

use pelhaim_core::testkit::*;
use pelhaim_core::{
    AlertEvent, AlertId, AlertKind, AlertState, Answer, AnswerOutcome, AnsweredBy, AssignTarget,
    Choice, Command, Decision, DecisionId, Effect, Engine, EngineConfig, Event, Input, LineKey,
    Origin, OwnerEvent, RestoredState, SessionKey, Snapshot, TagResolution, Ts, Who,
};

fn t0() -> Ts {
    ts("2026-11-23T10:00:00Z")
}

fn key(sid: &str) -> SessionKey {
    SessionKey::claude(sid)
}

fn cmd(e: &mut Engine, c: Command) -> Vec<Effect> {
    e.apply(Input::Command(c))
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

fn alert(s: &Snapshot, id: &str) -> pelhaim_core::Alert {
    s.alerts
        .iter()
        .find(|a| a.id.0 == id)
        .unwrap_or_else(|| panic!("no alert {id}"))
        .clone()
}

fn untagged(e: &mut Engine, k: &SessionKey, at: Ts) {
    discover_in(e, k, None, at);
    tag(e, k, "no tag here");
}

fn local(alert: &AlertId, choice: Choice) -> Answer {
    Answer {
        alert_id: alert.clone(),
        choice,
        origin: Origin::Local,
    }
}

fn remote(alert: &AlertId, choice: Choice) -> Answer {
    Answer {
        alert_id: alert.clone(),
        choice,
        origin: Origin::Remote {
            device_id: "phone".into(),
        },
    }
}

fn observed_question(e: &mut Engine, k: &SessionKey, slug: &str, header: &str) -> AlertId {
    discover_in(e, k, None, t0());
    tag(e, k, &format!("pelhaim-project: {slug}\nwork"));
    prompt(e, k, "work", t0());
    ask(
        e,
        k,
        "toolu_q",
        vec![question(header, "Proceed to C1?", &["Proceed", "Hold"])],
        t0() + secs(5),
    );
    answered(e, k, "toolu_q", "Proceed", t0() + secs(30));
    AlertId(format!("q:{}:toolu_q", k.0))
}

#[test]
fn assign_moves_train_and_later_untagged_subagents_only() {
    let mut e = engine(&["safeascent", "ethica"]);
    let p = key("U1");
    let (old, new) = (
        SessionKey::claude_sub("U1", "old"),
        SessionKey::claude_sub("U1", "new"),
    );
    untagged(&mut e, &p, t0());
    discover_in(&mut e, &old, Some(&p), t0() + secs(60));

    let at = t0() + secs(120);
    let fx = cmd(
        &mut e,
        Command::Assign {
            session: p.clone(),
            target: AssignTarget::Project("ethica".into()),
            at,
        },
    );
    let snap = e.snapshot();
    assert_eq!(line_of(&snap, &p), LineKey::Project("ethica".into()));
    assert_eq!(
        tag_of(&snap, &p),
        TagResolution::Assigned(AssignTarget::Project("ethica".into()))
    );
    assert_eq!(line_of(&snap, &old), LineKey::Unsorted);

    // F40: the command also answers the session's open Assign alert.
    let a = alert(&snap, "assign:claude:U1");
    assert_eq!(
        a.state,
        AlertState::Answered {
            answer: "ETHICA".into(),
            by: AnsweredBy::Local,
            at,
        }
    );
    assert_eq!(
        fx[0],
        Effect::Persist(
            OwnerEvent::Assigned {
                session: p.clone(),
                target: AssignTarget::Project("ethica".into()),
            },
            at
        )
    );
    assert!(fx.contains(&Effect::Alert(AlertEvent::Answered(a))));

    discover_in(&mut e, &new, Some(&p), t0() + secs(180));
    let snap = e.snapshot();
    assert_eq!(line_of(&snap, &new), LineKey::Project("ethica".into()));
    assert!(
        snap.alerts
            .iter()
            .all(|a| a.id.0 != format!("assign:{}", new.0))
    );
}

#[test]
fn assign_never_overrides_tagged_subagent() {
    let mut e = engine_tags_only(&["ethica", "cabn"]);
    let p = key("U1");
    let c = SessionKey::claude_sub("U1", "a1");
    let m = SessionKey::claude_sub("U1", "a2");
    untagged(&mut e, &p, t0());
    cmd(
        &mut e,
        Command::Assign {
            session: p.clone(),
            target: AssignTarget::Project("ethica".into()),
            at: t0() + secs(1),
        },
    );
    prompt(&mut e, &p, "go", t0() + secs(2));
    spawn(
        &mut e,
        &p,
        "toolu_c",
        "tiles",
        "pelhaim-project: cabn",
        t0() + secs(3),
    );
    discover_in(&mut e, &c, Some(&p), t0() + secs(4));
    link(&mut e, &c, &p, "toolu_c");
    spawn(
        &mut e,
        &p,
        "toolu_m",
        "typo",
        "pelhaim-project: cabm",
        t0() + secs(5),
    );
    discover_in(&mut e, &m, Some(&p), t0() + secs(6));
    link(&mut e, &m, &p, "toolu_m");

    let snap = e.snapshot();
    assert_eq!(line_of(&snap, &c), LineKey::Project("cabn".into()));
    assert_eq!(tag_of(&snap, &c), TagResolution::Tagged("cabn".into()));
    // Mis-tagged is not Untagged: the parent's assignment does not cover it.
    assert_eq!(line_of(&snap, &m), LineKey::Unsorted);
    assert_eq!(tag_of(&snap, &m), TagResolution::MisTagged("cabm".into()));
}

#[test]
fn first_answer_wins() {
    let mut e = engine(&["safeascent", "ethica"]);
    let k = key("U1");
    untagged(&mut e, &k, t0());
    let id = AlertId("assign:claude:U1".into());
    let at = t0() + secs(10);

    let (out, fx) = e.answer(local(&id, Choice::Options(vec![1])), at);
    assert_eq!(out, AnswerOutcome::Accepted);
    let snap = e.snapshot();
    let a = alert(&snap, &id.0);
    assert_eq!(
        a.state,
        AlertState::Answered {
            answer: "ETHICA".into(),
            by: AnsweredBy::Local,
            at,
        }
    );
    assert_eq!(
        fx,
        vec![
            Effect::Persist(
                OwnerEvent::Answered {
                    alert: id.clone(),
                    choice: Choice::Options(vec![1]),
                    origin: Origin::Local,
                },
                at
            ),
            Effect::Persist(
                OwnerEvent::Assigned {
                    session: k.clone(),
                    target: AssignTarget::Project("ethica".into()),
                },
                at
            ),
            Effect::Alert(AlertEvent::Answered(a)),
        ]
    );
    assert_eq!(line_of(&snap, &k), LineKey::Project("ethica".into()));

    let (out, fx) = e.answer(remote(&id, Choice::Options(vec![0])), at + secs(5));
    assert_eq!(
        out,
        AnswerOutcome::AlreadyAnswered {
            by: AnsweredBy::Local,
            at
        }
    );
    assert!(fx.is_empty());
    assert_eq!(
        line_of(&e.snapshot(), &k),
        LineKey::Project("ethica".into())
    );
}

#[test]
fn remote_answer_assigns_to_dispatch() {
    let mut e = engine(&["safeascent"]);
    let k = key("U1");
    untagged(&mut e, &k, t0());
    let id = AlertId("assign:claude:U1".into());
    let (out, _) = e.answer(remote(&id, Choice::Options(vec![1])), t0() + secs(3));
    assert_eq!(out, AnswerOutcome::Accepted);
    let snap = e.snapshot();
    assert_eq!(line_of(&snap, &k), LineKey::Dispatch);
    assert!(matches!(
        alert(&snap, &id.0).state,
        AlertState::Answered { by: AnsweredBy::Remote { ref device_id }, .. } if device_id == "phone"
    ));
}

#[test]
fn observed_question_not_answerable() {
    let mut e = engine(&["safeascent"]);
    let k = key("S1");
    discover_in(&mut e, &k, None, t0());
    tag(&mut e, &k, "pelhaim-project: safeascent\nwork");
    ask(
        &mut e,
        &k,
        "toolu_q",
        vec![question("Stage C1", "Proceed?", &["Proceed", "Hold"])],
        t0(),
    );
    let id = AlertId("q:claude:S1:toolu_q".into());
    for a in [
        local(&id, Choice::Options(vec![0])),
        remote(&id, Choice::Options(vec![0])),
    ] {
        let (out, fx) = e.answer(a, t0() + secs(1));
        assert_eq!(out, AnswerOutcome::NotAnswerable);
        assert!(fx.is_empty());
    }
    assert_eq!(alert(&e.snapshot(), &id.0).state, AlertState::Open);

    let (out, _) = e.answer(
        local(&AlertId("q:nope".into()), Choice::Options(vec![0])),
        t0(),
    );
    assert_eq!(out, AnswerOutcome::UnknownAlert);
}

#[test]
fn invalid_choice_rejected() {
    let mut e = engine(&["safeascent", "ethica"]);
    let k = key("U1");
    untagged(&mut e, &k, t0());
    let id = AlertId("assign:claude:U1".into());
    for choice in [
        Choice::Options(vec![99]),
        Choice::Text("x".into()),
        Choice::Options(vec![0, 1]),
        Choice::Options(vec![]),
    ] {
        let (out, fx) = e.answer(local(&id, choice.clone()), t0() + secs(1));
        assert_eq!(out, AnswerOutcome::InvalidChoice, "{choice:?}");
        assert!(fx.is_empty());
    }
    let snap = e.snapshot();
    assert_eq!(alert(&snap, &id.0).state, AlertState::Open);
    assert_eq!(line_of(&snap, &k), LineKey::Unsorted);
}

#[test]
fn dismiss_to_tray_and_reopen_persist() {
    let mut e = engine(&["safeascent"]);
    let k = key("U1");
    untagged(&mut e, &k, t0());
    let id = AlertId("assign:claude:U1".into());
    let at = t0() + secs(5);

    let fx = cmd(
        &mut e,
        Command::Dismiss {
            alert: id.clone(),
            at,
        },
    );
    assert_eq!(
        fx,
        vec![
            Effect::Persist(OwnerEvent::Dismissed { alert: id.clone() }, at),
            Effect::Alert(AlertEvent::Dismissed(id.clone())),
        ]
    );
    assert_eq!(alert(&e.snapshot(), &id.0).state, AlertState::Tray);
    assert!(
        cmd(
            &mut e,
            Command::Dismiss {
                alert: id.clone(),
                at
            }
        )
        .is_empty()
    );

    let fx = cmd(
        &mut e,
        Command::Reopen {
            alert: id.clone(),
            at,
        },
    );
    let a = alert(&e.snapshot(), &id.0);
    assert_eq!(a.state, AlertState::Open);
    assert_eq!(
        fx,
        vec![
            Effect::Persist(OwnerEvent::Reopened { alert: id.clone() }, at),
            Effect::Alert(AlertEvent::Updated(a)),
        ]
    );
    assert!(
        cmd(
            &mut e,
            Command::Reopen {
                alert: id.clone(),
                at
            }
        )
        .is_empty()
    );

    e.answer(local(&id, Choice::Options(vec![0])), at);
    let fx = cmd(
        &mut e,
        Command::Dismiss {
            alert: id.clone(),
            at,
        },
    );
    assert!(fx.is_empty());
    assert!(matches!(
        alert(&e.snapshot(), &id.0).state,
        AlertState::Answered { .. }
    ));
}

#[test]
fn rederived_alert_keeps_tray_state() {
    let id = AlertId("q:claude:S1:toolu_q".into());
    let restored = RestoredState {
        tray: BTreeSet::from([id.clone()]),
        ..RestoredState::default()
    };
    let mut e = Engine::new(cfg(&["safeascent"]), restored);
    let k = key("S1");
    discover_in(&mut e, &k, None, t0());
    tag(&mut e, &k, "pelhaim-project: safeascent\nwork");
    let fx = ask(
        &mut e,
        &k,
        "toolu_q",
        vec![question("Stage C1", "Proceed?", &["Proceed", "Hold"])],
        t0(),
    );
    let a = alert(&e.snapshot(), &id.0);
    assert_eq!(a.state, AlertState::Tray);
    assert!(a.is_open());
    assert!(fx.contains(&Effect::Alert(AlertEvent::Raised(a))));
}

#[test]
fn restored_assignment_places_session_without_assign_alert() {
    let k = key("U1");
    let restored = RestoredState {
        assignments: vec![(k.clone(), AssignTarget::Project("ethica".into()), t0())],
        ..RestoredState::default()
    };
    let mut e = Engine::new(cfg(&["ethica"]), restored);
    untagged(&mut e, &k, t0() - secs(3600));
    let snap = e.snapshot();
    assert_eq!(line_of(&snap, &k), LineKey::Project("ethica".into()));
    assert!(snap.alerts.is_empty());
}

#[test]
fn log_decision_title_why_who() {
    let mut e = engine(&["safeascent"]);
    let k = key("S1");
    let id = observed_question(&mut e, &k, "safeascent", "Stage C1");
    let at = ts("2026-11-23T23:30:00Z");
    let fx = cmd(
        &mut e,
        Command::LogDecision {
            alert: id.clone(),
            project: None,
            at,
        },
    );
    let d = Decision {
        id: DecisionId(1),
        project: "safeascent".into(),
        date: chrono::NaiveDate::from_ymd_opt(2026, 11, 23).unwrap(),
        title: "Stage C1: Proceed".into(),
        why: "Proceed to C1?".into(),
        who: Who::Owner,
        origin: Origin::Local,
        source_alert: Some(id.clone()),
        written_to_md: false,
    };
    assert_eq!(
        fx[..2],
        [
            Effect::Persist(OwnerEvent::DecisionLogged(d.clone()), at),
            Effect::ExportDecision(d.clone()),
        ]
    );
    let snap = e.snapshot();
    assert_eq!(snap.decisions_recent, vec![d]);
    assert_eq!(snap.decisions_unexported, 1);
    assert_eq!(alert(&snap, &id.0).decision, Some(DecisionId(1)));

    // A second log of the same alert writes nothing new.
    let fx = cmd(
        &mut e,
        Command::LogDecision {
            alert: id,
            project: None,
            at,
        },
    );
    assert!(fx.is_empty());
    assert_eq!(e.snapshot().decisions_recent.len(), 1);

    assert_eq!(Who::Owner.as_log_str(), "owner");
    assert_eq!(
        Who::AgentDefaultOwnerConfirmed.as_log_str(),
        "agent default, owner-confirmed"
    );
    assert_eq!(
        Who::AgentDefaultUnconfirmed.as_log_str(),
        "agent default, not yet confirmed"
    );
}

#[test]
fn log_decision_skips_alert_already_logged_before_restart() {
    let k = key("S1");
    let id = AlertId(format!("q:{}:toolu_q", k.0));
    let restored = Decision {
        id: DecisionId(1),
        project: "safeascent".into(),
        date: chrono::NaiveDate::from_ymd_opt(2026, 11, 23).unwrap(),
        title: "Stage C1: Proceed".into(),
        why: "Proceed to C1?".into(),
        who: Who::Owner,
        origin: Origin::Local,
        source_alert: Some(id.clone()),
        written_to_md: true,
    };
    let mut e = Engine::new(
        cfg(&["safeascent"]),
        RestoredState {
            decisions: vec![restored.clone()],
            ..RestoredState::default()
        },
    );
    assert_eq!(observed_question(&mut e, &k, "safeascent", "Stage C1"), id);
    assert_eq!(alert(&e.snapshot(), &id.0).decision, None);

    let fx = cmd(
        &mut e,
        Command::LogDecision {
            alert: id,
            project: None,
            at: t0() + secs(60),
        },
    );
    assert!(
        !fx.iter().any(|f| matches!(
            f,
            Effect::ExportDecision(_) | Effect::Persist(OwnerEvent::DecisionLogged(_), _)
        )),
        "{fx:?}"
    );
    assert_eq!(e.snapshot().decisions_recent, vec![restored]);
}

#[test]
fn log_decision_ids_dates_and_empty_header() {
    let old = Decision {
        id: DecisionId(4),
        project: "safeascent".into(),
        date: chrono::NaiveDate::from_ymd_opt(2026, 11, 1).unwrap(),
        title: "old".into(),
        why: "old".into(),
        who: Who::Owner,
        origin: Origin::Local,
        source_alert: None,
        written_to_md: true,
    };
    let mut e = Engine::new(
        EngineConfig {
            local_offset: chrono::FixedOffset::east_opt(2 * 3600).unwrap(),
            decision_log_write: false,
            ..cfg(&["safeascent"])
        },
        RestoredState {
            decisions: vec![old],
            ..RestoredState::default()
        },
    );
    let k = key("S1");
    let id = observed_question(&mut e, &k, "safeascent", "");
    let fx = cmd(
        &mut e,
        Command::LogDecision {
            alert: id,
            project: None,
            at: ts("2026-11-23T23:30:00Z"),
        },
    );
    let [
        Effect::Persist(OwnerEvent::DecisionLogged(d), _),
        Effect::Alert(_),
    ] = fx.as_slice()
    else {
        panic!("expected DecisionLogged and no export, got {fx:?}");
    };
    assert_eq!(d.id, DecisionId(5));
    assert_eq!(
        d.date,
        chrono::NaiveDate::from_ymd_opt(2026, 11, 24).unwrap()
    );
    assert_eq!(d.title, "Proceed to C1?: Proceed");
    assert_eq!(e.snapshot().decisions_recent[0].id, DecisionId(5));
}

#[test]
fn log_decision_for_assign_alert() {
    let mut e = engine(&["ethica"]);
    let k = key("0123456789abcdef");
    untagged(&mut e, &k, t0());
    let id = AlertId(format!("assign:{}", k.0));
    e.answer(local(&id, Choice::Options(vec![0])), t0() + secs(1));
    let fx = cmd(
        &mut e,
        Command::LogDecision {
            alert: id,
            project: None,
            at: t0() + secs(2),
        },
    );
    let Effect::Persist(OwnerEvent::DecisionLogged(d), _) = &fx[0] else {
        panic!("expected DecisionLogged, got {fx:?}");
    };
    assert_eq!(d.project, "ethica");
    assert_eq!(d.title, "Session 01234567 assigned to ETHICA");
    assert_eq!(d.why, "Untagged session placed by the owner");
}

#[test]
fn log_decision_on_dispatch_needs_project() {
    let mut e = engine(&["ethica"]);
    let k = key("D1");
    let id = observed_question(&mut e, &k, "dispatch", "Stage C1");
    let fx = cmd(
        &mut e,
        Command::LogDecision {
            alert: id.clone(),
            project: None,
            at: t0() + secs(60),
        },
    );
    assert!(
        !fx.iter()
            .any(|f| matches!(f, Effect::Persist(..) | Effect::ExportDecision(_)))
    );
    let snap = e.snapshot();
    assert!(snap.decisions_recent.is_empty());
    let failure = snap
        .alerts
        .iter()
        .find(|a| a.kind.tab() == "Failure")
        .expect("failure alert");
    assert!(matches!(&failure.kind, AlertKind::Failure { reason } if reason.contains("project")));
    assert!(fx.contains(&Effect::Alert(AlertEvent::Raised(failure.clone()))));

    let fx = cmd(
        &mut e,
        Command::LogDecision {
            alert: id,
            project: Some("ethica".into()),
            at: t0() + secs(61),
        },
    );
    assert!(matches!(
        &fx[0],
        Effect::Persist(OwnerEvent::DecisionLogged(d), _) if d.project == "ethica"
    ));
}

#[test]
fn log_decision_needs_answered_alert() {
    let mut e = engine(&["safeascent"]);
    let k = key("U1");
    untagged(&mut e, &k, t0());
    let fx = cmd(
        &mut e,
        Command::LogDecision {
            alert: AlertId("assign:claude:U1".into()),
            project: Some("safeascent".into()),
            at: t0(),
        },
    );
    assert!(!fx.iter().any(|f| matches!(f, Effect::Persist(..))));
    assert!(
        e.snapshot()
            .alerts
            .iter()
            .any(|a| a.kind.tab() == "Failure")
    );
}

#[test]
fn export_failure_raises_failure_alert_and_keeps_row_unexported() {
    let mut e = engine(&["safeascent"]);
    let k = key("S1");
    let id = observed_question(&mut e, &k, "safeascent", "Stage C1");
    cmd(
        &mut e,
        Command::LogDecision {
            alert: id,
            project: None,
            at: t0() + secs(60),
        },
    );
    let fx = e.apply(ev(Event::DecisionExportFailed {
        id: DecisionId(1),
        reason: "disk full".into(),
    }));
    let snap = e.snapshot();
    let f = alert(&snap, "decision-failure:1");
    assert_eq!(
        f.kind,
        AlertKind::Failure {
            reason: "Decision log not written: disk full".into()
        }
    );
    assert_eq!(f.kind.tab(), "Failure");
    assert!(fx.contains(&Effect::Alert(AlertEvent::Raised(f))));
    assert_eq!(snap.decisions_unexported, 1);

    let fx = cmd(&mut e, Command::ExportPending);
    let d = snap.decisions_recent[0].clone();
    assert_eq!(fx, vec![Effect::ExportDecision(d)]);

    let at = t0() + secs(90);
    let fx = e.apply(ev(Event::DecisionExported {
        id: DecisionId(1),
        at,
    }));
    assert_eq!(
        fx,
        vec![Effect::Persist(
            OwnerEvent::DecisionExported { id: DecisionId(1) },
            at
        )]
    );
    let snap = e.snapshot();
    assert_eq!(snap.decisions_unexported, 0);
    assert!(snap.decisions_recent[0].written_to_md);
    assert!(cmd(&mut e, Command::ExportPending).is_empty());
}

#[test]
fn export_pending_is_a_no_op_when_writes_are_off() {
    let unexported = Decision {
        id: DecisionId(1),
        project: "safeascent".into(),
        date: chrono::NaiveDate::from_ymd_opt(2026, 11, 23).unwrap(),
        title: "t".into(),
        why: "w".into(),
        who: Who::Owner,
        origin: Origin::Local,
        source_alert: None,
        written_to_md: false,
    };
    let mut e = Engine::new(
        EngineConfig {
            decision_log_write: false,
            ..cfg(&["safeascent"])
        },
        RestoredState {
            decisions: vec![unexported],
            ..RestoredState::default()
        },
    );
    assert!(cmd(&mut e, Command::ExportPending).is_empty());
    assert_eq!(e.snapshot().decisions_unexported, 1);
}

#[test]
fn open_decision_counts_fill_line_views() {
    let mut e = engine(&["safeascent", "ethica"]);
    e.apply(ev(Event::OpenDecisionCounts {
        counts: BTreeMap::from([("safeascent".to_string(), 2), ("gone".to_string(), 9)]),
    }));
    let snap = e.snapshot();
    let count = |name: &str| {
        snap.lines
            .iter()
            .find(|l| l.name == name)
            .unwrap()
            .open_decisions
    };
    assert_eq!(count("SAFEASCENT"), 2);
    assert_eq!(count("ETHICA"), 0);
    assert_eq!(count("DISPATCH"), 0);
}

// §7.7: an untagged subagent inherits nothing from a DISPATCH parent, so both
// ways of marking the orchestrator leave its untagged children on Unsorted.
#[test]
fn dispatch_parent_untagged_child_lands_on_unsorted_with_assign_alert() {
    for listed in [false, true] {
        let mut cfg = cfg(&["ethica"]);
        if listed {
            cfg.dispatch_sessions = vec!["U1".into()];
        }
        let mut e = Engine::new(cfg, RestoredState::default());
        let p = key("U1");
        let child = SessionKey::claude_sub("U1", "later");
        untagged(&mut e, &p, t0());
        if !listed {
            let id = AlertId("assign:claude:U1".into());
            let (out, _) = e.answer(local(&id, Choice::Options(vec![1])), t0() + secs(3));
            assert_eq!(out, AnswerOutcome::Accepted);
        }
        assert_eq!(line_of(&e.snapshot(), &p), LineKey::Dispatch);

        spawn(&mut e, &p, "toolu_c", "later", "no tag", t0() + secs(60));
        discover_in(&mut e, &child, Some(&p), t0() + secs(61));
        link(&mut e, &child, &p, "toolu_c");
        let snap = e.snapshot();
        assert_eq!(line_of(&snap, &child), LineKey::Unsorted, "listed={listed}");
        assert_eq!(tag_of(&snap, &child), TagResolution::Untagged);
        assert!(alert(&snap, &format!("assign:{}", child.0)).is_open());
    }
}

#[test]
fn long_terminal_answer_is_bounded_in_popup_and_decision_title() {
    let mut e = engine(&["safeascent"]);
    let k = key("S1");
    discover_in(&mut e, &k, None, t0());
    tag(&mut e, &k, "pelhaim-project: safeascent\nwork");
    prompt(&mut e, &k, "work", t0());
    let header = "H".repeat(200);
    ask(
        &mut e,
        &k,
        "toolu_q",
        vec![question(&header, "Go?", &["Yes"])],
        t0() + secs(5),
    );
    let long = format!("line one\nline two {}", "x".repeat(500));
    answered(&mut e, &k, "toolu_q", &long, t0() + secs(30));
    let id = AlertId(format!("q:{}:toolu_q", k.0));
    let AlertState::Answered { answer, .. } = alert(&e.snapshot(), &id.0).state else {
        panic!("not answered");
    };
    assert!(answer.starts_with("line one line two x"), "{answer}");
    assert_eq!(answer.chars().count(), 120);
    assert!(answer.ends_with('…'));

    let at = t0() + secs(60);
    cmd(
        &mut e,
        Command::LogDecision {
            alert: id,
            project: None,
            at,
        },
    );
    let title = &e.snapshot().decisions_recent[0].title;
    assert!(title.chars().count() <= 160, "{}", title.chars().count());
    assert!(!title.contains('\n'));
}

#[test]
fn store_write_failure_raises_one_failure_alert_until_closed() {
    let mut e = engine(&["safeascent"]);
    let fail = |kind: &str| {
        ev(Event::StoreWriteFailed {
            kind: kind.into(),
            reason: "database is locked".into(),
        })
    };
    let fx = e.apply(fail("Assigned"));
    let snap = e.snapshot();
    let f = alert(&snap, "store-failure");
    assert_eq!(
        f.kind,
        AlertKind::Failure {
            reason: "Board change not saved (Assigned): database is locked".into()
        }
    );
    assert!(fx.contains(&Effect::Alert(AlertEvent::Raised(f))));

    let fx = e.apply(fail("Dismissed"));
    assert!(!fx.iter().any(|f| matches!(f, Effect::Alert(_))));
    let snap = e.snapshot();
    assert_eq!(
        snap.alerts
            .iter()
            .filter(|a| a.id.0 == "store-failure")
            .count(),
        1
    );
}
