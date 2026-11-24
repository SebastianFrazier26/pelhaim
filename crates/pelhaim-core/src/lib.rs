pub mod engine;
pub mod event;
pub mod ids;
pub mod model;
pub mod number;
pub mod snapshot;
pub mod tag;
#[cfg(any(test, feature = "testkit"))]
pub mod testkit;
pub mod text;

pub use engine::{Command, Effect, Engine, EngineConfig, Input, ProjectCfg, RestoredState};
pub use event::{Event, QOption, Question, Tool, Ts};
pub use ids::{AlertId, DecisionId, ForkId, SessionKey, StationId, TrackId, TrainId};
pub use model::{
    Alert, AssignTarget, Counters, Decision, Fork, KeepAwakeStatus, LEG_BLOCKS, LineKey, Position,
    RuntimeRef, Station, StationStatus, TagResolution, Track, Train, TrainState,
};
pub use number::{format_train_number, train_number};
pub use snapshot::{LineView, Snapshot};
pub use tag::{DISPATCH_SLUG, TagParse, closest_slug, is_valid_slug, levenshtein, parse_tag};
pub use text::{sanitize_line, title_from_prompt, truncate_chars};
