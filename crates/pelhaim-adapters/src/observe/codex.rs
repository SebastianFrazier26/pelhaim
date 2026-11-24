use std::path::PathBuf;

use pelhaim_core::{Event, SessionKey, Tool, Ts, parse_tag, title_from_prompt};
use serde_json::Value;

use super::version::{CODEX_TESTED, is_untested};
use super::{str_at, transcript_tail};

/// Records held before `session_meta`; past this a file without one is not a rollout.
const PENDING_MAX: usize = 4096;

/// Harness-injected user messages: instructions and context the CLI adds,
/// and its notices after an abort or a subagent finish. None is the owner's prompt.
const NOT_PROMPT_PREFIXES: &[&str] = &[
    "# AGENTS.md instructions",
    "<environment_context>",
    "<user_instructions>",
    "<turn_aborted>",
    "<subagent_notification>",
    "<user_action>",
];

/// The VS Code extension wraps the typed prompt in IDE context; the self-tag
/// is the first line after the request header.
const IDE_CONTEXT: &str = "# Context from my IDE setup:";
const IDE_REQUEST: &str = "## My request for Codex:";

struct Held {
    record: Value,
    timed: bool,
    at: Ts,
}

/// Parses one Codex rollout file, one complete line at a time. The session key
/// comes from its first `session_meta`, so everything before that is held.
pub struct CodexFileParser {
    path: PathBuf,
    fallback_ts: Ts,
    key: Option<SessionKey>,
    skipped: u64,
    last_at: Option<Ts>,
    pending: Vec<Held>,
    prompted: bool,
}

fn input_text(content: Option<&Value>, kind: &str) -> String {
    content
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter(|i| i.get("type").and_then(Value::as_str) == Some(kind))
                .filter_map(|i| i.get("text").and_then(Value::as_str))
                .collect::<Vec<_>>()
                .join("\n")
        })
        .unwrap_or_default()
}

/// The owner's own words, or `None` for a harness-injected message.
fn owner_prompt(text: &str) -> Option<&str> {
    let head = text.trim_start();
    if head.is_empty() || NOT_PROMPT_PREFIXES.iter().any(|p| head.starts_with(p)) {
        return None;
    }
    if head.starts_with(IDE_CONTEXT)
        && let Some((_, request)) = head.split_once(IDE_REQUEST)
    {
        return Some(request.trim_start_matches(['\r', '\n']));
    }
    Some(text)
}

impl CodexFileParser {
    pub fn new(path: PathBuf, fallback_ts: Ts) -> Self {
        Self {
            path,
            fallback_ts,
            key: None,
            skipped: 0,
            last_at: None,
            pending: Vec::new(),
            prompted: false,
        }
    }

    pub fn key(&self) -> Option<&SessionKey> {
        self.key.as_ref()
    }

    pub fn skipped(&self) -> u64 {
        self.skipped
    }

    /// `line` is one complete record, without its newline.
    pub fn feed(&mut self, line: &[u8]) -> Vec<Event> {
        if line.trim_ascii().is_empty() {
            return Vec::new();
        }
        let record = match serde_json::from_slice::<Value>(line) {
            Ok(v @ Value::Object(_)) => v,
            _ => {
                self.skipped += 1;
                return Vec::new();
            }
        };
        let ts = str_at(&record, &["timestamp"]).and_then(|s| s.parse::<Ts>().ok());
        let at = ts.or(self.last_at).unwrap_or(self.fallback_ts);
        if ts.is_some() {
            self.last_at = ts;
        }
        let held = Held {
            record,
            timed: ts.is_some(),
            at,
        };
        let Some(key) = self.key.clone() else {
            return self.discover(held);
        };
        self.events(&key, &held)
    }

    fn discover(&mut self, held: Held) -> Vec<Event> {
        let payload = held.record.get("payload").unwrap_or(&Value::Null);
        let id = (str_at(&held.record, &["type"]) == Some("session_meta"))
            .then(|| str_at(payload, &["id"]))
            .flatten()
            .filter(|id| !id.is_empty());
        let Some(id) = id else {
            if self.pending.len() < PENDING_MAX {
                self.pending.push(held);
            } else {
                self.skipped += 1;
            }
            return Vec::new();
        };
        let key = SessionKey::codex(id);
        let version = str_at(payload, &["cli_version"]).map(str::to_string);
        let mut out = vec![Event::SessionDiscovered {
            key: key.clone(),
            tool: Tool::Codex,
            parent: None,
            path: self.path.clone(),
            cwd: str_at(payload, &["cwd"]).map(str::to_string),
            version: version.clone(),
            started: str_at(payload, &["timestamp"])
                .and_then(|s| s.parse::<Ts>().ok())
                .unwrap_or(held.at),
        }];
        if let Some(v) = version
            && is_untested(&v, CODEX_TESTED)
        {
            out.push(Event::FormatUntested {
                key: key.clone(),
                version: v,
            });
        }
        self.key = Some(key.clone());
        for early in std::mem::take(&mut self.pending) {
            out.extend(self.events(&key, &early));
        }
        out.extend(self.events(&key, &held));
        out
    }

    fn events(&mut self, key: &SessionKey, held: &Held) -> Vec<Event> {
        let mut events = self.map(key, &held.record, held.at);
        let timed = |e: &Event| e.at().is_some() && e.session() == Some(key);
        if held.timed && !events.iter().any(timed) {
            events.push(Event::Activity {
                key: key.clone(),
                at: held.at,
            });
        }
        events
    }

    fn map(&mut self, key: &SessionKey, record: &Value, at: Ts) -> Vec<Event> {
        let payload = record.get("payload").unwrap_or(&Value::Null);
        let key = key.clone();
        let call_id = || str_at(payload, &["call_id"]).map(str::to_string);
        match (str_at(record, &["type"]), str_at(payload, &["type"])) {
            (Some("response_item"), Some("message")) => {
                let content = payload.get("content");
                match str_at(payload, &["role"]) {
                    Some("user") => self.user(key, &input_text(content, "input_text"), at),
                    Some("assistant") => transcript_tail(&key, &input_text(content, "output_text")),
                    _ => Vec::new(),
                }
            }
            (Some("response_item"), Some("function_call" | "custom_tool_call")) => call_id()
                .map(|call_id| Event::ToolOpened {
                    key,
                    call_id,
                    name: str_at(payload, &["name"]).unwrap_or("").to_string(),
                    at,
                })
                .into_iter()
                .collect(),
            (Some("response_item"), Some("function_call_output" | "custom_tool_call_output")) => {
                call_id()
                    .map(|call_id| Event::ToolClosed {
                        key,
                        call_id,
                        // Codex has no background launches whose result is only an acknowledgement.
                        launched_only: false,
                        at,
                    })
                    .into_iter()
                    .collect()
            }
            (Some("event_msg"), Some("task_started")) => vec![Event::WorkStarted { key, at }],
            (Some("event_msg"), Some("task_complete")) => vec![Event::TurnEnded {
                key,
                final_text: str_at(payload, &["last_agent_message"])
                    .filter(|t| !t.is_empty())
                    .map(str::to_string),
                at,
            }],
            (Some("event_msg"), Some("turn_aborted")) => vec![Event::Failed {
                key,
                reason: "aborted".to_string(),
                at,
            }],
            _ => Vec::new(),
        }
    }

    fn user(&mut self, key: SessionKey, text: &str, at: Ts) -> Vec<Event> {
        let Some(prompt) = owner_prompt(text) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        if !self.prompted {
            self.prompted = true;
            out.push(Event::SessionTag {
                key: key.clone(),
                tag: parse_tag(prompt),
            });
        }
        out.push(Event::PromptStarted {
            key,
            title: title_from_prompt(prompt),
            at,
        });
        out
    }
}
