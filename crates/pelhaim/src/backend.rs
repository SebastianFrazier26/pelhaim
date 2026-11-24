use std::path::{Path, PathBuf};
use std::sync::Arc;

use pelhaim_board::UiAction;
use pelhaim_core::{Answer, Command, Input, Origin, Snapshot};
use pelhaim_gpu::UiBackend;
use pelhaim_remote::RemoteApi;

use crate::engine_task::EngineHandle;

const PAUSE_KEEP_AWAKE: chrono::TimeDelta = chrono::TimeDelta::hours(1);

/// The window's side of the engine. It runs on the UI thread, outside the runtime,
/// which `RemoteApi`'s blocking calls require.
pub struct Backend {
    handle: EngineHandle,
    rt: tokio::runtime::Handle,
}

impl Backend {
    pub fn new(handle: EngineHandle, rt: tokio::runtime::Handle) -> Backend {
        Backend { handle, rt }
    }

    fn command(&self, c: Command) {
        self.handle.send(Input::Command(c));
    }
}

impl UiBackend for Backend {
    fn snapshot(&self) -> Arc<Snapshot> {
        self.handle.snapshot()
    }

    fn perform(&self, a: UiAction) {
        let at = chrono::Utc::now();
        match a {
            UiAction::Answer(answer) => {
                let local = Answer {
                    origin: Origin::Local,
                    ..answer
                };
                match self.handle.answer(local) {
                    Ok(outcome) => tracing::debug!(?outcome, "local answer"),
                    Err(e) => tracing::warn!(error = %e, "local answer failed"),
                }
            }
            UiAction::Dismiss(alert) => {
                if let Err(e) = self.handle.dismiss(alert, Origin::Local) {
                    tracing::warn!(error = %e, "dismiss failed");
                }
            }
            UiAction::Reopen(alert) => self.command(Command::Reopen { alert, at }),
            UiAction::LogDecision { alert, project } => {
                self.command(Command::LogDecision { alert, project, at })
            }
            UiAction::ExportPending => self.command(Command::ExportPending),
            UiAction::PauseKeepAwake => self.command(Command::PauseKeepAwake {
                until: at + PAUSE_KEEP_AWAKE,
            }),
            UiAction::CopyToClipboard(text) => {
                if let Err(e) = arboard::Clipboard::new().and_then(|mut c| c.set_text(text)) {
                    tracing::warn!(error = %e, "clipboard copy failed");
                }
            }
            UiAction::Reveal(p) => match p.to_str().and_then(reveal_target) {
                Some(dir) => reveal(&dir),
                None => tracing::debug!("reveal skipped: not an existing directory"),
            },
            UiAction::Redraw => {}
        }
    }

    fn set_waker(&self, wake: Box<dyn Fn() + Send + Sync>) {
        let mut rx = self.handle.watch();
        self.rt.spawn(async move {
            while rx.changed().await.is_ok() {
                wake();
            }
        });
    }
}

/// Argv only, never a shell. macOS uses the absolute `open` for the same reason as caffeinate:
/// an earlier `open` on PATH must not stand in for the system one.
fn reveal(dir: &Path) {
    let mut cmd = if cfg!(target_os = "macos") {
        let mut c = std::process::Command::new("/usr/bin/open");
        c.arg("-R");
        c
    } else {
        std::process::Command::new("xdg-open")
    };
    match cmd.arg(dir).spawn() {
        Ok(mut child) => {
            std::thread::spawn(move || child.wait());
        }
        Err(e) => tracing::warn!(error = %e, "reveal failed"),
    }
}

/// F38: the board emits `Reveal(cwd)` raw; only an existing directory is revealed, by its
/// canonical path, so a stale or crafted cwd never reaches `open -R` / `xdg-open`.
pub fn reveal_target(cwd: &str) -> Option<PathBuf> {
    let p = std::fs::canonicalize(cwd).ok()?;
    p.is_dir().then_some(p)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keep_awake::Holder;
    use crate::store_worker::StoreWorker;
    use pelhaim_core::Engine;
    use pelhaim_core::testkit::cfg;
    use pelhaim_store::Store;
    use std::time::{Duration, Instant};

    #[test]
    fn reveal_only_existing_dirs() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("f.txt");
        std::fs::write(&file, "x").unwrap();
        assert_eq!(reveal_target(file.to_str().unwrap()), None);
        assert_eq!(
            reveal_target(dir.path().join("missing").to_str().unwrap()),
            None
        );
        let got = reveal_target(dir.path().to_str().unwrap()).unwrap();
        assert_eq!(got, std::fs::canonicalize(dir.path()).unwrap());
        assert!(got.is_absolute());
    }

    #[test]
    fn pause_keep_awake_is_one_hour_and_waker_fires() {
        let dir = tempfile::tempdir().unwrap();
        let rt = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .enable_all()
            .build()
            .unwrap();
        let store = Store::open(&dir.path().join("pelhaim.db")).unwrap();
        let (tx, rx) = crate::engine_task::input_channel();
        let worker = StoreWorker::spawn(store, dir.path().join("log.md"), Vec::new(), tx.clone());
        let engine = Engine::new(cfg(&["ethica"]), Default::default());
        let handle = EngineHandle::spawn(
            rt.handle(),
            engine,
            worker,
            Holder::new(None, std::process::id(), false),
            (tx, rx),
        );
        let b = Backend::new(handle, rt.handle().clone());
        let woken = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let flag = woken.clone();
        b.set_waker(Box::new(move || {
            flag.store(true, std::sync::atomic::Ordering::SeqCst)
        }));

        let before = chrono::Utc::now();
        b.perform(UiAction::PauseKeepAwake);
        let deadline = Instant::now() + Duration::from_secs(5);
        let until = loop {
            if let Some(u) = b.snapshot().keep_awake.paused_until {
                break u;
            }
            assert!(Instant::now() < deadline, "pause never reached the engine");
            std::thread::sleep(Duration::from_millis(10));
        };
        assert!(until >= before + PAUSE_KEEP_AWAKE);
        assert!(until <= chrono::Utc::now() + PAUSE_KEEP_AWAKE);
        while !woken.load(std::sync::atomic::Ordering::SeqCst) {
            assert!(Instant::now() < deadline, "waker never fired");
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}
