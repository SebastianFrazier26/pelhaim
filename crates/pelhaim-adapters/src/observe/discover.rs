use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use pelhaim_core::SessionKey;

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct ClaudeFile {
    pub key: SessionKey,
    pub path: PathBuf,
    pub parent: Option<SessionKey>,
    pub meta: Option<PathBuf>,
}

fn entries(dir: &Path) -> Vec<PathBuf> {
    fs::read_dir(dir)
        .map(|rd| rd.filter_map(|e| e.ok().map(|e| e.path())).collect())
        .unwrap_or_default()
}

fn file_mtime(path: &Path) -> Option<SystemTime> {
    let meta = fs::metadata(path).ok()?;
    meta.is_file().then(|| meta.modified().ok()).flatten()
}

fn stem_with<'a>(path: &'a Path, prefix: &str, ext: &str) -> Option<&'a str> {
    let name = path.file_name()?.to_str()?;
    let stem = name.strip_prefix(prefix)?.strip_suffix(ext)?;
    (!stem.is_empty()).then_some(stem)
}

struct Found {
    file: ClaudeFile,
    mtime: SystemTime,
    sid: String,
}

fn subagents(project: &Path, sid: &str) -> Vec<Found> {
    let dir = project.join(sid).join("subagents");
    entries(&dir)
        .into_iter()
        .filter_map(|path| {
            let agent = stem_with(&path, "agent-", ".jsonl")?.to_string();
            let mtime = file_mtime(&path)?;
            let meta = dir.join(format!("agent-{agent}.meta.json"));
            Some(Found {
                file: ClaudeFile {
                    key: SessionKey::claude_sub(sid, &agent),
                    path,
                    parent: Some(SessionKey::claude(sid)),
                    meta: meta.is_file().then_some(meta),
                },
                mtime,
                sid: sid.to_string(),
            })
        })
        .collect()
}

/// Mains come first, then subagents, each group by mtime ascending, so a
/// parent is always discovered before its children.
pub fn discover_claude(home: &Path, window: Duration, now: SystemTime) -> Vec<ClaudeFile> {
    // An mtime in the future counts as fresh.
    let fresh = |t: SystemTime| now.duration_since(t).ok().is_none_or(|age| age <= window);
    let mut mains = Vec::new();
    let mut subs = Vec::new();
    for project in entries(&home.join("projects")) {
        for path in entries(&project) {
            if path.is_dir() {
                if let Some(sid) = path.file_name().and_then(|n| n.to_str()) {
                    subs.extend(subagents(&project, sid));
                }
                continue;
            }
            let Some(sid) = stem_with(&path, "", ".jsonl") else {
                continue;
            };
            let Some(mtime) = file_mtime(&path) else {
                continue;
            };
            let sid = sid.to_string();
            mains.push(Found {
                file: ClaudeFile {
                    key: SessionKey::claude(&sid),
                    path,
                    parent: None,
                    meta: None,
                },
                mtime,
                sid,
            });
        }
    }
    subs.retain(|s| fresh(s.mtime));
    // F26: a parent stays silent while its long subagent runs.
    let busy: BTreeSet<&str> = subs.iter().map(|s| s.sid.as_str()).collect();
    mains.retain(|m| fresh(m.mtime) || busy.contains(m.sid.as_str()));
    let order = |f: &Found| (f.mtime, f.file.key.clone());
    mains.sort_by_key(order);
    subs.sort_by_key(order);
    mains.into_iter().chain(subs).map(|f| f.file).collect()
}
