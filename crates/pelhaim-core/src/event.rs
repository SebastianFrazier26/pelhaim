use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::ids::{DecisionId, SessionKey};
use crate::tag::TagParse;

pub type Ts = chrono::DateTime<chrono::Utc>;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Debug)]
pub enum Tool {
    Claude,
    Codex,
}

#[derive(Clone, PartialEq, Serialize, Deserialize, Debug)]
pub struct Question {
    pub header: String,
    pub question: String,
    pub multi_select: bool,
    pub options: Vec<QOption>,
}

#[derive(Clone, PartialEq, Serialize, Deserialize, Debug)]
pub struct QOption {
    pub label: String,
    pub description: String,
}

#[derive(Clone, PartialEq, Serialize, Deserialize, Debug)]
pub enum Event {
    SessionDiscovered {
        key: SessionKey,
        tool: Tool,
        parent: Option<SessionKey>,
        path: PathBuf,
        cwd: Option<String>,
        version: Option<String>,
        started: Ts,
    },
    FormatUntested {
        key: SessionKey,
        version: String,
    },
    TitleChanged {
        key: SessionKey,
        title: String,
    },
    /// The session's own first real user message.
    SessionTag {
        key: SessionKey,
        tag: TagParse,
    },
    SpawnRequested {
        parent: SessionKey,
        tool_use_id: String,
        description: String,
        tag: TagParse,
        at: Ts,
    },
    SubagentLinked {
        child: SessionKey,
        parent: SessionKey,
        tool_use_id: Option<String>,
        description: Option<String>,
    },
    PromptStarted {
        key: SessionKey,
        title: String,
        at: Ts,
    },
    ToolOpened {
        key: SessionKey,
        call_id: String,
        name: String,
        at: Ts,
    },
    ToolClosed {
        key: SessionKey,
        call_id: String,
        /// The result is only a background Agent/Task launch acknowledgement,
        /// not the subagent's finish.
        launched_only: bool,
        at: Ts,
    },
    QuestionAsked {
        key: SessionKey,
        call_id: String,
        questions: Vec<Question>,
        at: Ts,
    },
    QuestionAnswered {
        key: SessionKey,
        call_id: String,
        answer: String,
        at: Ts,
    },
    TurnEnded {
        key: SessionKey,
        final_text: Option<String>,
        at: Ts,
    },
    WorkStarted {
        key: SessionKey,
        at: Ts,
    },
    SubagentFinished {
        key: SessionKey,
        at: Ts,
    },
    Failed {
        key: SessionKey,
        reason: String,
        at: Ts,
    },
    /// Any other record: counts as a write.
    Activity {
        key: SessionKey,
        at: Ts,
    },
    /// Memory only; at most 20 lines are kept per train.
    TranscriptTail {
        key: SessionKey,
        line: String,
    },
    SessionReset {
        key: SessionKey,
    },
    SessionGone {
        key: SessionKey,
    },
    CatchUpDone {
        tool: Tool,
    },
    DecisionExported {
        id: DecisionId,
        at: Ts,
    },
    DecisionExportFailed {
        id: DecisionId,
        reason: String,
    },
    /// An owner event the store could not persist; `kind` names the event.
    StoreWriteFailed {
        kind: String,
        reason: String,
    },
    /// Keyed by project slug.
    OpenDecisionCounts {
        counts: BTreeMap<String, u32>,
    },
    KeepAwakeEnv {
        mechanism: Option<String>,
        on_battery: Option<bool>,
    },
}

impl Event {
    pub fn at(&self) -> Option<Ts> {
        use Event::*;
        match self {
            SessionDiscovered { started: at, .. }
            | SpawnRequested { at, .. }
            | PromptStarted { at, .. }
            | ToolOpened { at, .. }
            | ToolClosed { at, .. }
            | QuestionAsked { at, .. }
            | QuestionAnswered { at, .. }
            | TurnEnded { at, .. }
            | WorkStarted { at, .. }
            | SubagentFinished { at, .. }
            | Failed { at, .. }
            | Activity { at, .. }
            | DecisionExported { at, .. } => Some(*at),
            _ => None,
        }
    }

    pub fn session(&self) -> Option<&SessionKey> {
        use Event::*;
        match self {
            SessionDiscovered { key, .. }
            | FormatUntested { key, .. }
            | TitleChanged { key, .. }
            | SessionTag { key, .. }
            | PromptStarted { key, .. }
            | ToolOpened { key, .. }
            | ToolClosed { key, .. }
            | QuestionAsked { key, .. }
            | QuestionAnswered { key, .. }
            | TurnEnded { key, .. }
            | WorkStarted { key, .. }
            | SubagentFinished { key, .. }
            | Failed { key, .. }
            | Activity { key, .. }
            | TranscriptTail { key, .. }
            | SessionReset { key }
            | SessionGone { key } => Some(key),
            SpawnRequested { parent, .. } => Some(parent),
            SubagentLinked { child, .. } => Some(child),
            _ => None,
        }
    }
}
