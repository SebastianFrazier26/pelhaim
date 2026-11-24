use pelhaim_core::testkit::*;
use pelhaim_core::{
    Command, Effect, Engine, EngineConfig, Event, Input, KeepAwakeAction, RestoredState,
    SessionKey, Ts,
};

fn t0() -> Ts {
    ts("2026-11-23T10:00:00Z")
}

fn key(sid: &str) -> SessionKey {
    SessionKey::claude(sid)
}

fn keep(fx: &[Effect]) -> Vec<KeepAwakeAction> {
    fx.iter()
        .filter_map(|f| match f {
            Effect::KeepAwake(k) => Some(k.clone()),
            _ => None,
        })
        .collect()
}

fn with(f: impl FnOnce(&mut EngineConfig)) -> Engine {
    let mut c = cfg(&["safeascent"]);
    f(&mut c);
    Engine::new(c, RestoredState::default())
}

fn working(e: &mut Engine, k: &SessionKey, at: Ts) -> Vec<Effect> {
    discover_in(e, k, None, at);
    tag(e, k, "pelhaim-project: safeascent\nwork");
    prompt(e, k, "work", at)
}

#[test]
fn hold_on_first_working_release_60s_after_last() {
    let mut e = engine(&["safeascent"]);
    let (a, b) = (key("A1"), key("B1"));
    assert_eq!(keep(&working(&mut e, &a, t0())), [KeepAwakeAction::Hold]);
    assert!(keep(&working(&mut e, &b, t0() + secs(1))).is_empty());
    let s = e.snapshot().keep_awake;
    assert!(s.enabled && s.held);
    assert_eq!(s.count, 2);

    assert!(keep(&turn_end(&mut e, &a, None, t0() + secs(5))).is_empty());
    assert!(keep(&turn_end(&mut e, &b, None, t0() + secs(10))).is_empty());
    assert_eq!(e.snapshot().keep_awake.count, 0);
    assert!(e.snapshot().keep_awake.held);

    assert!(keep(&tick(&mut e, t0() + secs(10 + 59))).is_empty());
    assert!(e.snapshot().keep_awake.held);
    assert_eq!(
        keep(&tick(&mut e, t0() + secs(10 + 60))),
        [KeepAwakeAction::Release]
    );
    assert!(!e.snapshot().keep_awake.held);
    assert!(keep(&tick(&mut e, t0() + secs(200))).is_empty());
}

#[test]
fn rising_count_cancels_pending_release() {
    let mut e = engine(&["safeascent"]);
    let a = key("A1");
    working(&mut e, &a, t0());
    turn_end(&mut e, &a, None, t0() + secs(10));
    tick(&mut e, t0() + secs(40));
    assert!(keep(&prompt(&mut e, &a, "more", t0() + secs(50))).is_empty());
    assert!(keep(&tick(&mut e, t0() + secs(75))).is_empty());
    assert!(keep(&tick(&mut e, t0() + secs(130))).is_empty());
    assert!(e.snapshot().keep_awake.held);

    // The 60 s restart from the new drop, not the old one.
    turn_end(&mut e, &a, None, t0() + secs(140));
    assert!(keep(&tick(&mut e, t0() + secs(199))).is_empty());
    assert_eq!(
        keep(&tick(&mut e, t0() + secs(200))),
        [KeepAwakeAction::Release]
    );
}

#[test]
fn observed_trains_ignored_when_include_observed_false() {
    let mut e = with(|c| c.include_observed = false);
    let fx = working(&mut e, &key("A1"), t0());
    assert!(keep(&fx).is_empty());
    let s = e.snapshot().keep_awake;
    assert_eq!(s.count, 0);
    assert!(!s.held);
}

#[test]
fn disabled_feature_never_holds() {
    let mut e = with(|c| c.keep_awake_enabled = false);
    assert!(keep(&working(&mut e, &key("A1"), t0())).is_empty());
    let s = e.snapshot().keep_awake;
    assert!(!s.enabled && !s.held);
    assert_eq!(s.count, 1);
}

fn waiting(e: &mut Engine, k: &SessionKey) -> Vec<Effect> {
    working(e, k, t0());
    ask(
        e,
        k,
        "toolu_q",
        vec![question("Stage C1", "Proceed?", &["Proceed", "Hold"])],
        t0() + secs(1),
    )
}

#[test]
fn waiting_counts_only_with_remote_enabled() {
    let mut off = engine(&["safeascent"]);
    waiting(&mut off, &key("A1"));
    assert_eq!(off.snapshot().keep_awake.count, 0);
    assert_eq!(
        keep(&tick(&mut off, t0() + secs(61))),
        [KeepAwakeAction::Release]
    );

    let mut on = with(|c| c.remote_enabled = true);
    waiting(&mut on, &key("A1"));
    assert_eq!(on.snapshot().keep_awake.count, 1);
    assert!(keep(&tick(&mut on, t0() + secs(61))).is_empty());
    assert!(on.snapshot().keep_awake.held);

    // ProbablyWaiting counts as waiting.
    let mut pw = with(|c| c.remote_enabled = true);
    let k = key("B1");
    working(&mut pw, &k, t0());
    tool_open(&mut pw, &k, "toolu_b", "Bash", t0());
    tick(&mut pw, t0() + secs(25));
    let snap = pw.snapshot();
    assert_eq!(
        snap.trains.values().next().unwrap().state,
        pelhaim_core::TrainState::ProbablyWaiting
    );
    assert_eq!(snap.keep_awake.count, 1);
}

#[test]
fn pause_releases_then_resumes() {
    let mut e = engine(&["safeascent"]);
    working(&mut e, &key("A1"), t0());
    tick(&mut e, t0() + secs(1));
    let until = t0() + secs(3600);
    let fx = e.apply(Input::Command(Command::PauseKeepAwake { until }));
    assert_eq!(keep(&fx), [KeepAwakeAction::Release]);
    let s = e.snapshot().keep_awake;
    assert!(!s.held);
    assert_eq!(s.paused_until, Some(until));

    // Keep the train alive so the stall heuristic does not end the count.
    for m in 1..60 {
        activity(&mut e, &key("A1"), t0() + secs(60 * m));
        assert!(keep(&tick(&mut e, t0() + secs(60 * m + 1))).is_empty());
    }
    activity(&mut e, &key("A1"), until);
    assert_eq!(keep(&tick(&mut e, until)), [KeepAwakeAction::Hold]);
    let s = e.snapshot().keep_awake;
    assert!(s.held);
    assert_eq!(s.paused_until, None);
}

#[test]
fn pause_while_idle_emits_nothing_and_does_not_hold_with_zero_count() {
    let mut e = engine(&["safeascent"]);
    let until = t0() + secs(60);
    assert!(keep(&e.apply(Input::Command(Command::PauseKeepAwake { until }))).is_empty());
    assert!(keep(&tick(&mut e, until)).is_empty());
    assert!(!e.snapshot().keep_awake.held);
}

#[test]
fn keep_awake_env_is_reflected_in_status() {
    let mut e = engine(&[]);
    e.apply(Input::Event(Event::KeepAwakeEnv {
        mechanism: Some("caffeinate".into()),
        on_battery: Some(true),
    }));
    let s = e.snapshot().keep_awake;
    assert_eq!(s.mechanism.as_deref(), Some("caffeinate"));
    assert_eq!(s.on_battery, Some(true));
}
