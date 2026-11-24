#![allow(dead_code)]

use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use pelhaim_adapters::observe::ObserveOptions;
use pelhaim_adapters::{AdapterCommand, SessionAdapter};
use pelhaim_core::Event;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio::time::{Instant, timeout_at};

pub fn fixture(tool: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(tool)
}

pub fn copy_tree(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for entry in fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let dest = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_tree(&entry.path(), &dest);
        } else {
            fs::copy(entry.path(), &dest).unwrap();
        }
    }
}

pub fn files_under(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for entry in fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            out.extend(files_under(&path));
        } else {
            out.push(path);
        }
    }
    out.sort();
    out
}

pub fn set_mtime(path: &Path, t: SystemTime) {
    File::options()
        .append(true)
        .open(path)
        .unwrap()
        .set_modified(t)
        .unwrap();
}

/// F25: checked-in fixtures are older than the active window, so a live home is a copy touched to now.
pub fn live_home(tool: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    copy_tree(&fixture(tool), dir.path());
    let now = SystemTime::now();
    for f in files_under(dir.path()) {
        set_mtime(&f, now);
    }
    dir
}

pub fn opts(home: &Path) -> ObserveOptions {
    ObserveOptions::new(home.to_path_buf(), Duration::from_secs(48 * 3600))
}

pub struct Running {
    pub events: mpsc::Receiver<Event>,
    pub commands: mpsc::Sender<AdapterCommand>,
    pub task: JoinHandle<anyhow::Result<()>>,
    pub seen: Vec<Event>,
}

impl Running {
    pub fn start(adapter: Box<dyn SessionAdapter>, capacity: usize) -> Running {
        let (tx, events) = mpsc::channel(capacity);
        let (commands, rx) = mpsc::channel(8);
        let task = tokio::spawn(adapter.run(tx, rx));
        Running {
            events,
            commands,
            task,
            seen: Vec::new(),
        }
    }

    /// Waits for the first event matching `pred`, keeping everything received in `seen`.
    pub async fn wait_for(&mut self, within: Duration, pred: impl Fn(&Event) -> bool) -> Event {
        let deadline = Instant::now() + within;
        loop {
            match timeout_at(deadline, self.events.recv()).await {
                Ok(Some(e)) => {
                    self.seen.push(e.clone());
                    if pred(&e) {
                        return e;
                    }
                }
                Ok(None) => panic!("adapter stopped; seen {:#?}", self.seen),
                Err(_) => panic!("no matching event within {within:?}; seen {:#?}", self.seen),
            }
        }
    }

    pub async fn caught_up(&mut self) {
        self.wait_for(Duration::from_secs(10), |e| {
            matches!(e, Event::CatchUpDone { .. })
        })
        .await;
    }

    /// Collects whatever arrives within `d` without failing.
    pub async fn collect_for(&mut self, d: Duration) -> Vec<Event> {
        let deadline = Instant::now() + d;
        let mut got = Vec::new();
        while let Ok(Some(e)) = timeout_at(deadline, self.events.recv()).await {
            self.seen.push(e.clone());
            got.push(e);
        }
        got
    }

    pub async fn shutdown(self) {
        self.commands.send(AdapterCommand::Shutdown).await.unwrap();
        let Running {
            mut events, task, ..
        } = self;
        // Keep draining so a blocked send cannot stop the adapter reaching the command.
        let drain = tokio::spawn(async move { while events.recv().await.is_some() {} });
        let res = tokio::time::timeout(Duration::from_secs(5), task)
            .await
            .expect("adapter did not stop after Shutdown")
            .unwrap();
        res.unwrap();
        drain.await.unwrap();
    }
}

/// Runs a `once` adapter to completion and returns every event it sent.
pub async fn run_once(adapter: Box<dyn SessionAdapter>) -> Vec<Event> {
    let (tx, mut rx) = mpsc::channel(16);
    let (_commands, crx) = mpsc::channel(1);
    let task = tokio::spawn(adapter.run(tx, crx));
    let mut out = Vec::new();
    while let Some(e) = rx.recv().await {
        out.push(e);
    }
    task.await.unwrap().unwrap();
    out
}

pub fn append(path: &Path, line: &str) {
    use std::io::Write;
    let mut f = File::options().append(true).open(path).unwrap();
    f.write_all(line.as_bytes()).unwrap();
    f.write_all(b"\n").unwrap();
}
