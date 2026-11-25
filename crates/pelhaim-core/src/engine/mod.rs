mod alerts;
pub mod eta;
mod keep_awake;
mod owner;
mod sessions;

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::event::{Event, Ts};
use crate::ids::{AlertId, DecisionId, ForkId, SessionKey, StationId, TrackId, TrainId};
use crate::model::{
    Alert, AlertState, AnsweredBy, AssignTarget, Decision, Fork, LineKey, Origin, Station,
    StationTiming, Track, Train, TrainState,
};
use crate::snapshot::{LineView, Snapshot};

use alerts::child_active;
use keep_awake::KeepAwake;
use sessions::{PendingSpawn, SessionMeta};

const RECENT_DECISIONS: usize = 10;

#[derive(Clone, PartialEq, Debug)]
pub struct ProjectCfg {
    pub slug: String,
    pub name: String,
    pub order: i32,
    /// The repository root, expanded and canonical where it exists; a session
    /// whose repository root equals it lands on this line (§7.7).
    pub path: Option<std::path::PathBuf>,
    /// The line colour override, validated by the config loader.
    pub color: Option<String>,
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
    /// Stations kept per track; older ones fold into "+N earlier". At least 1.
    pub max_stations: usize,
    /// §7.7 (2026-11-24): untagged sessions go to their cwd's repository, and
    /// lines are created for repositories and tags not in config.
    pub auto_projects: bool,
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
            max_stations: 5,
            auto_projects: true,
        }
    }
}

#[derive(Clone, PartialEq, Serialize, Deserialize, Debug)]
pub enum Command {
    Assign {
        session: SessionKey,
        target: AssignTarget,
        at: Ts,
    },
    Dismiss {
        alert: AlertId,
        at: Ts,
    },
    Reopen {
        alert: AlertId,
        at: Ts,
    },
    LogDecision {
        alert: AlertId,
        project: Option<String>,
        at: Ts,
    },
    ExportPending,
    PauseKeepAwake {
        until: Ts,
    },
}

#[derive(Clone, PartialEq, Serialize, Deserialize, Debug)]
pub struct Answer {
    pub alert_id: AlertId,
    pub choice: Choice,
    pub origin: Origin,
}

#[derive(Clone, PartialEq, Serialize, Deserialize, Debug)]
pub enum Choice {
    Options(Vec<usize>),
    Text(String),
}

#[derive(Clone, PartialEq, Serialize, Deserialize, Debug)]
pub enum AnswerOutcome {
    Accepted,
    AlreadyAnswered { by: AnsweredBy, at: Ts },
    NotAnswerable,
    ViewOnlyFromRemote,
    InvalidChoice,
    UnknownAlert,
}

/// Why `Command::LogDecision` wrote nothing; surfaced as a `Failure` alert.
#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Debug)]
pub enum LogError {
    NotAnswered,
    NeedsProject,
    UnknownAlert,
}

impl std::fmt::Display for LogError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            LogError::NotAnswered => "the alert has not been answered",
            LogError::NeedsProject => "choose a project for this decision",
            LogError::UnknownAlert => "the alert no longer exists",
        })
    }
}

impl std::error::Error for LogError {}

#[derive(Clone, PartialEq, Serialize, Deserialize, Debug)]
pub enum Effect {
    Persist(OwnerEvent, Ts),
    ExportDecision(Decision),
    Alert(AlertEvent),
    KeepAwake(KeepAwakeAction),
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize, Debug)]
pub enum KeepAwakeAction {
    Hold,
    Release,
}

#[derive(Clone, PartialEq, Serialize, Deserialize, Debug)]
pub enum AlertEvent {
    Raised(Alert),
    Updated(Alert),
    Answered(Alert),
    Dismissed(AlertId),
}

#[derive(Clone, PartialEq, Serialize, Deserialize, Debug)]
pub enum OwnerEvent {
    Assigned {
        session: SessionKey,
        target: AssignTarget,
    },
    Dismissed {
        alert: AlertId,
    },
    Reopened {
        alert: AlertId,
    },
    Answered {
        alert: AlertId,
        choice: Choice,
        origin: Origin,
    },
    DecisionLogged(Decision),
    DecisionExported {
        id: DecisionId,
    },
    /// Not the owner's act, but owner-side state: the learned ETA correction.
    StationTimed(StationTiming),
}

#[derive(Clone, PartialEq, Serialize, Deserialize, Debug, Default)]
pub struct RestoredState {
    pub assignments: Vec<(SessionKey, AssignTarget, Ts)>,
    pub tray: BTreeSet<AlertId>,
    pub decisions: Vec<Decision>,
    /// Oldest first.
    pub timings: Vec<StationTiming>,
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
    /// Never holds an Expired alert: expiring removes it.
    alerts: Vec<Alert>,
    tray: BTreeSet<AlertId>,
    decisions: Vec<Decision>,
    open_counts: BTreeMap<String, u32>,
    keep: KeepAwake,
    effects: Vec<Effect>,
    dropped: u64,
    /// Subagent finishes that arrived before their child was known, keyed by child.
    early_finishes: BTreeMap<SessionKey, Ts>,
    /// Oldest first, at most `TIMINGS_KEPT`.
    timings: Vec<StationTiming>,
    timed: HashSet<String>,
    /// Names of lines for slugs not in config, slug to display name; kept for the run.
    auto_lines: BTreeMap<String, String>,
}

const TIMINGS_KEPT: usize = 1000;

impl Engine {
    pub fn new(cfg: EngineConfig, restored: RestoredState) -> Engine {
        let timed = restored.timings.iter().map(|t| t.key.clone()).collect();
        let mut timings = restored.timings;
        timings.drain(..timings.len().saturating_sub(TIMINGS_KEPT));
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
            tray: restored.tray,
            decisions: restored.decisions,
            open_counts: BTreeMap::new(),
            keep: KeepAwake::default(),
            effects: Vec::new(),
            dropped: 0,
            early_finishes: BTreeMap::new(),
            timings,
            timed,
            auto_lines: BTreeMap::new(),
        }
    }

    pub fn apply(&mut self, input: Input) -> Vec<Effect> {
        let (changed, clock, tick) = match input {
            Input::Event(e) => {
                let clock = e.at().map_or(self.now, |at| at.max(self.now));
                (self.on_event(e), clock, false)
            }
            Input::Command(c) => (self.on_command(c), self.now, false),
            Input::Tick(now) => {
                let new_second = now.timestamp() != self.now.timestamp();
                self.now = now;
                let moved = self.observe_heuristics(now);
                (new_second || moved, now, true)
            }
        };
        let awake = self.keep_awake_step(clock, tick);
        if changed || awake {
            self.version += 1;
        }
        std::mem::take(&mut self.effects)
    }

    /// The owner (or the phone) answering an alert. The first answer wins.
    pub fn answer(&mut self, a: Answer, now: Ts) -> (AnswerOutcome, Vec<Effect>) {
        let outcome = self.answer_alert(a, now);
        if outcome == AnswerOutcome::Accepted {
            self.version += 1;
        }
        (outcome, std::mem::take(&mut self.effects))
    }

    /// `snapshot().version` without building the snapshot.
    pub fn version(&self) -> u64 {
        self.version
    }

    /// Events for sessions the engine has not discovered are dropped; this counts them.
    pub fn dropped_events(&self) -> u64 {
        self.dropped
    }

    /// The latest write by the session or any of its active subagents. Done,
    /// Stalled and Failed children are not active: a child that went quiet or
    /// failed must not keep its parent alive forever.
    pub fn effective_last_write(&self, id: TrainId) -> Option<Ts> {
        let own = self.trains.get(&id)?.counters.last_seen;
        Some(
            self.trains
                .values()
                .filter(|t| t.parent == Some(id) && child_active(&t.state))
                .map(|t| t.counters.last_seen)
                .fold(own, Ts::max),
        )
    }

    pub fn snapshot(&self) -> Snapshot {
        let mut alerts: Vec<Alert> = self
            .alerts
            .iter()
            .filter(|a| a.state != AlertState::Expired)
            .cloned()
            .collect();
        alerts.sort_by_key(|a| a.created);
        let mut decisions = self.decisions.clone();
        decisions.sort_by_key(|d| std::cmp::Reverse(d.id));
        decisions.truncate(RECENT_DECISIONS);
        Snapshot {
            version: self.version,
            now: self.now,
            lines: self.line_views(),
            tracks: self.tracks.clone(),
            stations: self.stations.clone(),
            trains: self.trains.clone(),
            // A fork off an orchestrator has no parent track drawn (§5.3).
            forks: self
                .forks
                .values()
                .filter(|f| {
                    self.tracks.get(&f.parent_track).map(|t| &t.line) != Some(&LineKey::Dispatch)
                })
                .cloned()
                .collect(),
            alerts,
            decisions_recent: decisions,
            decisions_unexported: self.decisions.iter().filter(|d| !d.written_to_md).count() as u32,
            keep_awake: self.keep_awake_status(),
            utc_offset_secs: self.cfg.local_offset.local_minus_utc(),
            max_stations: self.cfg.max_stations.max(1) as u32,
        }
    }

    /// `(slug, name, color)`: configured lines by `order`, then auto-created ones by
    /// slug. An auto line shows only while a track is on it, so a session that
    /// sat there until its tag arrived leaves no empty line behind.
    fn projects_in_order(&self) -> Vec<(String, String, Option<String>)> {
        let mut projects: Vec<&ProjectCfg> = self.cfg.projects.iter().collect();
        projects.sort_by_key(|p| p.order);
        let occupied: BTreeSet<&str> = self
            .tracks
            .values()
            .filter_map(|t| match &t.line {
                LineKey::Project(s) => Some(s.as_str()),
                _ => None,
            })
            .collect();
        projects
            .into_iter()
            .map(|p| (p.slug.clone(), p.name.clone(), p.color.clone()))
            .chain(
                self.auto_lines
                    .iter()
                    .filter(|(s, _)| occupied.contains(s.as_str()))
                    .map(|(s, n)| (s.clone(), n.clone(), None)),
            )
            .collect()
    }

    fn line_views(&self) -> Vec<LineView> {
        let projects = self.projects_in_order();
        let mut lines = vec![self.line_view(LineKey::Dispatch, "DISPATCH".into())];
        let mut project_lines: Vec<LineView> = projects
            .into_iter()
            .map(|(slug, name, color)| LineView {
                color,
                ..self.line_view(LineKey::Project(slug), name)
            })
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
        // §5.3: the orchestrator draws no train; DISPATCH is a one-row count strip.
        let orchestrators = tracks.len();
        let running = if key == LineKey::Dispatch {
            tracks.clear();
            self.dispatch_running()
        } else {
            0
        };
        let open_decisions = match &key {
            LineKey::Project(slug) => self.open_counts.get(slug).copied().unwrap_or(0),
            _ => 0,
        };
        LineView {
            has_open_alert: self
                .alerts
                .iter()
                .any(|a| a.is_open() && self.alert_line(a) == &key),
            format_untested: tracks.iter().any(|(_, t)| t.version_untested),
            active: !tracks.is_empty() || orchestrators > 0,
            running,
            tracks: tracks.iter().map(|(tr, _)| tr.id).collect(),
            open_decisions,
            key,
            name,
            color: None,
        }
    }

    /// Active trains with an orchestrator among their ancestors.
    fn dispatch_running(&self) -> u32 {
        let on_dispatch = |id: TrainId| {
            self.trains
                .get(&id)
                .is_some_and(|t| self.tracks[&t.track].line == LineKey::Dispatch)
        };
        let descends = |t: &Train| {
            let mut p = t.parent;
            // The bound only guards against a malformed parent cycle.
            for _ in 0..64 {
                match p {
                    Some(id) if on_dispatch(id) => return true,
                    Some(id) => p = self.trains.get(&id).and_then(|t| t.parent),
                    None => return false,
                }
            }
            false
        };
        self.trains
            .values()
            .filter(|t| child_active(&t.state) && t.state != TrainState::Idle && descends(t))
            .count() as u32
    }

    /// The alert's train's current line; `alert.line` once the train is gone.
    fn alert_line<'a>(&'a self, a: &'a Alert) -> &'a LineKey {
        a.train
            .and_then(|id| self.trains.get(&id))
            .map_or(&a.line, |t| &self.tracks[&t.track].line)
    }

    fn train_has_open_alert(&self, id: TrainId) -> bool {
        self.alerts
            .iter()
            .any(|a| a.is_open() && a.train == Some(id))
    }
}
