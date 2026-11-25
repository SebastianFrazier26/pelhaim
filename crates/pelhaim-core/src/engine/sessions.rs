use std::collections::BTreeSet;
use std::path::PathBuf;

use super::alerts::child_active;
use super::eta::correction_factor;
use super::{Effect, Engine, OwnerEvent};
use crate::event::{Event, Tool, Ts};
use crate::ids::{SessionKey, TrainId};
use crate::model::{
    AssignTarget, Counters, Fork, LEG_BLOCKS, LineKey, Position, RuntimeRef, Station,
    StationStatus, StationTiming, TagResolution, Track, Train, TrainState,
};
use crate::number::train_number;
use crate::tag::{DISPATCH_SLUG, TagParse};

const TAIL_MAX: usize = 20;

pub(super) struct SessionMeta {
    tool: Tool,
    subagent: bool,
    title: Option<String>,
    description: Option<String>,
    self_tag: Option<TagParse>,
    spawn_tag: Option<TagParse>,
    /// Every milestone name seen on this track, folded ones included.
    milestones: BTreeSet<String>,
}

pub(super) struct PendingSpawn {
    parent: SessionKey,
    at: Position,
    description: String,
    tag: TagParse,
    closed: bool,
}

struct Discovered {
    key: SessionKey,
    tool: Tool,
    parent: Option<SessionKey>,
    path: PathBuf,
    cwd: Option<String>,
    started: Ts,
}

const EARLY_FINISH_CAP: usize = 256;

/// `claude:<sid>` for `claude:<sid>/<agent_id>`; `None` for a main session.
fn parent_of(key: &SessionKey) -> Option<&str> {
    key.0.split_once('/').map(|(p, _)| p)
}

impl Engine {
    pub(super) fn on_event(&mut self, e: Event) -> bool {
        // The engine models no CLI version; FormatUntested carries what the board needs.
        if let Event::SessionDiscovered {
            key,
            tool,
            parent,
            path,
            cwd,
            started,
            ..
        } = e
        {
            return self.discover(Discovered {
                key,
                tool,
                parent,
                path,
                cwd,
                started,
            });
        }
        let e = match e {
            Event::CatchUpDone { .. } => {
                let known: Vec<SessionKey> = self
                    .early_finishes
                    .keys()
                    .filter(|k| self.by_key.contains_key(*k))
                    .cloned()
                    .collect();
                let mut changed = false;
                for k in known {
                    changed |= self.apply_early_finish(&k);
                }
                return changed;
            }
            Event::DecisionExported { id, at } => return self.decision_exported(id, at),
            Event::DecisionExportFailed { id, reason } => {
                return self.decision_export_failed(id, reason);
            }
            Event::StoreWriteFailed { kind, reason } => {
                return self.store_write_failed(&kind, &reason);
            }
            Event::OpenDecisionCounts { counts } => {
                self.open_counts = counts;
                return true;
            }
            Event::KeepAwakeEnv {
                mechanism,
                on_battery,
            } => {
                self.keep.set_env(mechanism, on_battery);
                return true;
            }
            e => e,
        };
        let Some(key) = e.session().cloned() else {
            return false;
        };
        let Some(&id) = self.by_key.get(&key) else {
            if let Event::SubagentFinished { at, .. } = e
                && self.buffer_early_finish(&key, at)
            {
                return false;
            }
            self.dropped += 1;
            return false;
        };
        let linked = matches!(e, Event::SubagentLinked { .. }).then(|| key.clone());
        // A second finish (a parent's task notification after meta `finished`)
        // must not count as a write or re-berth the child.
        if matches!(e, Event::SubagentFinished { .. }) && self.trains[&id].state == TrainState::Done
        {
            return false;
        }
        let parent_before = self.trains[&id].parent;
        if let Some(at) = e.at() {
            self.touch(id, at);
            // F18: a write clears Stalled first; the event's own rule then sets the state.
            self.recover(id);
            if let Some(p) = parent_before {
                self.recover(p);
            }
        }
        match e {
            Event::FormatUntested { .. } => self.train_mut(id).version_untested = true,
            Event::TitleChanged { title, .. } => {
                self.meta_mut(&key).title = Some(title);
                self.relabel(&key);
            }
            Event::SessionTag { tag, .. } => {
                self.meta_mut(&key).self_tag = Some(tag);
                self.resolve(&key);
            }
            Event::SpawnRequested {
                tool_use_id,
                description,
                tag,
                ..
            } => {
                if !self.spawn_requests.contains_key(&tool_use_id) {
                    let at = self.trains[&id].position;
                    self.spawn_requests.insert(
                        tool_use_id.clone(),
                        PendingSpawn {
                            parent: key,
                            at,
                            description,
                            tag,
                            closed: false,
                        },
                    );
                }
                self.try_connect(&tool_use_id);
            }
            Event::SubagentLinked {
                parent,
                tool_use_id,
                description,
                ..
            } => {
                if let Some(d) = description {
                    self.meta_mut(&key).description = Some(d);
                    self.relabel(&key);
                }
                if let Some(&pid) = self.by_key.get(&parent) {
                    self.train_mut(id).parent.get_or_insert(pid);
                    self.resolve(&key);
                }
                if let Some(call) = tool_use_id
                    && self.links.get(&call) != Some(&key)
                {
                    self.links.insert(call.clone(), key);
                    self.try_connect(&call);
                }
            }
            Event::PromptStarted { .. } => self.prompt_started(id, &key),
            Event::Milestone {
                name,
                estimate_secs,
                at,
                ..
            } => self.milestone(id, &key, name, estimate_secs, at),
            Event::ToolOpened {
                call_id, name, at, ..
            } => {
                self.open_calls
                    .entry(key)
                    .or_default()
                    .insert(call_id, (name, at));
                self.tool_opened(id);
            }
            Event::ToolClosed {
                call_id,
                launched_only,
                ..
            } => {
                if let Some(calls) = self.open_calls.get_mut(&key) {
                    calls.remove(&call_id);
                }
                if !launched_only
                    && let Some(p) = self.spawn_requests.get_mut(&call_id)
                    && p.parent == key
                {
                    p.closed = true;
                    if let Some(child) = self.links.get(&call_id).cloned()
                        && let Some(&cid) = self.by_key.get(&child)
                    {
                        self.finish(cid);
                    }
                }
            }
            Event::QuestionAsked {
                call_id,
                questions,
                at,
                ..
            } => self.question_asked(id, &key, &call_id, questions, at),
            Event::QuestionAnswered {
                call_id,
                answer,
                at,
                ..
            } => self.question_answered(id, &key, &call_id, answer, at),
            Event::TurnEnded { final_text, at, .. } => {
                let subagent = self.meta[&key].subagent;
                if subagent {
                    self.finish(id);
                } else {
                    self.train_mut(id).position.berthed = true;
                    self.train_mut(id).state = TrainState::Idle;
                }
                let t = self.train_mut(id);
                t.counters.turns += 1;
                if !subagent {
                    self.settle_after_turn(id);
                }
                if let Some(text) = final_text {
                    self.raise_result(id, &key, &text, at);
                }
            }
            Event::WorkStarted { .. } => self.set_working(id),
            Event::SubagentFinished { .. } => self.finish(id),
            Event::Failed { reason, .. } => self.train_mut(id).state = TrainState::Failed(reason),
            Event::TranscriptTail { line, .. } => {
                let tail = &mut self.train_mut(id).tail;
                tail.push(line);
                if tail.len() > TAIL_MAX {
                    tail.drain(..tail.len() - TAIL_MAX);
                }
            }
            Event::SessionReset { .. } => self.reset(id, &key),
            Event::SessionGone { .. } => self.remove(id, &key),
            _ => {}
        }
        let parent_after = self.trains.get(&id).and_then(|t| t.parent);
        if let Some(p) = parent_after.or(parent_before) {
            self.settle_after_turn(p);
        }
        if let Some(child) = linked {
            self.apply_early_finish(&child);
        }
        true
    }

    /// A parent's catch-up can pass its task notification before the child file
    /// is discovered (mains are read first). Applying the finish at discovery
    /// would be undone by the child's own catch-up records, so it waits for the
    /// child's meta link, or for CatchUpDone.
    fn buffer_early_finish(&mut self, key: &SessionKey, at: Ts) -> bool {
        if parent_of(key).is_none() {
            return false;
        }
        if self.early_finishes.len() >= EARLY_FINISH_CAP && !self.early_finishes.contains_key(key) {
            return false;
        }
        self.early_finishes.insert(key.clone(), at);
        true
    }

    fn apply_early_finish(&mut self, key: &SessionKey) -> bool {
        match self.early_finishes.remove(key) {
            Some(at) => self.on_event(Event::SubagentFinished {
                key: key.clone(),
                at,
            }),
            None => false,
        }
    }

    /// A main session whose own turn has ended stays Working while any of its
    /// subagents is active: background subagents keep running after the
    /// parent's Agent call returns its launch acknowledgement (Review Focus 1).
    /// Returns whether the state changed.
    pub(super) fn settle_after_turn(&mut self, id: TrainId) -> bool {
        let Some(t) = self.trains.get(&id) else {
            return false;
        };
        if !t.position.berthed || !matches!(t.state, TrainState::Idle | TrainState::Working) {
            return false;
        }
        let busy = self
            .trains
            .values()
            .any(|c| c.parent == Some(id) && child_active(&c.state));
        let next = if busy {
            TrainState::Working
        } else {
            TrainState::Idle
        };
        let t = self.train_mut(id);
        let changed = t.state != next;
        t.state = next;
        changed
    }

    fn discover(&mut self, d: Discovered) -> bool {
        if let Some(&id) = self.by_key.get(&d.key) {
            self.train_mut(id).cwd = d.cwd;
            return true;
        }
        let id = self.ids.train();
        let track = self.ids.track();
        let taken: BTreeSet<u16> = self.trains.values().map(|t| t.number).collect();
        let local = d
            .started
            .with_timezone(&self.cfg.local_offset)
            .naive_local();
        let session_id = d.key.session_id().to_string();
        let runtime = match d.tool {
            Tool::Claude => RuntimeRef::ObservedClaude {
                path: d.path,
                agent_id: d.key.0.split_once('/').map(|(_, a)| a.to_string()),
                session_id,
            },
            Tool::Codex => RuntimeRef::ObservedCodex {
                path: d.path,
                session_id,
            },
        };
        let parent = d.parent.as_ref().and_then(|p| self.by_key.get(p)).copied();
        self.tracks.insert(
            track,
            Track {
                id: track,
                line: LineKey::Unsorted,
                session: d.key.clone(),
                stations: Vec::new(),
                origin: None,
                last_activity: d.started,
                folded: 0,
            },
        );
        self.trains.insert(
            id,
            Train {
                id,
                number: train_number(local, &taken),
                track,
                session: d.key.clone(),
                position: Position::default(),
                state: TrainState::Idle,
                tag: TagResolution::Untagged,
                runtime,
                parent,
                label: String::new(),
                counters: Counters {
                    tool_calls: 0,
                    turns: 0,
                    started: d.started,
                    last_seen: d.started,
                },
                cwd: d.cwd,
                version_untested: false,
                tail: Vec::new(),
            },
        );
        self.by_key.insert(d.key.clone(), id);
        self.meta.insert(
            d.key.clone(),
            SessionMeta {
                tool: d.tool,
                subagent: d.parent.is_some(),
                title: None,
                description: None,
                self_tag: None,
                spawn_tag: None,
                milestones: BTreeSet::new(),
            },
        );
        if d.parent.is_some() {
            self.train_mut(id).state = TrainState::Working;
        }
        self.relabel(&d.key);
        self.resolve(&d.key);
        if let Some(p) = parent {
            self.settle_after_turn(p);
        }
        true
    }

    fn touch(&mut self, id: TrainId, at: Ts) {
        let t = self.train_mut(id);
        t.counters.last_seen = t.counters.last_seen.max(at);
        let (track, parent) = (t.track, t.parent);
        let mut bump = |track| {
            if let Some(tr) = self.tracks.get_mut(&track) {
                tr.last_activity = tr.last_activity.max(at);
            }
        };
        bump(track);
        // Subagent writes count as the parent's activity (Review Focus 1); the
        // parent's own last_seen stays its own so heuristics can tell them apart.
        if let Some(parent_track) = parent.and_then(|p| self.trains.get(&p)).map(|p| p.track) {
            bump(parent_track);
        }
    }

    /// Prompts never make stations (§5.3); one only restarts the train.
    fn prompt_started(&mut self, id: TrainId, key: &SessionKey) {
        if self.meta[key].subagent {
            if matches!(self.trains[&id].state, TrainState::Failed(_)) {
                self.set_working(id);
            }
            return;
        }
        let t = self.train_mut(id);
        t.position.berthed = false;
        t.state = TrainState::Working;
    }

    /// §7.10: the marker closes the track's current station and opens the next
    /// one, which the train then runs towards. A repeated name is a no-op.
    fn milestone(
        &mut self,
        id: TrainId,
        key: &SessionKey,
        name: String,
        estimate_secs: Option<u32>,
        at: Ts,
    ) {
        if !self.meta_mut(key).milestones.insert(name.clone()) {
            return;
        }
        self.close_station(id, at);
        let track = self.trains[&id].track;
        let factor = correction_factor(&self.timings, self.project_of(track));
        let eta = estimate_secs.and_then(|e| {
            let secs = (f64::from(e) * factor).round() as i64;
            chrono::TimeDelta::try_seconds(secs).and_then(|d| at.checked_add_signed(d))
        });
        let station = self.ids.station();
        self.stations.insert(
            station,
            Station {
                id: station,
                track,
                title: name,
                status: StationStatus::Active,
                started: at,
                estimate_secs,
                eta,
            },
        );
        self.tracks
            .get_mut(&track)
            .expect("track")
            .stations
            .push(station);
        self.fold(track);
        let leg = (self.tracks[&track].stations.len() - 1) as u16;
        let p = &mut self.train_mut(id).position;
        p.leg = leg;
        p.block = 0;
    }

    fn project_of(&self, track: crate::ids::TrackId) -> Option<&str> {
        match &self.tracks.get(&track)?.line {
            LineKey::Project(s) => Some(s),
            _ => None,
        }
    }

    /// Marks the newest station Done and records its timing once.
    fn close_station(&mut self, id: TrainId, at: Ts) {
        let t = &self.trains[&id];
        let track = &self.tracks[&t.track];
        let Some(&sid) = track.stations.last() else {
            return;
        };
        let ordinal = track.folded as usize + track.stations.len() - 1;
        let key = format!("{}#{ordinal}", t.session.0);
        let project = self.project_of(t.track).map(str::to_string);
        let st = self.stations.get_mut(&sid).expect("station");
        let was_active = st.status == StationStatus::Active;
        st.status = StationStatus::Done;
        let (Some(estimate_secs), true) = (st.estimate_secs, was_active) else {
            return;
        };
        let actual = (at - st.started)
            .num_seconds()
            .clamp(0, i64::from(u32::MAX));
        if !self.timed.insert(key.clone()) {
            return;
        }
        let timing = StationTiming {
            key,
            project,
            estimate_secs,
            actual_secs: actual as u32,
        };
        self.timings.push(timing.clone());
        if self.timings.len() > super::TIMINGS_KEPT {
            self.timings.remove(0);
        }
        self.effects
            .push(Effect::Persist(OwnerEvent::StationTimed(timing), at));
    }

    /// Keeps at most `max_stations`; legs and fork points shift with the drop.
    fn fold(&mut self, track: crate::ids::TrackId) {
        let cap = self.cfg.max_stations.max(1);
        while self.tracks[&track].stations.len() > cap {
            let tr = self.tracks.get_mut(&track).expect("track");
            let gone = tr.stations.remove(0);
            tr.folded += 1;
            self.stations.remove(&gone);
            let shift = |p: &mut Position| {
                if p.leg == 0 {
                    p.block = 0;
                } else {
                    p.leg -= 1;
                }
            };
            for f in self.forks.values_mut().filter(|f| f.parent_track == track) {
                shift(&mut f.at);
            }
            let session = self.tracks[&track].session.clone();
            for p in self
                .spawn_requests
                .values_mut()
                .filter(|p| p.parent == session)
            {
                shift(&mut p.at);
            }
            if let Some(&tid) = self.by_key.get(&session) {
                shift(&mut self.train_mut(tid).position);
            }
        }
    }

    fn tool_opened(&mut self, id: TrainId) {
        let t = self.train_mut(id);
        t.counters.tool_calls += 1;
        t.position.berthed = false;
        // Bounded: with no station ahead the train waits one block short of the end.
        t.position.block = (t.position.block + 1).min(LEG_BLOCKS - 2);
        self.set_working(id);
    }

    fn set_working(&mut self, id: TrainId) {
        let t = self.train_mut(id);
        if !matches!(t.state, TrainState::Waiting(_)) {
            t.state = TrainState::Working;
        }
    }

    /// The train arrives: it berths at the end of its leg.
    fn finish(&mut self, id: TrainId) {
        let at = self.trains[&id].counters.last_seen;
        let t = self.train_mut(id);
        t.position.block = LEG_BLOCKS - 1;
        t.position.berthed = true;
        t.state = TrainState::Done;
        self.close_station(id, at);
    }

    fn try_connect(&mut self, call: &str) {
        let (Some(p), Some(child)) = (self.spawn_requests.get(call), self.links.get(call)) else {
            return;
        };
        let (Some(&pid), Some(&cid)) = (self.by_key.get(&p.parent), self.by_key.get(child)) else {
            return;
        };
        let child_track = self.trains[&cid].track;
        if self.tracks[&child_track].origin.is_some() {
            return;
        }
        let child = child.clone();
        let (at, tag, description, closed) = (p.at, p.tag.clone(), p.description.clone(), p.closed);
        let fork = self.ids.fork();
        self.forks.insert(
            fork,
            Fork {
                id: fork,
                parent_track: self.trains[&pid].track,
                at,
                child_track,
            },
        );
        self.tracks.get_mut(&child_track).expect("track").origin = Some(fork);
        self.train_mut(cid).parent = Some(pid);
        let meta = self.meta_mut(&child);
        meta.spawn_tag = Some(tag);
        meta.description.get_or_insert(description);
        self.relabel(&child);
        self.resolve(&child);
        if closed {
            self.finish(cid);
        }
        self.settle_after_turn(pid);
    }

    fn relabel(&mut self, key: &SessionKey) {
        let meta = &self.meta[key];
        let label = meta
            .title
            .clone()
            .or_else(|| meta.description.clone())
            .unwrap_or_else(|| format!("{:?} session", meta.tool));
        let id = self.by_key[key];
        self.train_mut(id).label = label;
    }

    pub(super) fn resolve(&mut self, key: &SessionKey) {
        let (tag, line) = self.resolution(key);
        let id = self.by_key[key];
        let t = self.train_mut(id);
        t.tag = tag;
        let track = t.track;
        self.tracks.get_mut(&track).expect("track").line = line;
        self.sync_assign_alert(key);
    }

    fn resolution(&self, key: &SessionKey) -> (TagResolution, LineKey) {
        let configured = |slug: &str| self.cfg.projects.iter().any(|p| p.slug == slug);
        let assigned = |target: &AssignTarget| {
            let line = match target {
                AssignTarget::Dispatch => LineKey::Dispatch,
                AssignTarget::Project(s) if configured(s) => LineKey::Project(s.clone()),
                AssignTarget::Project(_) => LineKey::Unsorted,
            };
            (TagResolution::Assigned(target.clone()), line)
        };
        if let Some((target, _)) = self.assignments.get(key) {
            return assigned(target);
        }
        let meta = &self.meta[key];
        let tagged = |t: &Option<TagParse>| match t {
            Some(TagParse::Tagged(s)) => Some(s.clone()),
            _ => None,
        };
        let tag = match tagged(&meta.spawn_tag).or_else(|| tagged(&meta.self_tag)) {
            Some(s) if s == DISPATCH_SLUG || configured(&s) => TagResolution::Tagged(s),
            Some(s) => TagResolution::MisTagged(s),
            None => TagResolution::Untagged,
        };
        // A subagent key carries its parent's session id, so only main sessions match the list.
        let listed = !meta.subagent
            && self
                .cfg
                .dispatch_sessions
                .iter()
                .any(|s| s == key.session_id());
        if tag == TagResolution::Untagged
            && let Some(target) = self.parent_assignment_for(key)
        {
            return assigned(target);
        }
        let line = match &tag {
            _ if listed => LineKey::Dispatch,
            TagResolution::Tagged(s) if s == DISPATCH_SLUG => LineKey::Dispatch,
            TagResolution::Tagged(s) => LineKey::Project(s.clone()),
            _ => LineKey::Unsorted,
        };
        (tag, line)
    }

    /// A subagent's tag is known once its spawn is linked; a main session's
    /// once its first user message is read.
    pub(super) fn tag_state_known(&self, key: &SessionKey) -> bool {
        let meta = &self.meta[key];
        if meta.subagent {
            meta.spawn_tag.is_some()
        } else {
            meta.self_tag.is_some()
        }
    }

    /// A project assignment covers the session's untagged subagents that
    /// started at or after it. DISPATCH is never inherited (§7.7), matching
    /// `[dispatch].sessions`, so an orchestrator's untagged children stay visible.
    fn parent_assignment_for(&self, key: &SessionKey) -> Option<&AssignTarget> {
        let child = &self.trains[&self.by_key[key]];
        let parent = self.trains.get(&child.parent?)?;
        let (target, at) = self.assignments.get(&parent.session)?;
        (matches!(target, AssignTarget::Project(_)) && child.counters.started >= *at)
            .then_some(target)
    }

    fn clear_forks_of(&mut self, track: crate::ids::TrackId) {
        let doomed: Vec<Fork> = self
            .forks
            .values()
            .filter(|f| f.parent_track == track || f.child_track == track)
            .cloned()
            .collect();
        for f in doomed {
            self.forks.remove(&f.id);
            if let Some(child) = self.tracks.get_mut(&f.child_track) {
                child.origin = None;
            }
        }
    }

    fn reset(&mut self, id: TrainId, key: &SessionKey) {
        let track = self.trains[&id].track;
        let tr = self.tracks.get_mut(&track).expect("track");
        tr.folded = 0;
        for s in std::mem::take(&mut tr.stations) {
            self.stations.remove(&s);
        }
        self.clear_forks_of(track);
        self.open_calls.remove(key);
        self.spawn_requests.retain(|_, p| &p.parent != key);
        self.links.retain(|_, c| c != key);
        self.expire_alerts_of(id);
        let t = self.train_mut(id);
        t.position = Position::default();
        t.state = TrainState::Idle;
        t.counters.tool_calls = 0;
        t.counters.turns = 0;
        t.tail.clear();
        self.meta_mut(key).self_tag = None;
        self.meta_mut(key).milestones.clear();
        if self.meta[key].subagent {
            self.train_mut(id).state = TrainState::Working;
        }
        self.relabel(key);
        self.resolve(key);
    }

    fn remove(&mut self, id: TrainId, key: &SessionKey) {
        let train = self.trains.remove(&id).expect("train");
        self.clear_forks_of(train.track);
        if let Some(track) = self.tracks.remove(&train.track) {
            for s in track.stations {
                self.stations.remove(&s);
            }
        }
        for t in self.trains.values_mut().filter(|t| t.parent == Some(id)) {
            t.parent = None;
        }
        self.by_key.remove(key);
        self.meta.remove(key);
        self.open_calls.remove(key);
        self.spawn_requests.retain(|_, p| &p.parent != key);
        self.links.retain(|_, c| c != key);
        self.early_finishes
            .retain(|child, _| child != key && parent_of(child) != Some(key.0.as_str()));
        self.expire_alerts_of(id);
    }

    pub(super) fn train_mut(&mut self, id: TrainId) -> &mut Train {
        self.trains.get_mut(&id).expect("train")
    }

    fn meta_mut(&mut self, key: &SessionKey) -> &mut SessionMeta {
        self.meta.get_mut(key).expect("session meta")
    }
}
