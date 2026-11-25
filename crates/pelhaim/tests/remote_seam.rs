use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use pelhaim::engine_task::{EngineHandle, input_channel};
use pelhaim::keep_awake::{Holder, Mechanism};
use pelhaim::store_worker::StoreWorker;
use pelhaim_core::testkit::{cfg, question};
use pelhaim_core::{
    AlertEvent, AlertId, AlertState, Answer, AnswerOutcome, AnsweredBy, Choice, Command, Engine,
    Event, Input, LineKey, Origin, SessionKey, Snapshot, Tool, Ts, parse_tag,
};
use pelhaim_remote::{RemoteApi, RemoteError};
use pelhaim_store::Store;
use pelhaim_store::decision_log::LogProject;

const LOG: &str = "# Decision log\n\n## ETHICA\n\n### Open decisions (need you)\n\n- Pick a judge\n\n### Decided\n\n---\n\n## Standing rules\n";
const WAIT: Duration = Duration::from_secs(5);

/// A multi-thread runtime whose workers are background threads; the test
/// thread is outside it, as the UI thread will be, so it may call `RemoteApi`.
struct Rig {
    rt: Option<tokio::runtime::Runtime>,
    handle: Option<EngineHandle>,
    dir: tempfile::TempDir,
}

impl Rig {
    fn new() -> Rig {
        Rig::with_holder(Holder::new(None, std::process::id(), false))
    }

    fn with_holder(holder: Holder) -> Rig {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("DECISION-LOG.md"), LOG).unwrap();
        let mut rig = Rig {
            rt: None,
            handle: None,
            dir,
        };
        rig.start(holder);
        rig
    }

    fn db(&self) -> PathBuf {
        self.dir.path().join("data/pelhaim.db")
    }

    fn log(&self) -> PathBuf {
        self.dir.path().join("DECISION-LOG.md")
    }

    fn start(&mut self, holder: Holder) {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .unwrap();
        let store = Store::open(&self.db()).unwrap();
        let restored = store.load().unwrap();
        let (tx, rx) = input_channel();
        let worker = StoreWorker::spawn(store, self.log(), log_projects(), tx.clone());
        let engine = Engine::new(cfg(&["ethica", "cabn"]), restored);
        self.handle = Some(EngineHandle::spawn(
            rt.handle(),
            engine,
            worker,
            holder,
            (tx, rx),
        ));
        self.rt = Some(rt);
    }

    fn h(&self) -> &EngineHandle {
        self.handle.as_ref().unwrap()
    }

    /// Dropping the last handle ends the engine task, which drops the store
    /// worker and so flushes its queue.
    fn stop(&mut self) {
        drop(self.handle.take());
        drop(self.rt.take());
    }

    fn wait_for(&self, what: &str, ok: impl Fn(&Snapshot) -> bool) -> std::sync::Arc<Snapshot> {
        let deadline = Instant::now() + WAIT;
        loop {
            let s = self.h().snapshot();
            if ok(&s) {
                return s;
            }
            assert!(Instant::now() < deadline, "timed out waiting for {what}");
            std::thread::sleep(Duration::from_millis(5));
        }
    }
}

fn log_projects() -> Vec<LogProject> {
    vec![
        LogProject::new("ethica", "ETHICA", None),
        LogProject::new("cabn", "CABN", None),
    ]
}

/// Near the wall clock: the engine's 1 s Tick uses `Utc::now()`, and old
/// timestamps would stall every train at once.
fn now() -> Ts {
    chrono::Utc::now()
}

fn discovered(key: &SessionKey, at: Ts) -> Input {
    Input::Event(Event::SessionDiscovered {
        key: key.clone(),
        tool: Tool::Claude,
        parent: None,
        path: PathBuf::from(format!("/fixtures/{}.jsonl", key.0)),
        cwd: None,
        repo: None,
        version: None,
        started: at,
    })
}

fn tagged(key: &SessionKey, text: &str) -> Input {
    Input::Event(Event::SessionTag {
        key: key.clone(),
        tag: parse_tag(text),
    })
}

fn untagged_session(h: &EngineHandle, sid: &str) -> (SessionKey, AlertId) {
    let k = SessionKey::claude(sid);
    h.send(discovered(&k, now()));
    h.send(tagged(&k, "no tag here"));
    let id = AlertId(format!("assign:{}", k.0));
    (k, id)
}

/// A question asked in an observed terminal session on the ethica line.
fn observed_question(h: &EngineHandle, sid: &str, answer_in_terminal: bool) -> AlertId {
    let k = SessionKey::claude(sid);
    let t = now();
    h.send(discovered(&k, t));
    h.send(tagged(&k, "pelhaim-project: ethica\nwork"));
    h.send(Input::Event(Event::PromptStarted {
        key: k.clone(),
        title: "work".into(),
        at: t,
    }));
    h.send(Input::Event(Event::QuestionAsked {
        key: k.clone(),
        call_id: "toolu_q".into(),
        questions: vec![question("Corpus", "Which corpus?", &["synthetic", "real"])],
        at: t,
    }));
    if answer_in_terminal {
        h.send(Input::Event(Event::QuestionAnswered {
            key: k.clone(),
            call_id: "toolu_q".into(),
            answer: "synthetic".into(),
            at: t,
        }));
    }
    AlertId(format!("q:{}:toolu_q", k.0))
}

fn phone() -> Origin {
    Origin::Remote {
        device_id: "phone".into(),
    }
}

fn pick(id: &AlertId, i: usize, origin: Origin) -> Answer {
    Answer {
        alert_id: id.clone(),
        choice: Choice::Options(vec![i]),
        origin,
    }
}

fn line_of(s: &Snapshot, k: &SessionKey) -> LineKey {
    let t = s.trains.values().find(|t| &t.session == k).expect("train");
    s.tracks[&t.track].line.clone()
}

fn alert_state(s: &Snapshot, id: &AlertId) -> AlertState {
    s.alerts
        .iter()
        .find(|a| &a.id == id)
        .unwrap_or_else(|| panic!("no alert {}", id.0))
        .state
        .clone()
}

fn next_alert(rx: &mut tokio::sync::broadcast::Receiver<AlertEvent>) -> AlertEvent {
    let deadline = Instant::now() + WAIT;
    loop {
        match rx.try_recv() {
            Ok(ev) => return ev,
            Err(tokio::sync::broadcast::error::TryRecvError::Empty)
                if Instant::now() < deadline =>
            {
                std::thread::sleep(Duration::from_millis(5))
            }
            Err(e) => panic!("no alert event: {e:?}"),
        }
    }
}

#[test]
fn remote_answer_assigns_project() {
    let rig = Rig::new();
    let (k, id) = untagged_session(rig.h(), "U1");
    // No wait: inputs queued before a request are folded before it is served.
    let out = rig.h().answer(pick(&id, 0, phone())).unwrap();
    assert_eq!(out, AnswerOutcome::Accepted);

    let s = rig.h().snapshot();
    assert_eq!(line_of(&s, &k), LineKey::Project("ethica".into()));
    assert!(matches!(
        alert_state(&s, &id),
        AlertState::Answered { by: AnsweredBy::Remote { device_id }, .. } if device_id == "phone"
    ));
    let board = rig.h().compact_snapshot();
    let ethica = board.lines.iter().find(|l| l.name == "ETHICA").unwrap();
    assert_eq!(ethica.trains.len(), 1);
    assert!(board.open_alerts.is_empty());
}

#[test]
fn first_answer_wins_across_origins() {
    let rig = Rig::new();
    let (k, id) = untagged_session(rig.h(), "U1");
    assert_eq!(
        rig.h().answer(pick(&id, 1, Origin::Local)).unwrap(),
        AnswerOutcome::Accepted
    );
    let late = rig.h().answer(pick(&id, 0, phone())).unwrap();
    assert!(
        matches!(
            late,
            AnswerOutcome::AlreadyAnswered {
                by: AnsweredBy::Local,
                ..
            }
        ),
        "{late:?}"
    );
    assert_eq!(
        line_of(&rig.h().snapshot(), &k),
        LineKey::Project("cabn".into())
    );
}

#[test]
fn observed_question_refuses_remote_and_local() {
    let rig = Rig::new();
    let id = observed_question(rig.h(), "S1", false);
    for origin in [phone(), Origin::Local] {
        assert_eq!(
            rig.h().answer(pick(&id, 0, origin)).unwrap(),
            AnswerOutcome::NotAnswerable
        );
    }
    assert_eq!(alert_state(&rig.h().snapshot(), &id), AlertState::Open);
}

#[test]
fn unknown_alert_answer_is_an_outcome_not_an_error() {
    let rig = Rig::new();
    let missing = AlertId("assign:claude:nobody".into());
    assert_eq!(
        rig.h().answer(pick(&missing, 0, phone())).unwrap(),
        AnswerOutcome::UnknownAlert
    );
    assert_eq!(
        rig.h().dismiss(missing, phone()),
        Err(RemoteError::UnknownAlert)
    );
}

#[test]
fn alert_events_broadcast() {
    let rig = Rig::new();
    let mut rx = rig.h().subscribe_alerts();
    let (_, id) = untagged_session(rig.h(), "U1");
    assert!(matches!(next_alert(&mut rx), AlertEvent::Raised(a) if a.id == id));
    rig.h().answer(pick(&id, 0, phone())).unwrap();
    assert!(matches!(next_alert(&mut rx), AlertEvent::Answered(a) if a.id == id));
}

#[test]
fn dismiss_via_remote_moves_to_tray() {
    let mut rig = Rig::new();
    let mut rx = rig.h().subscribe_alerts();
    let (_, id) = untagged_session(rig.h(), "U1");
    rig.h().dismiss(id.clone(), phone()).unwrap();
    assert_eq!(alert_state(&rig.h().snapshot(), &id), AlertState::Tray);
    assert!(matches!(next_alert(&mut rx), AlertEvent::Raised(_)));
    assert!(matches!(next_alert(&mut rx), AlertEvent::Dismissed(d) if d == id));
    assert!(
        rig.h()
            .compact_snapshot()
            .open_alerts
            .iter()
            .any(|a| a.id == id)
    );

    rig.stop();
    let tray = Store::open(&rig.db()).unwrap().load().unwrap().tray;
    assert!(tray.contains(&id));
}

#[test]
fn owner_assignment_survives_restart() {
    let mut rig = Rig::new();
    let (k, id) = untagged_session(rig.h(), "U1");
    rig.h().answer(pick(&id, 0, phone())).unwrap();
    rig.stop();

    let restored = Store::open(&rig.db()).unwrap().load().unwrap();
    let mut e = Engine::new(cfg(&["ethica", "cabn"]), restored);
    let t = now();
    e.apply(discovered(&k, t));
    e.apply(tagged(&k, "no tag here"));
    let s = e.snapshot();
    assert_eq!(line_of(&s, &k), LineKey::Project("ethica".into()));
    assert!(s.alerts.iter().all(|a| a.id != id || !a.is_open()));

    rig.start(Holder::new(None, std::process::id(), false));
    rig.h().send(discovered(&k, t));
    rig.h().send(tagged(&k, "no tag here"));
    let s = rig.wait_for("restored train", |s| s.trains.len() == 1);
    assert_eq!(line_of(&s, &k), LineKey::Project("ethica".into()));
}

#[test]
fn log_decision_exports_to_markdown() {
    let mut rig = Rig::new();
    let id = observed_question(rig.h(), "S1", true);
    rig.h().send(Input::Command(Command::LogDecision {
        alert: id.clone(),
        project: None,
        at: now(),
    }));
    let s = rig.wait_for("exported decision", |s| {
        s.decisions_recent.first().is_some_and(|d| d.written_to_md)
    });
    assert_eq!(s.decisions_unexported, 0);

    let doc = std::fs::read_to_string(rig.log()).unwrap();
    let entry = doc.find("Corpus: synthetic").expect("entry written");
    assert!(doc.find("## ETHICA").unwrap() < entry);
    assert!(entry < doc.find("## Standing rules").unwrap());
    assert_eq!(doc.matches("Corpus: synthetic").count(), 1);

    rig.stop();
    let rows = Store::open(&rig.db()).unwrap().load().unwrap().decisions;
    assert_eq!(rows.len(), 1);
    assert!(rows[0].written_to_md);
    assert_eq!(rows[0].source_alert.as_ref(), Some(&id));
}

#[test]
fn open_decision_counts_reach_the_line() {
    let rig = Rig::new();
    let s = rig.wait_for("open counts", |s| {
        s.lines
            .iter()
            .any(|l| l.key == LineKey::Project("ethica".into()) && l.open_decisions == 1)
    });
    assert!(
        s.lines
            .iter()
            .all(|l| l.key == LineKey::Project("ethica".into()) || l.open_decisions == 0)
    );
}

fn working(h: &EngineHandle, sid: &str) {
    let k = SessionKey::claude(sid);
    let t = now();
    h.send(discovered(&k, t));
    h.send(tagged(&k, "pelhaim-project: ethica\nwork"));
    h.send(Input::Event(Event::PromptStarted {
        key: k,
        title: "work".into(),
        at: t,
    }));
}

#[test]
fn keep_awake_effect_reaches_holder() {
    let rig = Rig::new();
    working(rig.h(), "W1");
    let s = rig.wait_for("hold", |s| s.keep_awake.held);
    assert_eq!(s.keep_awake.mechanism, None);
    assert_eq!(s.keep_awake.count, 1);
}

/// F48: the mechanism shows from startup, and a failed hold clears it.
#[test]
fn keep_awake_failure_clears_the_mechanism() {
    let broken = Mechanism::Caffeinate(Path::new("/nonexistent/pelhaim/caffeinate").into());
    let rig = Rig::with_holder(Holder::new(Some(broken), std::process::id(), false));
    rig.wait_for("startup mechanism", |s| {
        s.keep_awake.mechanism.as_deref() == Some("caffeinate")
    });
    working(rig.h(), "W1");
    rig.wait_for("failed hold", |s| {
        s.keep_awake.held && s.keep_awake.mechanism.is_none()
    });
}

#[test]
fn stopped_engine_reports_engine_stopped() {
    let mut rig = Rig::new();
    let h = rig.h().clone();
    rig.stop();
    let id = AlertId("assign:claude:U1".into());
    assert_eq!(
        h.answer(pick(&id, 0, phone())),
        Err(RemoteError::EngineStopped)
    );
    assert_eq!(h.dismiss(id, phone()), Err(RemoteError::EngineStopped));
}

/// Plan gap l: the blocking calls must never run on a tokio worker.
#[test]
#[should_panic(expected = "Cannot block the current thread from within a runtime")]
fn answer_inside_a_runtime_panics() {
    let rig = Rig::new();
    let h = rig.h().clone();
    let id = AlertId("assign:claude:U1".into());
    rig.rt
        .as_ref()
        .unwrap()
        .block_on(async move { h.answer(pick(&id, 0, phone())) })
        .unwrap();
}

/// F44 and F45: adapter events await capacity, and `watch` wakes on each new version.
#[test]
fn send_event_and_watch() {
    let rig = Rig::new();
    let h = rig.h().clone();
    let mut w = h.watch();
    let k = SessionKey::claude("A1");
    let seen = rig.rt.as_ref().unwrap().block_on(async move {
        w.borrow_and_update();
        for i in 0..5000 {
            h.send_event(Input::Event(Event::Activity {
                key: k.clone(),
                at: now(),
            }))
            .await;
            if i == 0 {
                h.send_event(discovered(&k, now())).await;
            }
        }
        tokio::time::timeout(WAIT, w.changed())
            .await
            .unwrap()
            .unwrap();
        w.borrow().version
    });
    assert!(seen > 0);
    let s = rig.wait_for("all activity folded", |s| {
        s.trains
            .values()
            .any(|t| t.counters.last_seen > t.counters.started)
    });
    assert_eq!(s.trains.len(), 1);
}
