use std::path::{Path, PathBuf};
use std::sync::mpsc as std_mpsc;
use std::thread;
use std::time::Duration;

use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use tokio::sync::mpsc;

const CHANNEL_CAPACITY: usize = 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WatchSignal {
    Changed(PathBuf),
    Poll,
}

/// Keeps the file watches and the poll ticker alive; dropping it stops both and closes the channel.
pub struct HomeWatcher {
    _watcher: RecommendedWatcher,
    _stop_poll: std_mpsc::Sender<()>,
}

impl HomeWatcher {
    pub fn start(
        roots: &[PathBuf],
        poll: Duration,
    ) -> notify::Result<(HomeWatcher, mpsc::Receiver<WatchSignal>)> {
        let (tx, rx) = mpsc::channel(CHANNEL_CAPACITY);

        let existing: Vec<(PathBuf, PathBuf)> = roots
            .iter()
            .filter_map(|root| {
                let canonical = root.canonicalize().ok()?;
                Some((root.clone(), canonical))
            })
            .collect();

        let event_tx = tx.clone();
        let mapping = existing.clone();
        let mut watcher =
            notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
                let Ok(event) = res else { return };
                if matches!(event.kind, EventKind::Access(_)) {
                    return;
                }
                for path in event.paths {
                    // A full channel drops the signal: the poll tick re-reads every tailed file anyway.
                    let _ = event_tx.try_send(WatchSignal::Changed(as_given(&mapping, path)));
                }
            })?;
        for (_, canonical) in &existing {
            watcher.watch(canonical, RecursiveMode::Recursive)?;
        }

        let (stop_tx, stop_rx) = std_mpsc::channel::<()>();
        thread::Builder::new()
            .name("pelhaim-poll".into())
            .spawn(move || {
                while let Err(std_mpsc::RecvTimeoutError::Timeout) = stop_rx.recv_timeout(poll) {
                    if let Err(mpsc::error::TrySendError::Closed(_)) =
                        tx.try_send(WatchSignal::Poll)
                    {
                        break;
                    }
                }
            })
            .map_err(notify::Error::io)?;

        Ok((
            HomeWatcher {
                _watcher: watcher,
                _stop_poll: stop_tx,
            },
            rx,
        ))
    }
}

/// Backends such as FSEvents report resolved paths (`/private/var/...` for `/var/...`);
/// map them back under the root as the caller spelled it so they match the caller's own paths.
fn as_given(roots: &[(PathBuf, PathBuf)], path: PathBuf) -> PathBuf {
    roots
        .iter()
        .find_map(|(given, canonical)| {
            path.strip_prefix(canonical)
                .ok()
                .map(|rest| join(given, rest))
        })
        .unwrap_or(path)
}

fn join(base: &Path, rest: &Path) -> PathBuf {
    if rest.as_os_str().is_empty() {
        base.to_path_buf()
    } else {
        base.join(rest)
    }
}
