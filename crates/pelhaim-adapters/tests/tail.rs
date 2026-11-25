use std::fs::{self, OpenOptions};
use std::io::Write;

use pelhaim_adapters::{TailRead, Tailer};

fn append(path: &std::path::Path, bytes: &[u8]) {
    let mut f = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .unwrap();
    f.write_all(bytes).unwrap();
}

fn lines(read: TailRead) -> Vec<Vec<u8>> {
    match read {
        TailRead::Lines(l) => l,
        other => panic!("expected Lines, got {other:?}"),
    }
}

#[test]
fn partial_last_line_waits_for_newline() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("s.jsonl");
    append(&path, b"a\nb");
    let mut t = Tailer::new(path.clone());
    assert_eq!(lines(t.read_new().unwrap()), vec![b"a".to_vec()]);
    assert_eq!(lines(t.read_new().unwrap()), Vec::<Vec<u8>>::new());
    append(&path, b"c\n");
    assert_eq!(lines(t.read_new().unwrap()), vec![b"bc".to_vec()]);
    assert_eq!(lines(t.read_new().unwrap()), Vec::<Vec<u8>>::new());
}

#[test]
fn crlf_stripped() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("s.jsonl");
    append(&path, b"one\r\ntwo\r\r\n\r\nthree\n");
    let mut t = Tailer::new(path);
    assert_eq!(
        lines(t.read_new().unwrap()),
        vec![
            b"one".to_vec(),
            b"two\r".to_vec(),
            b"".to_vec(),
            b"three".to_vec()
        ]
    );
}

#[test]
fn truncated_file_restarts_from_zero() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("s.jsonl");
    append(&path, b"a\nb\n");
    let mut t = Tailer::new(path.clone());
    assert_eq!(
        lines(t.read_new().unwrap()),
        vec![b"a".to_vec(), b"b".to_vec()]
    );
    OpenOptions::new()
        .write(true)
        .truncate(true)
        .open(&path)
        .unwrap();
    append(&path, b"x\n");
    assert!(matches!(t.read_new().unwrap(), TailRead::Reset));
    assert_eq!(lines(t.read_new().unwrap()), vec![b"x".to_vec()]);
    assert_eq!(lines(t.read_new().unwrap()), Vec::<Vec<u8>>::new());
}

#[cfg(unix)]
#[test]
fn replaced_file_resets() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("s.jsonl");
    append(&path, b"a\n");
    let mut t = Tailer::new(path.clone());
    assert_eq!(lines(t.read_new().unwrap()), vec![b"a".to_vec()]);

    // Longer than the old file, so only the inode change can reveal the swap.
    let other = dir.path().join("new.jsonl");
    append(&other, b"first\nsecond\n");
    fs::rename(&other, &path).unwrap();

    assert!(matches!(t.read_new().unwrap(), TailRead::Reset));
    assert_eq!(
        lines(t.read_new().unwrap()),
        vec![b"first".to_vec(), b"second".to_vec()]
    );
    assert_eq!(lines(t.read_new().unwrap()), Vec::<Vec<u8>>::new());
}

#[test]
fn deleted_file_is_gone() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("s.jsonl");
    let mut never = Tailer::new(dir.path().join("missing.jsonl"));
    assert!(matches!(never.read_new().unwrap(), TailRead::Gone));

    append(&path, b"a\n");
    let mut t = Tailer::new(path.clone());
    assert_eq!(lines(t.read_new().unwrap()), vec![b"a".to_vec()]);
    fs::remove_file(&path).unwrap();
    assert!(matches!(t.read_new().unwrap(), TailRead::Gone));
    assert!(matches!(t.read_new().unwrap(), TailRead::Gone));
}

#[cfg(unix)]
#[test]
fn recreated_after_delete_resets_before_rereading() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("s.jsonl");
    append(&path, b"a\n");
    let mut t = Tailer::new(path.clone());
    assert_eq!(lines(t.read_new().unwrap()), vec![b"a".to_vec()]);
    // Keep the old inode alive so the new file cannot reuse its number.
    let hold = dir.path().join("hold.jsonl");
    fs::hard_link(&path, &hold).unwrap();
    fs::remove_file(&path).unwrap();
    assert!(matches!(t.read_new().unwrap(), TailRead::Gone));
    append(&path, b"b\nc\n");
    assert!(matches!(t.read_new().unwrap(), TailRead::Reset));
    assert_eq!(
        lines(t.read_new().unwrap()),
        vec![b"b".to_vec(), b"c".to_vec()]
    );
}

fn read_all(t: &mut Tailer) -> Vec<Vec<Vec<u8>>> {
    let mut calls = Vec::new();
    loop {
        let got = lines(t.read_new().unwrap());
        if got.is_empty() {
            return calls;
        }
        calls.push(got);
    }
}

#[test]
fn file_larger_than_cap_is_consumed_over_several_calls() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("s.jsonl");
    let expected: Vec<Vec<u8>> = (0..100)
        .map(|i| format!("record-{i:03}").into_bytes())
        .collect();
    for rec in &expected {
        append(&path, rec);
        append(&path, b"\n");
    }
    // Each line is 11 bytes; a 50-byte cap fits exactly 4 per call.
    let mut t = Tailer::with_read_cap(path, 50);
    let calls = read_all(&mut t);
    assert!(
        calls.iter().all(|c| c.len() == 4),
        "cap overrun: {:?}",
        calls.iter().map(Vec::len).collect::<Vec<_>>()
    );
    assert_eq!(calls.len(), 25);
    assert_eq!(calls.concat(), expected);
}

#[test]
fn partial_trailing_line_is_held_across_capped_reads() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("s.jsonl");
    append(&path, b"aaaa\nbbbb\ncccc\ndd");
    let mut t = Tailer::with_read_cap(path.clone(), 10);
    assert_eq!(
        read_all(&mut t).concat(),
        vec![b"aaaa".to_vec(), b"bbbb".to_vec(), b"cccc".to_vec()]
    );
    assert_eq!(lines(t.read_new().unwrap()), Vec::<Vec<u8>>::new());
    append(&path, b"dd\r\neeee\n");
    assert_eq!(
        read_all(&mut t).concat(),
        vec![b"dddd".to_vec(), b"eeee".to_vec()]
    );
}

#[test]
fn line_longer_than_cap_is_returned_whole() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("s.jsonl");
    let long = vec![b'x'; 64];
    append(&path, &long);
    append(&path, b"\nshort\n");
    let mut t = Tailer::with_read_cap(path, 8);
    assert_eq!(read_all(&mut t), vec![vec![long], vec![b"short".to_vec()]]);
}

#[test]
fn default_cap_is_four_mib() {
    assert_eq!(pelhaim_adapters::READ_CAP, 4 * 1024 * 1024);
}
