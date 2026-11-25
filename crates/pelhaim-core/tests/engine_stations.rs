use pelhaim_core::testkit::*;
use pelhaim_core::{
    Effect, Engine, EngineConfig, LEG_BLOCKS, LineKey, OwnerEvent, Position, RestoredState,
    SessionKey, Snapshot, StationStatus, StationTiming, TrainState,
};

fn t(hms: &str) -> pelhaim_core::Ts {
    ts(&format!("2026-11-24T{hms}Z"))
}

fn titles(s: &Snapshot, k: &SessionKey) -> Vec<String> {
    let tr = s.trains.values().find(|x| &x.session == k).unwrap().track;
    s.tracks[&tr]
        .stations
        .iter()
        .map(|id| s.stations[id].title.clone())
        .collect()
}

fn session(e: &mut Engine, sid: &str, slug: &str) -> SessionKey {
    let k = SessionKey::claude(sid);
    discover_in(e, &k, None, t("10:00:00"));
    tag(e, &k, &format!("pelhaim-project: {slug}\nGo"));
    prompt(e, &k, "Go", t("10:00:01"));
    k
}

fn timed(fx: &[Effect]) -> Vec<StationTiming> {
    fx.iter()
        .filter_map(|f| match f {
            Effect::Persist(OwnerEvent::StationTimed(t), _) => Some(t.clone()),
            _ => None,
        })
        .collect()
}

fn with_cfg(f: impl FnOnce(&mut EngineConfig), restored: RestoredState) -> Engine {
    let mut c = cfg(&["ethica"]);
    f(&mut c);
    Engine::new(c, restored)
}

#[test]
fn a_marker_opens_a_station_and_the_next_closes_it() {
    let mut e = engine(&["ethica"]);
    let k = session(&mut e, "S1", "ethica");
    milestone(&mut e, &k, "pelhaim-milestone: parser", t("10:01:00"));
    tool_open(&mut e, &k, "c1", "Read", t("10:01:10"));
    let s = e.snapshot();
    assert_eq!(titles(&s, &k), ["parser"]);
    let tr = &s.trains.values().next().unwrap();
    assert_eq!(
        tr.position,
        Position {
            leg: 0,
            block: 1,
            berthed: false
        }
    );

    milestone(&mut e, &k, "pelhaim-milestone: store", t("10:05:00"));
    let s = e.snapshot();
    assert_eq!(titles(&s, &k), ["parser", "store"]);
    let st = &s.tracks[&s.trains.values().next().unwrap().track].stations;
    assert_eq!(s.stations[&st[0]].status, StationStatus::Done);
    assert_eq!(s.stations[&st[1]].status, StationStatus::Active);
    assert_eq!(s.trains.values().next().unwrap().position.leg, 1);
    assert_eq!(s.trains.values().next().unwrap().position.block, 0);
}

#[test]
fn a_repeated_name_is_a_no_op() {
    let mut e = engine(&["ethica"]);
    let k = session(&mut e, "S1", "ethica");
    milestone(&mut e, &k, "pelhaim-milestone: parser ~5m", t("10:01:00"));
    milestone(&mut e, &k, "pelhaim-milestone: store", t("10:02:00"));
    let before = e.snapshot();
    let fx = milestone(&mut e, &k, "pelhaim-milestone: parser ~1h", t("10:03:00"));
    assert!(timed(&fx).is_empty());
    let after = e.snapshot();
    assert_eq!(titles(&after, &k), ["parser", "store"]);
    assert_eq!(before.stations, after.stations);
}

#[test]
fn stations_past_the_cap_fold_into_earlier() {
    let mut e = with_cfg(|c| c.max_stations = 3, RestoredState::default());
    let k = session(&mut e, "S1", "ethica");
    for (i, n) in ["a", "b", "c", "d", "e"].iter().enumerate() {
        milestone(
            &mut e,
            &k,
            &format!("pelhaim-milestone: {n}"),
            t("10:01:00") + secs(i as i64),
        );
    }
    let s = e.snapshot();
    assert_eq!(titles(&s, &k), ["c", "d", "e"]);
    let tr = s.trains.values().next().unwrap();
    assert_eq!(s.tracks[&tr.track].folded, 2);
    assert_eq!(tr.position.leg, 2);
    assert_eq!(s.stations.len(), 3, "folded stations are dropped");
    // A folded name still counts as seen.
    milestone(&mut e, &k, "pelhaim-milestone: a", t("10:02:00"));
    assert_eq!(titles(&e.snapshot(), &k), ["c", "d", "e"]);
}

#[test]
fn a_fork_point_shifts_with_the_fold() {
    let mut e = with_cfg(|c| c.max_stations = 1, RestoredState::default());
    let k = session(&mut e, "S1", "ethica");
    milestone(&mut e, &k, "pelhaim-milestone: a", t("10:01:00"));
    milestone(&mut e, &k, "pelhaim-milestone: b", t("10:01:30"));
    tool_open(&mut e, &k, "toolu_A", "Read", t("10:01:40"));
    spawn(
        &mut e,
        &k,
        "toolu_A",
        "Judge",
        "pelhaim-project: ethica",
        t("10:02:00"),
    );
    let a = SessionKey::claude_sub("S1", "a1");
    discover_in(&mut e, &a, Some(&k), t("10:02:01"));
    link(&mut e, &a, &k, "toolu_A");
    assert_eq!(e.snapshot().forks[0].at.leg, 0);
    assert_eq!(e.snapshot().forks[0].at.block, 2);
    milestone(&mut e, &k, "pelhaim-milestone: c", t("10:03:00"));
    let f = &e.snapshot().forks[0];
    assert_eq!((f.at.leg, f.at.block), (0, 0));
}

#[test]
fn subagents_start_without_stations_and_finish_at_the_end() {
    let mut e = engine(&["ethica"]);
    let k = session(&mut e, "S1", "ethica");
    let a = SessionKey::claude_sub("S1", "a1");
    spawn(
        &mut e,
        &k,
        "toolu_A",
        "Judge",
        "pelhaim-project: ethica",
        t("10:02:00"),
    );
    discover_in(&mut e, &a, Some(&k), t("10:02:01"));
    link(&mut e, &a, &k, "toolu_A");
    assert!(titles(&e.snapshot(), &a).is_empty());
    milestone(&mut e, &a, "pelhaim-milestone: judge ~10m", t("10:03:00"));
    let fx = turn_end(&mut e, &a, None, t("10:15:00"));
    let s = e.snapshot();
    let tr = s.trains.values().find(|x| x.session == a).unwrap();
    assert_eq!(tr.state, TrainState::Done);
    assert_eq!(tr.position.block, LEG_BLOCKS - 1);
    let sid = s.tracks[&tr.track].stations[0];
    assert_eq!(s.stations[&sid].status, StationStatus::Done);
    assert_eq!(
        timed(&fx),
        [StationTiming {
            key: "claude:S1/a1#0".into(),
            project: Some("ethica".into()),
            estimate_secs: 600,
            actual_secs: 720,
        }]
    );
}

#[test]
fn eta_is_the_estimate_until_the_factor_has_samples() {
    let mut e = engine(&["ethica"]);
    let k = session(&mut e, "S1", "ethica");
    milestone(&mut e, &k, "pelhaim-milestone: parser ~20m", t("10:00:00"));
    milestone(&mut e, &k, "pelhaim-milestone: no estimate", t("10:30:00"));
    let s = e.snapshot();
    let st: Vec<_> = s.stations.values().collect();
    assert_eq!(st[0].estimate_secs, Some(1200));
    assert_eq!(st[0].eta, Some(t("10:20:00")));
    assert_eq!(st[1].eta, None);
}

#[test]
fn eta_uses_the_learned_factor() {
    let samples = (0..5)
        .map(|i| StationTiming {
            key: format!("old#{i}"),
            project: Some("ethica".into()),
            estimate_secs: 600,
            actual_secs: 1200,
        })
        .collect();
    let mut e = with_cfg(
        |_| {},
        RestoredState {
            timings: samples,
            ..RestoredState::default()
        },
    );
    let k = session(&mut e, "S1", "ethica");
    milestone(&mut e, &k, "pelhaim-milestone: parser ~20m", t("10:00:00"));
    let s = e.snapshot();
    assert_eq!(s.stations.values().next().unwrap().eta, Some(t("10:40:00")));
}

#[test]
fn completion_records_estimate_and_actual_once() {
    let mut e = engine(&["ethica"]);
    let k = session(&mut e, "S1", "ethica");
    milestone(&mut e, &k, "pelhaim-milestone: parser ~20m", t("10:00:00"));
    let fx = milestone(&mut e, &k, "pelhaim-milestone: store", t("10:25:00"));
    let got = timed(&fx);
    assert_eq!(
        got,
        [StationTiming {
            key: "claude:S1#0".into(),
            project: Some("ethica".into()),
            estimate_secs: 1200,
            actual_secs: 1500,
        }]
    );
    // A station without an estimate records nothing.
    let fx = milestone(&mut e, &k, "pelhaim-milestone: lint", t("10:30:00"));
    assert!(timed(&fx).is_empty());

    // A restart replays the transcript: the restored key records nothing twice.
    let mut e2 = with_cfg(
        |_| {},
        RestoredState {
            timings: got,
            ..RestoredState::default()
        },
    );
    let k = session(&mut e2, "S1", "ethica");
    milestone(&mut e2, &k, "pelhaim-milestone: parser ~20m", t("10:00:00"));
    let fx = milestone(&mut e2, &k, "pelhaim-milestone: store", t("10:25:00"));
    assert!(timed(&fx).is_empty());
}

#[test]
fn overdue_is_an_active_station_past_its_eta() {
    let mut e = engine(&["ethica"]);
    let k = session(&mut e, "S1", "ethica");
    milestone(&mut e, &k, "pelhaim-milestone: parser ~20m", t("10:00:00"));
    let st = e.snapshot().stations.values().next().unwrap().clone();
    assert!(!st.overdue(t("10:20:00")));
    assert!(st.overdue(t("10:20:01")));
    milestone(&mut e, &k, "pelhaim-milestone: store", t("10:30:00"));
    let done = e.snapshot().stations[&st.id].clone();
    assert!(
        !done.overdue(t("11:00:00")),
        "a done station is never overdue"
    );
}

#[test]
fn the_orchestrator_draws_no_train_but_still_alerts() {
    let mut e = engine(&["ethica"]);
    let d = SessionKey::claude("D1");
    discover_in(&mut e, &d, None, t("10:00:00"));
    tag(&mut e, &d, "pelhaim-project: dispatch\nRun");
    prompt(&mut e, &d, "Run", t("10:00:01"));
    spawn(
        &mut e,
        &d,
        "toolu_A",
        "Judge",
        "pelhaim-project: ethica",
        t("10:01:00"),
    );
    spawn(&mut e, &d, "toolu_B", "Loose", "no tag here", t("10:01:01"));
    let (a, b) = (
        SessionKey::claude_sub("D1", "a1"),
        SessionKey::claude_sub("D1", "b1"),
    );
    discover_in(&mut e, &a, Some(&d), t("10:01:02"));
    link(&mut e, &a, &d, "toolu_A");
    discover_in(&mut e, &b, Some(&d), t("10:01:03"));
    link(&mut e, &b, &d, "toolu_B");
    tool_open(&mut e, &d, "toolu_q", "AskUserQuestion", t("10:02:00"));
    ask(
        &mut e,
        &d,
        "toolu_q",
        vec![question("H", "Go?", &["Yes", "No"])],
        t("10:02:00"),
    );

    let s = e.snapshot();
    let line = |k: &LineKey| s.lines.iter().find(|l| &l.key == k).unwrap();
    let dispatch = line(&LineKey::Dispatch);
    assert!(dispatch.tracks.is_empty());
    assert!(dispatch.active && dispatch.has_open_alert);
    assert_eq!(dispatch.running, 2);
    assert_eq!(line(&LineKey::Project("ethica".into())).tracks.len(), 1);
    assert_eq!(line(&LineKey::Unsorted).tracks.len(), 1);
    assert!(s.forks.is_empty());
    assert!(s.alerts.iter().any(|al| al.kind.tab() == "Question"));
    assert!(s.alerts.iter().any(|al| al.kind.tab() == "Assign"));

    turn_end(&mut e, &a, None, t("10:05:00"));
    let s = e.snapshot();
    assert_eq!(s.lines[0].running, 1, "a done subagent no longer counts");
}

#[test]
fn listed_orchestrator_draws_no_train() {
    let mut e = with_cfg(
        |c| c.dispatch_sessions = vec!["D1".into()],
        RestoredState::default(),
    );
    let d = SessionKey::claude("D1");
    discover_in(&mut e, &d, None, t("10:00:00"));
    tag(&mut e, &d, "Run");
    let s = e.snapshot();
    assert!(s.lines[0].tracks.is_empty() && s.lines[0].active);
    assert_eq!(s.lines[0].running, 0);
}
