use std::fs::OpenOptions;
use std::io::Write;
use std::time::Duration;

use pelhaim_adapters::{HomeWatcher, WatchSignal};
use tokio::time::{Instant, timeout_at};

#[tokio::test]
async fn watcher_reports_change_and_polls() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("root");
    let sub = root.join("project");
    std::fs::create_dir_all(&sub).unwrap();
    let file = sub.join("s.jsonl");
    std::fs::write(&file, b"a\n").unwrap();
    let missing = dir.path().join("not-there");

    let (_watcher, mut rx) =
        HomeWatcher::start(&[missing, root.clone()], Duration::from_millis(200)).unwrap();
    // Let the backend arm its stream before the write it must see.
    tokio::time::sleep(Duration::from_millis(100)).await;
    OpenOptions::new()
        .append(true)
        .open(&file)
        .unwrap()
        .write_all(b"b\n")
        .unwrap();

    let deadline = Instant::now() + Duration::from_secs(2);
    let (mut changed, mut polled) = (false, false);
    while !(changed && polled) {
        match timeout_at(deadline, rx.recv()).await {
            Ok(Some(WatchSignal::Changed(p))) => changed |= p == file,
            Ok(Some(WatchSignal::Poll)) => polled = true,
            Ok(None) => panic!("watch channel closed"),
            Err(_) => panic!("within 2s: changed={changed} polled={polled}"),
        }
    }
}

#[test]
fn no_existing_roots_still_polls() {
    let dir = tempfile::tempdir().unwrap();
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .unwrap();
    rt.block_on(async {
        let (_w, mut rx) =
            HomeWatcher::start(&[dir.path().join("nope")], Duration::from_millis(50)).unwrap();
        let got = tokio::time::timeout(Duration::from_secs(2), rx.recv()).await;
        assert!(matches!(got, Ok(Some(WatchSignal::Poll))));
    });
}

#[test]
fn dropping_the_watcher_closes_the_channel() {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .unwrap();
    rt.block_on(async {
        let (w, mut rx) = HomeWatcher::start(&[], Duration::from_millis(50)).unwrap();
        drop(w);
        let end = tokio::time::timeout(Duration::from_secs(2), async {
            while rx.recv().await.is_some() {}
        })
        .await;
        assert!(end.is_ok(), "channel stayed open after drop");
    });
}
