mod claude;
mod codex;
mod discover;
mod version;

use pelhaim_core::{Event, SessionKey, sanitize_line, truncate_chars};
use serde_json::Value;

pub use claude::{ClaudeFileParser, parse_meta};
pub use codex::CodexFileParser;
pub use discover::{ClaudeFile, discover_claude, discover_codex};
pub use version::{CLAUDE_TESTED, CODEX_TESTED, is_untested};

const TAIL_MAX: usize = 120;

fn str_at<'a>(v: &'a Value, path: &[&str]) -> Option<&'a str> {
    path.iter().try_fold(v, |v, k| v.get(k))?.as_str()
}

fn transcript_tail(key: &SessionKey, text: &str) -> Vec<Event> {
    text.lines()
        .map(sanitize_line)
        .filter(|l| !l.is_empty())
        .map(|line| Event::TranscriptTail {
            key: key.clone(),
            line: truncate_chars(&line, TAIL_MAX),
        })
        .collect()
}
