use std::path::PathBuf;

use pelhaim_core::testkit::{cfg, discover, engine, ts};
use pelhaim_core::{
    Engine, Event, Input, LEG_BLOCKS, LineKey, Position, SessionKey, Snapshot, StationStatus,
    TagParse, TagResolution, Tool, Track, Train, TrainState, parse_tag, title_from_prompt,
};
use proptest::prelude::*;

fn ev(e: Event) -> Input {
    Input::Event(e)
}

fn key(sid: &str) -> SessionKey {
    SessionKey::claude(sid)
}

fn sub(sid: &str, agent: &str) -> SessionKey {
    SessionKey::claude_sub(sid, agent)
}

fn discover_at(
    e: &mut Engine,
    k: SessionKey,
    parent: Option<SessionKey>,
    cwd: Option<&str>,
    at: &str,
) {
    e.apply(ev(Event::SessionDiscovered {
        path: PathBuf::from(format!("/fixtures/{}.jsonl", k.0)),
        key: k,
        tool: Tool::Claude,
        parent,
        cwd: cwd.map(str::to_string),
        version: Some("2.1.331".into()),
        started: ts(at),
    }));
}

fn first_prompt(e: &mut Engine, k: &SessionKey, text: &str, at: &str) {
    e.apply(ev(Event::SessionTag {
        key: k.clone(),
        tag: parse_tag(text),
    }));
    prompt(e, k, text, at);
}

fn prompt(e: &mut Engine, k: &SessionKey, text: &str, at: &str) {
    e.apply(ev(Event::PromptStarted {
        key: k.clone(),
        title: title_from_prompt(text),
        at: ts(at),
    }));
}

fn tool_open(e: &mut Engine, k: &SessionKey, call: &str, name: &str, at: &str) {
    e.apply(ev(Event::ToolOpened {
        key: k.clone(),
        call_id: call.into(),
        name: name.into(),
        at: ts(at),
    }));
}

fn tool_close(e: &mut Engine, k: &SessionKey, call: &str, at: &str) {
    e.apply(ev(Event::ToolClosed {
        key: k.clone(),
        call_id: call.into(),
        at: ts(at),
    }));
}

fn turn_end(e: &mut Engine, k: &SessionKey, at: &str) {
    e.apply(ev(Event::TurnEnded {
        key: k.clone(),
        final_text: None,
        at: ts(at),
    }));
}

fn spawn(e: &mut Engine, parent: &SessionKey, call: &str, desc: &str, prompt_text: &str, at: &str) {
    e.apply(ev(Event::SpawnRequested {
        parent: parent.clone(),
        tool_use_id: call.into(),
        description: desc.into(),
        tag: parse_tag(prompt_text),
        at: ts(at),
    }));
}

fn link(e: &mut Engine, child: &SessionKey, parent: &SessionKey, call: &str, desc: &str) {
    e.apply(ev(Event::SubagentLinked {
        child: child.clone(),
        parent: parent.clone(),
        tool_use_id: Some(call.into()),
        description: Some(desc.into()),
    }));
}

fn train<'a>(s: &'a Snapshot, k: &SessionKey) -> &'a Train {
    s.trains
        .values()
        .find(|t| &t.session == k)
        .expect("train for session")
}

fn track<'a>(s: &'a Snapshot, k: &SessionKey) -> &'a Track {
    &s.tracks[&train(s, k).track]
}

fn station_titles(s: &Snapshot, k: &SessionKey) -> Vec<String> {
    track(s, k)
        .stations
        .iter()
        .map(|id| s.stations[id].title.clone())
        .collect()
}

fn line_tracks(s: &Snapshot, line: &LineKey) -> Vec<pelhaim_core::TrackId> {
    s.lines
        .iter()
        .find(|l| &l.key == line)
        .expect("line present")
        .tracks
        .clone()
}

fn without_version(mut s: Snapshot) -> Snapshot {
    s.version = 0;
    s
}

#[test]
fn tagged_session_lands_on_its_line() {
    let mut e = engine(&["safeascent", "ethica"]);
    let s1 = key("S1");
    discover_at(
        &mut e,
        s1.clone(),
        None,
        Some("/Users/o/Developer"),
        "2026-11-23T14:00:00Z",
    );
    first_prompt(
        &mut e,
        &s1,
        "pelhaim-project: ethica\nRun judge",
        "2026-11-23T14:00:05Z",
    );

    let s = e.snapshot();
    let t = train(&s, &s1);
    assert_eq!(t.tag, TagResolution::Tagged("ethica".into()));
    assert_eq!(track(&s, &s1).line, LineKey::Project("ethica".into()));
    assert_eq!(station_titles(&s, &s1), vec!["Run judge".to_string()]);
    assert_eq!(
        line_tracks(&s, &LineKey::Project("ethica".into())),
        vec![t.track]
    );
    assert!(line_tracks(&s, &LineKey::Unsorted).is_empty());
    assert_eq!(t.cwd.as_deref(), Some("/Users/o/Developer"));
}

#[test]
fn cwd_never_influences_line() {
    let mut e = engine(&["safeascent"]);
    let s1 = key("S1");
    discover_at(
        &mut e,
        s1.clone(),
        None,
        Some("/Users/o/Developer/safeascent"),
        "2026-11-23T14:00:00Z",
    );
    first_prompt(
        &mut e,
        &s1,
        "Fix the flaky risk test",
        "2026-11-23T14:00:05Z",
    );

    let s = e.snapshot();
    assert_eq!(train(&s, &s1).tag, TagResolution::Untagged);
    assert_eq!(track(&s, &s1).line, LineKey::Unsorted);
    assert!(line_tracks(&s, &LineKey::Project("safeascent".into())).is_empty());
}

#[test]
fn mis_tagged_goes_unsorted() {
    let mut e = engine(&["ethica"]);
    let s1 = key("S1");
    discover_at(&mut e, s1.clone(), None, None, "2026-11-23T14:00:00Z");
    first_prompt(
        &mut e,
        &s1,
        "pelhaim-project: ethika\nRun judge",
        "2026-11-23T14:00:05Z",
    );

    let s = e.snapshot();
    assert_eq!(
        train(&s, &s1).tag,
        TagResolution::MisTagged("ethika".into())
    );
    assert_eq!(track(&s, &s1).line, LineKey::Unsorted);
}

#[test]
fn dispatch_by_tag_and_by_config() {
    let mut e = engine(&["ethica"]);
    let s1 = key("S1");
    discover_at(&mut e, s1.clone(), None, None, "2026-11-23T14:00:00Z");
    first_prompt(
        &mut e,
        &s1,
        "pelhaim-project: dispatch\nPlan the week",
        "2026-11-23T14:00:05Z",
    );
    let s = e.snapshot();
    assert_eq!(train(&s, &s1).tag, TagResolution::Tagged("dispatch".into()));
    assert_eq!(track(&s, &s1).line, LineKey::Dispatch);

    let mut c = cfg(&["ethica"]);
    c.dispatch_sessions = vec!["S9".into()];
    let mut e = Engine::new(c, Default::default());
    let s9 = key("S9");
    discover_at(&mut e, s9.clone(), None, None, "2026-11-23T14:00:00Z");
    first_prompt(&mut e, &s9, "Plan the week", "2026-11-23T14:00:05Z");
    let child = sub("S9", "a1");
    discover_at(
        &mut e,
        child.clone(),
        Some(s9.clone()),
        None,
        "2026-11-23T14:01:00Z",
    );
    let s = e.snapshot();
    assert_eq!(track(&s, &s9).line, LineKey::Dispatch);
    assert_eq!(train(&s, &s9).tag, TagResolution::Untagged);
    // A subagent shares its parent's session id; the config listing covers main sessions only.
    assert_eq!(track(&s, &child).line, LineKey::Unsorted);
}

#[test]
fn dispatch_subagents_branch_to_two_lines() {
    let mut e = engine(&["ethica", "cabn"]);
    let s1 = key("S1");
    discover_at(&mut e, s1.clone(), None, None, "2026-11-23T14:00:00Z");
    first_prompt(
        &mut e,
        &s1,
        "pelhaim-project: dispatch\nFan out",
        "2026-11-23T14:00:05Z",
    );
    tool_open(&mut e, &s1, "toolu_A", "Agent", "2026-11-23T14:00:10Z");
    spawn(
        &mut e,
        &s1,
        "toolu_A",
        "Judge run",
        "pelhaim-project: ethica\nRun the judge",
        "2026-11-23T14:00:10Z",
    );
    tool_open(&mut e, &s1, "toolu_B", "Agent", "2026-11-23T14:00:11Z");
    spawn(
        &mut e,
        &s1,
        "toolu_B",
        "Sprite atlas",
        "pelhaim-project: cabn\nFix atlas",
        "2026-11-23T14:00:11Z",
    );
    let a = sub("S1", "a1");
    let b = sub("S1", "b2");
    discover_at(
        &mut e,
        a.clone(),
        Some(s1.clone()),
        None,
        "2026-11-23T14:00:12Z",
    );
    link(&mut e, &a, &s1, "toolu_A", "Judge run");
    discover_at(
        &mut e,
        b.clone(),
        Some(s1.clone()),
        None,
        "2026-11-23T14:00:13Z",
    );
    link(&mut e, &b, &s1, "toolu_B", "Sprite atlas");

    let s = e.snapshot();
    let parent_track = train(&s, &s1).track;
    assert_eq!(track(&s, &s1).line, LineKey::Dispatch);
    assert_eq!(s.forks.len(), 2);
    assert!(s.forks.iter().all(|f| f.parent_track == parent_track));

    let fa = s
        .forks
        .iter()
        .find(|f| f.child_track == train(&s, &a).track)
        .unwrap();
    let fb = s
        .forks
        .iter()
        .find(|f| f.child_track == train(&s, &b).track)
        .unwrap();
    assert_eq!(
        fa.at,
        Position {
            leg: 0,
            block: 1,
            berthed: false
        }
    );
    assert_eq!(
        fb.at,
        Position {
            leg: 0,
            block: 2,
            berthed: false
        }
    );
    assert_eq!(track(&s, &a).origin, Some(fa.id));

    assert_eq!(track(&s, &a).line, LineKey::Project("ethica".into()));
    assert_eq!(track(&s, &b).line, LineKey::Project("cabn".into()));
    assert_eq!(train(&s, &a).tag, TagResolution::Tagged("ethica".into()));
    assert_eq!(train(&s, &a).parent, Some(train(&s, &s1).id));
    assert_eq!(train(&s, &a).label, "Judge run");
    assert_eq!(train(&s, &a).state, TrainState::Working);
    assert_eq!(station_titles(&s, &a), vec!["Judge run".to_string()]);
    assert_eq!(station_titles(&s, &b), vec!["Sprite atlas".to_string()]);
}

#[test]
fn subagent_does_not_inherit_parent_tag() {
    let mut e = engine(&["ethica"]);
    let s1 = key("S1");
    discover_at(&mut e, s1.clone(), None, None, "2026-11-23T14:00:00Z");
    first_prompt(
        &mut e,
        &s1,
        "pelhaim-project: ethica\nRun judge",
        "2026-11-23T14:00:05Z",
    );
    tool_open(&mut e, &s1, "toolu_A", "Agent", "2026-11-23T14:00:10Z");
    spawn(
        &mut e,
        &s1,
        "toolu_A",
        "Helper",
        "Look into the scoring",
        "2026-11-23T14:00:10Z",
    );
    let a = sub("S1", "a1");
    discover_at(
        &mut e,
        a.clone(),
        Some(s1.clone()),
        None,
        "2026-11-23T14:00:12Z",
    );
    link(&mut e, &a, &s1, "toolu_A", "Helper");

    let s = e.snapshot();
    assert_eq!(track(&s, &s1).line, LineKey::Project("ethica".into()));
    assert_eq!(train(&s, &a).tag, TagResolution::Untagged);
    assert_eq!(track(&s, &a).line, LineKey::Unsorted);
    assert_eq!(s.forks.len(), 1);
}

fn spawn_link_run(link_first: bool) -> Snapshot {
    let mut e = engine(&["ethica"]);
    let s1 = key("S1");
    let a = sub("S1", "a1");
    discover_at(&mut e, s1.clone(), None, None, "2026-11-23T14:00:00Z");
    first_prompt(
        &mut e,
        &s1,
        "pelhaim-project: dispatch\nFan out",
        "2026-11-23T14:00:05Z",
    );
    tool_open(&mut e, &s1, "toolu_A", "Agent", "2026-11-23T14:00:10Z");
    if link_first {
        discover_at(
            &mut e,
            a.clone(),
            Some(s1.clone()),
            None,
            "2026-11-23T14:00:12Z",
        );
        link(&mut e, &a, &s1, "toolu_A", "Judge run");
        spawn(
            &mut e,
            &s1,
            "toolu_A",
            "Judge run",
            "pelhaim-project: ethica\nGo",
            "2026-11-23T14:00:10Z",
        );
    } else {
        spawn(
            &mut e,
            &s1,
            "toolu_A",
            "Judge run",
            "pelhaim-project: ethica\nGo",
            "2026-11-23T14:00:10Z",
        );
        discover_at(
            &mut e,
            a.clone(),
            Some(s1.clone()),
            None,
            "2026-11-23T14:00:12Z",
        );
        link(&mut e, &a, &s1, "toolu_A", "Judge run");
    }
    tool_open(&mut e, &a, "toolu_x", "Read", "2026-11-23T14:00:20Z");
    e.snapshot()
}

#[test]
fn link_before_spawn_equals_spawn_before_link() {
    let spawn_first = spawn_link_run(false);
    let link_first = spawn_link_run(true);
    assert_eq!(spawn_first.forks.len(), 1);
    assert_eq!(
        track(&spawn_first, &sub("S1", "a1")).line,
        LineKey::Project("ethica".into())
    );
    assert_eq!(without_version(spawn_first), without_version(link_first));
}

#[test]
fn train_never_passes_unfinished_station() {
    let mut e = engine(&[]);
    let s1 = key("S1");
    discover_at(&mut e, s1.clone(), None, None, "2026-11-23T14:00:00Z");
    first_prompt(&mut e, &s1, "Do the thing", "2026-11-23T14:00:01Z");
    for i in 0..30 {
        tool_open(
            &mut e,
            &s1,
            &format!("c{i}"),
            "Bash",
            "2026-11-23T14:00:02Z",
        );
    }
    let s = e.snapshot();
    let t = train(&s, &s1);
    assert_eq!(
        t.position,
        Position {
            leg: 0,
            block: LEG_BLOCKS - 2,
            berthed: false
        }
    );
    assert_eq!(t.state, TrainState::Working);
    assert_eq!(t.counters.tool_calls, 30);
    assert_eq!(
        s.stations[&track(&s, &s1).stations[0]].status,
        StationStatus::Active
    );

    turn_end(&mut e, &s1, "2026-11-23T14:01:00Z");
    let s = e.snapshot();
    let t = train(&s, &s1);
    assert_eq!(
        t.position,
        Position {
            leg: 0,
            block: LEG_BLOCKS - 1,
            berthed: true
        }
    );
    assert_eq!(t.state, TrainState::Idle);
    assert_eq!(t.counters.turns, 1);
    assert_eq!(
        s.stations[&track(&s, &s1).stations[0]].status,
        StationStatus::Done
    );

    tool_open(&mut e, &s1, "late", "Bash", "2026-11-23T14:01:05Z");
    let s = e.snapshot();
    let t = train(&s, &s1);
    assert_eq!(
        t.position,
        Position {
            leg: 0,
            block: LEG_BLOCKS - 2,
            berthed: false
        }
    );
    assert_eq!(t.state, TrainState::Working);
    assert_eq!(
        s.stations[&track(&s, &s1).stations[0]].status,
        StationStatus::Active
    );

    prompt(&mut e, &s1, "Next thing", "2026-11-23T14:02:00Z");
    let s = e.snapshot();
    let t = train(&s, &s1);
    assert_eq!(
        t.position,
        Position {
            leg: 1,
            block: 0,
            berthed: false
        }
    );
    let st = &track(&s, &s1).stations;
    assert_eq!(s.stations[&st[0]].status, StationStatus::Done);
    assert_eq!(s.stations[&st[1]].status, StationStatus::Active);
    assert_eq!(
        station_titles(&s, &s1),
        vec!["Do the thing".to_string(), "Next thing".into()]
    );
}

proptest! {
    #[test]
    fn movement_cap_holds(ops in proptest::collection::vec(0u8..3, 0..200)) {
        let mut e = engine(&[]);
        let s1 = key("S1");
        discover_at(&mut e, s1.clone(), None, None, "2026-11-23T14:00:00Z");
        first_prompt(&mut e, &s1, "Do the thing", "2026-11-23T14:00:01Z");
        let mut opened = 0u32;
        for op in ops {
            let at = ts("2026-11-23T14:00:02Z");
            let input = match op {
                0 => {
                    opened += 1;
                    Event::ToolOpened { key: s1.clone(), call_id: format!("c{opened}"), name: "Bash".into(), at }
                }
                1 => Event::ToolClosed { key: s1.clone(), call_id: format!("c{opened}"), at },
                _ => Event::Activity { key: s1.clone(), at },
            };
            e.apply(ev(input));
            let s = e.snapshot();
            let t = train(&s, &s1);
            prop_assert!(t.position.block <= LEG_BLOCKS - 2);
            prop_assert!(!t.position.berthed);
            prop_assert_eq!(t.position.leg, 0);
        }
    }
}

#[test]
fn agent_tool_result_finishes_child() {
    let mut e = engine(&["ethica"]);
    let s1 = key("S1");
    let a = sub("S1", "a1");
    let b = sub("S1", "b2");
    discover_at(&mut e, s1.clone(), None, None, "2026-11-23T14:00:00Z");
    first_prompt(
        &mut e,
        &s1,
        "pelhaim-project: dispatch\nFan out",
        "2026-11-23T14:00:05Z",
    );
    tool_open(&mut e, &s1, "toolu_A", "Agent", "2026-11-23T14:00:10Z");
    spawn(
        &mut e,
        &s1,
        "toolu_A",
        "A",
        "pelhaim-project: ethica\nGo",
        "2026-11-23T14:00:10Z",
    );
    tool_open(&mut e, &s1, "toolu_B", "Agent", "2026-11-23T14:00:10Z");
    spawn(
        &mut e,
        &s1,
        "toolu_B",
        "B",
        "pelhaim-project: ethica\nGo",
        "2026-11-23T14:00:10Z",
    );
    discover_at(
        &mut e,
        a.clone(),
        Some(s1.clone()),
        None,
        "2026-11-23T14:00:12Z",
    );
    link(&mut e, &a, &s1, "toolu_A", "A");
    tool_open(&mut e, &a, "toolu_r", "Read", "2026-11-23T14:00:20Z");

    tool_close(&mut e, &s1, "toolu_A", "2026-11-23T14:05:00Z");
    let s = e.snapshot();
    let child = train(&s, &a);
    assert_eq!(child.state, TrainState::Done);
    assert!(child.position.berthed);
    assert_eq!(child.position.block, LEG_BLOCKS - 1);
    assert_eq!(
        s.stations[&track(&s, &a).stations[0]].status,
        StationStatus::Done
    );
    assert_eq!(train(&s, &s1).state, TrainState::Working);

    // The parent's result arrives before the child is linked: the child is Done as soon as it links.
    tool_close(&mut e, &s1, "toolu_B", "2026-11-23T14:06:00Z");
    discover_at(
        &mut e,
        b.clone(),
        Some(s1.clone()),
        None,
        "2026-11-23T14:00:13Z",
    );
    link(&mut e, &b, &s1, "toolu_B", "B");
    let s = e.snapshot();
    assert_eq!(train(&s, &b).state, TrainState::Done);
    assert!(train(&s, &b).position.berthed);
}

#[test]
fn subagent_finished_event_finishes_child() {
    let mut e = engine(&[]);
    let s1 = key("S1");
    let a = sub("S1", "a1");
    discover_at(&mut e, s1.clone(), None, None, "2026-11-23T14:00:00Z");
    discover_at(
        &mut e,
        a.clone(),
        Some(s1.clone()),
        None,
        "2026-11-23T14:00:12Z",
    );
    e.apply(ev(Event::SubagentFinished {
        key: a.clone(),
        at: ts("2026-11-23T14:03:00Z"),
    }));
    let s = e.snapshot();
    assert_eq!(train(&s, &a).state, TrainState::Done);
    assert!(train(&s, &a).position.berthed);
}

fn replayable(e: &mut Engine, s1: &SessionKey, a: &SessionKey) {
    discover_at(e, s1.clone(), None, None, "2026-11-23T14:00:00Z");
    first_prompt(
        e,
        s1,
        "pelhaim-project: dispatch\nFan out",
        "2026-11-23T14:00:05Z",
    );
    tool_open(e, s1, "toolu_A", "Agent", "2026-11-23T14:00:10Z");
    spawn(
        e,
        s1,
        "toolu_A",
        "A",
        "pelhaim-project: ethica\nGo",
        "2026-11-23T14:00:10Z",
    );
    discover_at(e, a.clone(), Some(s1.clone()), None, "2026-11-23T14:00:12Z");
    link(e, a, s1, "toolu_A", "A");
    tool_close(e, s1, "toolu_A", "2026-11-23T14:01:00Z");
    turn_end(e, s1, "2026-11-23T14:01:05Z");
    prompt(e, s1, "Second round", "2026-11-23T14:02:00Z");
    tool_open(e, s1, "toolu_C", "Bash", "2026-11-23T14:02:10Z");
}

#[test]
fn session_reset_replay_has_no_duplicates() {
    let mut e = engine(&["ethica"]);
    let s1 = key("S1");
    let a = sub("S1", "a1");
    replayable(&mut e, &s1, &a);
    let before = e.snapshot();

    e.apply(ev(Event::SessionReset { key: s1.clone() }));
    let cleared = e.snapshot();
    assert!(track(&cleared, &s1).stations.is_empty());
    assert!(cleared.forks.is_empty());
    assert_eq!(track(&cleared, &a).origin, None);

    replayable(&mut e, &s1, &a);
    let after = e.snapshot();
    let (tb, ta) = (train(&before, &s1), train(&after, &s1));
    assert_eq!(ta.id, tb.id);
    assert_eq!(ta.number, tb.number);
    assert_eq!(ta.position, tb.position);
    assert_eq!(ta.state, tb.state);
    assert_eq!(ta.counters, tb.counters);
    assert_eq!(station_titles(&after, &s1), station_titles(&before, &s1));
    assert_eq!(after.stations.len(), before.stations.len());
    assert_eq!(after.trains.len(), 2);
    assert_eq!(after.forks.len(), 1);
    assert_eq!(after.forks[0].at, before.forks[0].at);
    assert_eq!(track(&after, &a).origin, Some(after.forks[0].id));
    assert_eq!(train(&after, &a).state, TrainState::Done);
}

#[test]
fn child_reset_relinks_to_a_single_fork() {
    let mut e = engine(&["ethica"]);
    let s1 = key("S1");
    let a = sub("S1", "a1");
    replayable(&mut e, &s1, &a);
    e.apply(ev(Event::SessionReset { key: a.clone() }));
    link(&mut e, &a, &s1, "toolu_A", "A");
    link(&mut e, &a, &s1, "toolu_A", "A");
    let s = e.snapshot();
    assert_eq!(s.forks.len(), 1);
    assert_eq!(track(&s, &a).stations.len(), 1);
    assert_eq!(track(&s, &a).line, LineKey::Project("ethica".into()));
}

#[test]
fn session_gone_removes_train_and_forks() {
    let mut e = engine(&["ethica"]);
    let s1 = key("S1");
    let a = sub("S1", "a1");
    replayable(&mut e, &s1, &a);
    let parent_track = train(&e.snapshot(), &s1).track;

    e.apply(ev(Event::SessionGone { key: s1.clone() }));
    let s = e.snapshot();
    assert!(s.trains.values().all(|t| t.session != s1));
    assert!(!s.tracks.contains_key(&parent_track));
    assert!(s.stations.values().all(|st| st.track != parent_track));
    assert!(s.forks.is_empty());
    assert!(line_tracks(&s, &LineKey::Dispatch).is_empty());
    let child = train(&s, &a);
    assert_eq!(child.parent, None);
    assert_eq!(track(&s, &a).origin, None);

    e.apply(ev(Event::SessionGone { key: a.clone() }));
    let s = e.snapshot();
    assert!(s.trains.is_empty() && s.tracks.is_empty() && s.stations.is_empty());
    assert!(s.lines.iter().all(|l| !l.active));
}

#[test]
fn lines_order_dispatch_projects_unsorted() {
    let e = engine(&["b", "a"]);
    let s = e.snapshot();
    let keys: Vec<LineKey> = s.lines.iter().map(|l| l.key.clone()).collect();
    assert_eq!(
        keys,
        vec![
            LineKey::Dispatch,
            LineKey::Project("b".into()),
            LineKey::Project("a".into()),
            LineKey::Unsorted
        ]
    );
    let names: Vec<&str> = s.lines.iter().map(|l| l.name.as_str()).collect();
    assert_eq!(names, vec!["DISPATCH", "B", "A", "UNSORTED"]);
    assert!(s.lines.iter().all(|l| !l.active && l.tracks.is_empty()));

    let mut c = cfg(&["b", "a"]);
    c.projects[0].order = 5;
    let s = Engine::new(c, Default::default()).snapshot();
    assert_eq!(s.lines[1].key, LineKey::Project("a".into()));
    assert_eq!(s.lines[2].key, LineKey::Project("b".into()));
}

#[test]
fn tracks_order_working_first_then_recent() {
    let mut e = engine(&[]);
    let (s1, s2, s3) = (key("S1"), key("S2"), key("S3"));
    for (k, at) in [
        (&s1, "2026-11-23T14:00:00Z"),
        (&s2, "2026-11-23T14:01:00Z"),
        (&s3, "2026-11-23T14:02:00Z"),
    ] {
        discover_at(&mut e, k.clone(), None, None, at);
        first_prompt(&mut e, k, "untagged", at);
        turn_end(&mut e, k, at);
    }
    tool_open(&mut e, &s1, "c", "Bash", "2026-11-23T14:00:30Z");
    let s = e.snapshot();
    let order = line_tracks(&s, &LineKey::Unsorted);
    let expect: Vec<_> = [&s1, &s3, &s2].iter().map(|k| train(&s, k).track).collect();
    assert_eq!(order, expect);
    assert!(s.lines.last().unwrap().active);
}

#[test]
fn numbers_labels_and_initial_state() {
    let mut c = cfg(&[]);
    c.local_offset = chrono::FixedOffset::west_opt(5 * 3600).unwrap();
    let mut e = Engine::new(c, Default::default());
    let (s1, s2) = (key("S1"), key("S2"));
    e.apply(discover("S1", "2026-11-23T19:23:10Z"));
    e.apply(discover("S2", "2026-11-23T19:23:50Z"));
    let s = e.snapshot();
    let t1 = train(&s, &s1);
    assert_eq!(t1.number, 1423);
    assert_eq!(train(&s, &s2).number, 1424);
    assert_eq!(t1.state, TrainState::Idle);
    assert_eq!(
        t1.position,
        Position {
            leg: 0,
            block: 0,
            berthed: false
        }
    );
    assert_eq!(t1.label, "Claude session");
    assert_eq!(t1.counters.started, ts("2026-11-23T19:23:10Z"));

    e.apply(ev(Event::TitleChanged {
        key: s1.clone(),
        title: "Rename me".into(),
    }));
    assert_eq!(train(&e.snapshot(), &s1).label, "Rename me");

    // A re-emitted discovery only refreshes cwd and version.
    e.apply(ev(Event::SessionDiscovered {
        key: s1.clone(),
        tool: Tool::Claude,
        parent: None,
        path: "/elsewhere".into(),
        cwd: Some("/new".into()),
        version: Some("2.2.0".into()),
        started: ts("2026-11-23T20:00:00Z"),
    }));
    let s = e.snapshot();
    assert_eq!(s.trains.len(), 2);
    let t1 = train(&s, &s1);
    assert_eq!((t1.number, t1.cwd.as_deref()), (1423, Some("/new")));
}

#[test]
fn failed_recovers_on_next_work() {
    let mut e = engine(&[]);
    let s1 = key("S1");
    e.apply(discover("S1", "2026-11-23T14:00:00Z"));
    first_prompt(&mut e, &s1, "Go", "2026-11-23T14:00:01Z");
    for (i, recover) in [
        Event::ToolOpened {
            key: s1.clone(),
            call_id: "c".into(),
            name: "Bash".into(),
            at: ts("2026-11-23T14:01:00Z"),
        },
        Event::PromptStarted {
            key: s1.clone(),
            title: "Again".into(),
            at: ts("2026-11-23T14:02:00Z"),
        },
        Event::WorkStarted {
            key: s1.clone(),
            at: ts("2026-11-23T14:03:00Z"),
        },
    ]
    .into_iter()
    .enumerate()
    {
        e.apply(ev(Event::Failed {
            key: s1.clone(),
            reason: "aborted".into(),
            at: ts("2026-11-23T14:00:30Z"),
        }));
        assert_eq!(
            train(&e.snapshot(), &s1).state,
            TrainState::Failed("aborted".into()),
            "case {i}"
        );
        e.apply(ev(recover));
        assert_eq!(
            train(&e.snapshot(), &s1).state,
            TrainState::Working,
            "case {i}"
        );
    }
}

#[test]
fn subagent_writes_count_as_parent_activity() {
    let mut e = engine(&["ethica"]);
    let s1 = key("S1");
    let a = sub("S1", "a1");
    discover_at(&mut e, s1.clone(), None, None, "2026-11-23T14:00:00Z");
    first_prompt(
        &mut e,
        &s1,
        "pelhaim-project: dispatch\nFan out",
        "2026-11-23T14:00:05Z",
    );
    tool_open(&mut e, &s1, "toolu_A", "Agent", "2026-11-23T14:00:10Z");
    spawn(
        &mut e,
        &s1,
        "toolu_A",
        "Long job",
        "pelhaim-project: ethica\nGo",
        "2026-11-23T14:00:10Z",
    );
    discover_at(
        &mut e,
        a.clone(),
        Some(s1.clone()),
        None,
        "2026-11-23T14:00:12Z",
    );
    link(&mut e, &a, &s1, "toolu_A", "Long job");
    e.apply(ev(Event::Activity {
        key: a.clone(),
        at: ts("2026-11-23T14:30:00Z"),
    }));
    e.apply(Input::Tick(ts("2026-11-23T14:30:05Z")));

    let s = e.snapshot();
    let parent = train(&s, &s1);
    assert_eq!(parent.state, TrainState::Working);
    assert_eq!(parent.counters.last_seen, ts("2026-11-23T14:00:10Z"));
    assert_eq!(track(&s, &s1).last_activity, ts("2026-11-23T14:30:00Z"));
    assert_eq!(
        e.effective_last_write(parent.id),
        Some(ts("2026-11-23T14:30:00Z"))
    );

    e.apply(ev(Event::SubagentFinished {
        key: a.clone(),
        at: ts("2026-11-23T14:31:00Z"),
    }));
    assert_eq!(
        e.effective_last_write(parent.id),
        Some(ts("2026-11-23T14:00:10Z"))
    );
}

#[test]
fn unknown_keys_are_dropped_and_counted() {
    let mut e = engine(&[]);
    let v = e.snapshot().version;
    e.apply(ev(Event::PromptStarted {
        key: key("nope"),
        title: "x".into(),
        at: ts("2026-11-23T14:00:00Z"),
    }));
    e.apply(ev(Event::SessionTag {
        key: key("nope"),
        tag: TagParse::Absent,
    }));
    assert_eq!(e.dropped_events(), 2);
    assert_eq!(e.snapshot().version, v);
    assert!(e.snapshot().trains.is_empty());
}

#[test]
fn tick_updates_now_and_bumps_version_per_second() {
    let mut e = engine(&[]);
    e.apply(Input::Tick(ts("2026-11-23T14:00:00.100Z")));
    let v = e.snapshot().version;
    e.apply(Input::Tick(ts("2026-11-23T14:00:00.900Z")));
    let s = e.snapshot();
    assert_eq!(s.now, ts("2026-11-23T14:00:00.900Z"));
    assert_eq!(s.version, v);
    e.apply(Input::Tick(ts("2026-11-23T14:00:01.000Z")));
    assert_eq!(e.snapshot().version, v + 1);
}

#[test]
fn transcript_tail_keeps_last_twenty_and_is_not_serialised() {
    let mut e = engine(&[]);
    let s1 = key("S1");
    e.apply(discover("S1", "2026-11-23T14:00:00Z"));
    for i in 0..25 {
        e.apply(ev(Event::TranscriptTail {
            key: s1.clone(),
            line: format!("l{i}"),
        }));
    }
    let s = e.snapshot();
    let tail = &train(&s, &s1).tail;
    assert_eq!(tail.len(), 20);
    assert_eq!((tail[0].as_str(), tail[19].as_str()), ("l5", "l24"));
    let json = serde_json::to_value(train(&s, &s1)).unwrap();
    assert!(json.get("tail").is_none());
    assert!(json.get("label").is_some());
}

#[test]
fn format_untested_marks_train_and_line() {
    let mut e = engine(&[]);
    let s1 = key("S1");
    e.apply(discover("S1", "2026-11-23T14:00:00Z"));
    e.apply(ev(Event::FormatUntested {
        key: s1.clone(),
        version: "9.9.9".into(),
    }));
    let s = e.snapshot();
    assert!(train(&s, &s1).version_untested);
    let unsorted = s.lines.iter().find(|l| l.key == LineKey::Unsorted).unwrap();
    assert!(unsorted.format_untested);
    assert!(!s.lines[0].format_untested);
}
