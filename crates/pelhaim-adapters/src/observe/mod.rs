mod claude;
mod codex;
mod discover;
mod version;

use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use async_trait::async_trait;
use pelhaim_core::{Event, SessionKey, Tool, Ts, parse_milestones, sanitize_line, truncate_chars};
use serde_json::Value;
use tokio::sync::mpsc;
use tokio::time::{Instant, MissedTickBehavior, interval_at};

use crate::{
    AdapterCommand, AdapterKind, Capabilities, HomeWatcher, SessionAdapter, TailRead, Tailer,
    WatchSignal,
};

pub use claude::{ClaudeFileParser, parse_meta};
pub use codex::CodexFileParser;
pub use discover::{ClaudeFile, discover_claude, discover_codex};
pub use version::{CLAUDE_TESTED, CODEX_TESTED, is_untested};

const TAIL_MAX: usize = 120;

fn str_at<'a>(v: &'a Value, path: &[&str]) -> Option<&'a str> {
    path.iter().try_fold(v, |v, k| v.get(k))?.as_str()
}

/// §7.10: one event per marker line, read from assistant text and real prompts only.
fn milestones(key: &SessionKey, text: &str, at: Ts) -> impl Iterator<Item = Event> {
    let key = key.clone();
    parse_milestones(text)
        .into_iter()
        .map(move |m| Event::Milestone {
            key: key.clone(),
            name: m.name,
            estimate_secs: m.estimate_secs,
            at,
        })
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ObserveOptions {
    pub home: PathBuf,
    pub active_window: Duration,
    pub poll: Duration,
    pub sweep: Duration,
    /// Catch up, send `CatchUpDone` and return, without watching.
    pub once: bool,
}

impl ObserveOptions {
    pub fn new(home: PathBuf, active_window: Duration) -> Self {
        Self {
            home,
            active_window,
            // 1 s rather than the spec's 2 s, so an alert lands within 2 s even when notify drops the event.
            poll: Duration::from_secs(1),
            sweep: Duration::from_secs(10),
            once: false,
        }
    }
}

const OBSERVE: Capabilities = Capabilities {
    can_answer: false,
    can_take_over: false,
    can_spawn: false,
    read_only: true,
};

pub struct ObserveClaude {
    opts: ObserveOptions,
}

impl ObserveClaude {
    pub fn new(opts: ObserveOptions) -> Self {
        Self { opts }
    }
}

#[async_trait]
impl SessionAdapter for ObserveClaude {
    fn kind(&self) -> AdapterKind {
        AdapterKind::ObserveClaude
    }

    fn capabilities(&self) -> Capabilities {
        OBSERVE
    }

    async fn run(
        self: Box<Self>,
        tx: mpsc::Sender<Event>,
        rx: mpsc::Receiver<AdapterCommand>,
    ) -> anyhow::Result<()> {
        Observer::<ClaudeSource>::run(self.opts, tx, rx).await
    }
}

pub struct ObserveCodex {
    opts: ObserveOptions,
}

impl ObserveCodex {
    pub fn new(opts: ObserveOptions) -> Self {
        Self { opts }
    }
}

#[async_trait]
impl SessionAdapter for ObserveCodex {
    fn kind(&self) -> AdapterKind {
        AdapterKind::ObserveCodex
    }

    fn capabilities(&self) -> Capabilities {
        OBSERVE
    }

    async fn run(
        self: Box<Self>,
        tx: mpsc::Sender<Event>,
        rx: mpsc::Receiver<AdapterCommand>,
    ) -> anyhow::Result<()> {
        Observer::<CodexSource>::run(self.opts, tx, rx).await
    }
}

/// F27: what differs between tools; `Observer` is the one catch-up, watch and sweep loop.
trait SessionSource {
    type File: Send;
    type Parser: Send;
    const TOOL: Tool;
    fn root(home: &Path) -> PathBuf;
    fn discover(home: &Path, window: Duration, now: SystemTime) -> Vec<Self::File>;
    fn path(file: &Self::File) -> &Path;
    fn parser(file: &Self::File, fallback_ts: Ts) -> Self::Parser;
    fn feed(parser: &mut Self::Parser, line: &[u8]) -> Vec<Event>;
    fn meta_path(_file: &Self::File) -> Option<PathBuf> {
        None
    }
    fn meta_events(_file: &Self::File, _parser: &Self::Parser, _json: &[u8]) -> Vec<Event> {
        Vec::new()
    }
}

struct ClaudeSource;

impl SessionSource for ClaudeSource {
    type File = ClaudeFile;
    type Parser = ClaudeFileParser;
    const TOOL: Tool = Tool::Claude;

    fn root(home: &Path) -> PathBuf {
        home.join("projects")
    }

    fn discover(home: &Path, window: Duration, now: SystemTime) -> Vec<ClaudeFile> {
        discover_claude(home, window, now)
    }

    fn path(file: &ClaudeFile) -> &Path {
        &file.path
    }

    fn parser(file: &ClaudeFile, fallback_ts: Ts) -> ClaudeFileParser {
        ClaudeFileParser::new(
            file.key.clone(),
            file.path.clone(),
            file.parent.clone(),
            fallback_ts,
        )
    }

    fn feed(parser: &mut ClaudeFileParser, line: &[u8]) -> Vec<Event> {
        parser.feed(line)
    }

    /// Discovery only names a meta file that already exists; one written later must still be seen.
    fn meta_path(file: &ClaudeFile) -> Option<PathBuf> {
        file.parent.as_ref()?;
        Some(
            file.meta
                .clone()
                .unwrap_or_else(|| file.path.with_extension("meta.json")),
        )
    }

    fn meta_events(file: &ClaudeFile, parser: &ClaudeFileParser, json: &[u8]) -> Vec<Event> {
        let Some(parent) = &file.parent else {
            return Vec::new();
        };
        let at = parser
            .last_at()
            .unwrap_or_else(|| Ts::from(SystemTime::now()));
        parse_meta(&file.key, parent, json, at)
    }
}

struct CodexSource;

impl SessionSource for CodexSource {
    type File = PathBuf;
    type Parser = CodexFileParser;
    const TOOL: Tool = Tool::Codex;

    fn root(home: &Path) -> PathBuf {
        home.join("sessions")
    }

    fn discover(home: &Path, window: Duration, now: SystemTime) -> Vec<PathBuf> {
        discover_codex(home, window, now)
    }

    fn path(file: &PathBuf) -> &Path {
        file
    }

    fn parser(file: &PathBuf, fallback_ts: Ts) -> CodexFileParser {
        CodexFileParser::new(file.clone(), fallback_ts)
    }

    fn feed(parser: &mut CodexFileParser, line: &[u8]) -> Vec<Event> {
        parser.feed(line)
    }
}

struct Tracked<S: SessionSource> {
    file: S::File,
    tailer: Tailer,
    parser: S::Parser,
    /// The key last announced from this file. It outlives a reset so `SessionReset` and
    /// `SessionGone` can name it; a file that never announces one is never reported.
    known: Option<SessionKey>,
    /// This parser has announced its session, so a meta link has a train to attach to.
    announced: bool,
    meta_seen: Option<Vec<u8>>,
}

impl<S: SessionSource> Tracked<S> {
    fn open(file: S::File) -> Self {
        let path = S::path(&file).to_path_buf();
        Self {
            parser: S::parser(&file, fallback_ts(&path)),
            tailer: Tailer::new(path),
            file,
            known: None,
            announced: false,
            meta_seen: None,
        }
    }

    fn feed(&mut self, lines: &[Vec<u8>]) -> Vec<Event> {
        let mut events = Vec::new();
        for line in lines {
            events.extend(S::feed(&mut self.parser, line));
        }
        for e in &events {
            if let Event::SessionDiscovered { key, .. } = e {
                self.known = Some(key.clone());
                self.announced = true;
            }
        }
        events
    }

    /// F15: a fresh parser re-reads from 0; the engine dedupes the replay.
    fn reset(&mut self) -> Vec<Event> {
        let path = S::path(&self.file).to_path_buf();
        self.parser = S::parser(&self.file, fallback_ts(&path));
        self.announced = false;
        self.meta_seen = None;
        self.known
            .iter()
            .map(|key| Event::SessionReset { key: key.clone() })
            .collect()
    }

    /// Re-read whenever its bytes change: `finished: true` there is the subagent's Done signal.
    fn meta_events(&mut self) -> Vec<Event> {
        if !self.announced {
            return Vec::new();
        }
        let Some(json) = S::meta_path(&self.file).and_then(|p| fs::read(p).ok()) else {
            return Vec::new();
        };
        if self.meta_seen.as_ref() == Some(&json) {
            return Vec::new();
        }
        let events = S::meta_events(&self.file, &self.parser, &json);
        self.meta_seen = Some(json);
        events
    }
}

fn fallback_ts(path: &Path) -> Ts {
    let mtime = fs::metadata(path).and_then(|m| m.modified());
    Ts::from(mtime.unwrap_or_else(|_| SystemTime::now()))
}

/// The event receiver is gone, so there is no one left to observe for.
struct Closed;

struct Observer<S: SessionSource> {
    opts: ObserveOptions,
    tx: mpsc::Sender<Event>,
    // Ordered by path, so a main transcript drains before its own `<sid>/subagents/` files.
    files: BTreeMap<PathBuf, Tracked<S>>,
}

impl<S: SessionSource> Observer<S> {
    async fn run(
        opts: ObserveOptions,
        tx: mpsc::Sender<Event>,
        mut rx: mpsc::Receiver<AdapterCommand>,
    ) -> anyhow::Result<()> {
        let mut obs = Observer::<S> {
            opts,
            tx,
            files: BTreeMap::new(),
        };
        let caught_up = match obs.sweep().await {
            Ok(()) => obs.emit(vec![Event::CatchUpDone { tool: S::TOOL }]).await,
            Err(Closed) => Err(Closed),
        };
        if caught_up.is_err() || obs.opts.once {
            return Ok(());
        }

        let (_watcher, mut signals) =
            HomeWatcher::start(&[S::root(&obs.opts.home)], obs.opts.poll)?;
        let mut sweep = interval_at(Instant::now() + obs.opts.sweep, obs.opts.sweep);
        sweep.set_missed_tick_behavior(MissedTickBehavior::Delay);
        loop {
            let step = tokio::select! {
                biased;
                cmd = rx.recv() => match cmd {
                    Some(AdapterCommand::Refresh) => obs.sweep().await,
                    // A dropped command sender leaves no way to stop the adapter later.
                    Some(AdapterCommand::Shutdown) | None => return Ok(()),
                },
                signal = signals.recv() => match signal {
                    Some(WatchSignal::Changed(path)) => obs.changed(&path).await,
                    Some(WatchSignal::Poll) => obs.poll().await,
                    None => anyhow::bail!("home watcher stopped"),
                },
                _ = sweep.tick() => obs.sweep().await,
            };
            if step.is_err() {
                return Ok(());
            }
        }
    }

    async fn emit(&self, events: Vec<Event>) -> Result<(), Closed> {
        for e in events {
            // F44: wait for capacity; a dropped event would leave a phantom alert.
            self.tx.send(e).await.map_err(|_| Closed)?;
        }
        Ok(())
    }

    /// Starts tailing files that entered the window, oldest first, and drops those that left it.
    async fn sweep(&mut self) -> Result<(), Closed> {
        let found = S::discover(&self.opts.home, self.opts.active_window, SystemTime::now());
        let live: HashSet<PathBuf> = found.iter().map(|f| S::path(f).to_path_buf()).collect();
        for file in found {
            let path = S::path(&file).to_path_buf();
            if self.files.contains_key(&path) {
                continue;
            }
            self.files.insert(path.clone(), Tracked::open(file));
            self.drain(&path).await?;
        }
        let stale: Vec<PathBuf> = self
            .files
            .keys()
            .filter(|p| !live.contains(*p))
            .cloned()
            .collect();
        for path in stale {
            self.untrack(&path).await?;
        }
        Ok(())
    }

    async fn poll(&mut self) -> Result<(), Closed> {
        let paths: Vec<PathBuf> = self.files.keys().cloned().collect();
        for path in paths {
            self.drain(&path).await?;
        }
        Ok(())
    }

    async fn changed(&mut self, path: &Path) -> Result<(), Closed> {
        if self.files.contains_key(path) {
            return self.drain(path).await;
        }
        let owner = self
            .files
            .iter()
            .find(|(_, t)| S::meta_path(&t.file).as_deref() == Some(path))
            .map(|(p, _)| p.clone());
        match owner {
            Some(owner) => self.drain(&owner).await,
            None => Ok(()),
        }
    }

    /// Reads until caught up: one `read_new` returns at most `READ_CAP` bytes.
    async fn drain(&mut self, path: &Path) -> Result<(), Closed> {
        loop {
            let Some(t) = self.files.get_mut(path) else {
                return Ok(());
            };
            let events = match t.tailer.read_new() {
                Ok(TailRead::Lines(lines)) if lines.is_empty() => break,
                Ok(TailRead::Lines(lines)) => t.feed(&lines),
                Ok(TailRead::Reset) => t.reset(),
                Ok(TailRead::Gone) => return self.untrack(path).await,
                // Unreadable for now (permissions, a racing rename); the next poll retries.
                Err(_) => break,
            };
            self.emit(events).await?;
        }
        let events = self
            .files
            .get_mut(path)
            .map(Tracked::meta_events)
            .unwrap_or_default();
        self.emit(events).await
    }

    async fn untrack(&mut self, path: &Path) -> Result<(), Closed> {
        let Some(key) = self.files.remove(path).and_then(|t| t.known) else {
            return Ok(());
        };
        // Another tracked file may name the same session; while it lives, so does the train.
        if self.files.values().any(|t| t.known.as_ref() == Some(&key)) {
            return Ok(());
        }
        self.emit(vec![Event::SessionGone { key }]).await
    }
}
