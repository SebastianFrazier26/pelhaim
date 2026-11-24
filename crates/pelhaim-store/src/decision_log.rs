use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use pelhaim_core::{Decision, DecisionId, sanitize_line};
use sha2::{Digest, Sha256};

const OPEN_HEADING: &str = "### Open decisions (need you)";
const DECIDED_HEADING: &str = "### Decided";
const STANDING_RULES: &str = "## Standing rules";

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct LogProject {
    pub slug: String,
    pub name: String,
    /// The `## ` heading this project's entries go under: `log_section`, else `name`.
    pub section_key: String,
}

impl LogProject {
    pub fn new(slug: &str, name: &str, log_section: Option<&str>) -> Self {
        Self {
            slug: slug.to_string(),
            name: name.to_string(),
            section_key: log_section.unwrap_or(name).to_string(),
        }
    }
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum DecisionLogError {
    /// The log is never created: a missing file means the configured path is wrong.
    NotFound(PathBuf),
    /// Another writer changed the file between read and rename, twice.
    Conflict,
    Io(String),
}

impl std::fmt::Display for DecisionLogError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DecisionLogError::NotFound(p) => {
                write!(f, "decision log not found at {}", p.display())
            }
            DecisionLogError::Conflict => {
                write!(f, "decision log kept changing while pelhaim wrote it")
            }
            DecisionLogError::Io(e) => write!(f, "decision log: {e}"),
        }
    }
}

impl std::error::Error for DecisionLogError {}

fn io_err(what: &str, path: &Path, e: std::io::Error) -> DecisionLogError {
    DecisionLogError::Io(format!("{what} {}: {e}", path.display()))
}

/// Makes the export idempotent per decision: a crash between the rename and recording
/// `DecisionExported` replays the export, which then finds this marker and skips.
fn marker(id: DecisionId) -> String {
    format!("<!-- pelhaim:decision:{} -->", id.0)
}

pub fn render_entry(d: &Decision, eol: &str) -> String {
    format!(
        "- **{date}: {title}**{eol}  - Why: {why}{eol}  - Who: {who}. {marker}{eol}{eol}",
        date = d.date.format("%Y-%m-%d"),
        title = sanitize_line(&d.title),
        why = sanitize_line(&d.why),
        who = d.who.as_log_str(),
        marker = marker(d.id),
    )
}

struct Line<'a> {
    start: usize,
    text: &'a str,
}

impl Line<'_> {
    fn heading2(&self) -> Option<&str> {
        self.text.strip_prefix("## ")
    }

    fn is_section_end(&self) -> bool {
        self.text.starts_with("## ") || self.text.trim_end() == "---"
    }
}

fn lines(doc: &str) -> Vec<Line<'_>> {
    let mut start = 0;
    doc.split_inclusive('\n')
        .map(|raw| {
            let line = Line {
                start,
                text: raw.trim_end_matches('\n').trim_end_matches('\r'),
            };
            start += raw.len();
            line
        })
        .collect()
}

fn eol_of(doc: &str) -> &'static str {
    if doc.contains("\r\n") { "\r\n" } else { "\n" }
}

fn heading_matches(heading: &str, key: &str) -> bool {
    let h = heading.trim_end().to_lowercase();
    let k = key.trim().to_lowercase();
    if k.is_empty() {
        return false;
    }
    match h.strip_prefix(&k) {
        Some(rest) => rest.is_empty() || rest.starts_with(' '),
        None => false,
    }
}

/// Index of the matching `## ` line, and the index one past the section's last line.
fn find_section(ls: &[Line<'_>], key: &str) -> Option<(usize, usize)> {
    let head = ls
        .iter()
        .position(|l| l.heading2().is_some_and(|h| heading_matches(h, key)))?;
    let end = ls[head + 1..]
        .iter()
        .position(Line::is_section_end)
        .map_or(ls.len(), |i| head + 1 + i);
    Some((head, end))
}

fn offset_of(ls: &[Line<'_>], i: usize, doc: &str) -> usize {
    ls.get(i).map_or(doc.len(), |l| l.start)
}

/// Splices `entry` into `doc` without touching any other byte. Malformed or interleaved
/// sections are left as they are: the first matching heading wins.
pub fn insert_entry(doc: &str, p: &LogProject, entry: &str) -> String {
    let eol = eol_of(doc);
    let ls = lines(doc);
    let (at, mut inserted) = match find_section(&ls, &p.section_key) {
        Some((head, end)) => {
            match (head + 1..end).find(|&i| ls[i].text.trim_end() == DECIDED_HEADING) {
                Some(decided) => {
                    let mut i = decided + 1;
                    while i < end && ls[i].text.trim().is_empty() {
                        i += 1;
                    }
                    (offset_of(&ls, i, doc), entry.to_string())
                }
                None => (
                    offset_of(&ls, end, doc),
                    format!("{DECIDED_HEADING}{eol}{eol}{entry}"),
                ),
            }
        }
        None => {
            let rules = ls
                .iter()
                .position(|l| l.text.trim_end() == STANDING_RULES)
                .unwrap_or(ls.len());
            let name = sanitize_line(&p.section_key);
            (
                offset_of(&ls, rules, doc),
                format!(
                    "## {name}{eol}{eol}{OPEN_HEADING}{eol}{eol}{DECIDED_HEADING}{eol}{eol}{entry}"
                ),
            )
        }
    };
    if at == doc.len() && !doc.is_empty() && !doc.ends_with('\n') {
        inserted.insert_str(0, eol);
    }
    let mut out = String::with_capacity(doc.len() + inserted.len());
    out.push_str(&doc[..at]);
    out.push_str(&inserted);
    out.push_str(&doc[at..]);
    out
}

pub fn write_decision(path: &Path, p: &LogProject, d: &Decision) -> Result<(), DecisionLogError> {
    write_decision_with_hook(path, p, d, &mut |_| {})
}

/// `before_rename` runs after the temp file and backup are written, just before the
/// re-read hash check. Tests use it to simulate the other writer.
#[doc(hidden)]
pub fn write_decision_with_hook(
    path: &Path,
    p: &LogProject,
    d: &Decision,
    before_rename: &mut dyn FnMut(&Path),
) -> Result<(), DecisionLogError> {
    // Renaming over the canonical target, not the given path, keeps a symlinked log a symlink.
    let target = fs::canonicalize(path).map_err(|e| match e.kind() {
        std::io::ErrorKind::NotFound => DecisionLogError::NotFound(path.to_path_buf()),
        _ => io_err("resolving", path, e),
    })?;
    let dir = target
        .parent()
        .ok_or_else(|| DecisionLogError::Io(format!("{} has no parent", target.display())))?;
    let file_name = target
        .file_name()
        .ok_or_else(|| DecisionLogError::Io(format!("{} has no file name", target.display())))?
        .to_string_lossy()
        .into_owned();
    let tmp = dir.join(format!(".{file_name}.pelhaim-tmp-{}", std::process::id()));
    let bak = dir.join(format!("{file_name}.pelhaim-bak"));
    let marker = marker(d.id);

    for _ in 0..2 {
        let before = fs::read(&target).map_err(|e| io_err("reading", &target, e))?;
        let hash = Sha256::digest(&before);
        let doc = std::str::from_utf8(&before)
            .map_err(|e| DecisionLogError::Io(format!("{} is not UTF-8: {e}", target.display())))?;
        if doc.contains(&marker) {
            return Ok(());
        }
        let new = insert_entry(doc, p, &render_entry(d, eol_of(doc)));

        write_temp(&tmp, &target, new.as_bytes())?;
        if let Err(e) = fs::copy(&target, &bak) {
            let _ = fs::remove_file(&tmp);
            return Err(io_err("backing up", &target, e));
        }
        before_rename(&target);

        let now = match fs::read(&target) {
            Ok(b) => b,
            Err(e) => {
                let _ = fs::remove_file(&tmp);
                return Err(io_err("re-reading", &target, e));
            }
        };
        if Sha256::digest(&now) != hash {
            let _ = fs::remove_file(&tmp);
            continue;
        }
        return fs::rename(&tmp, &target).map_err(|e| {
            let _ = fs::remove_file(&tmp);
            io_err("replacing", &target, e)
        });
    }
    Err(DecisionLogError::Conflict)
}

fn write_temp(tmp: &Path, target: &Path, bytes: &[u8]) -> Result<(), DecisionLogError> {
    let result = (|| {
        let mut f = fs::File::create(tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
        fs::set_permissions(tmp, fs::metadata(target)?.permissions())
    })();
    result.map_err(|e| {
        let _ = fs::remove_file(tmp);
        io_err("writing", tmp, e)
    })
}

/// Read-only: each project's top-level bullets under `### Open decisions (need you)`,
/// as their first line trimmed. A project with no section maps to an empty list.
pub fn open_decisions(doc: &str, ps: &[LogProject]) -> BTreeMap<String, Vec<String>> {
    let ls = lines(doc);
    ps.iter()
        .map(|p| {
            let items = find_section(&ls, &p.section_key)
                .and_then(|(head, end)| {
                    let open = (head + 1..end).find(|&i| ls[i].text.trim_end() == OPEN_HEADING)?;
                    Some(
                        ls[open + 1..]
                            .iter()
                            .take_while(|l| {
                                !l.text.starts_with("### ") && !l.text.starts_with("## ")
                            })
                            .filter(|l| l.text.starts_with("- "))
                            .map(|l| l.text.trim().to_string())
                            .collect(),
                    )
                })
                .unwrap_or_default();
            (p.slug.clone(), items)
        })
        .collect()
}
