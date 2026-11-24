use std::time::Duration;

use super::{AlertEvent, Effect, Engine};
use crate::event::{Question, Ts};
use crate::ids::{AlertId, SessionKey, TrainId};
use crate::model::{
    Alert, AlertKind, AlertState, AnsweredBy, AssignOption, AssignTarget, LineKey, TagResolution,
    TrainState,
};
use crate::tag::closest_slug;
use crate::text::truncate_chars;

/// Calls that legitimately stay open with no writes: a question is its own
/// alert, and Agent/Task wait on a subagent (Review Focus 1).
const PERMISSION_EXEMPT: [&str; 3] = ["AskUserQuestion", "Agent", "Task"];

const RESULT_CHARS: usize = 600;

/// Whether a subagent keeps its parent green and counts as its activity.
pub(super) fn child_active(s: &TrainState) -> bool {
    !matches!(
        s,
        TrainState::Done | TrainState::Stalled | TrainState::Failed(_)
    )
}

fn cutoff(now: Ts, after: Option<Duration>) -> Option<Ts> {
    let d = chrono::TimeDelta::from_std(after?).ok()?;
    now.checked_sub_signed(d)
}

impl Engine {
    fn new_alert(
        &self,
        id: AlertId,
        train: TrainId,
        kind: AlertKind,
        created: Ts,
        answerable: bool,
    ) -> Alert {
        let t = &self.trains[&train];
        Alert {
            id,
            train: Some(train),
            line: self.tracks[&t.track].line.clone(),
            kind,
            created,
            state: AlertState::Open,
            answerable,
            decision: None,
        }
    }

    /// Re-deriving an alert that already exists is a no-op, so replays never
    /// duplicate it; one the owner had dismissed comes back in the tray.
    pub(super) fn raise(&mut self, mut a: Alert) {
        if self.alerts.iter().any(|x| x.id == a.id) {
            return;
        }
        if self.tray.contains(&a.id) {
            a.state = AlertState::Tray;
        }
        self.alerts.push(a.clone());
        self.effects.push(Effect::Alert(AlertEvent::Raised(a)));
    }

    pub(super) fn expire_where(&mut self, doomed: impl Fn(&Alert) -> bool) {
        let (gone, kept) = std::mem::take(&mut self.alerts)
            .into_iter()
            .partition(|a| doomed(a));
        self.alerts = kept;
        for mut a in gone {
            a.state = AlertState::Expired;
            self.effects.push(Effect::Alert(AlertEvent::Updated(a)));
        }
    }

    pub(super) fn expire_alerts_of(&mut self, id: TrainId) {
        self.expire_where(|a| a.train == Some(id));
    }

    pub(super) fn question_asked(
        &mut self,
        id: TrainId,
        key: &SessionKey,
        call_id: &str,
        questions: Vec<Question>,
        at: Ts,
    ) {
        let aid = AlertId(format!("q:{}:{call_id}", key.0));
        if self.alerts.iter().any(|a| a.id == aid && !a.is_open()) {
            return;
        }
        let alert = self.new_alert(
            aid.clone(),
            id,
            AlertKind::Question { questions },
            at,
            false,
        );
        self.raise(alert);
        self.train_mut(id).state = TrainState::Waiting(aid);
    }

    pub(super) fn question_answered(
        &mut self,
        id: TrainId,
        key: &SessionKey,
        call_id: &str,
        answer: String,
        at: Ts,
    ) {
        let aid = AlertId(format!("q:{}:{call_id}", key.0));
        if let Some(a) = self.alerts.iter_mut().find(|a| a.id == aid && a.is_open()) {
            a.state = AlertState::Answered {
                answer,
                by: AnsweredBy::Terminal,
                at,
            };
            let a = a.clone();
            self.effects.push(Effect::Alert(AlertEvent::Answered(a)));
        }
        let t = self.train_mut(id);
        if t.state == TrainState::Waiting(aid) {
            t.state = TrainState::Working;
        }
    }

    pub(super) fn raise_result(&mut self, id: TrainId, key: &SessionKey, text: &str, at: Ts) {
        if !self.cfg.result_alerts {
            return;
        }
        let turns = self.trains[&id].counters.turns;
        let kind = AlertKind::Result {
            summary: truncate_chars(text, RESULT_CHARS),
        };
        let alert = self.new_alert(
            AlertId(format!("result:{}:{turns}", key.0)),
            id,
            kind,
            at,
            false,
        );
        self.raise(alert);
    }

    /// Keeps `assign:<key>` in step with the session's resolution: present
    /// while it sits on Unsorted untagged or mis-tagged, expired otherwise.
    /// An answered one is left alone. Until the tag has been seen the session
    /// only looks untagged, so it gets no alert yet.
    pub(super) fn sync_assign_alert(&mut self, key: &SessionKey) {
        let id = self.by_key[key];
        let t = &self.trains[&id];
        let aid = AlertId(format!("assign:{}", key.0));
        let unsorted = self.tracks[&t.track].line == LineKey::Unsorted && self.tag_state_known(key);
        let raw = match &t.tag {
            TagResolution::Untagged if unsorted => Some(None),
            TagResolution::MisTagged(raw) if unsorted => Some(Some(raw.clone())),
            _ => None,
        };
        let Some(raw) = raw else {
            self.expire_where(|a| a.id == aid && a.is_open());
            return;
        };
        let created = t.counters.last_seen;
        let kind = self.assign_kind(key, raw.as_deref());
        match self.alerts.iter_mut().find(|a| a.id == aid) {
            Some(a) if a.is_open() && a.kind != kind => {
                a.kind = kind;
                let a = a.clone();
                self.effects.push(Effect::Alert(AlertEvent::Updated(a)));
            }
            Some(_) => {}
            None => {
                let alert = self.new_alert(aid, id, kind, created, true);
                self.raise(alert);
            }
        }
    }

    fn assign_kind(&self, key: &SessionKey, mis_tag: Option<&str>) -> AlertKind {
        let projects = self.projects_in_order();
        let slugs: Vec<String> = projects.iter().map(|p| p.slug.clone()).collect();
        let mut options: Vec<AssignOption> = projects
            .iter()
            .map(|p| AssignOption {
                label: p.name.clone(),
                target: AssignTarget::Project(p.slug.clone()),
            })
            .collect();
        options.push(AssignOption {
            label: "This is the orchestrator".into(),
            target: AssignTarget::Dispatch,
        });
        AlertKind::AssignProject {
            session: key.clone(),
            options,
            prefill: mis_tag.and_then(|raw| closest_slug(raw, &slugs)),
        }
    }

    /// The §8.1 permission and stall heuristics, run on every Tick.
    pub(super) fn observe_heuristics(&mut self, now: Ts) -> bool {
        let stall_cut = cutoff(now, self.cfg.stall_after);
        let permission_cut = cutoff(now, self.cfg.permission_after);
        let ids: Vec<TrainId> = self.trains.keys().copied().collect();
        let mut changed = false;
        for &id in &ids {
            let t = &self.trains[&id];
            // A berthed Working train has finished its own turn and only waits on
            // its subagents, which carry their own heuristics.
            if t.position.berthed
                || !matches!(t.state, TrainState::Working | TrainState::ProbablyWaiting)
            {
                continue;
            }
            let last = self.effective_last_write(id).expect("train");
            if stall_cut.is_some_and(|c| last <= c) {
                self.stall(id, now);
                changed = true;
                continue;
            }
            let waiting = permission_cut.is_some_and(|c| {
                last <= c
                    && self.open_calls.get(&t.session).is_some_and(|calls| {
                        calls.values().any(|(name, at)| {
                            !PERMISSION_EXEMPT.contains(&name.as_str()) && *at <= c
                        })
                    })
            });
            let next = if waiting {
                TrainState::ProbablyWaiting
            } else {
                TrainState::Working
            };
            if t.state != next {
                self.train_mut(id).state = next;
                changed = true;
            }
        }
        for id in ids {
            changed |= self.settle_after_turn(id);
        }
        changed
    }

    fn stall(&mut self, id: TrainId, now: Ts) {
        let t = self.train_mut(id);
        t.state = TrainState::Stalled;
        let aid = AlertId(format!("stalled:{}:{}", t.session.0, t.position.leg));
        let alert = self.new_alert(aid, id, AlertKind::Stalled, now, false);
        self.raise(alert);
    }

    /// A Stalled or ProbablyWaiting train that writes again is Working; its
    /// Stalled alert expires.
    pub(super) fn recover(&mut self, id: TrainId) {
        let Some(t) = self.trains.get_mut(&id) else {
            return;
        };
        match t.state {
            TrainState::Stalled => {
                t.state = TrainState::Working;
                self.expire_where(|a| a.train == Some(id) && a.kind == AlertKind::Stalled);
            }
            TrainState::ProbablyWaiting => t.state = TrainState::Working,
            _ => {}
        }
    }
}
