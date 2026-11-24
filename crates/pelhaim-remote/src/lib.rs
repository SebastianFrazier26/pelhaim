mod compact;

use pelhaim_core::{AlertEvent, AlertId, Answer, AnswerOutcome, Origin};

pub use compact::{
    COMPACT_VERSION, CompactAlert, CompactBoard, CompactLine, CompactTrain, compact,
};

/// The one surface the local UI and a future phone share (spec §8.7.1). Answers
/// carry their `Origin`, and the first answer to an alert wins.
///
/// `answer` and `dismiss` block the calling thread until the engine replies.
/// Never call them from inside a tokio worker (an async task or `block_on`):
/// tokio panics there with "Cannot block the current thread from within a
/// runtime". Call them from a plain thread, or from `spawn_blocking`.
pub trait RemoteApi: Send + Sync {
    fn subscribe_alerts(&self) -> tokio::sync::broadcast::Receiver<AlertEvent>;
    fn compact_snapshot(&self) -> CompactBoard;
    /// An unknown alert id is `Ok(AnswerOutcome::UnknownAlert)` (F46).
    fn answer(&self, a: Answer) -> Result<AnswerOutcome, RemoteError>;
    fn dismiss(&self, alert_id: AlertId, origin: Origin) -> Result<(), RemoteError>;
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RemoteError {
    EngineStopped,
    /// Only `dismiss` returns this.
    UnknownAlert,
}

impl std::fmt::Display for RemoteError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            RemoteError::EngineStopped => "the engine has stopped",
            RemoteError::UnknownAlert => "no such alert",
        })
    }
}

impl std::error::Error for RemoteError {}
