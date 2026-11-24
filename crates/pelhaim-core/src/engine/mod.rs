mod sessions;

use std::collections::{BTreeMap, HashMap};
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::event::{Event, Ts};
use crate::ids::{ForkId, SessionKey, StationId, TrackId, TrainId};
use crate::model::{
    Alert, AssignTarget, Fork, KeepAwakeStatus, LineKey, Station, Track, Train, TrainState,
};
use crate::snapshot::{LineView, Snapshot};

use sessions::{PendingSpawn, SessionMeta};

#[derive(Clone, PartialEq, Debug)]
pub struct ProjectCfg {
    pub slug: String,
    pub name: String,
    pub order: i32,
}

#[derive(Clone, PartialEq, Debug)]
pub struct EngineConfig {
    pub projects: Vec<ProjectCfg>,
    pub dispatch_sessions: Vec<String>,
    pub permission_after: Option<Duration>,
    pub stall_after: Option<Duration>,
    pub result_alerts: bool,
    pub float_alert_lines: bool,
    pub keep_awake_enabled: bool,
    pub include_observed: bool,
    pub remote_enabled: bool,
    pub decision_log_write: bool,
    pub local_offset: chrono::FixedOffset,
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            projects: Vec::new(),
            dispatch_sessions: Vec::new(),
            permission_after: Some(Duration::from_secs(20)),
            stall_after: Some(Duration::from_secs(10 * 60)),
            result_alerts: false,
            float_alert_lines: true,
            keep_awake_enabled: true,
            include_observed: true,
            remote_enabled: false,
            decision_log_write: true,
            local_offset: chrono::FixedOffset::east_opt(0).expect("zero offset"),
        }
    }
}

// Task 4 adds the owner commands.
#[derive(Clone, PartialEq, Serialize, Deserialize, Debug)]
pub enum Command {}

// Task 4 adds the effects.
#[derive(Clone, PartialEq, Serialize, Deserialize, Debug)]
pub enum Effect {}

#[derive(Clone, PartialEq, Serialize, Deserialize, Debug, Default)]
pub struct RestoredState {
    pub assignments: Vec<(SessionKey, AssignTarget, Ts)>,
}

#[derive(Clone, PartialEq, Debug)]
pub enum Input {
    Event(Event),
    Command(Command),
    Tick(Ts),
}

#[derive(Default)]
struct IdGen {
    train: u32,
    track: u32,
    station: u32,
    fork: u32,
}

impl IdGen {
    fn train(&mut self) -> TrainId {
        self.train += 1;
        TrainId(self.train)
    }
    fn track(&mut self) -> TrackId {
        self.track += 1;
        TrackId(self.track)
    }
    fn station(&mut self) -> StationId {
        self.station += 1;
        StationId(self.station)
    }
    fn fork(&mut self) -> ForkId {
        self.fork += 1;
        ForkId(self.fork)
    }
}

pub struct Engine {
    cfg: EngineConfig,
    version: u64,
    now: Ts,
    ids: IdGen,
    trains: BTreeMap<TrainId, Train>,
    tracks: BTreeMap<TrackId, Track>,
    stations: BTreeMap<StationId, Station>,
    forks: BTreeMap<ForkId, Fork>,
    by_key: BTreeMap<SessionKey, TrainId>,
    meta: BTreeMap<SessionKey, SessionMeta>,
    assignments: BTreeMap<SessionKey, (AssignTarget, Ts)>,
    spawn_requests: HashMap<String, PendingSpawn>,
    links: HashMap<String, SessionKey>,
    open_calls: HashMap<SessionKey, BTreeMap<String, (String, Ts)>>,
    alerts: Vec<Alert>,
    dropped: u64,
}

impl Engine {
    pub fn new(cfg: EngineConfig, restored: RestoredState) -> Engine {
        Engine {
            cfg,
            version: 0,
            now: chrono::DateTime::UNIX_EPOCH,
            ids: IdGen::default(),
            trains: BTreeMap::new(),
            tracks: BTreeMap::new(),
            stations: BTreeMap::new(),
            forks: BTreeMap::new(),
            by_key: BTreeMap::new(),
            meta: BTreeMap::new(),
            assignments: restored
                .assignments
                .into_iter()
                .map(|(k, t, at)| (k, (t, at)))
                .collect(),
            spawn_requests: HashMap::new(),
            links: HashMap::new(),
            open_calls: HashMap::new(),
            alerts: Vec::new(),
            dropped: 0,
        }
    }

    pub fn apply(&mut self, input: Input) -> Vec<Effect> {
        let changed = match input {
            Input::Event(e) => self.on_event(e),
            Input::Command(c) => match c {},
            Input::Tick(now) => {
                let new_second = now.timestamp() != self.now.timestamp();
                self.now = now;
                new_second
            }
        };
        if changed {
            self.version += 1;
        }
        Vec::new()
    }

    /// Events for sessions the engine has not discovered are dropped; this counts them.
    pub fn dropped_events(&self) -> u64 {
        self.dropped
    }

    /// The latest write by the session or any of its live (non-Done) subagents.
    pub fn effective_last_write(&self, id: TrainId) -> Option<Ts> {
        let own = self.trains.get(&id)?.counters.last_seen;
        Some(
            self.trains
                .values()
                .filter(|t| t.parent == Some(id) && t.state != TrainState::Done)
                .map(|t| t.counters.last_seen)
                .fold(own, Ts::max),
        )
    }

    pub fn snapshot(&self) -> Snapshot {
        Snapshot {
            version: self.version,
            now: self.now,
            lines: self.line_views(),
            tracks: self.tracks.clone(),
            stations: self.stations.clone(),
            trains: self.trains.clone(),
            forks: self.forks.values().cloned().collect(),
            alerts: self
                .alerts
                .iter()
                .filter(|a| a.is_open())
                .cloned()
                .collect(),
            decisions_recent: Vec::new(),
            decisions_unexported: 0,
            keep_awake: KeepAwakeStatus::default(),
        }
    }

    fn line_views(&self) -> Vec<LineView> {
        let mut projects: Vec<&ProjectCfg> = self.cfg.projects.iter().collect();
        projects.sort_by_key(|p| p.order);
        let mut lines = vec![self.line_view(LineKey::Dispatch, "DISPATCH".into())];
        let mut project_lines: Vec<LineView> = projects
            .iter()
            .map(|p| self.line_view(LineKey::Project(p.slug.clone()), p.name.clone()))
            .collect();
        if self.cfg.float_alert_lines {
            project_lines.sort_by_key(|l| !l.has_open_alert);
        }
        lines.extend(project_lines);
        lines.push(self.line_view(LineKey::Unsorted, "UNSORTED".into()));
        lines
    }

    fn line_view(&self, key: LineKey, name: String) -> LineView {
        let mut tracks: Vec<(&Track, &Train)> = self
            .trains
            .values()
            .map(|t| (&self.tracks[&t.track], t))
            .filter(|(tr, _)| tr.line == key)
            .collect();
        tracks.sort_by(|(ta, a), (tb, b)| {
            let rank = |t: &Train| {
                (
                    !self.train_has_open_alert(t.id),
                    t.state != TrainState::Working,
                )
            };
            rank(a)
                .cmp(&rank(b))
                .then(tb.last_activity.cmp(&ta.last_activity))
                .then(ta.id.cmp(&tb.id))
        });
        LineView {
            has_open_alert: self.alerts.iter().any(|a| a.is_open() && a.line == key),
            format_untested: tracks.iter().any(|(_, t)| t.version_untested),
            active: !tracks.is_empty(),
            tracks: tracks.iter().map(|(tr, _)| tr.id).collect(),
            open_decisions: 0,
            key,
            name,
        }
    }

    fn train_has_open_alert(&self, id: TrainId) -> bool {
        self.alerts
            .iter()
            .any(|a| a.is_open() && a.train == Some(id))
    }
}
