use std::collections::BTreeMap;
use std::path::PathBuf;

use crate::engine::{Effect, Engine, EngineConfig, Input, ProjectCfg, RestoredState};
use crate::event::{Event, QOption, Question, Tool, Ts};
use crate::ids::SessionKey;
use crate::snapshot::Snapshot;
use crate::tag::parse_tag;

pub fn ts(s: &str) -> Ts {
    chrono::DateTime::parse_from_rfc3339(s)
        .unwrap_or_else(|e| panic!("bad RFC 3339 timestamp {s:?}: {e}"))
        .to_utc()
}

pub fn secs(n: i64) -> chrono::TimeDelta {
    chrono::TimeDelta::seconds(n)
}

pub fn cfg(slugs: &[&str]) -> EngineConfig {
    EngineConfig {
        projects: slugs
            .iter()
            .enumerate()
            .map(|(i, s)| ProjectCfg {
                slug: s.to_string(),
                name: s.to_uppercase(),
                order: i as i32,
            })
            .collect(),
        ..EngineConfig::default()
    }
}

pub fn engine(slugs: &[&str]) -> Engine {
    Engine::new(cfg(slugs), RestoredState::default())
}

/// A Claude main session with session id `key`.
pub fn discover(key: &str, at: &str) -> Input {
    Input::Event(Event::SessionDiscovered {
        key: SessionKey::claude(key),
        tool: Tool::Claude,
        parent: None,
        path: PathBuf::from(format!("/fixtures/claude/{key}.jsonl")),
        cwd: None,
        version: None,
        started: ts(at),
    })
}

pub fn ev(e: Event) -> Input {
    Input::Event(e)
}

pub fn discover_in(e: &mut Engine, key: &SessionKey, parent: Option<&SessionKey>, at: Ts) {
    let tool = if key.0.starts_with("codex:") {
        Tool::Codex
    } else {
        Tool::Claude
    };
    e.apply(ev(Event::SessionDiscovered {
        key: key.clone(),
        tool,
        parent: parent.cloned(),
        path: PathBuf::from(format!("/fixtures/{}.jsonl", key.0)),
        cwd: None,
        version: None,
        started: at,
    }));
}

/// The session's first user message; its first line may carry the tag.
pub fn tag(e: &mut Engine, key: &SessionKey, text: &str) -> Vec<Effect> {
    e.apply(ev(Event::SessionTag {
        key: key.clone(),
        tag: parse_tag(text),
    }))
}

pub fn prompt(e: &mut Engine, key: &SessionKey, title: &str, at: Ts) -> Vec<Effect> {
    e.apply(ev(Event::PromptStarted {
        key: key.clone(),
        title: title.into(),
        at,
    }))
}

pub fn tool_open(e: &mut Engine, key: &SessionKey, call: &str, name: &str, at: Ts) -> Vec<Effect> {
    e.apply(ev(Event::ToolOpened {
        key: key.clone(),
        call_id: call.into(),
        name: name.into(),
        at,
    }))
}

pub fn tool_close(e: &mut Engine, key: &SessionKey, call: &str, at: Ts) -> Vec<Effect> {
    e.apply(ev(Event::ToolClosed {
        key: key.clone(),
        call_id: call.into(),
        launched_only: false,
        at,
    }))
}

pub fn background_launched(e: &mut Engine, key: &SessionKey, call: &str, at: Ts) -> Vec<Effect> {
    e.apply(ev(Event::ToolClosed {
        key: key.clone(),
        call_id: call.into(),
        launched_only: true,
        at,
    }))
}

pub fn activity(e: &mut Engine, key: &SessionKey, at: Ts) -> Vec<Effect> {
    e.apply(ev(Event::Activity {
        key: key.clone(),
        at,
    }))
}

pub fn turn_end(e: &mut Engine, key: &SessionKey, final_text: Option<&str>, at: Ts) -> Vec<Effect> {
    e.apply(ev(Event::TurnEnded {
        key: key.clone(),
        final_text: final_text.map(str::to_string),
        at,
    }))
}

pub fn tick(e: &mut Engine, at: Ts) -> Vec<Effect> {
    e.apply(Input::Tick(at))
}

pub fn question(header: &str, text: &str, labels: &[&str]) -> Question {
    Question {
        header: header.into(),
        question: text.into(),
        multi_select: false,
        options: labels
            .iter()
            .map(|l| QOption {
                label: l.to_string(),
                description: format!("{l} description"),
            })
            .collect(),
    }
}

pub fn ask(
    e: &mut Engine,
    key: &SessionKey,
    call: &str,
    questions: Vec<Question>,
    at: Ts,
) -> Vec<Effect> {
    e.apply(ev(Event::QuestionAsked {
        key: key.clone(),
        call_id: call.into(),
        questions,
        at,
    }))
}

pub fn answered(e: &mut Engine, key: &SessionKey, call: &str, answer: &str, at: Ts) -> Vec<Effect> {
    e.apply(ev(Event::QuestionAnswered {
        key: key.clone(),
        call_id: call.into(),
        answer: answer.into(),
        at,
    }))
}

/// The parent's `Agent` tool_use: opens the call and records the spawn.
pub fn spawn(e: &mut Engine, parent: &SessionKey, call: &str, desc: &str, tag_text: &str, at: Ts) {
    tool_open(e, parent, call, "Agent", at);
    e.apply(ev(Event::SpawnRequested {
        parent: parent.clone(),
        tool_use_id: call.into(),
        description: desc.into(),
        tag: parse_tag(tag_text),
        at,
    }));
}

pub fn link(e: &mut Engine, child: &SessionKey, parent: &SessionKey, call: &str) {
    e.apply(ev(Event::SubagentLinked {
        child: child.clone(),
        parent: parent.clone(),
        tool_use_id: Some(call.into()),
        description: None,
    }));
}

/// The scripted board Tasks 14–16 draw and test against (see the Task 4 brief).
pub fn sample_board() -> Snapshot {
    let t = |hm: &str| ts(&format!("2026-11-23T{hm}Z"));
    let mut e = Engine::new(
        cfg(&["safeascent", "ethica", "cabn", "loosy-goose", "stringcan"]),
        RestoredState::default(),
    );
    e.apply(ev(Event::KeepAwakeEnv {
        mechanism: Some("caffeinate".into()),
        on_battery: Some(false),
    }));

    let d1 = SessionKey::claude("D1");
    discover_in(&mut e, &d1, None, t("13:30:00"));
    tag(&mut e, &d1, "pelhaim-project: dispatch\nRun the M1 board");
    prompt(&mut e, &d1, "Run the M1 board", t("13:30:05"));
    let (a1, a2) = (
        SessionKey::claude_sub("D1", "a1"),
        SessionKey::claude_sub("D1", "a2"),
    );
    spawn(
        &mut e,
        &d1,
        "toolu_e",
        "Ethica scenarios",
        "pelhaim-project: ethica",
        t("13:31:00"),
    );
    discover_in(&mut e, &a1, Some(&d1), t("13:31:02"));
    link(&mut e, &a1, &d1, "toolu_e");
    spawn(
        &mut e,
        &d1,
        "toolu_c",
        "cabn tile set",
        "pelhaim-project: cabn",
        t("13:32:00"),
    );
    discover_in(&mut e, &a2, Some(&d1), t("13:32:02"));
    link(&mut e, &a2, &d1, "toolu_c");
    turn_end(&mut e, &a2, None, t("13:40:00"));
    tool_close(&mut e, &d1, "toolu_c", t("13:40:01"));

    let e1 = SessionKey::claude("E1");
    discover_in(&mut e, &e1, None, t("13:50:00"));
    tag(&mut e, &e1, "pelhaim-project: ethica\nReview the corpus");
    prompt(&mut e, &e1, "Review the corpus", t("13:50:05"));
    tool_open(&mut e, &e1, "toolu_r", "Read", t("13:51:00"));
    tool_close(&mut e, &e1, "toolu_r", t("13:51:01"));
    activity(&mut e, &e1, t("13:55:00"));

    let l1 = SessionKey::codex("L1");
    discover_in(&mut e, &l1, None, t("14:00:00"));
    tag(
        &mut e,
        &l1,
        "pelhaim-project: loosy-goose\nRun the slot experiment",
    );
    e.apply(ev(Event::WorkStarted {
        key: l1.clone(),
        at: t("14:00:05"),
    }));
    e.apply(ev(Event::Failed {
        key: l1.clone(),
        reason: "aborted".into(),
        at: t("14:05:00"),
    }));

    let u1 = SessionKey::claude("U1a2b3c4d5");
    discover_in(&mut e, &u1, None, t("14:10:00"));
    tag(&mut e, &u1, "Tidy the notes");
    prompt(&mut e, &u1, "Tidy the notes", t("14:10:05"));
    turn_end(&mut e, &u1, None, t("14:11:00"));

    let s1 = SessionKey::claude("S1");
    discover_in(&mut e, &s1, None, t("14:23:00"));
    tag(
        &mut e,
        &s1,
        "pelhaim-project: safeascent\nStage C1 rehearsal",
    );
    prompt(&mut e, &s1, "Stage C1 rehearsal", t("14:23:10"));
    tool_open(&mut e, &s1, "toolu_q", "AskUserQuestion", t("14:24:00"));
    ask(
        &mut e,
        &s1,
        "toolu_q",
        vec![question(
            "Stage C1",
            "Proceed to C1?",
            &["Proceed to C1 rehearsal", "Hold at B"],
        )],
        t("14:24:00"),
    );

    activity(&mut e, &a1, t("14:28:00"));
    e.apply(ev(Event::OpenDecisionCounts {
        counts: BTreeMap::from([("safeascent".to_string(), 2)]),
    }));
    tick(&mut e, t("14:30:00"));
    e.snapshot()
}
