use pelhaim_core::{
    AlertId, LineKey, RuntimeRef, Snapshot, TrackId, Train, TrainState, format_train_number,
};
use serde::{Deserialize, Serialize};

pub const COMPACT_VERSION: u32 = 1;

/// What the phone may see (§8.7.1). Deliberately free of transcript-derived
/// strings: line names come from config, everything else is a number, a
/// state or a fixed word.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize, Debug)]
pub struct CompactBoard {
    pub version: u32,
    pub lines: Vec<CompactLine>,
    pub open_alerts: Vec<CompactAlert>,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize, Debug)]
pub struct CompactLine {
    pub name: String,
    pub trains: Vec<CompactTrain>,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize, Debug)]
pub struct CompactTrain {
    pub number: String,
    /// `working`, `idle`, `waiting`, `failed`, `done` or `stalled`.
    pub state: String,
    /// `station N`, counting from 1.
    pub station: String,
    /// `<tool> session` or `<tool> subagent`.
    pub label: String,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize, Debug)]
pub struct CompactAlert {
    pub id: AlertId,
    /// `AlertKind::tab()`.
    pub kind: String,
    pub train_number: Option<String>,
    pub line: String,
}

pub fn compact(s: &Snapshot) -> CompactBoard {
    let train_on = |track: TrackId| s.trains.values().find(|t| t.track == track);
    let lines = s
        .lines
        .iter()
        .map(|l| CompactLine {
            name: l.name.clone(),
            trains: l
                .tracks
                .iter()
                .filter_map(|&tr| train_on(tr))
                .map(compact_train)
                .collect(),
        })
        .collect();
    let open_alerts = s
        .alerts
        .iter()
        .filter(|a| a.is_open())
        .map(|a| {
            let train = a.train.and_then(|id| s.trains.get(&id));
            let line = train
                .and_then(|t| s.tracks.get(&t.track))
                .map_or(&a.line, |tr| &tr.line);
            CompactAlert {
                id: a.id.clone(),
                kind: a.kind.tab().to_string(),
                train_number: train.map(|t| format_train_number(t.number)),
                line: line_name(s, line),
            }
        })
        .collect();
    CompactBoard {
        version: COMPACT_VERSION,
        lines,
        open_alerts,
    }
}

fn compact_train(t: &Train) -> CompactTrain {
    // F47: ProbablyWaiting is drawn yellow on the board, so the phone says waiting too.
    let state = match t.state {
        TrainState::Idle => "idle",
        TrainState::Working => "working",
        TrainState::Waiting(_) | TrainState::ProbablyWaiting => "waiting",
        TrainState::Failed(_) => "failed",
        TrainState::Done => "done",
        TrainState::Stalled => "stalled",
    };
    let tool = match t.runtime {
        RuntimeRef::ObservedClaude { .. } => "claude",
        RuntimeRef::ObservedCodex { .. } => "codex",
    };
    let kind = if t.parent.is_some() {
        "subagent"
    } else {
        "session"
    };
    CompactTrain {
        number: format_train_number(t.number),
        state: state.to_string(),
        station: format!("station {}", u32::from(t.position.leg) + 1),
        label: format!("{tool} {kind}"),
    }
}

/// A project removed from config keeps its alert's slug, which also came from config.
fn line_name(s: &Snapshot, key: &LineKey) -> String {
    s.lines
        .iter()
        .find(|l| &l.key == key)
        .map(|l| l.name.clone())
        .unwrap_or_else(|| match key {
            LineKey::Dispatch => "DISPATCH".into(),
            LineKey::Project(slug) => slug.clone(),
            LineKey::Unsorted => "UNSORTED".into(),
        })
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use pelhaim_core::testkit::*;
    use pelhaim_core::{Engine, Event, SessionKey, Tool, parse_tag};

    use super::*;

    const SENTINELS: [&str; 5] = [
        "SENTINEL_PROMPT",
        "SENTINEL_DESC",
        "SENTINEL_Q",
        "SENTINEL_CWD",
        "SENTINEL_TAIL",
    ];

    fn t(hms: &str) -> pelhaim_core::Ts {
        ts(&format!("2026-11-23T{hms}Z"))
    }

    fn discover_with_cwd(e: &mut Engine, key: &SessionKey, parent: Option<&SessionKey>, at: &str) {
        e.apply(ev(Event::SessionDiscovered {
            key: key.clone(),
            tool: Tool::Claude,
            parent: parent.cloned(),
            path: PathBuf::from("/fixtures/x.jsonl"),
            cwd: Some("/SENTINEL_CWD".into()),
            repo: None,
            version: None,
            started: t(at),
        }));
    }

    /// Every transcript-derived field carries a sentinel: prompt title, spawn
    /// description, question text, cwd and tail.
    fn sentinel_snapshot() -> pelhaim_core::Snapshot {
        let mut e = engine(&["ethica"]);
        let s1 = SessionKey::claude("S1");
        discover_with_cwd(&mut e, &s1, None, "14:23:00");
        tag(&mut e, &s1, "pelhaim-project: ethica\nSENTINEL_PROMPT");
        prompt(&mut e, &s1, "SENTINEL_PROMPT", t("14:23:10"));
        milestone(
            &mut e,
            &s1,
            "pelhaim-milestone: SENTINEL_PROMPT ~5m",
            t("14:23:10"),
        );
        e.apply(ev(Event::TranscriptTail {
            key: s1.clone(),
            line: "SENTINEL_TAIL".into(),
        }));
        let a1 = SessionKey::claude_sub("S1", "a1");
        spawn(
            &mut e,
            &s1,
            "toolu_a",
            "SENTINEL_DESC",
            "SENTINEL_DESC",
            t("14:24:00"),
        );
        discover_with_cwd(&mut e, &a1, Some(&s1), "14:24:01");
        link(&mut e, &a1, &s1, "toolu_a");
        e.apply(ev(Event::SessionTag {
            key: a1.clone(),
            tag: parse_tag("SENTINEL_DESC"),
        }));
        tool_open(&mut e, &s1, "toolu_q", "AskUserQuestion", t("14:25:00"));
        ask(
            &mut e,
            &s1,
            "toolu_q",
            vec![question("SENTINEL_Q", "SENTINEL_Q?", &["SENTINEL_Q yes"])],
            t("14:25:00"),
        );
        tick(&mut e, t("14:25:30"));
        e.snapshot()
    }

    #[test]
    fn compact_board_has_no_transcript_text() {
        let snap = sentinel_snapshot();
        let full = serde_json::to_string(&snap).unwrap();
        for s in &SENTINELS[..4] {
            assert!(full.contains(s), "fixture never set {s}");
        }
        // `Train.tail` is `serde(skip)`, so it is checked on the struct.
        assert!(
            snap.trains
                .values()
                .any(|t| t.tail.iter().any(|l| l.contains("SENTINEL_TAIL")))
        );

        let board = compact(&snap);
        let json = serde_json::to_string(&board).unwrap();
        for s in SENTINELS {
            assert!(!json.contains(s), "{s} leaked: {json}");
        }
        assert_eq!(board.version, 1);
        assert!(board.lines.iter().flat_map(|l| &l.trains).count() > 0);
        assert!(!board.open_alerts.is_empty());
    }

    #[test]
    fn compact_maps_lines_trains_and_alerts() {
        let snap = sentinel_snapshot();
        let board = compact(&snap);
        let names: Vec<&str> = board.lines.iter().map(|l| l.name.as_str()).collect();
        assert_eq!(names, ["DISPATCH", "ETHICA", "UNSORTED"]);

        let ethica = &board.lines[1].trains;
        let parent = ethica
            .iter()
            .find(|t| t.label == "claude session")
            .expect("parent train");
        assert_eq!(parent.number, "1423");
        assert_eq!(parent.state, "waiting");
        assert_eq!(parent.station, "station 1");
        let child = board
            .lines
            .iter()
            .flat_map(|l| &l.trains)
            .find(|t| t.label == "claude subagent")
            .expect("subagent train");
        assert_eq!(child.state, "working");

        let q = board
            .open_alerts
            .iter()
            .find(|a| a.id.0 == "q:claude:S1:toolu_q")
            .expect("question alert");
        assert_eq!(q.kind, "Question");
        assert_eq!(q.train_number.as_deref(), Some("1423"));
        assert_eq!(q.line, "ETHICA");
    }

    #[test]
    fn compact_states_use_the_closed_vocabulary() {
        let snap = sample_board();
        let board = compact(&snap);
        let states: std::collections::BTreeSet<&str> = board
            .lines
            .iter()
            .flat_map(|l| &l.trains)
            .map(|t| t.state.as_str())
            .collect();
        let allowed = ["working", "idle", "waiting", "failed", "done", "stalled"];
        assert!(states.iter().all(|s| allowed.contains(s)), "{states:?}");
        assert!(states.contains("failed") && states.contains("waiting"));
    }

    #[test]
    fn answered_alerts_are_not_listed() {
        let mut e = engine(&["ethica"]);
        let k = SessionKey::claude("S1");
        discover_in(&mut e, &k, None, t("10:00:00"));
        tag(&mut e, &k, "pelhaim-project: ethica\nwork");
        prompt(&mut e, &k, "work", t("10:00:00"));
        ask(
            &mut e,
            &k,
            "toolu_q",
            vec![question("H", "Q?", &["a", "b"])],
            t("10:00:05"),
        );
        assert_eq!(compact(&e.snapshot()).open_alerts.len(), 1);
        answered(&mut e, &k, "toolu_q", "a", t("10:00:10"));
        assert!(compact(&e.snapshot()).open_alerts.is_empty());
    }
}
