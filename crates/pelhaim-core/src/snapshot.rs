use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::event::Ts;
use crate::ids::{StationId, TrackId, TrainId};
use crate::model::{Alert, Decision, Fork, KeepAwakeStatus, LineKey, Station, Track, Train};

#[derive(Clone, PartialEq, Serialize, Deserialize, Debug)]
pub struct Snapshot {
    pub version: u64,
    pub now: Ts,
    pub lines: Vec<LineView>,
    pub tracks: BTreeMap<TrackId, Track>,
    pub stations: BTreeMap<StationId, Station>,
    pub trains: BTreeMap<TrainId, Train>,
    pub forks: Vec<Fork>,
    pub alerts: Vec<Alert>,
    pub decisions_recent: Vec<Decision>,
    pub decisions_unexported: u32,
    pub keep_awake: KeepAwakeStatus,
    /// The local offset arrival clocks are shown in.
    pub utc_offset_secs: i32,
    /// `[board].max_stations`: the board folds a line's merged stations to this many.
    #[serde(default = "default_max_stations")]
    pub max_stations: u32,
}

fn default_max_stations() -> u32 {
    5
}

#[derive(Clone, PartialEq, Serialize, Deserialize, Debug)]
pub struct LineView {
    pub key: LineKey,
    pub name: String,
    pub tracks: Vec<TrackId>,
    pub open_decisions: u32,
    pub format_untested: bool,
    pub has_open_alert: bool,
    pub active: bool,
    /// DISPATCH only: active trains descended from an orchestrator session.
    pub running: u32,
    /// `[[project]].color`, unparsed: the board owns the line palette.
    #[serde(default)]
    pub color: Option<String>,
}
