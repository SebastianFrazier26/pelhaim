use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use pelhaim_core::{
    Event, QOption, Question, SessionKey, TagParse, Tool, Ts, parse_tag, sanitize_line,
    title_from_prompt,
};
use serde_json::Value;

use super::version::{CLAUDE_TESTED, is_untested};
use super::{str_at, transcript_tail};

const LAUNCH_ACK: &str = "Async agent launched";

struct OpenCall {
    spawn: bool,
    background: bool,
}

/// Parses one Claude Code transcript (main or subagent), one complete line at a time.
pub struct ClaudeFileParser {
    key: SessionKey,
    path: PathBuf,
    parent: Option<SessionKey>,
    fallback_ts: Ts,
    skipped: u64,
    last_at: Option<Ts>,
    started: Option<Ts>,
    // F23: events wait here until the first timestamp lets SessionDiscovered go out.
    pending: Vec<Event>,
    cwd: Option<String>,
    version: Option<String>,
    announced: (bool, bool),
    open: HashMap<String, OpenCall>,
    questions: HashSet<String>,
    prompted: bool,
    /// Agent ids of this session's background launches, until a task notification finishes them.
    launched: HashSet<String>,
}

fn text_blocks(blocks: &[Value]) -> impl Iterator<Item = &str> {
    blocks
        .iter()
        .filter(|b| b.get("type").and_then(Value::as_str) == Some("text"))
        .filter_map(|b| b.get("text").and_then(Value::as_str))
}

/// A `tool_result` or user `content`: a string, or the joined `text` blocks.
fn content_text(content: Option<&Value>) -> String {
    match content {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Array(blocks)) => text_blocks(blocks).collect::<Vec<_>>().join("\n"),
        _ => String::new(),
    }
}

fn first_text(content: Option<&Value>) -> Option<&str> {
    match content? {
        Value::String(s) => Some(s),
        Value::Array(blocks) => text_blocks(blocks).next(),
        _ => None,
    }
}

fn clean(v: Option<&Value>) -> String {
    sanitize_line(v.and_then(Value::as_str).unwrap_or(""))
}

fn questions(input: &Value) -> Vec<Question> {
    let Some(qs) = input.get("questions").and_then(Value::as_array) else {
        return Vec::new();
    };
    qs.iter()
        .map(|q| Question {
            header: clean(q.get("header")),
            question: clean(q.get("question")),
            multi_select: q.get("multiSelect").and_then(Value::as_bool) == Some(true),
            options: q
                .get("options")
                .and_then(Value::as_array)
                .map(|opts| {
                    opts.iter()
                        .map(|o| QOption {
                            label: clean(o.get("label")),
                            description: clean(o.get("description")),
                        })
                        .collect()
                })
                .unwrap_or_default(),
        })
        .collect()
}

/// Harness-injected user records: subagent completions, `!` shell echoes and
/// slash-command output. None of them is the owner starting a prompt.
const NOT_PROMPT_PREFIXES: &[&str] = &[
    "<task-notification>",
    "<bash-input>",
    "<bash-stdout>",
    "<bash-stderr>",
    "<local-command",
    "<command-",
];

fn is_real_prompt(record: &Value, text: &str) -> bool {
    let flag = |k: &str| record.get(k).and_then(Value::as_bool) == Some(true);
    let human_origin = record
        .get("origin")
        .and_then(|o| o.get("kind"))
        .is_none_or(|k| k.as_str() == Some("human"));
    let head = text.trim_start();
    !flag("isMeta")
        && !flag("isCompactSummary")
        && human_origin
        && !head.is_empty()
        && !NOT_PROMPT_PREFIXES.iter().any(|p| head.starts_with(p))
}

impl ClaudeFileParser {
    pub fn new(
        key: SessionKey,
        path: PathBuf,
        parent: Option<SessionKey>,
        fallback_ts: Ts,
    ) -> Self {
        Self {
            key,
            path,
            parent,
            fallback_ts,
            skipped: 0,
            last_at: None,
            started: None,
            pending: Vec::new(),
            cwd: None,
            version: None,
            announced: (false, false),
            open: HashMap::new(),
            questions: HashSet::new(),
            prompted: false,
            launched: HashSet::new(),
        }
    }

    pub fn skipped(&self) -> u64 {
        self.skipped
    }

    /// The last record timestamp seen, which `parse_meta` takes as `at`.
    pub fn last_at(&self) -> Option<Ts> {
        self.last_at
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
        if self.cwd.is_none() {
            self.cwd = str_at(&record, &["cwd"]).map(str::to_string);
        }
        if self.version.is_none() {
            self.version = str_at(&record, &["version"]).map(str::to_string);
        }

        let mut events = self.map(&record, at);
        let timed = |e: &Event| e.at().is_some() && e.session() == Some(&self.key);
        if ts.is_some() && !events.iter().any(timed) {
            events.push(Event::Activity {
                key: self.key.clone(),
                at,
            });
        }

        let mut out = Vec::new();
        if self.started.is_none() {
            let Some(started) = ts else {
                self.pending.extend(events);
                return out;
            };
            self.started = Some(started);
            self.announce(&mut out);
            out.append(&mut self.pending);
        } else if self.announced != (self.cwd.is_some(), self.version.is_some()) {
            // F15: a repeat SessionDiscovered only fills in cwd and version.
            self.announce(&mut out);
        }
        out.extend(events);
        out
    }

    fn announce(&mut self, out: &mut Vec<Event>) {
        let Some(started) = self.started else {
            return;
        };
        out.push(Event::SessionDiscovered {
            key: self.key.clone(),
            tool: Tool::Claude,
            parent: self.parent.clone(),
            path: self.path.clone(),
            cwd: self.cwd.clone(),
            version: self.version.clone(),
            started,
        });
        if !self.announced.1
            && let Some(v) = &self.version
            && is_untested(v, CLAUDE_TESTED)
        {
            out.push(Event::FormatUntested {
                key: self.key.clone(),
                version: v.clone(),
            });
        }
        self.announced = (self.cwd.is_some(), self.version.is_some());
    }

    fn map(&mut self, record: &Value, at: Ts) -> Vec<Event> {
        match str_at(record, &["type"]) {
            Some("ai-title") => {
                let title = clean(record.get("aiTitle"));
                if title.is_empty() {
                    return Vec::new();
                }
                vec![Event::TitleChanged {
                    key: self.key.clone(),
                    title,
                }]
            }
            Some("user") => self.user(record, at),
            Some("assistant") => self.assistant(record, at),
            Some("system") => match str_at(record, &["subtype"]) {
                Some(sub) if sub.contains("error") => vec![Event::Failed {
                    key: self.key.clone(),
                    reason: sub.to_string(),
                    at,
                }],
                _ => Vec::new(),
            },
            _ => Vec::new(),
        }
    }

    fn user(&mut self, record: &Value, at: Ts) -> Vec<Event> {
        let content = record.get("message").and_then(|m| m.get("content"));
        let results: Vec<&Value> = content
            .and_then(Value::as_array)
            .map(|blocks| {
                blocks
                    .iter()
                    .filter(|b| b.get("type").and_then(Value::as_str) == Some("tool_result"))
                    .collect()
            })
            .unwrap_or_default();
        if !results.is_empty() {
            let record_agent = (results.len() == 1)
                .then(|| str_at(record, &["toolUseResult", "agentId"]))
                .flatten();
            return results
                .into_iter()
                .flat_map(|r| self.tool_result(r, record_agent, at))
                .collect();
        }

        let text = content_text(content);
        if !is_real_prompt(record, &text) {
            return self.task_notification(&text, at).into_iter().collect();
        }
        let mut out = Vec::new();
        let first = !self.prompted;
        self.prompted = true;
        if first {
            out.push(Event::SessionTag {
                key: self.key.clone(),
                tag: parse_tag(&text),
            });
        }
        if self.parent.is_none() {
            out.push(Event::PromptStarted {
                key: self.key.clone(),
                title: title_from_prompt(&text),
                at,
            });
        }
        out
    }

    /// Fallback finish for a background child (meta `finished` is primary); the
    /// engine ignores it for a child already Done. Notifications for ids this
    /// session never launched, such as background shell tasks, are dropped.
    fn task_notification(&mut self, text: &str, at: Ts) -> Option<Event> {
        let body = text.trim_start().strip_prefix("<task-notification>")?;
        let (_, rest) = body.split_once("<task-id>")?;
        let (id, _) = rest.split_once("</task-id>")?;
        if !self.launched.remove(id.trim()) {
            return None;
        }
        Some(Event::SubagentFinished {
            key: SessionKey::claude_sub(self.key.session_id(), id.trim()),
            at,
        })
    }

    fn tool_result(&mut self, block: &Value, record_agent: Option<&str>, at: Ts) -> Vec<Event> {
        let Some(id) = block.get("tool_use_id").and_then(Value::as_str) else {
            return Vec::new();
        };
        let content = block.get("content");
        let mut out = Vec::new();
        if self.questions.remove(id) {
            out.push(Event::QuestionAnswered {
                key: self.key.clone(),
                call_id: id.to_string(),
                answer: content_text(content),
                at,
            });
        }
        let launched_only = self.open.remove(id).is_some_and(|call| {
            call.spawn
                && (call.background
                    || first_text(content).is_some_and(|t| t.starts_with(LAUNCH_ACK)))
        });
        if launched_only {
            let from_text = || {
                content_text(content)
                    .lines()
                    .find_map(|l| l.trim().strip_prefix("agentId:"))
                    // On disk the id is followed by usage prose on the same line.
                    .and_then(|rest| rest.split_whitespace().next())
                    .map(str::to_string)
            };
            if let Some(agent) = record_agent.map(str::to_string).or_else(from_text)
                && !agent.is_empty()
            {
                self.launched.insert(agent);
            }
        }
        out.push(Event::ToolClosed {
            key: self.key.clone(),
            call_id: id.to_string(),
            launched_only,
            at,
        });
        out
    }

    fn assistant(&mut self, record: &Value, at: Ts) -> Vec<Event> {
        let message = record.get("message");
        let blocks = message
            .and_then(|m| m.get("content"))
            .and_then(Value::as_array)
            .map(Vec::as_slice)
            .unwrap_or_default();
        let mut out = Vec::new();
        for block in blocks {
            match block.get("type").and_then(Value::as_str) {
                Some("tool_use") => self.tool_use(block, at, &mut out),
                Some("text") => {
                    let text = block.get("text").and_then(Value::as_str).unwrap_or("");
                    out.extend(transcript_tail(&self.key, text));
                }
                _ => {}
            }
        }
        let stop = message.and_then(|m| str_at(m, &["stop_reason"]));
        if matches!(stop, Some("end_turn" | "stop_sequence")) {
            let text = text_blocks(blocks).collect::<Vec<_>>().join("\n");
            out.push(Event::TurnEnded {
                key: self.key.clone(),
                final_text: (!text.is_empty()).then_some(text),
                at,
            });
        }
        out
    }

    fn tool_use(&mut self, block: &Value, at: Ts, out: &mut Vec<Event>) {
        let Some(id) = block.get("id").and_then(Value::as_str) else {
            return;
        };
        let name = block.get("name").and_then(Value::as_str).unwrap_or("");
        let input = block.get("input").unwrap_or(&Value::Null);
        let spawn = matches!(name, "Agent" | "Task");
        self.open.insert(
            id.to_string(),
            OpenCall {
                spawn,
                background: input.get("run_in_background").and_then(Value::as_bool) == Some(true),
            },
        );
        out.push(Event::ToolOpened {
            key: self.key.clone(),
            call_id: id.to_string(),
            name: name.to_string(),
            at,
        });
        if name == "AskUserQuestion" {
            self.questions.insert(id.to_string());
            out.push(Event::QuestionAsked {
                key: self.key.clone(),
                call_id: id.to_string(),
                questions: questions(input),
                at,
            });
        } else if spawn {
            out.push(Event::SpawnRequested {
                parent: self.key.clone(),
                tool_use_id: id.to_string(),
                description: clean(input.get("description")),
                tag: input
                    .get("prompt")
                    .and_then(Value::as_str)
                    .map_or(TagParse::Absent, parse_tag),
                at,
            });
        }
    }
}

pub fn parse_meta(child: &SessionKey, parent: &SessionKey, json: &[u8], at: Ts) -> Vec<Event> {
    let Ok(meta @ Value::Object(_)) = serde_json::from_slice::<Value>(json) else {
        return Vec::new();
    };
    let mut out = vec![Event::SubagentLinked {
        child: child.clone(),
        parent: parent.clone(),
        tool_use_id: str_at(&meta, &["toolUseId"]).map(str::to_string),
        description: str_at(&meta, &["description"]).map(sanitize_line),
    }];
    if meta.get("finished").and_then(Value::as_bool) == Some(true) {
        out.push(Event::SubagentFinished {
            key: child.clone(),
            at,
        });
    }
    out
}
