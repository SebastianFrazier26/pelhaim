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

pub use engine::{
    AlertEvent, Answer, AnswerOutcome, Choice, Command, Effect, Engine, EngineConfig, Input,
    KeepAwakeAction, LogError, OwnerEvent, ProjectCfg, RestoredState,
};
pub use event::{Event, QOption, Question, Tool, Ts};
pub use ids::{AlertId, DecisionId, ForkId, SessionKey, StationId, TrackId, TrainId};
pub use model::{
    Alert, AlertKind, AlertState, AnsweredBy, AssignOption, AssignTarget, Counters, Decision, Fork,
    KeepAwakeStatus, LEG_BLOCKS, LineKey, Origin, Position, RuntimeRef, Station, StationStatus,
    TagResolution, Track, Train, TrainState, Who,
};
pub use number::{format_train_number, train_number};
pub use snapshot::{LineView, Snapshot};
pub use tag::{DISPATCH_SLUG, TagParse, closest_slug, is_valid_slug, levenshtein, parse_tag};
pub use text::{sanitize_line, title_from_prompt, truncate_chars};
