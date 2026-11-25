use serde::{Deserialize, Serialize};

use crate::text::{sanitize_line, truncate_chars};

const MARKER_PREFIX: &str = "pelhaim-milestone: ";

/// Longer names are cut to this many chars, the last being `…`.
pub const MILESTONE_NAME_MAX: usize = 32;

/// Estimates above a week are treated as part of the name, not as an estimate.
pub const ESTIMATE_MAX_SECS: u32 = 7 * 24 * 3600;

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize, Debug)]
pub struct Milestone {
    pub name: String,
    pub estimate_secs: Option<u32>,
}

/// Spec §7.10: every line that is exactly `pelhaim-milestone: <name>` (an
/// optional ` ~<duration>` at the end), outside fenced code blocks. Leading
/// spaces and tabs and one trailing `\r` are ignored, as for the tag.
pub fn parse_milestones(text: &str) -> Vec<Milestone> {
    let mut out = Vec::new();
    let mut fence: Option<(char, usize)> = None;
    for raw in text.split('\n') {
        let line = raw.strip_suffix('\r').unwrap_or(raw);
        let line = line.trim_start_matches([' ', '\t']);
        if let Some(open) = fence {
            if closes_fence(line, open) {
                fence = None;
            }
            continue;
        }
        if let Some(open) = opens_fence(line) {
            fence = Some(open);
            continue;
        }
        if let Some(rest) = line.strip_prefix(MARKER_PREFIX)
            && let Some(m) = milestone(rest)
        {
            out.push(m);
        }
    }
    out
}

fn fence_run(line: &str) -> Option<(char, usize)> {
    let c = line.chars().next().filter(|c| matches!(c, '`' | '~'))?;
    let n = line.chars().take_while(|&x| x == c).count();
    (n >= 3).then_some((c, n))
}

fn opens_fence(line: &str) -> Option<(char, usize)> {
    fence_run(line)
}

/// CommonMark: the same char, at least as many, and nothing after but spaces.
fn closes_fence(line: &str, (c, n): (char, usize)) -> bool {
    match fence_run(line) {
        Some((cc, nn)) if cc == c && nn >= n => line[nn * c.len_utf8()..].trim().is_empty(),
        _ => false,
    }
}

fn milestone(rest: &str) -> Option<Milestone> {
    let rest = sanitize_line(rest);
    let split = rest
        .rsplit_once('~')
        .filter(|(name, _)| name.is_empty() || name.ends_with(' '));
    let (name, estimate_secs) = match split {
        Some((name, est)) => match parse_estimate(est) {
            Some(secs) => (name.trim_end().to_string(), Some(secs)),
            None => (rest.clone(), None),
        },
        None => (rest.clone(), None),
    };
    if name.is_empty() {
        return None;
    }
    Some(Milestone {
        name: truncate_chars(&name, MILESTONE_NAME_MAX),
        estimate_secs,
    })
}

/// `1h30m`, `20m`, `45s`, `2h5m10s`: units in h, m, s order, each at most
/// once, total above zero and at most `ESTIMATE_MAX_SECS`.
pub fn parse_estimate(s: &str) -> Option<u32> {
    let mut total: u64 = 0;
    let mut rank = 0;
    let mut num = String::new();
    if s.is_empty() {
        return None;
    }
    for c in s.chars() {
        if c.is_ascii_digit() {
            num.push(c);
            continue;
        }
        let (r, scale) = match c {
            'h' => (1, 3600),
            'm' => (2, 60),
            's' => (3, 1),
            _ => return None,
        };
        if num.is_empty() || r <= rank || num.len() > 7 {
            return None;
        }
        rank = r;
        total += num.parse::<u64>().ok()? * scale;
        num.clear();
    }
    if !num.is_empty() || total == 0 || total > u64::from(ESTIMATE_MAX_SECS) {
        return None;
    }
    u32::try_from(total).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(name: &str, est: Option<u32>) -> Milestone {
        Milestone {
            name: name.into(),
            estimate_secs: est,
        }
    }

    #[test]
    fn marker_without_estimate() {
        assert_eq!(
            parse_milestones("pelhaim-milestone: tests green"),
            vec![m("tests green", None)]
        );
    }

    #[test]
    fn marker_with_estimates() {
        assert_eq!(
            parse_milestones("pelhaim-milestone: parser ~20m"),
            vec![m("parser", Some(1200))]
        );
        assert_eq!(
            parse_milestones("pelhaim-milestone: store ~1h30m"),
            vec![m("store", Some(5400))]
        );
        assert_eq!(
            parse_milestones("pelhaim-milestone: lint ~45s"),
            vec![m("lint", Some(45))]
        );
    }

    #[test]
    fn bad_estimate_stays_in_the_name() {
        assert_eq!(
            parse_milestones("pelhaim-milestone: fix ~soon"),
            vec![m("fix ~soon", None)]
        );
        assert_eq!(
            parse_milestones("pelhaim-milestone: fix ~0m"),
            vec![m("fix ~0m", None)]
        );
        assert_eq!(
            parse_milestones("pelhaim-milestone: fix ~30m1h"),
            vec![m("fix ~30m1h", None)]
        );
    }

    #[test]
    fn estimate_alone_is_not_a_marker() {
        assert!(parse_milestones("pelhaim-milestone: ~20m").is_empty());
        assert!(parse_milestones("pelhaim-milestone: ").is_empty());
        assert!(parse_milestones("pelhaim-milestone:    ").is_empty());
    }

    #[test]
    fn marker_on_any_line_of_its_own() {
        let text = "Done with step one.\n  pelhaim-milestone: step one ~5m\r\nnext\n\tpelhaim-milestone: step two";
        assert_eq!(
            parse_milestones(text),
            vec![m("step one", Some(300)), m("step two", None)]
        );
    }

    #[test]
    fn marker_inside_a_line_does_not_fire() {
        assert!(parse_milestones("I will write pelhaim-milestone: x later").is_empty());
        assert!(parse_milestones("`pelhaim-milestone: x`").is_empty());
        assert!(parse_milestones("> pelhaim-milestone: x").is_empty());
    }

    #[test]
    fn key_is_exact() {
        assert!(parse_milestones("Pelhaim-Milestone: x").is_empty());
        assert!(parse_milestones("pelhaim-milestone:x").is_empty());
        assert!(parse_milestones("pelhaim-project: x").is_empty());
    }

    #[test]
    fn markers_in_code_fences_do_not_fire() {
        let text = "```\npelhaim-milestone: a\n```\npelhaim-milestone: b\n~~~~text\npelhaim-milestone: c\n~~~\npelhaim-milestone: d\n~~~~\n";
        assert_eq!(parse_milestones(text), vec![m("b", None)]);
    }

    #[test]
    fn a_shorter_or_other_fence_does_not_close() {
        let text = "````\n```\npelhaim-milestone: a\n````\npelhaim-milestone: b";
        assert_eq!(parse_milestones(text), vec![m("b", None)]);
        let text = "```\n~~~\npelhaim-milestone: a\n```x\npelhaim-milestone: b";
        assert!(parse_milestones(text).is_empty());
    }

    #[test]
    fn an_unclosed_fence_hides_the_rest() {
        assert!(parse_milestones("```rust\npelhaim-milestone: a").is_empty());
    }

    #[test]
    fn name_is_sanitised_and_capped() {
        assert_eq!(
            parse_milestones("pelhaim-milestone: \x1b[31mred\x1b[0m\tthing  ~1m")[0],
            m("red thing", Some(60))
        );
        let long = format!("pelhaim-milestone: {}", "é".repeat(50));
        let got = &parse_milestones(&long)[0].name;
        assert_eq!(got.chars().count(), MILESTONE_NAME_MAX);
        assert!(got.ends_with('…'));
    }

    #[test]
    fn estimate_grammar() {
        assert_eq!(parse_estimate("2h5m10s"), Some(7510));
        assert_eq!(parse_estimate("90m"), Some(5400));
        assert_eq!(parse_estimate("168h"), Some(ESTIMATE_MAX_SECS));
        assert_eq!(parse_estimate("169h"), None);
        assert_eq!(parse_estimate("1h1h"), None);
        assert_eq!(parse_estimate("m"), None);
        assert_eq!(parse_estimate("10"), None);
        assert_eq!(parse_estimate(""), None);
        assert_eq!(parse_estimate("1d"), None);
        assert_eq!(parse_estimate("99999999999h"), None);
    }
}
