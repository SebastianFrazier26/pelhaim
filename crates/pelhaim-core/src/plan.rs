use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use serde::{Deserialize, Serialize};

use crate::event::Tool;
use crate::ids::PlanId;
use crate::milestone::{MILESTONE_NAME_MAX, estimate_secs_from_hours};
use crate::text::sanitize_line;

const STEP_ID_MAX: usize = 40;
const ESTIMATE_HOURS_MAX: f64 = 40.0;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Debug)]
#[serde(rename_all = "snake_case")]
pub enum SignoffFlag {
    Decision,
    Merge,
    Prod,
    Security,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize, Debug)]
pub enum Check {
    Command { argv: Vec<String> },
    Manual { text: String },
}

#[derive(Clone, PartialEq, Serialize, Deserialize, Debug)]
pub struct PlanStep {
    pub id: String,
    /// Also the station and marker name, so it must survive the marker parser unchanged.
    pub title: String,
    pub detail: String,
    pub depends_on: Vec<String>,
    pub tool: Tool,
    pub model: Option<String>,
    /// `None` only for `pelhaim launch` without `--estimate`: no clock, nothing recorded.
    pub estimate_hours: Option<f64>,
    pub design: bool,
    pub security: bool,
    pub signoff: Vec<SignoffFlag>,
    pub acceptance: Vec<Check>,
    pub keyword_override: bool,
    pub skip: bool,
}

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Debug)]
pub enum PlanStatus {
    Draft,
    Approved,
    Running,
    Done,
    Abandoned,
}

#[derive(Clone, PartialEq, Serialize, Deserialize, Debug)]
pub struct Plan {
    pub id: PlanId,
    pub project: String,
    pub goal: String,
    pub steps: Vec<PlanStep>,
    pub status: PlanStatus,
    pub revision: u32,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize, Debug)]
pub enum PlanError {
    Empty,
    InvalidId { step: String },
    BadTitle { step: String },
    DuplicateId { step: String },
    UnknownDependency { step: String, dep: String },
    SelfDependency { step: String },
    Cycle { steps: Vec<String> },
    NoRoot,
    BadEstimate { step: String },
}

impl fmt::Display for PlanError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => write!(f, "the plan has no steps"),
            Self::InvalidId { step } => write!(
                f,
                "step {step:?}: id must be 1-{STEP_ID_MAX} chars of a-z, 0-9 and -"
            ),
            Self::BadTitle { step } => write!(
                f,
                "step {step:?}: title must be non-empty, single-spaced, without control chars, and at most {MILESTONE_NAME_MAX} chars"
            ),
            Self::DuplicateId { step } => write!(f, "step {step:?}: id used more than once"),
            Self::UnknownDependency { step, dep } => {
                write!(f, "step {step:?}: depends on unknown step {dep:?}")
            }
            Self::SelfDependency { step } => write!(f, "step {step:?}: depends on itself"),
            Self::Cycle { steps } => write!(f, "steps {} form a cycle", steps.join(", ")),
            Self::NoRoot => write!(f, "no step can start: every step has a dependency"),
            Self::BadEstimate { step } => write!(
                f,
                "step {step:?}: estimate_hours must be above 0 and at most {ESTIMATE_HOURS_MAX}"
            ),
        }
    }
}

impl std::error::Error for PlanError {}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize, Debug)]
pub enum FlagReason {
    DesignField,
    SecurityField,
    DesignKeyword(String),
    SecurityKeyword(String),
    ActionKeyword(String),
}

pub const DESIGN_KEYWORDS: &[&str] = &[
    "architecture",
    "design",
    "library",
    "framework",
    "approach",
    "data model",
    "schema",
    "dependency",
    "api shape",
];
pub const SECURITY_KEYWORDS: &[&str] = &[
    "auth",
    "authn",
    "authz",
    "secret",
    "secrets",
    "credential",
    "credentials",
    "token",
    "tokens",
    "permission",
    "permissions",
    "sandbox",
    "encrypt",
    "encryption",
    "password",
    "exposure",
    "cors",
];
pub const MERGE_KEYWORDS: &[&str] = &["push", "merge", "pull request", "pr"];
pub const PROD_KEYWORDS: &[&str] = &["deploy", "release", "prod", "production"];

fn valid_step_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= STEP_ID_MAX
        && id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

fn valid_title(t: &str) -> bool {
    !t.is_empty() && sanitize_line(t) == t && t.chars().count() <= MILESTONE_NAME_MAX
}

fn valid_estimate(h: Option<f64>) -> bool {
    h.is_none_or(|h| h.is_finite() && h > 0.0 && h <= ESTIMATE_HOURS_MAX)
}

pub fn validate(plan: &Plan) -> Result<(), Vec<PlanError>> {
    if plan.steps.is_empty() {
        return Err(vec![PlanError::Empty]);
    }
    let ids: BTreeSet<&str> = plan.steps.iter().map(|s| s.id.as_str()).collect();
    let mut seen = BTreeSet::new();
    let mut errs = Vec::new();
    for s in &plan.steps {
        let step = || s.id.clone();
        if !valid_step_id(&s.id) {
            errs.push(PlanError::InvalidId { step: step() });
        }
        if !valid_title(&s.title) {
            errs.push(PlanError::BadTitle { step: step() });
        }
        if !seen.insert(s.id.as_str()) {
            errs.push(PlanError::DuplicateId { step: step() });
        }
        for dep in &s.depends_on {
            if dep == &s.id {
                errs.push(PlanError::SelfDependency { step: step() });
            } else if !ids.contains(dep.as_str()) {
                errs.push(PlanError::UnknownDependency {
                    step: step(),
                    dep: dep.clone(),
                });
            }
        }
        if !valid_estimate(s.estimate_hours) {
            errs.push(PlanError::BadEstimate { step: step() });
        }
    }
    let cyclic = cycle_steps(plan);
    if !cyclic.is_empty() {
        errs.push(PlanError::Cycle { steps: cyclic });
    }
    if plan.steps.iter().all(|s| !s.depends_on.is_empty()) {
        errs.push(PlanError::NoRoot);
    }
    if errs.is_empty() { Ok(()) } else { Err(errs) }
}

/// Self-edges and unknown deps are reported on their own, so the graph leaves them out.
/// A duplicated id keeps its first step's edges.
fn dep_graph(plan: &Plan) -> BTreeMap<&str, BTreeSet<&str>> {
    let mut g: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
    for s in &plan.steps {
        g.entry(s.id.as_str()).or_default();
    }
    let mut filled = BTreeSet::new();
    for s in &plan.steps {
        if !filled.insert(s.id.as_str()) {
            continue;
        }
        let deps: BTreeSet<&str> = s
            .depends_on
            .iter()
            .map(String::as_str)
            .filter(|d| *d != s.id && g.contains_key(d))
            .collect();
        g.insert(s.id.as_str(), deps);
    }
    g
}

/// Kahn's algorithm from both ends: what neither pass can peel is on, or between, cycles.
/// Steps merely downstream (or upstream) of a cycle peel off and are not named.
fn cycle_steps(plan: &Plan) -> Vec<String> {
    let deps = dep_graph(plan);
    let mut left: BTreeSet<&str> = deps.keys().copied().collect();
    let peel = |left: &mut BTreeSet<&str>, forward: bool| loop {
        let free: Vec<&str> = left
            .iter()
            .copied()
            .filter(|&n| {
                if forward {
                    deps[n].iter().all(|d| !left.contains(d))
                } else {
                    left.iter().all(|&m| !deps[m].contains(n))
                }
            })
            .collect();
        if free.is_empty() {
            break;
        }
        for n in free {
            left.remove(n);
        }
    };
    peel(&mut left, true);
    peel(&mut left, false);
    let mut out = Vec::new();
    for s in &plan.steps {
        if left.remove(s.id.as_str()) {
            out.push(s.id.clone());
        }
    }
    out
}

/// One entry per flag (field reasons before keywords, keywords in list order),
/// omitting flags the step already has.
pub fn derive_flags(step: &PlanStep) -> Vec<(SignoffFlag, FlagReason)> {
    let mut found: Vec<(SignoffFlag, FlagReason)> = Vec::new();
    if step.design {
        found.push((SignoffFlag::Decision, FlagReason::DesignField));
    }
    if step.security {
        found.push((SignoffFlag::Security, FlagReason::SecurityField));
    }
    if !step.keyword_override {
        let words = words(&format!("{} {}", step.title, step.detail));
        let groups: [KeywordGroup; 4] = [
            (
                DESIGN_KEYWORDS,
                SignoffFlag::Decision,
                FlagReason::DesignKeyword,
            ),
            (
                SECURITY_KEYWORDS,
                SignoffFlag::Security,
                FlagReason::SecurityKeyword,
            ),
            (
                MERGE_KEYWORDS,
                SignoffFlag::Merge,
                FlagReason::ActionKeyword,
            ),
            (PROD_KEYWORDS, SignoffFlag::Prod, FlagReason::ActionKeyword),
        ];
        for (keywords, flag, reason) in groups {
            if let Some(k) = keywords.iter().find(|k| contains_phrase(&words, k)) {
                found.push((flag, reason((*k).to_string())));
            }
        }
    }
    let mut out: Vec<(SignoffFlag, FlagReason)> = Vec::new();
    for (flag, reason) in found {
        if !step.signoff.contains(&flag) && out.iter().all(|(f, _)| *f != flag) {
            out.push((flag, reason));
        }
    }
    out
}

type KeywordGroup = (
    &'static [&'static str],
    SignoffFlag,
    fn(String) -> FlagReason,
);

fn words(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_ascii_lowercase)
        .collect()
}

fn contains_phrase(words: &[String], phrase: &str) -> bool {
    let want: Vec<&str> = phrase.split(' ').collect();
    words
        .windows(want.len())
        .any(|w| w.iter().zip(&want).all(|(a, b)| a == b))
}

pub fn apply_derived_flags(plan: &mut Plan) -> BTreeMap<String, Vec<(SignoffFlag, FlagReason)>> {
    let mut added = BTreeMap::new();
    for s in &mut plan.steps {
        let flags = derive_flags(s);
        if flags.is_empty() {
            continue;
        }
        s.signoff.extend(flags.iter().map(|(f, _)| *f));
        added.insert(s.id.clone(), flags);
    }
    added
}

pub fn requires_signoff(step: &PlanStep) -> bool {
    !step.signoff.is_empty()
        || step
            .acceptance
            .iter()
            .any(|c| matches!(c, Check::Manual { .. }))
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize, Debug)]
pub struct PlanLayout {
    pub columns: BTreeMap<String, u16>,
    pub chains: Vec<Vec<String>>,
}

/// Deps with skipped steps replaced by their own effective deps, in plan order.
/// Unknown deps are dropped.
pub fn effective_deps(plan: &Plan, step: &str) -> Vec<String> {
    let by_id = index(plan);
    let mut found = BTreeSet::new();
    let mut visited = BTreeSet::new();
    let mut stack: Vec<&str> = by_id
        .get(step)
        .map(|s| s.depends_on.iter().map(String::as_str).collect())
        .unwrap_or_default();
    while let Some(d) = stack.pop() {
        if d == step || !visited.insert(d) {
            continue;
        }
        let Some(dep) = by_id.get(d) else { continue };
        if dep.skip {
            stack.extend(dep.depends_on.iter().map(String::as_str));
        } else {
            found.insert(d);
        }
    }
    plan.steps
        .iter()
        .map(|s| s.id.as_str())
        .filter(|id| found.remove(id))
        .map(str::to_string)
        .collect()
}

fn index(plan: &Plan) -> BTreeMap<&str, &PlanStep> {
    let mut by_id = BTreeMap::new();
    for s in &plan.steps {
        by_id.entry(s.id.as_str()).or_insert(s);
    }
    by_id
}

/// Non-skipped steps, each with its effective deps, in a dependency-first order.
/// Steps on a cycle never become ready and are left out.
fn live_order(plan: &Plan) -> Vec<(String, Vec<String>)> {
    let mut pending: Vec<(String, Vec<String>)> = Vec::new();
    let mut seen = BTreeSet::new();
    for s in plan.steps.iter().filter(|s| !s.skip) {
        if seen.insert(s.id.as_str()) {
            pending.push((s.id.clone(), effective_deps(plan, &s.id)));
        }
    }
    let mut done = BTreeSet::new();
    let mut order = Vec::new();
    loop {
        let (ready, rest): (Vec<_>, Vec<_>) = pending
            .into_iter()
            .partition(|(_, deps)| deps.iter().all(|d| done.contains(d)));
        if ready.is_empty() {
            return order;
        }
        done.extend(ready.iter().map(|(id, _)| id.clone()));
        order.extend(ready);
        pending = rest;
    }
}

pub fn layout(plan: &Plan) -> PlanLayout {
    let order = live_order(plan);
    let mut columns: BTreeMap<String, u16> = BTreeMap::new();
    for (id, deps) in &order {
        let col = deps
            .iter()
            .map(|d| columns[d].saturating_add(1))
            .max()
            .unwrap_or(0);
        columns.insert(id.clone(), col);
    }

    let pos: BTreeMap<&str, usize> = plan
        .steps
        .iter()
        .enumerate()
        .rev()
        .map(|(i, s)| (s.id.as_str(), i))
        .collect();
    let mut succs: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for (id, deps) in &order {
        for d in deps {
            succs.entry(d.as_str()).or_default().push(id.as_str());
        }
    }
    let mut left: BTreeSet<&str> = order.iter().map(|(id, _)| id.as_str()).collect();
    let mut chains = Vec::new();
    while !left.is_empty() {
        // Longest path starting at each step, ties to the lexicographically smallest
        // sequence of plan positions; reverse dependency order makes successors ready first.
        let mut best: BTreeMap<&str, Vec<usize>> = BTreeMap::new();
        for (id, _) in order.iter().rev() {
            let id = id.as_str();
            if !left.contains(id) {
                continue;
            }
            let tail = succs
                .get(id)
                .into_iter()
                .flatten()
                .filter_map(|s| best.get(s))
                .fold(None::<&Vec<usize>>, |acc, p| match acc {
                    Some(a) if !better(p, a) => Some(a),
                    _ => Some(p),
                });
            let mut path = vec![pos[id]];
            path.extend(tail.into_iter().flatten());
            best.insert(id, path);
        }
        let Some(path) = best.values().fold(None::<&Vec<usize>>, |acc, p| match acc {
            Some(a) if !better(p, a) => Some(a),
            _ => Some(p),
        }) else {
            break;
        };
        let chain: Vec<String> = path.iter().map(|&i| plan.steps[i].id.clone()).collect();
        for id in &chain {
            left.remove(id.as_str());
        }
        chains.push(chain);
    }
    PlanLayout { columns, chains }
}

fn better(p: &[usize], than: &[usize]) -> bool {
    p.len() > than.len() || (p.len() == than.len() && p < than)
}

/// Finish offsets in seconds from the anchor for non-skipped steps. `fixed` pins a
/// step's finish (Done: actual; Active: max(eta, now) − anchor). Other steps:
/// finish = max(finish(deps), 0) + estimate × factor; a step with no estimate adds 0
/// and its own entry is absent (no clock).
pub fn projected_finish(
    plan: &Plan,
    factor: f64,
    fixed: &BTreeMap<String, i64>,
) -> BTreeMap<String, i64> {
    let by_id = index(plan);
    let mut finish: BTreeMap<String, i64> = BTreeMap::new();
    let mut out = BTreeMap::new();
    for (id, deps) in live_order(plan) {
        let estimate = by_id[id.as_str()].estimate_hours;
        let end = if let Some(&t) = fixed.get(&id) {
            out.insert(id.clone(), t);
            t
        } else {
            let start = deps.iter().map(|d| finish[d]).max().unwrap_or(0).max(0);
            match estimate {
                Some(h) => {
                    // The marker's minute-rounded seconds, so ETAs match what timings record.
                    let secs = f64::from(estimate_secs_from_hours(h)) * factor;
                    let t = start + secs.round() as i64;
                    out.insert(id.clone(), t);
                    t
                }
                None => start,
            }
        };
        finish.insert(id, end);
    }
    out
}
