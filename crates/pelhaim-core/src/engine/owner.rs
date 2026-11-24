use super::{
    AlertEvent, Answer, AnswerOutcome, Choice, Command, Effect, Engine, LogError, OwnerEvent,
};
use crate::event::{Question, Ts};
use crate::ids::{AlertId, DecisionId, SessionKey, TrainId};
use crate::model::{
    Alert, AlertKind, AlertState, AnsweredBy, AssignTarget, Decision, LineKey, Origin, Who,
};
use crate::text::truncate_chars;

const QUESTION_TITLE_CHARS: usize = 60;

impl Engine {
    pub(super) fn on_command(&mut self, c: Command) -> bool {
        match c {
            Command::Assign {
                session,
                target,
                at,
            } => {
                self.effects.push(Effect::Persist(
                    OwnerEvent::Assigned {
                        session: session.clone(),
                        target: target.clone(),
                    },
                    at,
                ));
                self.answer_assign_alert(&session, &target, at);
                self.assign(session, target, at);
                true
            }
            Command::Dismiss { alert, at } => self.dismiss(alert, at),
            Command::Reopen { alert, at } => self.reopen(alert, at),
            Command::LogDecision { alert, project, at } => {
                if let Err(e) = self.log_decision(&alert, project, at) {
                    let source = self.alerts.iter().find(|a| a.id == alert);
                    let (train, line) =
                        source.map_or((None, LineKey::Unsorted), |a| (a.train, a.line.clone()));
                    self.raise_failure(
                        AlertId(format!("log-error:{}", alert.0)),
                        train,
                        line,
                        format!("Decision not logged: {e}"),
                        at,
                    );
                }
                true
            }
            Command::ExportPending => {
                // F19: `write = "off"` never writes, not even on request.
                if self.cfg.decision_log_write {
                    for d in self.decisions.iter().filter(|d| !d.written_to_md) {
                        self.effects.push(Effect::ExportDecision(d.clone()));
                    }
                }
                false
            }
            Command::PauseKeepAwake { until } => {
                self.pause_keep_awake(until);
                true
            }
        }
    }

    /// Applies to the session's train and to its untagged subagents that start
    /// at or after `at` (resolved in `parent_assignment_for`), now or later.
    fn assign(&mut self, session: SessionKey, target: AssignTarget, at: Ts) {
        self.assignments.insert(session.clone(), (target, at));
        let Some(&id) = self.by_key.get(&session) else {
            return;
        };
        self.resolve(&session);
        let children: Vec<SessionKey> = self
            .trains
            .values()
            .filter(|t| t.parent == Some(id))
            .map(|t| t.session.clone())
            .collect();
        for c in children {
            self.resolve(&c);
        }
    }

    /// F40: `Command::Assign` also answers the session's open Assign alert.
    fn answer_assign_alert(&mut self, session: &SessionKey, target: &AssignTarget, at: Ts) {
        let aid = AlertId(format!("assign:{}", session.0));
        let fallback = self.target_name(target);
        let Some(a) = self.alerts.iter_mut().find(|a| a.id == aid && a.is_open()) else {
            return;
        };
        let answer = match &a.kind {
            AlertKind::AssignProject { options, .. } => options
                .iter()
                .find(|o| &o.target == target)
                .map_or(fallback, |o| o.label.clone()),
            _ => fallback,
        };
        a.state = AlertState::Answered {
            answer,
            by: AnsweredBy::Local,
            at,
        };
        let a = a.clone();
        self.effects.push(Effect::Alert(AlertEvent::Answered(a)));
    }

    fn target_name(&self, target: &AssignTarget) -> String {
        match target {
            AssignTarget::Dispatch => "DISPATCH".into(),
            AssignTarget::Project(slug) => self
                .cfg
                .projects
                .iter()
                .find(|p| &p.slug == slug)
                .map_or_else(|| slug.clone(), |p| p.name.clone()),
        }
    }

    fn dismiss(&mut self, id: AlertId, at: Ts) -> bool {
        let Some(a) = self
            .alerts
            .iter_mut()
            .find(|a| a.id == id && a.state == AlertState::Open)
        else {
            return false;
        };
        a.state = AlertState::Tray;
        self.tray.insert(id.clone());
        self.effects.push(Effect::Persist(
            OwnerEvent::Dismissed { alert: id.clone() },
            at,
        ));
        self.effects.push(Effect::Alert(AlertEvent::Dismissed(id)));
        true
    }

    fn reopen(&mut self, id: AlertId, at: Ts) -> bool {
        let Some(a) = self
            .alerts
            .iter_mut()
            .find(|a| a.id == id && a.state == AlertState::Tray)
        else {
            return false;
        };
        a.state = AlertState::Open;
        let a = a.clone();
        self.tray.remove(&id);
        self.effects
            .push(Effect::Persist(OwnerEvent::Reopened { alert: id }, at));
        self.effects.push(Effect::Alert(AlertEvent::Updated(a)));
        true
    }

    pub(super) fn answer_alert(&mut self, a: Answer, now: Ts) -> AnswerOutcome {
        let Some(idx) = self.alerts.iter().position(|x| x.id == a.alert_id) else {
            return AnswerOutcome::UnknownAlert;
        };
        let alert = &self.alerts[idx];
        if let AlertState::Answered { by, at, .. } = &alert.state {
            return AnswerOutcome::AlreadyAnswered {
                by: by.clone(),
                at: *at,
            };
        }
        if !alert.answerable {
            return AnswerOutcome::NotAnswerable;
        }
        if matches!(a.origin, Origin::Remote { .. }) && !alert.kind.remote_answerable() {
            return AnswerOutcome::ViewOnlyFromRemote;
        }
        let (answer, assignment) = match (&alert.kind, &a.choice) {
            (
                AlertKind::AssignProject {
                    session, options, ..
                },
                Choice::Options(picked),
            ) => match picked.as_slice() {
                [i] if *i < options.len() => (
                    options[*i].label.clone(),
                    Some((session.clone(), options[*i].target.clone())),
                ),
                _ => return AnswerOutcome::InvalidChoice,
            },
            (AlertKind::AssignProject { .. }, Choice::Text(_)) => {
                return AnswerOutcome::InvalidChoice;
            }
            (AlertKind::Question { questions }, choice) => match question_answer(questions, choice)
            {
                Some(text) => (text, None),
                None => return AnswerOutcome::InvalidChoice,
            },
            _ => return AnswerOutcome::NotAnswerable,
        };
        self.effects.push(Effect::Persist(
            OwnerEvent::Answered {
                alert: a.alert_id,
                choice: a.choice,
                origin: a.origin.clone(),
            },
            now,
        ));
        if let Some((session, target)) = &assignment {
            self.effects.push(Effect::Persist(
                OwnerEvent::Assigned {
                    session: session.clone(),
                    target: target.clone(),
                },
                now,
            ));
        }
        let alert = &mut self.alerts[idx];
        alert.state = AlertState::Answered {
            answer,
            by: a.origin.into(),
            at: now,
        };
        let alert = alert.clone();
        self.effects
            .push(Effect::Alert(AlertEvent::Answered(alert)));
        if let Some((session, target)) = assignment {
            self.assign(session, target, now);
        }
        AnswerOutcome::Accepted
    }

    fn log_decision(
        &mut self,
        alert_id: &AlertId,
        project: Option<String>,
        at: Ts,
    ) -> Result<(), LogError> {
        let idx = self
            .alerts
            .iter()
            .position(|a| &a.id == alert_id)
            .ok_or(LogError::UnknownAlert)?;
        let a = &self.alerts[idx];
        let AlertState::Answered { answer, .. } = &a.state else {
            return Err(LogError::NotAnswered);
        };
        // Logging the same answer twice would duplicate the entry in the owner's log.
        // Re-derived alerts lose `decision` across a restart, so restored decisions
        // are checked by source alert as well.
        if a.decision.is_some()
            || self
                .decisions
                .iter()
                .any(|d| d.source_alert.as_ref() == Some(alert_id))
        {
            return Ok(());
        }
        let project = match project {
            Some(p) => p,
            None => match self.alert_line(a) {
                LineKey::Project(slug) => slug.clone(),
                LineKey::Dispatch | LineKey::Unsorted => return Err(LogError::NeedsProject),
            },
        };
        let (title, why) = match &a.kind {
            AlertKind::Question { questions } => {
                let head = questions.first().map_or_else(String::new, |q| {
                    if q.header.is_empty() {
                        truncate_chars(&q.question, QUESTION_TITLE_CHARS)
                    } else {
                        q.header.clone()
                    }
                });
                let why = questions
                    .iter()
                    .map(|q| q.question.as_str())
                    .collect::<Vec<_>>()
                    .join("; ");
                (format!("{head}: {answer}"), why)
            }
            AlertKind::AssignProject { session, .. } => {
                let target = self
                    .assignments
                    .get(session)
                    .map_or_else(|| answer.clone(), |(t, _)| self.target_name(t));
                let short: String = session.session_id().chars().take(8).collect();
                (
                    format!("Session {short} assigned to {target}"),
                    "Untagged session placed by the owner".to_string(),
                )
            }
            _ => return Err(LogError::NotAnswered),
        };
        let id = DecisionId(self.decisions.iter().map(|d| d.id.0).max().unwrap_or(0) + 1);
        let decision = Decision {
            id,
            project,
            date: at.with_timezone(&self.cfg.local_offset).date_naive(),
            title,
            why,
            who: Who::Owner,
            origin: Origin::Local,
            source_alert: Some(alert_id.clone()),
            written_to_md: false,
        };
        self.decisions.push(decision.clone());
        let alert = &mut self.alerts[idx];
        alert.decision = Some(id);
        let alert = alert.clone();
        self.effects.push(Effect::Persist(
            OwnerEvent::DecisionLogged(decision.clone()),
            at,
        ));
        if self.cfg.decision_log_write {
            self.effects.push(Effect::ExportDecision(decision));
        }
        self.effects.push(Effect::Alert(AlertEvent::Updated(alert)));
        Ok(())
    }

    pub(super) fn decision_exported(&mut self, id: DecisionId, at: Ts) -> bool {
        let Some(d) = self
            .decisions
            .iter_mut()
            .find(|d| d.id == id && !d.written_to_md)
        else {
            return false;
        };
        d.written_to_md = true;
        self.effects
            .push(Effect::Persist(OwnerEvent::DecisionExported { id }, at));
        true
    }

    pub(super) fn decision_export_failed(&mut self, id: DecisionId, reason: String) -> bool {
        let decision = self.decisions.iter().find(|d| d.id == id);
        let line = decision
            .filter(|d| self.cfg.projects.iter().any(|p| p.slug == d.project))
            .map_or(LineKey::Unsorted, |d| LineKey::Project(d.project.clone()));
        let train = decision
            .and_then(|d| d.source_alert.as_ref())
            .and_then(|s| self.alerts.iter().find(|a| &a.id == s))
            .and_then(|a| a.train);
        self.raise_failure(
            AlertId(format!("decision-failure:{}", id.0)),
            train,
            line,
            format!("Decision log not written: {reason}"),
            self.now,
        );
        true
    }

    /// A repeat failure replaces the earlier alert and pops up again, even if
    /// the owner had dismissed it.
    fn raise_failure(
        &mut self,
        id: AlertId,
        train: Option<TrainId>,
        line: LineKey,
        reason: String,
        at: Ts,
    ) {
        self.alerts.retain(|a| a.id != id);
        let alert = Alert {
            id,
            train,
            line,
            kind: AlertKind::Failure { reason },
            created: at,
            state: AlertState::Open,
            answerable: false,
            decision: None,
        };
        self.alerts.push(alert.clone());
        self.effects.push(Effect::Alert(AlertEvent::Raised(alert)));
    }
}

fn question_answer(questions: &[Question], choice: &Choice) -> Option<String> {
    let q = questions.first()?;
    match choice {
        Choice::Options(picked)
            if !picked.is_empty()
                && (picked.len() == 1 || q.multi_select)
                && picked.iter().all(|&i| i < q.options.len()) =>
        {
            let labels: Vec<&str> = picked
                .iter()
                .map(|&i| q.options[i].label.as_str())
                .collect();
            Some(labels.join(", "))
        }
        Choice::Text(text) if !text.trim().is_empty() => Some(text.clone()),
        _ => None,
    }
}
