use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::event::{Question, Ts};
use crate::ids::{AlertId, DecisionId, ForkId, SessionKey, StationId, TrackId, TrainId};

pub const LEG_BLOCKS: u16 = 10;

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Debug)]
pub enum LineKey {
    Dispatch,
    Project(String),
    Unsorted,
}

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Debug)]
pub enum AssignTarget {
    Project(String),
    Dispatch,
}

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Debug)]
pub enum TagResolution {
    Tagged(String),
    Untagged,
    MisTagged(String),
    Assigned(AssignTarget),
}

#[derive(Clone, PartialEq, Serialize, Deserialize, Debug)]
pub enum TrainState {
    Idle,
    Working,
    Waiting(AlertId),
    ProbablyWaiting,
    Failed(String),
    Done,
    Stalled,
}

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Debug)]
pub enum StationStatus {
    Active,
    Done,
}

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Debug, Default)]
pub struct Position {
    pub leg: u16,
    pub block: u16,
    pub berthed: bool,
}

#[derive(Clone, PartialEq, Serialize, Deserialize, Debug)]
pub enum RuntimeRef {
    ObservedClaude {
        path: PathBuf,
        session_id: String,
        agent_id: Option<String>,
    },
    ObservedCodex {
        path: PathBuf,
        session_id: String,
    },
}

#[derive(Clone, PartialEq, Serialize, Deserialize, Debug)]
pub struct Counters {
    pub tool_calls: u32,
    pub turns: u32,
    pub started: Ts,
    pub last_seen: Ts,
}

#[derive(Clone, PartialEq, Serialize, Deserialize, Debug)]
pub struct Track {
    pub id: TrackId,
    pub line: LineKey,
    pub session: SessionKey,
    pub stations: Vec<StationId>,
    pub origin: Option<ForkId>,
    pub last_activity: Ts,
}

#[derive(Clone, PartialEq, Serialize, Deserialize, Debug)]
pub struct Station {
    pub id: StationId,
    pub track: TrackId,
    pub title: String,
    pub status: StationStatus,
}

#[derive(Clone, PartialEq, Serialize, Deserialize, Debug)]
pub struct Train {
    pub id: TrainId,
    pub number: u16,
    pub track: TrackId,
    pub session: SessionKey,
    pub position: Position,
    pub state: TrainState,
    pub tag: TagResolution,
    pub runtime: RuntimeRef,
    pub parent: Option<TrainId>,
    pub label: String,
    pub counters: Counters,
    pub cwd: Option<String>,
    pub version_untested: bool,
    #[serde(skip)]
    pub tail: Vec<String>,
}

#[derive(Clone, PartialEq, Serialize, Deserialize, Debug)]
pub struct Fork {
    pub id: ForkId,
    pub parent_track: TrackId,
    pub at: Position,
    pub child_track: TrackId,
}

#[derive(Clone, PartialEq, Serialize, Deserialize, Debug)]
pub enum Origin {
    Local,
    Remote { device_id: String },
}

#[derive(Clone, PartialEq, Serialize, Deserialize, Debug)]
pub enum AnsweredBy {
    Local,
    Remote { device_id: String },
    Terminal,
}

impl From<Origin> for AnsweredBy {
    fn from(o: Origin) -> Self {
        match o {
            Origin::Local => AnsweredBy::Local,
            Origin::Remote { device_id } => AnsweredBy::Remote { device_id },
        }
    }
}

#[derive(Clone, PartialEq, Serialize, Deserialize, Debug)]
pub struct AssignOption {
    pub label: String,
    pub target: AssignTarget,
}

#[derive(Clone, PartialEq, Serialize, Deserialize, Debug)]
pub enum AlertKind {
    Question {
        questions: Vec<Question>,
    },
    Result {
        summary: String,
    },
    Stalled,
    AssignProject {
        session: SessionKey,
        options: Vec<AssignOption>,
        prefill: Option<usize>,
    },
    Failure {
        reason: String,
    },
}

impl AlertKind {
    pub fn tab(&self) -> &'static str {
        match self {
            AlertKind::Question { .. } => "Question",
            AlertKind::Result { .. } => "Result",
            AlertKind::Stalled => "Stalled",
            AlertKind::AssignProject { .. } => "Assign",
            AlertKind::Failure { .. } => "Failure",
        }
    }

    pub fn remote_answerable(&self) -> bool {
        matches!(
            self,
            AlertKind::Question { .. } | AlertKind::AssignProject { .. }
        )
    }
}

#[derive(Clone, PartialEq, Serialize, Deserialize, Debug)]
pub enum AlertState {
    Open,
    Tray,
    Answered {
        answer: String,
        by: AnsweredBy,
        at: Ts,
    },
    Expired,
}

#[derive(Clone, PartialEq, Serialize, Deserialize, Debug)]
pub struct Alert {
    pub id: AlertId,
    pub train: Option<TrainId>,
    pub line: LineKey,
    pub kind: AlertKind,
    pub created: Ts,
    pub state: AlertState,
    pub answerable: bool,
    pub decision: Option<DecisionId>,
}

impl Alert {
    /// Still waiting on the owner: shown as a popup or parked in the tray.
    pub fn is_open(&self) -> bool {
        matches!(self.state, AlertState::Open | AlertState::Tray)
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Debug)]
pub enum Who {
    Owner,
    AgentDefaultOwnerConfirmed,
    AgentDefaultUnconfirmed,
}

impl Who {
    pub fn as_log_str(&self) -> &'static str {
        match self {
            Who::Owner => "owner",
            Who::AgentDefaultOwnerConfirmed => "agent default, owner-confirmed",
            Who::AgentDefaultUnconfirmed => "agent default, not yet confirmed",
        }
    }
}

#[derive(Clone, PartialEq, Serialize, Deserialize, Debug)]
pub struct Decision {
    pub id: DecisionId,
    pub project: String,
    pub date: chrono::NaiveDate,
    pub title: String,
    pub why: String,
    pub who: Who,
    pub origin: Origin,
    pub source_alert: Option<AlertId>,
    pub written_to_md: bool,
}

#[derive(Clone, PartialEq, Serialize, Deserialize, Debug, Default)]
pub struct KeepAwakeStatus {
    pub enabled: bool,
    pub held: bool,
    pub count: u32,
    pub paused_until: Option<Ts>,
    pub mechanism: Option<String>,
    pub on_battery: Option<bool>,
}
