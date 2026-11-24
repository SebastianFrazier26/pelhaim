use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::event::Ts;
use crate::ids::{AlertId, ForkId, SessionKey, StationId, TrackId, TrainId};

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

// Stubs: Task 4 replaces Alert, Decision and KeepAwakeStatus with the full types.
#[derive(Clone, PartialEq, Serialize, Deserialize, Debug)]
pub struct Alert {
    pub id: AlertId,
    pub train: Option<TrainId>,
    pub line: LineKey,
}

impl Alert {
    pub fn is_open(&self) -> bool {
        true
    }
}

#[derive(Clone, PartialEq, Serialize, Deserialize, Debug)]
pub struct Decision {}

#[derive(Clone, PartialEq, Serialize, Deserialize, Debug, Default)]
pub struct KeepAwakeStatus {}
