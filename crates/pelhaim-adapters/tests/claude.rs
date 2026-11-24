use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use pelhaim_adapters::observe::{
    CLAUDE_TESTED, ClaudeFile, ClaudeFileParser, discover_claude, is_untested, parse_meta,
};
use pelhaim_adapters::{TailRead, Tailer};
use pelhaim_core::{Event, QOption, Question, SessionKey, TagParse, Tool, Ts};

fn fixture_home() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/claude")
}

fn project_dir() -> PathBuf {
    fixture_home().join("projects/-Users-owner")
}

fn ts(s: &str) -> Ts {
    s.parse().unwrap()
}

fn fallback() -> Ts {
    ts("2026-11-23T00:00:00Z")
}

/// Reads through the real tailer, so a final line without its newline is never fed.
fn parse_file(
    path: &Path,
    key: SessionKey,
    parent: Option<SessionKey>,
) -> (Vec<Event>, ClaudeFileParser) {
    let mut parser = ClaudeFileParser::new(key, path.to_path_buf(), parent, fallback());
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

fn s1() -> Vec<Event> {
    parse_file(
        &project_dir().join("S1.jsonl"),
        SessionKey::claude("S1"),
        None,
    )
    .0
}

fn sub(agent: &str) -> Vec<Event> {
    parse_file(
        &project_dir().join(format!("S1/subagents/agent-{agent}.jsonl")),
        SessionKey::claude_sub("S1", agent),
        Some(SessionKey::claude("S1")),
    )
    .0
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

#[test]
fn s1_full_event_sequence() {
    let events = s1();
    assert_eq!(
        kinds(&events),
        [
            "SessionDiscovered",
            "SessionTag",
            "PromptStarted",
            "TitleChanged",
            "ToolOpened",
            "SpawnRequested",
            "ToolOpened",
            "SpawnRequested",
            "ToolClosed",
            "TranscriptTail",
            "TranscriptTail",
            "Activity",
            "ToolOpened",
            "QuestionAsked",
            "QuestionAnswered",
            "ToolClosed",
            "ToolClosed",
            "TranscriptTail",
            "TranscriptTail",
            "TurnEnded",
            "Activity",
            "Activity",
            "Activity",
            "PromptStarted",
        ]
    );
    let key = SessionKey::claude("S1");
    assert_eq!(
        events[0],
        Event::SessionDiscovered {
            key: key.clone(),
            tool: Tool::Claude,
            parent: None,
            path: project_dir().join("S1.jsonl"),
            cwd: Some("/Users/owner".into()),
            version: Some("2.1.331".into()),
            started: ts("2026-11-23T14:23:00Z"),
        }
    );
    assert_eq!(
        events[1],
        Event::SessionTag {
            key: key.clone(),
            tag: TagParse::Tagged("dispatch".into()),
        }
    );
    assert_eq!(
        events[2],
        Event::PromptStarted {
            key: key.clone(),
            title: "Plan the week".into(),
            at: ts("2026-11-23T14:23:00Z"),
        }
    );
    assert_eq!(
        events[3],
        Event::TitleChanged {
            key: key.clone(),
            title: "Weekly plan — café".into(),
        }
    );
    assert_eq!(
        events[9],
        Event::TranscriptTail {
            key: key.clone(),
            line: "Judge is running.".into(),
        }
    );
    assert_eq!(
        events[19],
        Event::TurnEnded {
            key: key.clone(),
            final_text: Some("- [x] judge run\n- [ ] corpus review".into()),
            at: ts("2026-11-23T14:32:00Z"),
        }
    );
    // Review Focus 2: ANSI, NUL and CJK in a prompt.
    assert_eq!(
        events[23],
        Event::PromptStarted {
            key,
            title: "Now ship it to 漢字 users".into(),
            at: ts("2026-11-23T14:41:00Z"),
        }
    );
}

#[test]
fn agent_tool_use_yields_spawn_with_tag() {
    let events = s1();
    let spawns: Vec<_> = events
        .iter()
        .filter(|e| matches!(e, Event::SpawnRequested { .. }))
        .cloned()
        .collect();
    assert_eq!(
        spawns,
        [
            Event::SpawnRequested {
                parent: SessionKey::claude("S1"),
                tool_use_id: "toolu_A1".into(),
                description: "Run the judge".into(),
                tag: TagParse::Tagged("ethica".into()),
                at: ts("2026-11-23T14:23:05Z"),
            },
            Event::SpawnRequested {
                parent: SessionKey::claude("S1"),
                tool_use_id: "toolu_A2".into(),
                description: "Review the corpus".into(),
                tag: TagParse::Absent,
                at: ts("2026-11-23T14:23:06Z"),
            },
        ]
    );
}

fn closes(events: &[Event]) -> Vec<(String, bool)> {
    events
        .iter()
        .filter_map(|e| match e {
            Event::ToolClosed {
                call_id,
                launched_only,
                ..
            } => Some((call_id.clone(), *launched_only)),
            _ => None,
        })
        .collect()
}

#[test]
fn background_agent_launch_closes_launched_only() {
    assert_eq!(
        closes(&s1()),
        [
            ("toolu_A2".to_string(), true),
            ("toolu_Q1".to_string(), false),
            ("toolu_A1".to_string(), false),
        ]
    );
}

fn feed_all(parser: &mut ClaudeFileParser, lines: &[&str]) -> Vec<Event> {
    lines
        .iter()
        .flat_map(|l| parser.feed(l.as_bytes()))
        .collect()
}

#[test]
fn run_in_background_input_closes_launched_only() {
    let mut p = ClaudeFileParser::new(
        SessionKey::claude("B"),
        PathBuf::from("B.jsonl"),
        None,
        fallback(),
    );
    let events = feed_all(
        &mut p,
        &[
            r#"{"type":"assistant","timestamp":"2026-11-23T10:00:00Z","message":{"stop_reason":"tool_use","content":[{"type":"tool_use","id":"t1","name":"Task","input":{"description":"d","prompt":"p","run_in_background":true}}]}}"#,
            r#"{"type":"assistant","timestamp":"2026-11-23T10:00:01Z","message":{"stop_reason":"tool_use","content":[{"type":"tool_use","id":"t2","name":"Bash","input":{"command":"x","run_in_background":true}}]}}"#,
            r#"{"type":"user","timestamp":"2026-11-23T10:00:02Z","message":{"content":[{"type":"tool_result","tool_use_id":"t1","content":"queued"}]}}"#,
            r#"{"type":"user","timestamp":"2026-11-23T10:00:03Z","message":{"content":[{"type":"tool_result","tool_use_id":"t2","content":"Async agent launched"}]}}"#,
        ],
    );
    // Only Agent/Task calls can be launch acknowledgements.
    assert_eq!(
        closes(&events),
        [("t1".to_string(), true), ("t2".to_string(), false)]
    );
}

#[test]
fn ask_user_question_open_and_close() {
    let events = s1();
    let asked = events
        .iter()
        .find(|e| matches!(e, Event::QuestionAsked { .. }))
        .unwrap();
    assert_eq!(
        *asked,
        Event::QuestionAsked {
            key: SessionKey::claude("S1"),
            call_id: "toolu_Q1".into(),
            questions: vec![Question {
                header: "Stage C1".into(),
                question: "Proceed to C1?".into(),
                multi_select: false,
                options: vec![
                    QOption {
                        label: "Proceed".into(),
                        description: "run rehearsal".into(),
                    },
                    QOption {
                        label: "Hold".into(),
                        description: "wait".into(),
                    },
                ],
            }],
            at: ts("2026-11-23T14:30:00Z"),
        }
    );
    let answered = events
        .iter()
        .find(|e| matches!(e, Event::QuestionAnswered { .. }))
        .unwrap();
    assert_eq!(
        *answered,
        Event::QuestionAnswered {
            key: SessionKey::claude("S1"),
            call_id: "toolu_Q1".into(),
            answer: "Proceed".into(),
            at: ts("2026-11-23T14:31:00Z"),
        }
    );
}

#[test]
fn string_tool_result_answers_question() {
    let mut p = ClaudeFileParser::new(
        SessionKey::claude("Q"),
        PathBuf::from("Q.jsonl"),
        None,
        fallback(),
    );
    let events = feed_all(
        &mut p,
        &[
            r#"{"type":"assistant","timestamp":"2026-11-23T10:00:00Z","message":{"stop_reason":"tool_use","content":[{"type":"tool_use","id":"q","name":"AskUserQuestion","input":{"questions":[{"question":"Go?","header":"H","multiSelect":true,"options":[]}]}}]}}"#,
            r#"{"type":"user","timestamp":"2026-11-23T10:00:05Z","message":{"content":[{"type":"tool_result","tool_use_id":"q","content":"Yes"}]}}"#,
        ],
    );
    assert!(events.contains(&Event::QuestionAnswered {
        key: SessionKey::claude("Q"),
        call_id: "q".into(),
        answer: "Yes".into(),
        at: ts("2026-11-23T10:00:05Z"),
    }));
}

#[test]
fn end_turn_only_on_stop_reason() {
    let events = s1();
    let ends: Vec<_> = events
        .iter()
        .filter(|e| matches!(e, Event::TurnEnded { .. }))
        .collect();
    assert_eq!(
        ends.len(),
        1,
        "the stop_reason tool_use text record must not end the turn"
    );
    assert_eq!(ends[0].at(), Some(ts("2026-11-23T14:32:00Z")));

    let mut p = ClaudeFileParser::new(
        SessionKey::claude("E"),
        PathBuf::from("E.jsonl"),
        None,
        fallback(),
    );
    let events = feed_all(
        &mut p,
        &[
            r#"{"type":"assistant","timestamp":"2026-11-23T10:00:00Z","message":{"stop_reason":"stop_sequence","content":[{"type":"thinking","thinking":"x"}]}}"#,
        ],
    );
    assert_eq!(
        events.last(),
        Some(&Event::TurnEnded {
            key: SessionKey::claude("E"),
            final_text: None,
            at: ts("2026-11-23T10:00:00Z"),
        })
    );
}

#[test]
fn subagent_first_user_is_tag_only() {
    let a2 = sub("a2");
    assert_eq!(
        kinds(&a2),
        ["SessionDiscovered", "SessionTag", "Activity", "ToolOpened"]
    );
    assert_eq!(
        a2[0],
        Event::SessionDiscovered {
            key: SessionKey::claude_sub("S1", "a2"),
            tool: Tool::Claude,
            parent: Some(SessionKey::claude("S1")),
            path: project_dir().join("S1/subagents/agent-a2.jsonl"),
            cwd: Some("/Users/owner/Developer".into()),
            version: Some("2.1.331".into()),
            started: ts("2026-11-23T14:23:08Z"),
        }
    );
    assert_eq!(
        a2[1],
        Event::SessionTag {
            key: SessionKey::claude_sub("S1", "a2"),
            tag: TagParse::Absent,
        }
    );

    let a1 = sub("a1");
    assert!(!a1.iter().any(|e| matches!(e, Event::PromptStarted { .. })));
    assert_eq!(
        a1[1],
        Event::SessionTag {
            key: SessionKey::claude_sub("S1", "a1"),
            tag: TagParse::Tagged("ethica".into()),
        }
    );
}

#[test]
fn meta_links_and_finishes() {
    let child = SessionKey::claude_sub("S1", "a1");
    let parent = SessionKey::claude("S1");
    let (_, parser) = parse_file(
        &project_dir().join("S1/subagents/agent-a1.jsonl"),
        child.clone(),
        Some(parent.clone()),
    );
    let at = parser.last_at().unwrap();
    assert_eq!(at, ts("2026-11-23T14:31:20Z"));
    let json = fs::read(project_dir().join("S1/subagents/agent-a1.meta.json")).unwrap();
    assert_eq!(
        parse_meta(&child, &parent, &json, at),
        [
            Event::SubagentLinked {
                child: child.clone(),
                parent: parent.clone(),
                tool_use_id: Some("toolu_A1".into()),
                description: Some("Run the judge".into()),
            },
            Event::SubagentFinished { key: child, at },
        ]
    );

    let child2 = SessionKey::claude_sub("S1", "a2");
    let json = fs::read(project_dir().join("S1/subagents/agent-a2.meta.json")).unwrap();
    assert_eq!(
        parse_meta(&child2, &parent, &json, at),
        [Event::SubagentLinked {
            child: child2.clone(),
            parent: parent.clone(),
            tool_use_id: Some("toolu_A2".into()),
            description: Some("Review the corpus".into()),
        }]
    );
    assert!(parse_meta(&child2, &parent, b"{not json", at).is_empty());
    assert!(parse_meta(&child2, &parent, b"[1]", at).is_empty());
}

#[test]
fn invalid_lines_skipped_not_fatal() {
    let (events, parser) = parse_file(
        &project_dir().join("S3.jsonl"),
        SessionKey::claude("S3"),
        None,
    );
    assert_eq!(parser.skipped(), 1);
    assert_eq!(
        kinds(&events),
        [
            "SessionDiscovered",
            "FormatUntested",
            "SessionTag",
            "PromptStarted",
            "ToolOpened",
            "Failed",
        ]
    );

    let mut p = ClaudeFileParser::new(
        SessionKey::claude("N"),
        PathBuf::from("N.jsonl"),
        None,
        fallback(),
    );
    for bad in ["[1,2]", "\"str\"", "42", "null", "{", "\u{fffd}"] {
        assert!(p.feed(bad.as_bytes()).is_empty());
    }
    assert!(p.feed(&[0xff, 0xfe]).is_empty());
    assert_eq!(p.skipped(), 7);
}

#[test]
fn record_without_timestamp_uses_previous() {
    let (events, _) = parse_file(
        &project_dir().join("S3.jsonl"),
        SessionKey::claude("S3"),
        None,
    );
    let opened = events
        .iter()
        .find(|e| matches!(e, Event::ToolOpened { .. }))
        .unwrap();
    assert_eq!(
        *opened,
        Event::ToolOpened {
            key: SessionKey::claude("S3"),
            call_id: "toolu_R1".into(),
            name: "Read".into(),
            at: ts("2026-11-23T10:00:00Z"),
        }
    );
}

#[test]
fn records_before_first_timestamp_are_buffered_until_discovered() {
    let key = SessionKey::claude("F");
    let mut p = ClaudeFileParser::new(key.clone(), PathBuf::from("F.jsonl"), None, fallback());
    assert!(p.last_at().is_none());
    let early = feed_all(
        &mut p,
        &[
            r#"{"type":"ai-title","aiTitle":"Early"}"#,
            r#"{"type":"user","cwd":"/w","version":"2.1.331","message":{"content":"pelhaim-project: cabn\nFirst"}}"#,
        ],
    );
    assert!(
        early.is_empty(),
        "nothing reaches the engine before SessionDiscovered"
    );
    let events = p.feed(br#"{"type":"queue-operation","timestamp":"2026-11-23T10:00:00Z"}"#);
    assert_eq!(
        events,
        [
            Event::SessionDiscovered {
                key: key.clone(),
                tool: Tool::Claude,
                parent: None,
                path: PathBuf::from("F.jsonl"),
                cwd: Some("/w".into()),
                version: Some("2.1.331".into()),
                started: ts("2026-11-23T10:00:00Z"),
            },
            Event::TitleChanged {
                key: key.clone(),
                title: "Early".into(),
            },
            Event::SessionTag {
                key: key.clone(),
                tag: TagParse::Tagged("cabn".into()),
            },
            Event::PromptStarted {
                key: key.clone(),
                title: "First".into(),
                at: fallback(),
            },
            Event::Activity {
                key,
                at: ts("2026-11-23T10:00:00Z"),
            },
        ]
    );
    assert_eq!(p.last_at(), Some(ts("2026-11-23T10:00:00Z")));
}

#[test]
fn cwd_learned_after_discovery_rediscovers() {
    let (events, _) = parse_file(
        &project_dir().join("S2.jsonl"),
        SessionKey::claude("S2"),
        None,
    );
    let key = SessionKey::claude("S2");
    let discovered = |cwd: Option<&str>, version: Option<&str>| Event::SessionDiscovered {
        key: key.clone(),
        tool: Tool::Claude,
        parent: None,
        path: project_dir().join("S2.jsonl"),
        cwd: cwd.map(Into::into),
        version: version.map(Into::into),
        started: ts("2026-11-23T09:00:00Z"),
    };
    assert_eq!(events[0], discovered(None, None));
    assert_eq!(
        events[2],
        discovered(Some("/Users/owner/Developer"), Some("2.1.331"))
    );
    assert_eq!(
        kinds(&events),
        [
            "SessionDiscovered",
            "Activity",
            "SessionDiscovered",
            "SessionTag",
            "PromptStarted",
            "TranscriptTail",
            "TurnEnded",
        ]
    );
    assert_eq!(
        events[3],
        Event::SessionTag {
            key,
            tag: TagParse::Absent,
        }
    );
}

#[test]
fn transcript_tail_sanitizes_and_truncates() {
    let mut p = ClaudeFileParser::new(
        SessionKey::claude("T"),
        PathBuf::from("T.jsonl"),
        None,
        fallback(),
    );
    let long = "é".repeat(200);
    let line = format!(
        r#"{{"type":"assistant","timestamp":"2026-11-23T10:00:00Z","message":{{"stop_reason":"tool_use","content":[{{"type":"text","text":"a\u001b[1mb\u0007c\n\n  \n{long}"}}]}}}}"#
    );
    let tails: Vec<String> = p
        .feed(line.as_bytes())
        .into_iter()
        .filter_map(|e| match e {
            Event::TranscriptTail { line, .. } => Some(line),
            _ => None,
        })
        .collect();
    assert_eq!(tails.len(), 2);
    assert_eq!(tails[0], "a b c");
    assert_eq!(tails[1].chars().count(), 120);
    assert!(tails[1].ends_with('…'));
}

#[test]
fn untested_version_flagged() {
    assert!(is_untested("9.9.9", CLAUDE_TESTED));
    assert!(!is_untested("2.1.331", CLAUDE_TESTED));
    assert!(is_untested("garbage", CLAUDE_TESTED));
    assert!(!is_untested("2.1.330", CLAUDE_TESTED));
    assert!(!is_untested("2.1", CLAUDE_TESTED));
    assert!(is_untested("2.1.331.1", CLAUDE_TESTED));
    assert!(is_untested("2.10.0", CLAUDE_TESTED));
    assert!(is_untested("2.1.331-beta", CLAUDE_TESTED));
    assert!(is_untested("", CLAUDE_TESTED));
    assert!(!is_untested("1.0.0", &["0.9.0", "1.0.0"]));
    assert!(is_untested("1.0.0", &[]));
}

#[test]
fn api_error_subtype_fails_train() {
    let (events, _) = parse_file(
        &project_dir().join("S3.jsonl"),
        SessionKey::claude("S3"),
        None,
    );
    assert_eq!(
        events.last(),
        Some(&Event::Failed {
            key: SessionKey::claude("S3"),
            reason: "api_error".into(),
            at: ts("2026-11-23T10:00:30Z"),
        })
    );
    assert_eq!(
        events[1],
        Event::FormatUntested {
            key: SessionKey::claude("S3"),
            version: "9.9.9".into(),
        }
    );
}

fn copy_tree(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for entry in fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let dest = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_tree(&entry.path(), &dest);
        } else {
            fs::copy(entry.path(), &dest).unwrap();
        }
    }
}

/// F25: checked-in mtimes are checkout times, so the test sets its own.
fn home_with_mtimes(now: SystemTime, ages: &[(&str, u64)]) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    copy_tree(&fixture_home(), dir.path());
    let proj = dir.path().join("projects/-Users-owner");
    for (rel, age) in ages {
        let f = fs::File::options()
            .write(true)
            .open(proj.join(rel))
            .unwrap();
        f.set_modified(now - Duration::from_secs(*age)).unwrap();
    }
    dir
}

fn summary(files: &[ClaudeFile]) -> Vec<(String, Option<String>, bool)> {
    files
        .iter()
        .map(|f| {
            (
                f.key.0.clone(),
                f.parent.as_ref().map(|p| p.0.clone()),
                f.meta.is_some(),
            )
        })
        .collect()
}

#[test]
fn discovery_finds_mains_then_subagents_within_window() {
    let now = SystemTime::UNIX_EPOCH + Duration::from_secs(1_800_000_000);
    let home = home_with_mtimes(
        now,
        &[
            ("S1.jsonl", 300),
            ("S2.jsonl", 100),
            ("S3.jsonl", 3 * 86_400),
            ("S1/subagents/agent-a1.jsonl", 200),
            ("S1/subagents/agent-a2.jsonl", 250),
        ],
    );
    let files = discover_claude(home.path(), Duration::from_secs(48 * 3600), now);
    assert_eq!(
        summary(&files),
        [
            ("claude:S1".to_string(), None, false),
            ("claude:S2".to_string(), None, false),
            (
                "claude:S1/a2".to_string(),
                Some("claude:S1".to_string()),
                true
            ),
            (
                "claude:S1/a1".to_string(),
                Some("claude:S1".to_string()),
                true
            ),
        ]
    );
    let proj = home.path().join("projects/-Users-owner");
    assert_eq!(files[0].path, proj.join("S1.jsonl"));
    assert_eq!(files[3].path, proj.join("S1/subagents/agent-a1.jsonl"));
    assert_eq!(
        files[3].meta,
        Some(proj.join("S1/subagents/agent-a1.meta.json"))
    );
}

#[test]
fn discovery_keeps_quiet_parent_of_active_subagent() {
    let now = SystemTime::UNIX_EPOCH + Duration::from_secs(1_800_000_000);
    let day = 86_400;
    let home = home_with_mtimes(
        now,
        &[
            ("S1.jsonl", 5 * day),
            ("S2.jsonl", 5 * day),
            ("S3.jsonl", 5 * day),
            ("S1/subagents/agent-a1.jsonl", 10),
            ("S1/subagents/agent-a2.jsonl", 5 * day),
        ],
    );
    fs::remove_file(
        home.path()
            .join("projects/-Users-owner/S1/subagents/agent-a1.meta.json"),
    )
    .unwrap();
    let files = discover_claude(home.path(), Duration::from_secs(2 * day), now);
    assert_eq!(
        summary(&files),
        [
            ("claude:S1".to_string(), None, false),
            (
                "claude:S1/a1".to_string(),
                Some("claude:S1".to_string()),
                false
            ),
        ]
    );
}

#[test]
fn discovery_of_missing_home_is_empty() {
    let dir = tempfile::tempdir().unwrap();
    assert!(
        discover_claude(
            &dir.path().join("nope"),
            Duration::from_secs(60),
            SystemTime::now()
        )
        .is_empty()
    );
}
