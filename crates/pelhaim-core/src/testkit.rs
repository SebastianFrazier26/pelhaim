use std::path::PathBuf;

use crate::engine::{Engine, EngineConfig, Input, ProjectCfg, RestoredState};
use crate::event::{Event, Tool, Ts};
use crate::ids::SessionKey;

pub fn ts(s: &str) -> Ts {
    chrono::DateTime::parse_from_rfc3339(s)
        .unwrap_or_else(|e| panic!("bad RFC 3339 timestamp {s:?}: {e}"))
        .to_utc()
}

pub fn cfg(slugs: &[&str]) -> EngineConfig {
    EngineConfig {
        projects: slugs
            .iter()
            .enumerate()
            .map(|(i, s)| ProjectCfg {
                slug: s.to_string(),
                name: s.to_uppercase(),
                order: i as i32,
            })
            .collect(),
        ..EngineConfig::default()
    }
}

pub fn engine(slugs: &[&str]) -> Engine {
    Engine::new(cfg(slugs), RestoredState::default())
}

/// A Claude main session with session id `key`.
pub fn discover(key: &str, at: &str) -> Input {
    Input::Event(Event::SessionDiscovered {
        key: SessionKey::claude(key),
        tool: Tool::Claude,
        parent: None,
        path: PathBuf::from(format!("/fixtures/claude/{key}.jsonl")),
        cwd: None,
        version: None,
        started: ts(at),
    })
}
