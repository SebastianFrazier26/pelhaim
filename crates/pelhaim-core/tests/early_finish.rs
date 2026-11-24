use pelhaim_core::testkit::*;
use pelhaim_core::{Engine, Event, SessionKey, Snapshot, Tool, TrainState, Ts};

fn t0() -> Ts {
    ts("2026-11-24T08:00:00Z")
}

fn state(s: &Snapshot, k: &SessionKey) -> TrainState {
    s.trains
        .values()
        .find(|t| &t.session == k)
        .unwrap()
        .state
        .clone()
}

fn finish(e: &mut Engine, key: &SessionKey, at: Ts) {
    e.apply(ev(Event::SubagentFinished {
        key: key.clone(),
        at,
    }));
}

/// Parent turn ended after a background launch, so it waits on its child.
fn parent_waiting(e: &mut Engine, parent: &SessionKey, child: &SessionKey) {
    discover_in(e, parent, None, t0());
    prompt(e, parent, "Ship", t0());
    spawn(e, parent, "c1", "Notes", "", t0());
    tool_open(e, parent, "c1", "Agent", t0());
    background_launched(e, parent, "c1", t0() + secs(1));
    turn_end(e, parent, None, t0() + secs(2));
    finish(e, child, t0() + secs(60));
}

fn child_arrives(e: &mut Engine, parent: &SessionKey, child: &SessionKey) {
    discover_in(e, child, Some(parent), t0() + secs(1));
    tool_open(e, child, "h1", "SubagentHandback", t0() + secs(50));
    tool_close(e, child, "h1", t0() + secs(51));
}

#[test]
fn early_finish_applies_on_link() {
    let mut e = engine(&[]);
    let (p, c) = (SessionKey::claude("P"), SessionKey::claude_sub("P", "a1"));
    parent_waiting(&mut e, &p, &c);
    assert_eq!(
        e.dropped_events(),
        0,
        "an early finish is held, not dropped"
    );
    child_arrives(&mut e, &p, &c);
    assert_ne!(state(&e.snapshot(), &c), TrainState::Done);
    link(&mut e, &c, &p, "c1");
    let s = e.snapshot();
    assert_eq!(state(&s, &c), TrainState::Done);
    assert_eq!(state(&s, &p), TrainState::Idle);
}

#[test]
fn early_finish_applies_on_catch_up_done_without_meta() {
    let mut e = engine(&[]);
    let (p, c) = (SessionKey::claude("P"), SessionKey::claude_sub("P", "a1"));
    parent_waiting(&mut e, &p, &c);
    child_arrives(&mut e, &p, &c);
    e.apply(ev(Event::CatchUpDone { tool: Tool::Claude }));
    let s = e.snapshot();
    assert_eq!(state(&s, &c), TrainState::Done);
    assert_eq!(state(&s, &p), TrainState::Idle);
}

#[test]
fn early_finish_is_dropped_when_parent_goes() {
    let mut e = engine(&[]);
    let (p, c) = (SessionKey::claude("P"), SessionKey::claude_sub("P", "a1"));
    parent_waiting(&mut e, &p, &c);
    e.apply(ev(Event::SessionGone { key: p.clone() }));
    discover_in(&mut e, &c, None, t0() + secs(70));
    tool_open(&mut e, &c, "h1", "Read", t0() + secs(71));
    link(&mut e, &c, &p, "c1");
    e.apply(ev(Event::CatchUpDone { tool: Tool::Claude }));
    assert_ne!(state(&e.snapshot(), &c), TrainState::Done);
}

#[test]
fn early_finish_buffer_is_capped() {
    let mut e = engine(&[]);
    for i in 0..256 {
        finish(&mut e, &SessionKey::claude_sub("P", &format!("a{i}")), t0());
    }
    assert_eq!(e.dropped_events(), 0);
    // A repeat for a held child still fits; a new child past the cap is dropped.
    finish(&mut e, &SessionKey::claude_sub("P", "a0"), t0());
    assert_eq!(e.dropped_events(), 0);
    finish(&mut e, &SessionKey::claude_sub("P", "a256"), t0());
    assert_eq!(e.dropped_events(), 1);
}

#[test]
fn unknown_main_session_finish_is_dropped_not_held() {
    let mut e = engine(&[]);
    finish(&mut e, &SessionKey::claude("M"), t0());
    assert_eq!(e.dropped_events(), 1);
}
