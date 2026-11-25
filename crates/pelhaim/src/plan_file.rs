use std::fmt::Write as _;
use std::ops::Range;
use std::path::{Path, PathBuf};

use pelhaim_core::{
    Check, Plan, PlanError, PlanStep, SignoffFlag, Tool, is_valid_slug, plan::validate,
    sanitize_line,
};
use serde::{Deserialize, Serialize};
use toml::Spanned;

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct PlanFileError {
    pub line: Option<usize>,
    pub message: String,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum RenderError {
    NoEstimate { step: String },
}

impl std::fmt::Display for RenderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoEstimate { step } => write!(
                f,
                "step {step:?} has no estimate_hours, which the plan file requires"
            ),
        }
    }
}

impl std::error::Error for RenderError {}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct BadSlug {
    pub project: String,
}

impl std::fmt::Display for BadSlug {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "project {:?} is not a valid slug", self.project)
    }
}

impl std::error::Error for BadSlug {}

/// The core `Tool` and `Check` serialise for M1's persisted JSON (`"Claude"`,
/// `{Command={argv=…}}`), so the file and the planner schema use their own shape (F10).
#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Debug, Default)]
#[serde(rename_all = "lowercase")]
pub(crate) enum WireTool {
    #[default]
    Claude,
    Codex,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize, Debug)]
#[serde(rename_all = "lowercase", deny_unknown_fields)]
pub(crate) enum WireCheck {
    Command(Vec<String>),
    Manual(String),
}

#[derive(Clone, PartialEq, Serialize, Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub(crate) struct WireStep {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub detail: String,
    #[serde(default)]
    pub depends_on: Vec<String>,
    #[serde(default)]
    pub tool: WireTool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    pub estimate_hours: f64,
    #[serde(default)]
    pub design: bool,
    #[serde(default)]
    pub security: bool,
    #[serde(default)]
    pub signoff: Vec<SignoffFlag>,
    #[serde(default)]
    pub keyword_override: bool,
    #[serde(default)]
    pub skip: bool,
    #[serde(default)]
    pub acceptance: Vec<WireCheck>,
}

pub(crate) fn step_from_wire(w: WireStep) -> PlanStep {
    PlanStep {
        id: w.id,
        title: w.title,
        detail: w.detail,
        depends_on: w.depends_on,
        tool: match w.tool {
            WireTool::Claude => Tool::Claude,
            WireTool::Codex => Tool::Codex,
        },
        model: w.model,
        estimate_hours: Some(w.estimate_hours),
        design: w.design,
        security: w.security,
        signoff: w.signoff,
        acceptance: w
            .acceptance
            .into_iter()
            .map(|c| match c {
                WireCheck::Command(argv) => Check::Command { argv },
                WireCheck::Manual(text) => Check::Manual { text },
            })
            .collect(),
        keyword_override: w.keyword_override,
        skip: w.skip,
    }
}

pub(crate) fn step_to_wire(s: &PlanStep) -> Result<WireStep, RenderError> {
    let estimate_hours = s
        .estimate_hours
        .ok_or_else(|| RenderError::NoEstimate { step: s.id.clone() })?;
    Ok(WireStep {
        id: s.id.clone(),
        title: s.title.clone(),
        detail: s.detail.clone(),
        depends_on: s.depends_on.clone(),
        tool: match s.tool {
            Tool::Claude => WireTool::Claude,
            Tool::Codex => WireTool::Codex,
        },
        model: s.model.clone(),
        estimate_hours,
        design: s.design,
        security: s.security,
        signoff: s.signoff.clone(),
        keyword_override: s.keyword_override,
        skip: s.skip,
        acceptance: s
            .acceptance
            .iter()
            .map(|c| match c {
                Check::Command { argv } => WireCheck::Command(argv.clone()),
                Check::Manual { text } => WireCheck::Manual(text.clone()),
            })
            .collect(),
    })
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WireFile {
    #[serde(default)]
    step: Vec<Spanned<WireStep>>,
}

const COMMENT_COLUMN: usize = 31;

fn push_line(out: &mut String, kv: &str, comment: Option<&str>) {
    out.push_str(kv);
    if let Some(c) = comment {
        let pad = COMMENT_COLUMN.saturating_sub(kv.chars().count()).max(1);
        out.extend(std::iter::repeat_n(' ', pad));
        out.push_str("# ");
        out.push_str(c);
    }
    out.push('\n');
}

/// Escapes everything TOML treats specially, including every `"`, so the same body is
/// valid inside both `"…"` and `"""…"""`; only `\n` is kept raw in the multi-line form.
fn escape(s: &str, keep_newlines: bool) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' if keep_newlines => out.push('\n'),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            c if c.is_control() => {
                let _ = write!(out, "\\u{:04X}", u32::from(c));
            }
            c => out.push(c),
        }
    }
    out
}

fn quoted(s: &str) -> String {
    format!("\"{}\"", escape(s, false))
}

fn string_array<S: AsRef<str>>(items: &[S]) -> String {
    let parts: Vec<String> = items.iter().map(|s| quoted(s.as_ref())).collect();
    format!("[{}]", parts.join(", "))
}

fn float(h: f64) -> String {
    if h.is_nan() {
        "nan".into()
    } else if h.is_infinite() {
        if h > 0.0 { "inf".into() } else { "-inf".into() }
    } else {
        format!("{h:?}")
    }
}

fn flag_name(f: SignoffFlag) -> &'static str {
    match f {
        SignoffFlag::Decision => "decision",
        SignoffFlag::Merge => "merge",
        SignoffFlag::Prod => "prod",
        SignoffFlag::Security => "security",
    }
}

/// `estimate_hours` is required in the file (F28). Only `pelhaim launch` without
/// `--estimate` makes a `None` estimate, and those plans never reach the file, so this
/// refuses rather than invent a value the owner would then have to notice and fix.
pub fn render(plan: &Plan) -> Result<String, RenderError> {
    let mut out = String::new();
    let _ = writeln!(
        out,
        "# pelhaim plan for {}: {}",
        sanitize_line(&plan.project),
        sanitize_line(&plan.goal)
    );
    let _ = writeln!(
        out,
        "# Save to apply. Lines starting with # are ignored. Revision {}.",
        plan.revision
    );
    for s in &plan.steps {
        let w = step_to_wire(s)?;
        out.push_str("\n[[step]]\n");
        push_line(&mut out, &format!("id = {}", quoted(&w.id)), None);
        push_line(
            &mut out,
            &format!("title = {}", quoted(&w.title)),
            Some("at most 32 characters: it is the station name"),
        );
        push_line(
            &mut out,
            &format!("detail = \"\"\"\n{}\"\"\"", escape(&w.detail, true)),
            None,
        );
        push_line(
            &mut out,
            &format!("depends_on = {}", string_array(&w.depends_on)),
            None,
        );
        let tool = match w.tool {
            WireTool::Claude => "claude",
            WireTool::Codex => "codex",
        };
        push_line(
            &mut out,
            &format!("tool = \"{tool}\""),
            Some("claude | codex"),
        );
        match &w.model {
            Some(m) => push_line(&mut out, &format!("model = {}", quoted(m)), None),
            None => push_line(&mut out, "# model = \"…\"", None),
        }
        push_line(
            &mut out,
            &format!("estimate_hours = {}", float(w.estimate_hours)),
            Some("agent hours, required"),
        );
        push_line(&mut out, &format!("design = {}", w.design), None);
        push_line(&mut out, &format!("security = {}", w.security), None);
        let flags: Vec<&str> = w.signoff.iter().map(|f| flag_name(*f)).collect();
        push_line(
            &mut out,
            &format!("signoff = {}", string_array(&flags)),
            Some("decision | merge | prod | security (derived flags are added automatically)"),
        );
        push_line(
            &mut out,
            &format!("keyword_override = {}", w.keyword_override),
            None,
        );
        push_line(&mut out, &format!("skip = {}", w.skip), None);
        if w.acceptance.is_empty() {
            push_line(&mut out, "acceptance = []", None);
        } else {
            out.push_str("acceptance = [\n");
            for c in &w.acceptance {
                match c {
                    WireCheck::Command(argv) => {
                        let _ = writeln!(out, "  {{ command = {} }},", string_array(argv));
                    }
                    WireCheck::Manual(text) => {
                        let _ = writeln!(out, "  {{ manual = {} }},", quoted(text));
                    }
                }
            }
            out.push_str("]\n");
        }
    }
    Ok(out)
}

fn line_of(text: &str, offset: usize) -> usize {
    let end = offset.min(text.len());
    text.as_bytes()[..end]
        .iter()
        .filter(|&&b| b == b'\n')
        .count()
        + 1
}

/// Each step-scoped error goes to its step's `[[step]]` header; a repeated id goes to the
/// repeat, since `validate` reports one `DuplicateId` per extra occurrence.
fn step_index(steps: &[PlanStep], err: &PlanError, dup_seen: &mut Vec<usize>) -> Option<usize> {
    let first = |id: &str| steps.iter().position(|s| s.id == id);
    match err {
        PlanError::Empty | PlanError::NoRoot => None,
        PlanError::Cycle { steps: ids } => ids.first().and_then(|id| first(id)),
        PlanError::DuplicateId { step } => {
            let next = steps
                .iter()
                .enumerate()
                .filter(|(_, s)| &s.id == step)
                .map(|(i, _)| i)
                .skip(1)
                .find(|i| !dup_seen.contains(i));
            if let Some(i) = next {
                dup_seen.push(i);
            }
            next
        }
        PlanError::InvalidId { step }
        | PlanError::BadTitle { step }
        | PlanError::UnknownDependency { step, .. }
        | PlanError::SelfDependency { step }
        | PlanError::BadEstimate { step } => first(step),
    }
}

pub fn parse(text: &str, base: &Plan) -> Result<Vec<PlanStep>, Vec<PlanFileError>> {
    let file: WireFile = toml::from_str(text).map_err(|e| {
        vec![PlanFileError {
            line: e.span().map(|s| line_of(text, s.start)),
            message: e.message().to_string(),
        }]
    })?;
    let spans: Vec<Range<usize>> = file.step.iter().map(Spanned::span).collect();
    let steps: Vec<PlanStep> = file
        .step
        .into_iter()
        .map(|s| step_from_wire(s.into_inner()))
        .collect();
    let plan = Plan {
        steps,
        ..base.clone()
    };
    if let Err(errs) = validate(&plan) {
        let mut dup_seen = Vec::new();
        return Err(errs
            .iter()
            .map(|e| PlanFileError {
                line: step_index(&plan.steps, e, &mut dup_seen)
                    .map(|i| line_of(text, spans[i].start)),
                message: e.to_string(),
            })
            .collect());
    }
    Ok(plan.steps)
}

/// One file per plan, not per revision, so an open editor keeps saving to it (F22).
/// The slug check keeps `plan.project` from naming a path outside `<data_dir>/plans`.
pub fn plan_path(data_dir: &Path, plan: &Plan) -> Result<PathBuf, BadSlug> {
    if !is_valid_slug(&plan.project) {
        return Err(BadSlug {
            project: plan.project.clone(),
        });
    }
    Ok(data_dir
        .join("plans")
        .join(format!("{}-{}.toml", plan.project, plan.id.0)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use pelhaim_core::{Check, PlanId, PlanStatus, SignoffFlag, Tool};

    fn step(id: &str, title: &str, deps: &[&str], est: f64) -> PlanStep {
        PlanStep {
            id: id.into(),
            title: title.into(),
            detail: String::new(),
            depends_on: deps.iter().map(|d| d.to_string()).collect(),
            tool: Tool::Claude,
            model: None,
            estimate_hours: Some(est),
            design: false,
            security: false,
            signoff: vec![],
            acceptance: vec![],
            keyword_override: false,
            skip: false,
        }
    }

    fn base() -> Plan {
        Plan {
            id: PlanId(7),
            project: "safeascent".into(),
            goal: "Ship the risk page".into(),
            steps: vec![],
            status: PlanStatus::Draft,
            revision: 3,
        }
    }

    fn four_step_plan() -> Plan {
        let mut a = step("migrate", "Migrate the schema", &[], 2.0);
        a.detail = "Add the table.\nKeep \"old\" rows \\ intact.\n\n  Indented line.\n".into();
        a.design = true;
        a.signoff = vec![SignoffFlag::Decision];
        a.acceptance = vec![
            Check::Command {
                argv: vec!["cargo".into(), "test".into(), "--workspace".into()],
            },
            Check::Manual {
                text: "Check the dashboard renders".into(),
            },
        ];
        let mut b = step("api", "Expose the API", &["migrate"], 0.5);
        b.tool = Tool::Codex;
        b.model = Some("gpt-5.4".into());
        b.security = true;
        b.signoff = vec![SignoffFlag::Security, SignoffFlag::Merge];
        let mut c = step("ui", "Draw the page", &["migrate"], 1.25);
        c.keyword_override = true;
        c.acceptance = vec![Check::Command {
            argv: vec!["npm".into(), "test".into()],
        }];
        let mut d = step("release", "Release it", &["api", "ui"], 40.0);
        d.skip = true;
        d.signoff = vec![SignoffFlag::Prod];
        Plan {
            steps: vec![a, b, c, d],
            ..base()
        }
    }

    fn one_step(body: &str) -> String {
        format!("[[step]]\nid = \"a\"\ntitle = \"Do a\"\n{body}")
    }

    #[test]
    fn render_parse_round_trip() {
        let plan = four_step_plan();
        let text = render(&plan).unwrap();
        assert!(text.starts_with(
            "# pelhaim plan for safeascent: Ship the risk page\n# Save to apply. Lines starting with # are ignored. Revision 3.\n"
        ));
        assert_eq!(parse(&text, &base()).unwrap(), plan.steps);
    }

    #[test]
    fn rendered_step_matches_documented_format() {
        let mut s = step("migrate", "Migrate the schema", &[], 2.0);
        s.detail = "Line one.".into();
        s.acceptance = vec![
            Check::Command {
                argv: vec!["cargo".into(), "test".into(), "--workspace".into()],
            },
            Check::Manual {
                text: "Check the dashboard renders".into(),
            },
        ];
        let text = render(&Plan {
            steps: vec![s],
            ..base()
        })
        .unwrap();
        let expected = r#"
[[step]]
id = "migrate"
title = "Migrate the schema"   # at most 32 characters: it is the station name
detail = """
Line one."""
depends_on = []
tool = "claude"                # claude | codex
# model = "…"
estimate_hours = 2.0           # agent hours, required
design = false
security = false
signoff = []                   # decision | merge | prod | security (derived flags are added automatically)
keyword_override = false
skip = false
acceptance = [
  { command = ["cargo", "test", "--workspace"] },
  { manual = "Check the dashboard renders" },
]
"#;
        assert!(text.ends_with(expected), "{text}");
    }

    #[test]
    fn tool_and_checks_use_lowercase_wire_shape() {
        let text = render(&four_step_plan()).unwrap();
        assert!(text.contains("tool = \"codex\""));
        assert!(text.contains("{ manual = "));
        assert!(text.contains("{ command = [\"npm\", \"test\"] }"));
        assert!(text.contains("signoff = [\"security\", \"merge\"]"));
    }

    #[test]
    fn missing_fields_default_except_estimate() {
        let steps = parse(&one_step("estimate_hours = 1.0\n"), &base()).unwrap();
        let mut want = step("a", "Do a", &[], 1.0);
        want.title = "Do a".into();
        assert_eq!(steps, vec![want]);
    }

    #[test]
    fn missing_estimate_is_an_error() {
        let errs = parse(&one_step("tool = \"codex\"\n"), &base()).unwrap_err();
        assert_eq!(errs.len(), 1);
        assert!(errs[0].message.contains("estimate_hours"), "{errs:?}");
        assert_eq!(errs[0].line, Some(1));
    }

    #[test]
    fn parse_error_has_step_header_line() {
        let text = "# header\n\n[[step]]\nid = \"a\"\ntitle = \"Do a\"\nestimate_hours = 1.0\n\n[[step]]\nid = \"b\"\ntitle = \"Do b\"\ndetail = \"\"\"\n[[step]]\n\"\"\"\ndepends_on = [\"nope\"]\nestimate_hours = 50.0\n";
        let errs = parse(text, &base()).unwrap_err();
        assert_eq!(errs.len(), 2, "{errs:?}");
        assert!(errs.iter().all(|e| e.line == Some(8)), "{errs:?}");
        assert!(
            errs.iter()
                .any(|e| e.message.contains("unknown step \"nope\""))
        );
        assert!(errs.iter().any(|e| e.message.contains("estimate_hours")));
    }

    #[test]
    fn plan_wide_errors_have_no_line() {
        let errs = parse("# nothing\n", &base()).unwrap_err();
        assert_eq!(errs.len(), 1);
        assert_eq!(errs[0].line, None);
    }

    #[test]
    fn duplicate_id_points_at_the_repeat() {
        let text = "[[step]]\nid = \"a\"\ntitle = \"One\"\nestimate_hours = 1.0\n[[step]]\nid = \"a\"\ntitle = \"Two\"\nestimate_hours = 1.0\n";
        let errs = parse(text, &base()).unwrap_err();
        assert_eq!(errs.len(), 1, "{errs:?}");
        assert_eq!(errs[0].line, Some(5));
    }

    #[test]
    fn syntax_error_line_reported() {
        let text = "[[step]]\nid = \"a\"\ntitle = \"Do a\"\nestimate_hours = = 1\n";
        let errs = parse(text, &base()).unwrap_err();
        assert_eq!(errs.len(), 1);
        assert_eq!(errs[0].line, Some(4), "{errs:?}");
    }

    #[test]
    fn unknown_field_rejected() {
        let errs = parse(&one_step("estimate_hours = 1.0\nowner = \"me\"\n"), &base()).unwrap_err();
        assert_eq!(errs.len(), 1);
        assert!(errs[0].message.contains("owner"), "{errs:?}");
        assert_eq!(errs[0].line, Some(5));

        let top = format!("goal = \"x\"\n{}", one_step("estimate_hours = 1.0\n"));
        let errs = parse(&top, &base()).unwrap_err();
        assert!(errs[0].message.contains("goal"), "{errs:?}");
        assert_eq!(errs[0].line, Some(1));

        let bad_tool = one_step("estimate_hours = 1.0\ntool = \"Claude\"\n");
        assert!(parse(&bad_tool, &base()).is_err());
        let bad_check = one_step("estimate_hours = 1.0\nacceptance = [{ argv = [\"x\"] }]\n");
        assert!(parse(&bad_check, &base()).is_err());
    }

    #[test]
    fn hostile_strings_round_trip() {
        let mut s = step("a", "Quote \" and \\ slash", &[], 1.0);
        s.detail = "\"\"\" closes?\ttab\r\nbell\u{7}\n\"".into();
        s.acceptance = vec![Check::Manual {
            text: "it's \"fine\"\n# not a comment".into(),
        }];
        s.model = Some("m\"x".into());
        let mut plan = Plan {
            steps: vec![s],
            ..base()
        };
        plan.goal = "two\nlines".into();
        let text = render(&plan).unwrap();
        assert!(text.starts_with("# pelhaim plan for safeascent: two lines\n"));
        assert_eq!(parse(&text, &base()).unwrap(), plan.steps);
    }

    #[test]
    fn none_estimate_is_not_rendered() {
        let mut plan = four_step_plan();
        plan.steps[2].estimate_hours = None;
        assert_eq!(
            render(&plan),
            Err(RenderError::NoEstimate { step: "ui".into() })
        );
    }

    #[test]
    fn plan_path_rejects_bad_slug() {
        for project in ["../etc", "a/b", "", "Up", "a b"] {
            let p = Plan {
                project: project.into(),
                ..base()
            };
            assert_eq!(
                plan_path(Path::new("/data"), &p),
                Err(BadSlug {
                    project: project.into()
                })
            );
        }
    }

    #[test]
    fn plan_path_is_fixed_across_revisions() {
        let d = Path::new("/data");
        let mut p = four_step_plan();
        let first = plan_path(d, &p).unwrap();
        p.revision += 5;
        p.steps.pop();
        assert_eq!(first, plan_path(d, &p).unwrap());
        assert_eq!(first, PathBuf::from("/data/plans/safeascent-7.toml"));
    }
}
