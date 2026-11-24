mod common;

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

use common::{Running, append, live_home, opts, run_once, set_mtime};
use pelhaim_adapters::AdapterCommand;
use pelhaim_adapters::observe::{ObserveClaude, ObserveCodex, ObserveOptions};
use pelhaim_core::{Event, SessionKey};

fn project(home: &Path) -> PathBuf {
    home.join("projects/-Users-owner")
}

fn claude(home: &Path, f: impl FnOnce(ObserveOptions) -> ObserveOptions) -> Running {
    Running::start(Box::new(ObserveClaude::new(f(opts(home)))), 64)
}

fn is_discovered(e: &Event, key: &SessionKey) -> bool {
    matches!(e, Event::SessionDiscovered { key: k, .. } if k == key)
}

const ASK: &str = r#"{"type":"assistant","uuid":"a9","parentUuid":"u4","sessionId":"S1","timestamp":"2026-11-23T15:00:00.000Z","version":"2.1.331","message":{"role":"assistant","stop_reason":"tool_use","content":[{"type":"tool_use","id":"toolu_Q9","name":"AskUserQuestion","input":{"questions":[{"question":"Ship it?","header":"Ship","multiSelect":false,"options":[{"label":"Yes","description":"go"},{"label":"No","description":"wait"}]}]}}]}}"#;

#[tokio::test]
async fn ask_user_question_event_within_two_seconds() {
    let home = live_home("claude");
    let mut run = claude(home.path(), |o| o);
    run.caught_up().await;

    let started = Instant::now();
    append(&project(home.path()).join("S1.jsonl"), ASK);
    let got = run
        .wait_for(
            Duration::from_secs(3),
            |e| matches!(e, Event::QuestionAsked { call_id, .. } if call_id == "toolu_Q9"),
        )
        .await;
    let took = started.elapsed();
    assert!(
        took < Duration::from_secs(2),
        "QuestionAsked after {took:?}"
    );
    assert_eq!(got.session(), Some(&SessionKey::claude("S1")));
    run.shutdown().await;
}

#[tokio::test]
async fn new_subagent_file_starts_tailing() {
    let home = live_home("claude");
    let mut run = claude(home.path(), |o| ObserveOptions {
        sweep: Duration::from_millis(300),
        ..o
    });
    run.caught_up().await;

    let dir = project(home.path()).join("S1/subagents");
    fs::write(
        dir.join("agent-a3.meta.json"),
        r#"{"agentType":"general-purpose","description":"Write docs","toolUseId":"toolu_A3"}"#,
    )
    .unwrap();
    fs::write(
        dir.join("agent-a3.jsonl"),
        concat!(
            r#"{"type":"user","uuid":"d1","parentUuid":null,"sessionId":"S1","agentId":"a3","isSidechain":true,"timestamp":"2026-11-23T15:01:00.000Z","cwd":"/Users/owner","version":"2.1.331","message":{"role":"user","content":"Write docs"}}"#,
            "\n"
        ),
    )
    .unwrap();

    let child = SessionKey::claude_sub("S1", "a3");
    run.wait_for(Duration::from_secs(3), |e| is_discovered(e, &child))
        .await;
    let link = run
        .wait_for(
            Duration::from_secs(3),
            |e| matches!(e, Event::SubagentLinked { child: c, .. } if *c == child),
        )
        .await;
    assert_eq!(
        link,
        Event::SubagentLinked {
            child: child.clone(),
            parent: SessionKey::claude("S1"),
            tool_use_id: Some("toolu_A3".into()),
            description: Some("Write docs".into()),
        }
    );

    // The new file is tailed from then on, not only read once.
    append(
        &dir.join("agent-a3.jsonl"),
        r#"{"type":"assistant","uuid":"d2","parentUuid":"d1","sessionId":"S1","agentId":"a3","isSidechain":true,"timestamp":"2026-11-23T15:01:05.000Z","version":"2.1.331","message":{"role":"assistant","stop_reason":"tool_use","content":[{"type":"tool_use","id":"toolu_W1","name":"Write","input":{}}]}}"#,
    );
    run.wait_for(Duration::from_secs(2), |e| {
        matches!(e, Event::ToolOpened { key, call_id, .. } if *key == child && call_id == "toolu_W1")
    })
    .await;
    run.shutdown().await;
}

#[tokio::test]
async fn deleted_session_emits_gone() {
    let home = live_home("claude");
    let mut run = claude(home.path(), |o| o);
    run.caught_up().await;

    fs::remove_file(project(home.path()).join("S2.jsonl")).unwrap();
    run.wait_for(Duration::from_secs(2), |e| {
        *e == Event::SessionGone {
            key: SessionKey::claude("S2"),
        }
    })
    .await;
    run.shutdown().await;
}

#[tokio::test]
async fn session_leaving_the_window_emits_gone_on_refresh() {
    let home = live_home("claude");
    let mut run = claude(home.path(), |o| ObserveOptions {
        sweep: Duration::from_secs(3600),
        ..o
    });
    run.caught_up().await;

    let old = SystemTime::now() - Duration::from_secs(49 * 3600);
    set_mtime(&project(home.path()).join("S3.jsonl"), old);
    run.commands.send(AdapterCommand::Refresh).await.unwrap();
    run.wait_for(Duration::from_secs(2), |e| {
        *e == Event::SessionGone {
            key: SessionKey::claude("S3"),
        }
    })
    .await;
    run.shutdown().await;
}

#[tokio::test]
async fn refresh_discovers_a_new_main_session() {
    let home = live_home("claude");
    let mut run = claude(home.path(), |o| ObserveOptions {
        sweep: Duration::from_secs(3600),
        ..o
    });
    run.caught_up().await;

    let s2 = fs::read(project(home.path()).join("S2.jsonl")).unwrap();
    let s9 = String::from_utf8(s2).unwrap().replace("\"S2\"", "\"S9\"");
    fs::write(project(home.path()).join("S9.jsonl"), s9).unwrap();
    run.commands.send(AdapterCommand::Refresh).await.unwrap();
    let s9 = SessionKey::claude("S9");
    run.wait_for(Duration::from_secs(2), |e| is_discovered(e, &s9))
        .await;
    run.shutdown().await;
}

#[tokio::test]
async fn truncated_session_resets_then_replays() {
    let home = live_home("claude");
    let mut run = claude(home.path(), |o| o);
    run.caught_up().await;

    let path = project(home.path()).join("S2.jsonl");
    let text = fs::read_to_string(&path).unwrap();
    let first_two: String = text.lines().take(2).map(|l| format!("{l}\n")).collect();
    fs::write(&path, first_two).unwrap();

    let s2 = SessionKey::claude("S2");
    run.wait_for(Duration::from_secs(2), |e| {
        *e == Event::SessionReset { key: s2.clone() }
    })
    .await;
    run.wait_for(Duration::from_secs(2), |e| is_discovered(e, &s2))
        .await;
    run.shutdown().await;
}

#[cfg(unix)]
#[tokio::test]
async fn replaced_session_resets_then_replays() {
    let home = live_home("claude");
    let mut run = claude(home.path(), |o| o);
    run.caught_up().await;

    let path = project(home.path()).join("S2.jsonl");
    let tmp = home.path().join("S2.new");
    fs::copy(&path, &tmp).unwrap();
    append(&tmp, ASK.replace("\"S1\"", "\"S2\"").as_str());
    fs::rename(&tmp, &path).unwrap();

    let s2 = SessionKey::claude("S2");
    run.wait_for(Duration::from_secs(2), |e| {
        *e == Event::SessionReset { key: s2.clone() }
    })
    .await;
    run.wait_for(
        Duration::from_secs(2),
        |e| matches!(e, Event::QuestionAsked { key, .. } if *key == s2),
    )
    .await;
    run.shutdown().await;
}

#[tokio::test]
async fn meta_change_finishes_the_subagent() {
    let home = live_home("claude");
    let mut run = claude(home.path(), |o| o);
    run.caught_up().await;
    let a2 = SessionKey::claude_sub("S1", "a2");
    assert!(
        !run.seen
            .iter()
            .any(|e| matches!(e, Event::SubagentFinished { key, .. } if *key == a2)),
        "a2's meta has no finished flag yet"
    );

    fs::write(
        project(home.path()).join("S1/subagents/agent-a2.meta.json"),
        r#"{"agentType":"general-purpose","description":"Review the corpus","toolUseId":"toolu_A2","model":"m","spawnDepth":1,"finished":true}"#,
    )
    .unwrap();
    run.wait_for(
        Duration::from_secs(2),
        |e| matches!(e, Event::SubagentFinished { key, .. } if *key == a2),
    )
    .await;
    run.shutdown().await;
}

#[tokio::test]
async fn catch_up_reads_past_the_per_call_read_cap() {
    let home = live_home("claude");
    let path = project(home.path()).join("S5.jsonl");
    let mut text = String::from(concat!(
        r#"{"type":"user","uuid":"e0","parentUuid":null,"sessionId":"S5","timestamp":"2026-11-23T15:00:00.000Z","cwd":"/Users/owner","version":"2.1.331","message":{"role":"user","content":"Big"}}"#,
        "\n"
    ));
    let filler = "x".repeat(1000);
    while text.len() < 6 * 1024 * 1024 {
        text.push_str(&format!(
            r#"{{"type":"system","uuid":"f","sessionId":"S5","timestamp":"2026-11-23T15:00:01.000Z","subtype":"note","content":"{filler}"}}"#
        ));
        text.push('\n');
    }
    text.push_str(&ASK.replace("\"S1\"", "\"S5\""));
    text.push('\n');
    fs::write(&path, text).unwrap();

    let events = run_once(Box::new(ObserveClaude::new(ObserveOptions {
        once: true,
        ..opts(home.path())
    })))
    .await;
    let s5 = SessionKey::claude("S5");
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::QuestionAsked { key, .. } if *key == s5)),
        "the record after the 4 MiB cap was never read"
    );
}

/// Review Focus 5: a first run over dozens of sessions through a small channel drops nothing.
#[tokio::test]
async fn catch_up_over_many_sessions_drops_nothing() {
    let home = live_home("claude");
    let s2 = fs::read_to_string(project(home.path()).join("S2.jsonl")).unwrap();
    for i in 0..60 {
        let sid = format!("H{i:02}");
        fs::write(
            project(home.path()).join(format!("{sid}.jsonl")),
            s2.replace("\"S2\"", &format!("\"{sid}\"")),
        )
        .unwrap();
    }
    let (tx, mut rx) = tokio::sync::mpsc::channel(1);
    let (_commands, crx) = tokio::sync::mpsc::channel(1);
    let adapter: Box<dyn pelhaim_adapters::SessionAdapter> =
        Box::new(ObserveClaude::new(ObserveOptions {
            once: true,
            ..opts(home.path())
        }));
    let task = tokio::spawn(adapter.run(tx, crx));
    let mut discovered = std::collections::BTreeSet::new();
    let mut caught_up = false;
    while let Some(e) = rx.recv().await {
        match &e {
            Event::SessionDiscovered { key, .. } if key.session_id().starts_with('H') => {
                discovered.insert(key.clone());
            }
            Event::CatchUpDone { .. } => caught_up = true,
            _ => {}
        }
        tokio::task::yield_now().await;
    }
    task.await.unwrap().unwrap();
    assert_eq!(discovered.len(), 60);
    assert!(caught_up);
}

#[tokio::test]
async fn codex_file_without_session_meta_is_ignored() {
    let home = live_home("codex");
    let day = home.path().join("sessions/2026/11/23");
    fs::write(
        day.join("rollout-2026-11-23T12-00-00-junk.jsonl"),
        "{\"timestamp\":\"2026-11-23T12:00:00Z\",\"type\":\"event_msg\",\"payload\":{}}\nnot json\n",
    )
    .unwrap();
    let mut run = Running::start(Box::new(ObserveCodex::new(opts(home.path()))), 64);
    run.caught_up().await;
    let keys: Vec<String> = run
        .seen
        .iter()
        .filter_map(|e| match e {
            Event::SessionDiscovered { key, .. } => Some(key.session_id().to_string()),
            _ => None,
        })
        .collect();
    assert_eq!(keys, ["C1", "C2"]);
    append(&day.join("rollout-2026-11-23T12-00-00-junk.jsonl"), "{}");
    let later = run.collect_for(Duration::from_millis(1500)).await;
    assert!(later.is_empty(), "{later:?}");
    run.shutdown().await;
}
