use std::collections::BTreeSet;
use std::path::PathBuf;

use super::Engine;
use super::alerts::child_active;
use crate::event::{Event, Tool, Ts};
use crate::ids::{SessionKey, TrainId};
use crate::model::{
    AssignTarget, Counters, Fork, LEG_BLOCKS, LineKey, Position, RuntimeRef, Station,
    StationStatus, TagResolution, Track, Train, TrainState,
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
            Event::CatchUpDone { .. } => return false,
            Event::DecisionExported { id, at } => return self.decision_exported(id, at),
            Event::DecisionExportFailed { id, reason } => {
                return self.decision_export_failed(id, reason);
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
            self.dropped += 1;
            return false;
        };
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
            Event::PromptStarted { title, .. } => self.prompt_started(id, &key, title),
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
                self.berth(id);
                let t = self.train_mut(id);
                t.state = if subagent {
                    TrainState::Done
                } else {
                    TrainState::Idle
                };
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
        true
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
            },
        );
        if d.parent.is_some() {
            self.start_subagent(id);
        }
        self.relabel(&d.key);
        self.resolve(&d.key);
        if let Some(p) = parent {
            self.settle_after_turn(p);
        }
        true
    }

    fn start_subagent(&mut self, id: TrainId) {
        let track = self.trains[&id].track;
        let station = self.ids.station();
        self.stations.insert(
            station,
            Station {
                id: station,
                track,
                title: String::new(),
                status: StationStatus::Active,
            },
        );
        self.tracks
            .get_mut(&track)
            .expect("track")
            .stations
            .push(station);
        self.train_mut(id).state = TrainState::Working;
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

    fn prompt_started(&mut self, id: TrainId, key: &SessionKey, title: String) {
        if self.meta[key].subagent {
            if matches!(self.trains[&id].state, TrainState::Failed(_)) {
                self.set_working(id);
            }
            return;
        }
        let track = self.trains[&id].track;
        if let Some(prev) = self.tracks[&track].stations.last() {
            self.stations.get_mut(prev).expect("station").status = StationStatus::Done;
        }
        let station = self.ids.station();
        self.stations.insert(
            station,
            Station {
                id: station,
                track,
                title,
                status: StationStatus::Active,
            },
        );
        let stations = &mut self.tracks.get_mut(&track).expect("track").stations;
        stations.push(station);
        let leg = (stations.len() - 1) as u16;
        let t = self.train_mut(id);
        t.position = Position {
            leg,
            block: 0,
            berthed: false,
        };
        t.state = TrainState::Working;
    }

    fn tool_opened(&mut self, id: TrainId) {
        let t = self.train_mut(id);
        t.counters.tool_calls += 1;
        if t.position.berthed {
            t.position.berthed = false;
            t.position.block = LEG_BLOCKS - 2;
            self.set_current_station(id, StationStatus::Active);
        } else {
            t.position.block = (t.position.block + 1).min(LEG_BLOCKS - 2);
        }
        self.set_working(id);
    }

    fn set_working(&mut self, id: TrainId) {
        let t = self.train_mut(id);
        if !matches!(t.state, TrainState::Waiting(_)) {
            t.state = TrainState::Working;
        }
    }

    fn berth(&mut self, id: TrainId) {
        let p = &mut self.train_mut(id).position;
        p.block = LEG_BLOCKS - 1;
        p.berthed = true;
        self.set_current_station(id, StationStatus::Done);
    }

    fn finish(&mut self, id: TrainId) {
        self.berth(id);
        self.train_mut(id).state = TrainState::Done;
    }

    fn set_current_station(&mut self, id: TrainId, status: StationStatus) {
        let t = &self.trains[&id];
        let current = self.tracks[&t.track].stations.get(t.position.leg as usize);
        if let Some(s) = current.and_then(|s| self.stations.get_mut(s)) {
            s.status = status;
        }
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
        let station_title = meta.description.clone().unwrap_or_else(|| label.clone());
        let subagent = meta.subagent;
        let id = self.by_key[key];
        let t = self.train_mut(id);
        t.label = label;
        let track = t.track;
        if subagent && let Some(s) = self.tracks[&track].stations.first() {
            self.stations.get_mut(s).expect("station").title = station_title;
        }
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

    /// An owner assignment covers the session's untagged subagents that
    /// started at or after it.
    fn parent_assignment_for(&self, key: &SessionKey) -> Option<&AssignTarget> {
        let child = &self.trains[&self.by_key[key]];
        let parent = self.trains.get(&child.parent?)?;
        let (target, at) = self.assignments.get(&parent.session)?;
        (child.counters.started >= *at).then_some(target)
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
        for s in std::mem::take(&mut self.tracks.get_mut(&track).expect("track").stations) {
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
        if self.meta[key].subagent {
            self.start_subagent(id);
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
        self.expire_alerts_of(id);
    }

    pub(super) fn train_mut(&mut self, id: TrainId) -> &mut Train {
        self.trains.get_mut(&id).expect("train")
    }

    fn meta_mut(&mut self, key: &SessionKey) -> &mut SessionMeta {
        self.meta.get_mut(key).expect("session meta")
    }
}
