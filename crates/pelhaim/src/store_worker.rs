use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::mpsc as std_mpsc;
use std::thread::JoinHandle;

use pelhaim_core::{Decision, Event, Input, OwnerEvent, Ts};
use pelhaim_store::Store;
use pelhaim_store::decision_log::{LogProject, open_decisions, write_decision};
use tokio::sync::mpsc;

pub enum StoreJob {
    Persist(OwnerEvent, Ts),
    Export(Decision),
    RefreshOpenCounts,
}

/// SQLite and the decision log are blocking IO, so they run on their own
/// thread rather than in the engine task. Jobs run in the order sent.
pub struct StoreWorker {
    jobs: Option<std_mpsc::Sender<StoreJob>>,
    thread: Option<JoinHandle<()>>,
}

impl StoreWorker {
    /// F43: `back` is a Sender of the engine's input channel, which the caller
    /// creates before spawning either side.
    pub fn spawn(
        store: Store,
        log_path: PathBuf,
        projects: Vec<LogProject>,
        back: mpsc::Sender<Input>,
    ) -> StoreWorker {
        let (tx, rx) = std_mpsc::channel();
        let mut w = Worker {
            store,
            log_path,
            projects,
            back,
        };
        let thread = std::thread::Builder::new()
            .name("pelhaim-store".into())
            .spawn(move || {
                for job in rx {
                    w.run(job);
                }
            })
            .expect("spawn the store thread");
        StoreWorker {
            jobs: Some(tx),
            thread: Some(thread),
        }
    }

    pub fn send(&self, job: StoreJob) {
        if let Some(tx) = &self.jobs
            && tx.send(job).is_err()
        {
            tracing::error!("store worker has stopped; job dropped");
        }
    }
}

/// Finishes every queued job before returning, so owner events sent before
/// shutdown reach the database.
impl Drop for StoreWorker {
    fn drop(&mut self) {
        drop(self.jobs.take());
        if let Some(t) = self.thread.take()
            && t.join().is_err()
        {
            tracing::error!("store worker panicked");
        }
    }
}

struct Worker {
    store: Store,
    log_path: PathBuf,
    projects: Vec<LogProject>,
    back: mpsc::Sender<Input>,
}

impl Worker {
    fn run(&mut self, job: StoreJob) {
        match job {
            StoreJob::Persist(ev, at) => {
                if let Err(e) = self.store.append(&ev, at) {
                    tracing::error!(kind = owner_kind(&ev), error = %e, "persisting owner event failed");
                    self.reply(Event::StoreWriteFailed {
                        kind: owner_kind(&ev).to_string(),
                        reason: e.to_string(),
                    });
                }
            }
            StoreJob::Export(d) => {
                let event = self.export(&d);
                self.reply(event);
            }
            StoreJob::RefreshOpenCounts => {
                if let Some(counts) = self.open_counts() {
                    self.reply(Event::OpenDecisionCounts { counts });
                }
            }
        }
    }

    /// Writes the file before the engine records `DecisionExported`. A crash in
    /// between leaves the row unexported; the replayed export finds the entry's
    /// marker and writes nothing, so the log never gets a duplicate.
    fn export(&self, d: &Decision) -> Event {
        // An auto-created line (§7.7) has no `[[project]]`: its display name is
        // the section key, matched and created by the same rules as `name`.
        let fallback;
        let p = match self.projects.iter().find(|p| p.slug == d.project) {
            Some(p) => p,
            None => match &d.project_name {
                Some(name) => {
                    fallback = LogProject::new(&d.project, name, None);
                    &fallback
                }
                None => {
                    return Event::DecisionExportFailed {
                        id: d.id,
                        reason: format!("no configured project {}", d.project),
                    };
                }
            },
        };
        match write_decision(&self.log_path, p, d) {
            Ok(()) => Event::DecisionExported {
                id: d.id,
                at: chrono::Utc::now(),
            },
            Err(e) => {
                tracing::warn!(decision = d.id.0, error = %e, "decision export failed");
                Event::DecisionExportFailed {
                    id: d.id,
                    reason: e.to_string(),
                }
            }
        }
    }

    /// `None` keeps the engine's last counts: only a missing file means "no open decisions".
    fn open_counts(&self) -> Option<BTreeMap<String, u32>> {
        match std::fs::read_to_string(&self.log_path) {
            Ok(doc) => Some(
                open_decisions(&doc, &self.projects)
                    .into_iter()
                    .map(|(slug, items)| (slug, items.len() as u32))
                    .collect(),
            ),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Some(BTreeMap::new()),
            Err(e) => {
                tracing::warn!(error = %e, "reading the decision log failed");
                None
            }
        }
    }

    fn reply(&self, ev: Event) {
        if self.back.blocking_send(Input::Event(ev)).is_err() {
            tracing::debug!("engine stopped; store reply dropped");
        }
    }
}

fn owner_kind(ev: &OwnerEvent) -> &'static str {
    match ev {
        OwnerEvent::Assigned { .. } => "Assigned",
        OwnerEvent::Dismissed { .. } => "Dismissed",
        OwnerEvent::Reopened { .. } => "Reopened",
        OwnerEvent::Answered { .. } => "Answered",
        OwnerEvent::DecisionLogged(_) => "DecisionLogged",
        OwnerEvent::DecisionExported { .. } => "DecisionExported",
        OwnerEvent::StationTimed(_) => "StationTimed",
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use pelhaim_core::{AlertId, DecisionId, Origin, Who};

    use super::*;

    const LOG: &str = "# Decision log\n\n## ETHICA\n\n### Open decisions (need you)\n\n- Pick a corpus\n- Pick a judge\n\n### Decided\n\n---\n\n## Standing rules\n";

    struct Fixture {
        _dir: tempfile::TempDir,
        db: PathBuf,
        log: PathBuf,
    }

    fn fixture(log: Option<&str>) -> Fixture {
        let dir = tempfile::tempdir().unwrap();
        let log_path = dir.path().join("DECISION-LOG.md");
        if let Some(doc) = log {
            std::fs::write(&log_path, doc).unwrap();
        }
        Fixture {
            db: dir.path().join("data/pelhaim.db"),
            log: log_path,
            _dir: dir,
        }
    }

    fn worker(f: &Fixture) -> (StoreWorker, mpsc::Receiver<Input>) {
        let (tx, rx) = mpsc::channel(16);
        let w = StoreWorker::spawn(
            Store::open(&f.db).unwrap(),
            f.log.clone(),
            vec![LogProject::new("ethica", "ETHICA", None)],
            tx,
        );
        (w, rx)
    }

    fn recv(rx: &mut mpsc::Receiver<Input>) -> Event {
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        loop {
            match rx.try_recv() {
                Ok(Input::Event(e)) => return e,
                Ok(other) => panic!("unexpected {other:?}"),
                Err(_) if std::time::Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(5))
                }
                Err(e) => panic!("no reply: {e:?}"),
            }
        }
    }

    fn decision(project: &str) -> Decision {
        Decision {
            id: DecisionId(1),
            project: project.into(),
            date: chrono::NaiveDate::from_ymd_opt(2026, 11, 23).unwrap(),
            title: "Corpus: synthetic".into(),
            why: "Which corpus?".into(),
            who: Who::Owner,
            origin: Origin::Local,
            source_alert: Some(AlertId("q:claude:S1:toolu_q".into())),
            written_to_md: false,
            project_name: None,
        }
    }

    #[test]
    fn export_crash_replay_writes_the_entry_once() {
        let f = fixture(Some(LOG));
        let d = decision("ethica");
        {
            let (w, mut rx) = worker(&f);
            w.send(StoreJob::Persist(
                OwnerEvent::DecisionLogged(d.clone()),
                chrono::Utc::now(),
            ));
            w.send(StoreJob::Export(d.clone()));
            assert!(matches!(recv(&mut rx), Event::DecisionExported { id, .. } if id == d.id));
            // The crash: `DecisionExported` never comes back as a Persist job.
        }
        let restored = Store::open(&f.db).unwrap().load().unwrap();
        assert_eq!(restored.decisions.len(), 1);
        assert!(!restored.decisions[0].written_to_md);

        let (w, mut rx) = worker(&f);
        w.send(StoreJob::Export(restored.decisions[0].clone()));
        assert!(matches!(recv(&mut rx), Event::DecisionExported { id, .. } if id == d.id));
        let doc = std::fs::read_to_string(&f.log).unwrap();
        assert_eq!(doc.matches("Corpus: synthetic").count(), 1, "{doc}");
        assert_eq!(doc.matches("pelhaim:decision:1:").count(), 1);
    }

    #[test]
    fn persist_failure_raises_an_event_and_a_later_relog_is_not_duplicated() {
        let f = fixture(Some(LOG));
        let d = decision("ethica");
        {
            let (w, mut rx) = worker(&f);
            rusqlite::Connection::open(&f.db)
                .unwrap()
                .execute_batch("DROP TABLE events")
                .unwrap();
            w.send(StoreJob::Persist(
                OwnerEvent::DecisionLogged(d.clone()),
                chrono::Utc::now(),
            ));
            assert!(matches!(
                recv(&mut rx),
                Event::StoreWriteFailed { kind, .. } if kind == "DecisionLogged"
            ));
            w.send(StoreJob::Export(d.clone()));
            assert!(matches!(recv(&mut rx), Event::DecisionExported { .. }));
        }
        // After a restart nothing records the logged alert, so the owner logs it
        // again on a later day and the id restarts at 1.
        let again = Decision {
            date: chrono::NaiveDate::from_ymd_opt(2026, 11, 26).unwrap(),
            ..d
        };
        let (w, mut rx) = worker(&Fixture {
            db: f.db.with_file_name("fresh.db"),
            log: f.log.clone(),
            _dir: tempfile::tempdir().unwrap(),
        });
        w.send(StoreJob::Export(again));
        assert!(matches!(recv(&mut rx), Event::DecisionExported { .. }));
        let doc = std::fs::read_to_string(&f.log).unwrap();
        assert_eq!(doc.matches("Corpus: synthetic").count(), 1, "{doc}");
    }

    #[test]
    fn export_failures_come_back_as_events() {
        let f = fixture(None);
        let (w, mut rx) = worker(&f);
        w.send(StoreJob::Export(decision("ethica")));
        assert!(
            matches!(recv(&mut rx), Event::DecisionExportFailed { reason, .. } if reason.contains("not found"))
        );
        w.send(StoreJob::Export(decision("nowhere")));
        assert!(
            matches!(recv(&mut rx), Event::DecisionExportFailed { reason, .. } if reason.contains("nowhere"))
        );
    }

    const AUTO_LOG: &str = "# Decision log\n\n## How to use this log\n\nRead me.\n\n## SafeAscent\n\n### Open decisions (need you)\n\n### Decided\n\n- older\n\n---\n\n## cabn and the portfolio\n\n### Decided\n\n---\n\n## Standing rules\n\n- rule\n";

    fn auto(project: &str, name: &str) -> Decision {
        Decision {
            project_name: Some(name.into()),
            ..decision(project)
        }
    }

    /// The entry sits under `heading`'s `### Decided` and every other byte is kept.
    fn assert_inserted_under(before: &str, after: &str, d: &Decision, heading: &str) {
        let entry = pelhaim_store::decision_log::render_entry(d, "\n");
        let at = after
            .find(&entry)
            .unwrap_or_else(|| panic!("{after}\n---\n{entry}"));
        assert_eq!(after.matches(&entry).count(), 1);
        assert_eq!(after.replacen(&entry, "", 1), before);
        let section = after[..at].rfind("\n## ").unwrap();
        assert!(after[section + 1..].starts_with(heading), "{after}");
    }

    #[test]
    fn auto_line_decisions_go_under_the_line_name() {
        let f = fixture(Some(AUTO_LOG));
        let (w, mut rx) = worker(&f);
        let sa = auto("safeascent", "SafeAscent");
        w.send(StoreJob::Export(sa.clone()));
        assert!(matches!(recv(&mut rx), Event::DecisionExported { .. }));
        let after_sa = std::fs::read_to_string(&f.log).unwrap();
        assert_inserted_under(AUTO_LOG, &after_sa, &sa, "## SafeAscent\n");

        // The space-suffix rule: "cabn" matches "## cabn and the portfolio".
        let cabn = Decision {
            id: DecisionId(2),
            title: "Tiles: pixel".into(),
            ..auto("cabn", "cabn")
        };
        w.send(StoreJob::Export(cabn.clone()));
        assert!(matches!(recv(&mut rx), Event::DecisionExported { .. }));
        let after = std::fs::read_to_string(&f.log).unwrap();
        assert_inserted_under(&after_sa, &after, &cabn, "## cabn and the portfolio\n");

        // Idempotent: the replayed export finds its marker and writes nothing.
        w.send(StoreJob::Export(cabn));
        assert!(matches!(recv(&mut rx), Event::DecisionExported { .. }));
        assert_eq!(std::fs::read_to_string(&f.log).unwrap(), after);
    }

    #[test]
    fn auto_line_names_never_match_the_log_scaffolding() {
        for name in ["How", "Standing"] {
            let f = fixture(Some(AUTO_LOG));
            let (w, mut rx) = worker(&f);
            let d = auto(&name.to_lowercase(), name);
            w.send(StoreJob::Export(d.clone()));
            assert!(matches!(recv(&mut rx), Event::DecisionExported { .. }));
            let after = std::fs::read_to_string(&f.log).unwrap();
            let new = format!("## {name}\n\n### Open decisions (need you)\n\n### Decided\n\n");
            let at = after.find(&new).unwrap_or_else(|| panic!("{after}"));
            assert!(at < after.find("## Standing rules").unwrap());
            assert!(after.contains("## How to use this log\n\nRead me.\n"));
            assert!(after.contains("## Standing rules\n\n- rule\n"));
        }
    }

    #[test]
    fn auto_line_export_never_creates_the_log() {
        let f = fixture(None);
        let (w, mut rx) = worker(&f);
        w.send(StoreJob::Export(auto("safeascent", "SafeAscent")));
        assert!(
            matches!(recv(&mut rx), Event::DecisionExportFailed { reason, .. } if reason.contains("not found"))
        );
        assert!(!f.log.exists());
    }

    #[test]
    fn open_counts_read_the_log_and_a_missing_file_is_empty() {
        let f = fixture(Some(LOG));
        let (w, mut rx) = worker(&f);
        w.send(StoreJob::RefreshOpenCounts);
        let Event::OpenDecisionCounts { counts } = recv(&mut rx) else {
            panic!("expected counts")
        };
        assert_eq!(counts, BTreeMap::from([("ethica".to_string(), 2)]));

        std::fs::remove_file(&f.log).unwrap();
        w.send(StoreJob::RefreshOpenCounts);
        let Event::OpenDecisionCounts { counts } = recv(&mut rx) else {
            panic!("expected counts")
        };
        assert!(counts.is_empty());
    }

    #[test]
    fn drop_flushes_queued_persists() {
        let f = fixture(None);
        let (w, _rx) = worker(&f);
        for i in 0..50 {
            w.send(StoreJob::Persist(
                OwnerEvent::Dismissed {
                    alert: AlertId(format!("a{i}")),
                },
                chrono::Utc::now(),
            ));
        }
        drop(w);
        assert_eq!(Store::open(&f.db).unwrap().load().unwrap().tray.len(), 50);
    }
}
