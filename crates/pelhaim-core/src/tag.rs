use serde::{Deserialize, Serialize};

pub const DISPATCH_SLUG: &str = "dispatch";

const TAG_PREFIX: &str = "pelhaim-project: ";

#[derive(Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Debug)]
pub enum TagParse {
    Tagged(String),
    Absent,
}

/// `^[a-z0-9-]+$`, shared by tag parsing and config validation.
pub fn is_valid_slug(s: &str) -> bool {
    !s.is_empty()
        && s.bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

/// A repository directory name as a slug (§7.7): ASCII lowercased, every run
/// of other characters one `-`, trimmed. `None` when nothing is left.
pub fn slugify(name: &str) -> Option<String> {
    let mut out = String::new();
    for c in name.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if !out.is_empty() && !out.ends_with('-') {
            out.push('-');
        }
    }
    let out = out.trim_end_matches('-');
    (!out.is_empty()).then(|| out.to_string())
}

/// Spec §7.7: only line 1 counts. Only spaces and tabs are stripped in front
/// (F12), so a text starting with a newline is untagged rather than letting a
/// tag on a later line through.
pub fn parse_tag(text: &str) -> TagParse {
    let line = text.split('\n').next().unwrap_or("");
    let line = line.trim_start_matches([' ', '\t']);
    let line = line.strip_suffix('\r').unwrap_or(line);
    match line.strip_prefix(TAG_PREFIX) {
        Some(slug) if is_valid_slug(slug) => TagParse::Tagged(slug.to_string()),
        _ => TagParse::Absent,
    }
}

pub fn levenshtein(a: &str, b: &str) -> usize {
    let b: Vec<char> = b.chars().collect();
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut cur = vec![0; b.len() + 1];
    for (i, ca) in a.chars().enumerate() {
        cur[0] = i + 1;
        for (j, &cb) in b.iter().enumerate() {
            let sub = prev[j] + usize::from(ca != cb);
            cur[j + 1] = sub.min(prev[j + 1] + 1).min(cur[j] + 1);
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    prev[b.len()]
}

/// Index of the slug nearest to `raw`; ties go to the lower index.
pub fn closest_slug(raw: &str, slugs: &[String]) -> Option<usize> {
    let mut best: Option<(usize, usize)> = None;
    for (i, slug) in slugs.iter().enumerate() {
        let d = levenshtein(raw, slug);
        if best.is_none_or(|(_, bd)| d < bd) {
            best = Some((i, d));
        }
    }
    best.map(|(i, _)| i)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_tag_parses() {
        assert_eq!(
            parse_tag("pelhaim-project: safeascent\nGo"),
            TagParse::Tagged("safeascent".into())
        );
    }

    #[test]
    fn leading_ws_and_cr_ignored() {
        assert_eq!(
            parse_tag("  pelhaim-project: ethica\r\nx"),
            TagParse::Tagged("ethica".into())
        );
    }

    #[test]
    fn leading_tab_ignored() {
        assert_eq!(
            parse_tag("\t pelhaim-project: ethica"),
            TagParse::Tagged("ethica".into())
        );
    }

    #[test]
    fn leading_newline_is_absent() {
        assert_eq!(parse_tag("\npelhaim-project: ethica"), TagParse::Absent);
        assert_eq!(parse_tag("  \n\npelhaim-project: ethica"), TagParse::Absent);
    }

    #[test]
    fn only_one_trailing_cr_is_stripped() {
        assert_eq!(
            parse_tag("pelhaim-project: ethica\r\r\nx"),
            TagParse::Absent
        );
    }

    #[test]
    fn malformed_key_is_absent() {
        assert_eq!(parse_tag("Pelhaim-Project: ethica"), TagParse::Absent);
    }

    #[test]
    fn no_space_is_absent() {
        assert_eq!(parse_tag("pelhaim-project:ethica"), TagParse::Absent);
    }

    #[test]
    fn trailing_space_is_absent() {
        assert_eq!(parse_tag("pelhaim-project: ethica "), TagParse::Absent);
    }

    #[test]
    fn uppercase_slug_is_absent() {
        assert_eq!(parse_tag("pelhaim-project: SafeAscent"), TagParse::Absent);
    }

    #[test]
    fn empty_slug_is_absent() {
        assert_eq!(parse_tag("pelhaim-project: "), TagParse::Absent);
        assert_eq!(parse_tag(""), TagParse::Absent);
    }

    #[test]
    fn tag_on_line_two_is_absent() {
        assert_eq!(
            parse_tag("hello\npelhaim-project: ethica"),
            TagParse::Absent
        );
    }

    #[test]
    fn tag_in_code_fence_is_absent() {
        assert_eq!(
            parse_tag("```\npelhaim-project: ethica\n```"),
            TagParse::Absent
        );
    }

    #[test]
    fn dispatch_parses() {
        assert_eq!(
            parse_tag("pelhaim-project: dispatch"),
            TagParse::Tagged(DISPATCH_SLUG.into())
        );
    }

    #[test]
    fn slug_rule() {
        assert!(is_valid_slug("loosy-goose"));
        assert!(is_valid_slug("m1-2"));
        assert!(!is_valid_slug(""));
        assert!(!is_valid_slug("Ethica"));
        assert!(!is_valid_slug("eth_ica"));
        assert!(!is_valid_slug("ethica "));
        assert!(!is_valid_slug("éthica"));
    }

    #[test]
    fn slugify_repo_names() {
        assert_eq!(slugify("SafeAscent").as_deref(), Some("safeascent"));
        assert_eq!(slugify("loosy_goose").as_deref(), Some("loosy-goose"));
        assert_eq!(slugify("g.l.a.s.s").as_deref(), Some("g-l-a-s-s"));
        assert_eq!(slugify("  MLB  fansent!").as_deref(), Some("mlb-fansent"));
        assert_eq!(slugify("éthica").as_deref(), Some("thica"));
        assert_eq!(slugify("---"), None);
        assert_eq!(slugify(""), None);
        assert!(slugify("Pelhaim M1").is_some_and(|s| is_valid_slug(&s)));
    }

    #[test]
    fn closest_slug_by_edit_distance() {
        let s = vec![
            "safeascent".to_string(),
            "ethica".into(),
            "loosy-goose".into(),
        ];
        assert_eq!(closest_slug("ethika", &s), Some(1));
        assert_eq!(closest_slug("safe-ascent", &s), Some(0));
        assert_eq!(levenshtein("kitten", "sitting"), 3);
    }

    #[test]
    fn closest_slug_ties_go_to_lower_index_and_empty_is_none() {
        let s = vec!["ab".to_string(), "ba".into()];
        assert_eq!(closest_slug("aa", &s), Some(0));
        assert_eq!(closest_slug("x", &[]), None);
    }

    #[test]
    fn levenshtein_counts_chars_not_bytes() {
        assert_eq!(levenshtein("", ""), 0);
        assert_eq!(levenshtein("abc", ""), 3);
        assert_eq!(levenshtein("", "🚂漢"), 2);
        assert_eq!(levenshtein("é", "e"), 1);
    }
}
