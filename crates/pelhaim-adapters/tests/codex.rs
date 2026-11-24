use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use pelhaim_adapters::observe::{CODEX_TESTED, CodexFileParser, discover_codex, is_untested};
use pelhaim_adapters::{TailRead, Tailer};
use pelhaim_core::{
    Engine, EngineConfig, Event, Input, RestoredState, SessionKey, TagParse, Tool, TrainState, Ts,
};

fn fixture_home() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/codex")
}

fn day_dir() -> PathBuf {
    fixture_home().join("sessions/2026/11/23")
}

const C1: &str = "rollout-2026-11-23T09-42-00-C1.jsonl";
const C2: &str = "rollout-2026-11-23T11-00-00-C2.jsonl";

fn ts(s: &str) -> Ts {
    s.parse().unwrap()
}

fn fallback() -> Ts {
    ts("2026-11-23T00:00:00Z")
}

/// Reads through the real tailer, so a final line without its newline is never fed.
fn parse_file(path: &Path) -> (Vec<Event>, CodexFileParser) {
    let mut parser = CodexFileParser::new(path.to_path_buf(), fallback());
    let mut tailer = Tailer::new(path.to_path_buf());
    let mut events = Vec::new();
    loop {
        match tailer.read_new().unwrap() {
            TailRead::Lines(lines) if !lines.is_empty() => {
                for line in lines {
                    events.extend(parser.feed(&line));
                }
            }
            other => {
                assert!(matches!(other, TailRead::Lines(_)), "unexpected {other:?}");
                break;
            }
        }
    }
    (events, parser)
}

fn c1() -> Vec<Event> {
    parse_file(&day_dir().join(C1)).0
}

fn c2() -> Vec<Event> {
    parse_file(&day_dir().join(C2)).0
}

fn parser() -> CodexFileParser {
    CodexFileParser::new(PathBuf::from("rollout-X.jsonl"), fallback())
}

fn feed_all(parser: &mut CodexFileParser, lines: &[&str]) -> Vec<Event> {
    lines
        .iter()
        .flat_map(|l| parser.feed(l.as_bytes()))
        .collect()
}

fn meta(id: &str, at: &str) -> String {
    format!(
        r#"{{"timestamp":"{at}","type":"session_meta","payload":{{"id":"{id}","timestamp":"{at}","cwd":"/w/{id}","cli_version":"0.158.0"}}}}"#
    )
}

fn user(at: &str, text: &str) -> String {
    let text = serde_json::to_string(text).unwrap();
    format!(
        r#"{{"timestamp":"{at}","type":"response_item","payload":{{"type":"message","role":"user","content":[{{"type":"input_text","text":{text}}}]}}}}"#
    )
}

fn kind(e: &Event) -> String {
    format!("{e:?}")
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric())
        .collect()
}

fn kinds(events: &[Event]) -> Vec<String> {
    events.iter().map(kind).collect()
}

fn tags(events: &[Event]) -> Vec<TagParse> {
    events
        .iter()
        .filter_map(|e| match e {
            Event::SessionTag { tag, .. } => Some(tag.clone()),
            _ => None,
        })
        .collect()
}

fn prompt_titles(events: &[Event]) -> Vec<String> {
    events
        .iter()
        .filter_map(|e| match e {
            Event::PromptStarted { title, .. } => Some(title.clone()),
            _ => None,
        })
        .collect()
}

fn tails(events: &[Event]) -> Vec<String> {
    events
        .iter()
        .filter_map(|e| match e {
            Event::TranscriptTail { line, .. } => Some(line.clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn c1_full_event_sequence() {
    let events = c1();
    assert_eq!(
        kinds(&events),
        [
            "SessionDiscovered",
            "Activity",
            "Activity",
            "WorkStarted",
            "Activity",
            "Activity",
            "Activity",
            "SessionTag",
            "PromptStarted",
            "Activity",
            "TranscriptTail",
            "TranscriptTail",
            "Activity",
            "ToolOpened",
            "ToolClosed",
            "ToolOpened",
            "ToolClosed",
            "Activity",
            "TranscriptTail",
            "Activity",
            "TurnEnded",
        ]
    );
    assert_eq!(
        events[0],
        Event::SessionDiscovered {
            key: SessionKey::codex("C1"),
            tool: Tool::Codex,
            parent: None,
            path: day_dir().join(C1),
            cwd: Some("/Users/owner/Developer/ethica".into()),
            version: Some("0.158.0".into()),
            started: ts("2026-11-23T09:42:00Z"),
        }
    );
    assert!(
        events
            .iter()
            .all(|e| e.session().is_none_or(|k| *k == SessionKey::codex("C1")))
    );
}

#[test]
fn injected_user_messages_skipped_for_tag() {
    let events = c1();
    assert_eq!(tags(&events), [TagParse::Tagged("ethica".into())]);
    assert_eq!(prompt_titles(&events), ["Run judge"]);
}

#[test]
fn task_lifecycle_maps() {
    let events = c1();
    let key = SessionKey::codex("C1");
    let pick = |want: &[&str]| -> Vec<Event> {
        events
            .iter()
            .filter(|e| want.contains(&kind(e).as_str()))
            .cloned()
            .collect()
    };
    assert_eq!(
        pick(&["WorkStarted", "ToolOpened", "ToolClosed", "TurnEnded"]),
        [
            Event::WorkStarted {
                key: key.clone(),
                at: ts("2026-11-23T09:42:01Z"),
            },
            Event::ToolOpened {
                key: key.clone(),
                call_id: "call_A1".into(),
                name: "exec_command".into(),
                at: ts("2026-11-23T09:42:04Z"),
            },
            Event::ToolClosed {
                key: key.clone(),
                call_id: "call_A1".into(),
                launched_only: false,
                at: ts("2026-11-23T09:42:09Z"),
            },
            Event::ToolOpened {
                key: key.clone(),
                call_id: "call_A2".into(),
                name: "apply_patch".into(),
                at: ts("2026-11-23T09:42:10Z"),
            },
            Event::ToolClosed {
                key: key.clone(),
                call_id: "call_A2".into(),
                launched_only: false,
                at: ts("2026-11-23T09:42:11Z"),
            },
            Event::TurnEnded {
                key: key.clone(),
                final_text: Some("Done".into()),
                at: ts("2026-11-23T09:42:13.100Z"),
            },
        ]
    );
    assert_eq!(
        tails(&events),
        ["Running the judge.", "One moment.", "Done"]
    );
}

#[test]
fn turn_aborted_fails() {
    let events = c2();
    let failed: Vec<&Event> = events
        .iter()
        .filter(|e| matches!(e, Event::Failed { .. }))
        .collect();
    assert_eq!(
        failed,
        [&Event::Failed {
            key: SessionKey::codex("C2"),
            reason: "aborted".into(),
            at: ts("2026-11-23T11:00:06Z"),
        }]
    );
}

#[test]
fn untagged_codex_session() {
    let events = c2();
    assert!(matches!(
        &events[0],
        Event::SessionDiscovered { version: Some(v), .. } if v == "0.162.1"
    ));
    assert_eq!(tags(&events), [TagParse::Absent]);
    // The `<turn_aborted>` user record is the harness, not a second prompt.
    assert_eq!(prompt_titles(&events), ["Résumé the café notes ✓"]);
    assert_eq!(
        kinds(&events),
        [
            "SessionDiscovered",
            "Activity",
            "WorkStarted",
            "Activity",
            "SessionTag",
            "PromptStarted",
            "TranscriptTail",
            "Activity",
            "Failed",
            "Activity",
            "Activity",
        ]
    );
}

#[test]
fn second_session_meta_ignored() {
    let mut p = parser();
    let events = feed_all(
        &mut p,
        &[
            &meta("first", "2026-11-23T10:00:00Z"),
            &meta("second", "2026-11-23T10:00:05Z"),
            &user("2026-11-23T10:00:06Z", "hello"),
        ],
    );
    assert_eq!(p.key(), Some(&SessionKey::codex("first")));
    assert_eq!(
        kinds(&events),
        [
            "SessionDiscovered",
            "Activity",
            "Activity",
            "SessionTag",
            "PromptStarted",
        ]
    );
    assert!(
        events
            .iter()
            .all(|e| e.session() == Some(&SessionKey::codex("first")))
    );
    assert_eq!(
        events[2],
        Event::Activity {
            key: SessionKey::codex("first"),
            at: ts("2026-11-23T10:00:05Z"),
        }
    );
}

#[test]
fn lines_before_meta_buffered() {
    let mut p = parser();
    assert_eq!(p.key(), None);
    let early = [
        r#"{"timestamp":"2026-11-23T10:00:00Z","type":"event_msg","payload":{"type":"task_started"}}"#
            .to_string(),
        user("2026-11-23T10:00:01Z", "pelhaim-project: cabn\nDraw the atlas"),
    ];
    for line in &early {
        assert!(p.feed(line.as_bytes()).is_empty());
    }
    assert_eq!(p.key(), None);
    let events = p.feed(meta("M", "2026-11-23T10:00:02Z").as_bytes());
    let key = SessionKey::codex("M");
    assert_eq!(p.key(), Some(&key));
    assert_eq!(
        events,
        [
            Event::SessionDiscovered {
                key: key.clone(),
                tool: Tool::Codex,
                parent: None,
                path: PathBuf::from("rollout-X.jsonl"),
                cwd: Some("/w/M".into()),
                version: Some("0.158.0".into()),
                started: ts("2026-11-23T10:00:02Z"),
            },
            Event::WorkStarted {
                key: key.clone(),
                at: ts("2026-11-23T10:00:00Z"),
            },
            Event::SessionTag {
                key: key.clone(),
                tag: TagParse::Tagged("cabn".into()),
            },
            Event::PromptStarted {
                key: key.clone(),
                title: "Draw the atlas".into(),
                at: ts("2026-11-23T10:00:01Z"),
            },
            Event::Activity {
                key: key.clone(),
                at: ts("2026-11-23T10:00:02Z"),
            },
        ]
    );
}

#[test]
fn started_falls_back_to_line_timestamp() {
    let mut p = parser();
    let events = p.feed(
        br#"{"timestamp":"2026-11-23T10:00:09Z","type":"session_meta","payload":{"id":"N"}}"#,
    );
    assert_eq!(
        events[0],
        Event::SessionDiscovered {
            key: SessionKey::codex("N"),
            tool: Tool::Codex,
            parent: None,
            path: PathBuf::from("rollout-X.jsonl"),
            cwd: None,
            version: None,
            started: ts("2026-11-23T10:00:09Z"),
        }
    );
}

#[test]
fn meta_without_id_does_not_discover() {
    let mut p = parser();
    assert!(
        p.feed(
            br#"{"timestamp":"2026-11-23T10:00:00Z","type":"session_meta","payload":{"cwd":"/w"}}"#
        )
        .is_empty()
    );
    assert_eq!(p.key(), None);
    let events = p.feed(meta("L", "2026-11-23T10:00:01Z").as_bytes());
    assert_eq!(
        kinds(&events),
        ["SessionDiscovered", "Activity", "Activity"]
    );
}

#[test]
fn harness_injected_user_messages_are_activity() {
    let mut p = parser();
    let injected = [
        "# AGENTS.md instructions for /w\n\n<INSTRUCTIONS>\npelhaim-project: wrong\n</INSTRUCTIONS>",
        "  <environment_context>\n<cwd>/w</cwd>\n</environment_context>",
        "<user_instructions>\npelhaim-project: wrong\n</user_instructions>",
        "<turn_aborted>\nThe turn was interrupted.\n</turn_aborted>",
        "<subagent_notification>\n{\"status\":\"done\"}\n</subagent_notification>",
        "<user_action>\nreview\n</user_action>",
        "",
    ];
    let mut lines = vec![meta("H", "2026-11-23T10:00:00Z")];
    lines.extend(injected.iter().map(|t| user("2026-11-23T10:00:01Z", t)));
    lines.push(user("2026-11-23T10:00:02Z", "First real ask"));
    lines.push(user(
        "2026-11-23T10:00:03Z",
        "<turn_aborted>\nx\n</turn_aborted>",
    ));
    lines.push(user(
        "2026-11-23T10:00:04Z",
        "pelhaim-project: late\nSecond ask",
    ));
    let refs: Vec<&str> = lines.iter().map(String::as_str).collect();
    let events = feed_all(&mut p, &refs);
    assert_eq!(tags(&events), [TagParse::Absent]);
    assert_eq!(prompt_titles(&events), ["First real ask", "Second ask"]);
    let activities = events
        .iter()
        .filter(|e| matches!(e, Event::Activity { .. }))
        .count();
    assert_eq!(activities, 1 + injected.len() + 1);
}

#[test]
fn ide_context_prompt_reads_the_request() {
    let mut p = parser();
    let text = "# Context from my IDE setup:\n\n## Active file: src/main.rs\n\n## Open tabs:\n- main.rs: src/main.rs\n\n## My request for Codex:\npelhaim-project: cabn\nFix the sprite atlas\n";
    let events = feed_all(
        &mut p,
        &[
            &meta("V", "2026-11-23T10:00:00Z"),
            &user("2026-11-23T10:00:01Z", text),
        ],
    );
    assert_eq!(tags(&events), [TagParse::Tagged("cabn".into())]);
    assert_eq!(prompt_titles(&events), ["Fix the sprite atlas"]);
}

#[test]
fn developer_message_is_activity_only() {
    let mut p = parser();
    let events = feed_all(
        &mut p,
        &[
            &meta("D", "2026-11-23T10:00:00Z"),
            r#"{"timestamp":"2026-11-23T10:00:01Z","type":"response_item","payload":{"type":"message","role":"developer","content":[{"type":"input_text","text":"pelhaim-project: dev\nhi"}]}}"#,
        ],
    );
    assert_eq!(
        kinds(&events),
        ["SessionDiscovered", "Activity", "Activity"]
    );
}

#[test]
fn non_ascii_and_controls_in_prompt_and_tail() {
    let mut p = parser();
    let long = "日".repeat(200);
    let assistant = format!(
        r#"{{"timestamp":"2026-11-23T10:00:02Z","type":"response_item","payload":{{"type":"message","role":"assistant","content":[{{"type":"output_text","text":"a\u001b[1mb\u0007c\n\n  \n{long}"}},{{"type":"reasoning_text","text":"hidden"}}]}}}}"#
    );
    let events = feed_all(
        &mut p,
        &[
            &meta("U", "2026-11-23T10:00:00Z"),
            &user(
                "2026-11-23T10:00:01Z",
                "pelhaim-project: cabn\n\u{1b}[31mÜber\u{7} naïve 東京\n",
            ),
            &assistant,
        ],
    );
    let titles = prompt_titles(&events);
    assert_eq!(titles.len(), 1);
    assert!(!titles[0].contains("pelhaim-project"));
    assert!(titles[0].contains("Über"));
    assert!(titles[0].contains("東京"));
    assert!(!titles[0].chars().any(char::is_control));
    let tails = tails(&events);
    assert_eq!(tails.len(), 2);
    assert_eq!(tails[0], "a b c");
    assert_eq!(tails[1].chars().count(), 120);
    assert!(tails[1].ends_with('…'));
}

#[test]
fn long_prompt_title_truncated() {
    let mut p = parser();
    let ask = "é".repeat(300);
    let events = feed_all(
        &mut p,
        &[
            &meta("T", "2026-11-23T10:00:00Z"),
            &user("2026-11-23T10:00:01Z", &ask),
        ],
    );
    let titles = prompt_titles(&events);
    assert!(titles[0].chars().count() <= 40);
    assert!(titles[0].ends_with('…'));
}

#[test]
fn record_without_timestamp_uses_previous() {
    let mut p = parser();
    let events = feed_all(
        &mut p,
        &[
            &meta("P", "2026-11-23T10:00:00Z"),
            r#"{"timestamp":"2026-11-23T10:00:07Z","type":"event_msg","payload":{"type":"token_count"}}"#,
            r#"{"type":"response_item","payload":{"type":"function_call","name":"shell","call_id":"c9"}}"#,
        ],
    );
    assert_eq!(
        events.last(),
        Some(&Event::ToolOpened {
            key: SessionKey::codex("P"),
            call_id: "c9".into(),
            name: "shell".into(),
            at: ts("2026-11-23T10:00:07Z"),
        })
    );

    let mut q = parser();
    let events = q.feed(br#"{"type":"session_meta","payload":{"id":"Q"}}"#);
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].at(), Some(fallback()));
}

#[test]
fn task_complete_without_message_has_no_final_text() {
    let mut p = parser();
    let events = feed_all(
        &mut p,
        &[
            &meta("F", "2026-11-23T10:00:00Z"),
            r#"{"timestamp":"2026-11-23T10:00:01Z","type":"event_msg","payload":{"type":"task_complete","last_agent_message":null}}"#,
            r#"{"timestamp":"2026-11-23T10:00:02Z","type":"event_msg","payload":{"type":"task_complete","last_agent_message":""}}"#,
        ],
    );
    let finals: Vec<Option<String>> = events
        .iter()
        .filter_map(|e| match e {
            Event::TurnEnded { final_text, .. } => Some(final_text.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(finals, [None, None]);
}

#[test]
fn invalid_lines_skipped_not_fatal() {
    let mut p = parser();
    for bad in ["[1,2]", "\"str\"", "42", "null", "{", "\u{fffd}"] {
        assert!(p.feed(bad.as_bytes()).is_empty());
    }
    assert!(p.feed(&[0xff, 0xfe]).is_empty());
    assert!(p.feed(b"   ").is_empty());
    assert_eq!(p.skipped(), 7);
    assert_eq!(
        kinds(&p.feed(meta("S", "2026-11-23T10:00:00Z").as_bytes())),
        ["SessionDiscovered", "Activity"]
    );
}

#[test]
fn untested_version_flagged() {
    assert!(!is_untested("0.158.0", CODEX_TESTED));
    assert!(!is_untested("0.162.1", CODEX_TESTED));
    assert!(is_untested("0.162.2", CODEX_TESTED));
    assert!(is_untested("0.163.0", CODEX_TESTED));
    let mut p = parser();
    let events = p.feed(
        br#"{"timestamp":"2026-11-23T10:00:00Z","type":"session_meta","payload":{"id":"V","cli_version":"0.163.0"}}"#,
    );
    assert_eq!(
        events[1],
        Event::FormatUntested {
            key: SessionKey::codex("V"),
            version: "0.163.0".into(),
        }
    );
}

#[test]
fn c1_drives_a_tagged_train_to_idle() {
    let mut engine = Engine::new(EngineConfig::default(), RestoredState::default());
    for e in c1() {
        engine.apply(Input::Event(e));
    }
    engine.apply(Input::Event(Event::CatchUpDone { tool: Tool::Codex }));
    let snap = engine.snapshot();
    let train = snap
        .trains
        .values()
        .find(|t| t.session == SessionKey::codex("C1"))
        .unwrap();
    assert_eq!(train.state, TrainState::Idle);
    assert_eq!(engine.dropped_events(), 0);
}

/// F25: checked-in mtimes are checkout times, so the test sets its own.
fn home_with(now: SystemTime, files: &[(&str, u64)]) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    for (rel, age) in files {
        let path = dir.path().join(rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let f = fs::File::create(&path).unwrap();
        f.set_modified(now - Duration::from_secs(*age)).unwrap();
    }
    dir
}

#[test]
fn discovery_globs_rollouts_by_mtime_within_window() {
    let now = SystemTime::now();
    let home = home_with(
        now,
        &[
            (
                "sessions/2026/11/23/rollout-2026-11-23T09-42-00-C1.jsonl",
                60,
            ),
            (
                "sessions/2026/11/23/rollout-2026-11-23T11-00-00-C2.jsonl",
                600,
            ),
            (
                "sessions/2026/11/22/rollout-2026-11-22T08-00-00-OLD.jsonl",
                90_000,
            ),
            (
                "sessions/2026/11/21/rollout-2026-11-21T08-00-00-B.jsonl",
                300,
            ),
            ("sessions/2026/11/23/notes.jsonl", 10),
            ("sessions/2026/11/23/rollout-x.json", 10),
            ("sessions/2026/11/rollout-shallow.jsonl", 10),
            ("sessions/2026/11/23/deep/rollout-deep.jsonl", 10),
            ("archived_sessions/rollout-2026-11-23T09-00-00-A.jsonl", 10),
        ],
    );
    let found = discover_codex(home.path(), Duration::from_secs(3600), now);
    let names: Vec<String> = found
        .iter()
        .map(|p| p.strip_prefix(home.path()).unwrap().display().to_string())
        .collect();
    assert_eq!(
        names,
        [
            "sessions/2026/11/23/rollout-2026-11-23T11-00-00-C2.jsonl",
            "sessions/2026/11/21/rollout-2026-11-21T08-00-00-B.jsonl",
            "sessions/2026/11/23/rollout-2026-11-23T09-42-00-C1.jsonl",
        ]
    );
}

#[test]
fn discovery_counts_future_mtime_as_fresh() {
    let now = SystemTime::now();
    let home = tempfile::tempdir().unwrap();
    let path = home.path().join("sessions/2026/11/23/rollout-F.jsonl");
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let f = fs::File::create(&path).unwrap();
    f.set_modified(now + Duration::from_secs(600)).unwrap();
    assert_eq!(
        discover_codex(home.path(), Duration::from_secs(60), now),
        [path]
    );
}

#[test]
fn discovery_of_missing_home_is_empty() {
    let home = tempfile::tempdir().unwrap();
    assert!(
        discover_codex(
            &home.path().join("nope"),
            Duration::from_secs(60),
            SystemTime::now()
        )
        .is_empty()
    );
}
