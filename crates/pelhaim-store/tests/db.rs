use chrono::{NaiveDate, TimeZone, Utc};
use pelhaim_core::{
    AlertId, AssignTarget, Choice, Decision, DecisionId, Origin, OwnerEvent, RestoredState,
    SessionKey, Ts, Who,
};
use pelhaim_store::Store;
use rusqlite::Connection;

fn t(secs: i64) -> Ts {
    Utc.timestamp_opt(1_795_000_000 + secs, 123_456_789)
        .unwrap()
}

fn decision(id: i64, alert: &str) -> Decision {
    Decision {
        id: DecisionId(id),
        project: "pelhaim".into(),
        date: NaiveDate::from_ymd_opt(2026, 11, 23).unwrap(),
        title: format!("Decision {id}"),
        why: "because".into(),
        who: Who::Owner,
        origin: Origin::Local,
        source_alert: Some(AlertId(alert.into())),
        written_to_md: false,
    }
}

fn open(dir: &tempfile::TempDir) -> Store {
    Store::open(&dir.path().join("nested/data/pelhaim.db")).unwrap()
}

#[test]
fn append_then_load_restores_assignments_tray_decisions() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = open(&dir);
    assert_eq!(s.load().unwrap(), RestoredState::default());

    let a = SessionKey::claude("S1");
    let b = SessionKey::claude("S2");
    s.append(
        &OwnerEvent::Assigned {
            session: a.clone(),
            target: AssignTarget::Dispatch,
        },
        t(1),
    )
    .unwrap();
    s.append(
        &OwnerEvent::Assigned {
            session: b.clone(),
            target: AssignTarget::Project("x".into()),
        },
        t(2),
    )
    .unwrap();
    // A reassignment replaces the earlier target and time.
    s.append(
        &OwnerEvent::Assigned {
            session: a.clone(),
            target: AssignTarget::Project("pelhaim".into()),
        },
        t(3),
    )
    .unwrap();
    s.append(
        &OwnerEvent::Dismissed {
            alert: AlertId("q:claude:S1:c1".into()),
        },
        t(4),
    )
    .unwrap();
    s.append(
        &OwnerEvent::Answered {
            alert: AlertId("q:claude:S1:c2".into()),
            choice: Choice::Options(vec![0]),
            origin: Origin::Remote {
                device_id: "phone".into(),
            },
        },
        t(5),
    )
    .unwrap();
    s.append(
        &OwnerEvent::DecisionLogged(decision(1, "q:claude:S1:c2")),
        t(6),
    )
    .unwrap();

    let r = s.load().unwrap();
    assert_eq!(
        r.assignments,
        vec![
            (a, AssignTarget::Project("pelhaim".into()), t(3)),
            (b, AssignTarget::Project("x".into()), t(2)),
        ]
    );
    assert_eq!(
        r.tray.into_iter().collect::<Vec<_>>(),
        vec![AlertId("q:claude:S1:c1".into())]
    );
    assert_eq!(r.decisions, vec![decision(1, "q:claude:S1:c2")]);
}

#[test]
fn reopen_removes_from_tray() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = open(&dir);
    let kept = AlertId("q:claude:S1:c1".into());
    let reopened = AlertId("assign:claude:S2".into());
    for alert in [&kept, &reopened] {
        s.append(
            &OwnerEvent::Dismissed {
                alert: alert.clone(),
            },
            t(1),
        )
        .unwrap();
    }
    s.append(
        &OwnerEvent::Reopened {
            alert: reopened.clone(),
        },
        t(2),
    )
    .unwrap();
    assert_eq!(
        s.load().unwrap().tray.into_iter().collect::<Vec<_>>(),
        vec![kept]
    );

    // Dismissing again after a reopen parks it again.
    s.append(
        &OwnerEvent::Dismissed {
            alert: reopened.clone(),
        },
        t(3),
    )
    .unwrap();
    assert!(s.load().unwrap().tray.contains(&reopened));
}

#[test]
fn decision_exported_flips_flag() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = open(&dir);
    s.append(&OwnerEvent::DecisionLogged(decision(1, "q:a")), t(1))
        .unwrap();
    s.append(&OwnerEvent::DecisionLogged(decision(2, "q:b")), t(2))
        .unwrap();
    s.append(&OwnerEvent::DecisionExported { id: DecisionId(1) }, t(3))
        .unwrap();

    let loaded = s.load().unwrap().decisions;
    assert_eq!(loaded.len(), 2);
    assert!(loaded[0].written_to_md);
    assert!(!loaded[1].written_to_md);

    let newest_first = s.decisions(10).unwrap();
    assert_eq!(
        newest_first.iter().map(|d| d.id).collect::<Vec<_>>(),
        vec![DecisionId(2), DecisionId(1)]
    );
    assert!(newest_first[1].written_to_md);
    assert_eq!(s.decisions(1).unwrap().len(), 1);
    assert_eq!(s.decisions(1).unwrap()[0].id, DecisionId(2));
}

#[test]
fn logging_the_same_alert_twice_keeps_one_decision() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = open(&dir);
    s.append(&OwnerEvent::DecisionLogged(decision(1, "q:a")), t(1))
        .unwrap();
    s.append(&OwnerEvent::DecisionLogged(decision(1, "q:a")), t(2))
        .unwrap();
    // After a restart the engine no longer knows the alert was logged and
    // may log it again under a fresh id.
    s.append(&OwnerEvent::DecisionLogged(decision(2, "q:a")), t(3))
        .unwrap();
    let mut unsourced = decision(3, "unused");
    unsourced.source_alert = None;
    let mut unsourced2 = decision(4, "unused");
    unsourced2.source_alert = None;
    s.append(&OwnerEvent::DecisionLogged(unsourced), t(4))
        .unwrap();
    s.append(&OwnerEvent::DecisionLogged(unsourced2), t(5))
        .unwrap();

    let ids: Vec<_> = s.load().unwrap().decisions.iter().map(|d| d.id).collect();
    assert_eq!(ids, vec![DecisionId(1), DecisionId(3), DecisionId(4)]);
    let before = s.load().unwrap();
    s.rebuild_projections().unwrap();
    assert_eq!(s.load().unwrap(), before);
}

#[test]
fn event_replay_equals_projections() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = open(&dir);
    let mut next_decision = 1;
    for i in 0..50i64 {
        let session = SessionKey::claude(&format!("S{}", i % 7));
        let alert = AlertId(format!("q:claude:S{}:c{}", i % 5, i % 3));
        let ev = match i % 6 {
            0 => OwnerEvent::Assigned {
                session,
                target: if i % 4 == 0 {
                    AssignTarget::Dispatch
                } else {
                    AssignTarget::Project(format!("p{}", i % 3))
                },
            },
            1 => OwnerEvent::Dismissed { alert },
            2 => OwnerEvent::Reopened { alert },
            3 => OwnerEvent::Answered {
                alert,
                choice: Choice::Text(format!("answer {i}")),
                origin: Origin::Local,
            },
            4 => {
                let d = decision(next_decision, &format!("q:{i}"));
                next_decision += 1;
                OwnerEvent::DecisionLogged(d)
            }
            _ => OwnerEvent::DecisionExported {
                id: DecisionId(i % 4 + 1),
            },
        };
        s.append(&ev, t(i)).unwrap();
    }
    let a = s.load().unwrap();
    assert!(!a.assignments.is_empty());
    assert!(!a.tray.is_empty());
    assert!(a.decisions.iter().any(|d| d.written_to_md));
    assert!(a.decisions.iter().any(|d| !d.written_to_md));

    s.rebuild_projections().unwrap();
    assert_eq!(s.load().unwrap(), a);

    let rows: i64 = Connection::open(dir.path().join("nested/data/pelhaim.db"))
        .unwrap()
        .query_row(
            "SELECT count(*) FROM events WHERE source = 'owner'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(rows, 50);
}

#[test]
fn reopening_db_keeps_data_and_schema_version() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("nested/data/pelhaim.db");
    {
        let mut s = Store::open(&path).unwrap();
        s.append(
            &OwnerEvent::Assigned {
                session: SessionKey::claude("S1"),
                target: AssignTarget::Dispatch,
            },
            t(1),
        )
        .unwrap();
        s.append(
            &OwnerEvent::Dismissed {
                alert: AlertId("q:x".into()),
            },
            t(2),
        )
        .unwrap();
        s.append(&OwnerEvent::DecisionLogged(decision(1, "q:x")), t(3))
            .unwrap();
    }
    let before = Store::open(&path).unwrap().load().unwrap();
    let s = Store::open(&path).unwrap();
    assert_eq!(s.load().unwrap(), before);
    assert_eq!(before.assignments.len(), 1);
    assert_eq!(before.tray.len(), 1);
    assert_eq!(before.decisions.len(), 1);
    drop(s);

    let conn = Connection::open(&path).unwrap();
    let version: i64 = conn
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .unwrap();
    assert_eq!(version, 1);
    let kinds: Vec<String> = conn
        .prepare("SELECT kind FROM events ORDER BY id")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(kinds, vec!["Assigned", "Dismissed", "DecisionLogged"]);
}

#[test]
fn newer_schema_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("pelhaim.db");
    Connection::open(&path)
        .unwrap()
        .execute_batch("PRAGMA user_version = 99")
        .unwrap();
    assert!(Store::open(&path).is_err());
}

// A concurrent `pelhaim decisions` or `--headless` reader must not make the
// board's writes fail with SQLITE_BUSY.
#[test]
fn append_succeeds_while_another_connection_reads_or_briefly_writes() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("nested/data/pelhaim.db");
    let mut s = Store::open(&path).unwrap();
    let dismissed = |i: u32| OwnerEvent::Dismissed {
        alert: AlertId(format!("a{i}")),
    };

    let reader = Connection::open(&path).unwrap();
    reader.execute_batch("BEGIN").unwrap();
    let _: i64 = reader
        .query_row("SELECT count(*) FROM events", [], |r| r.get(0))
        .unwrap();
    s.append(&dismissed(1), t(1)).unwrap();
    reader.execute_batch("COMMIT").unwrap();

    let writer = Connection::open(&path).unwrap();
    writer.execute_batch("BEGIN IMMEDIATE").unwrap();
    let release = std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(200));
        writer.execute_batch("COMMIT").unwrap();
    });
    s.append(&dismissed(2), t(2)).unwrap();
    release.join().unwrap();
    assert_eq!(s.load().unwrap().tray.len(), 2);
}
