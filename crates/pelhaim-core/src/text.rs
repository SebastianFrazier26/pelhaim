use crate::tag::{TagParse, parse_tag};

const TITLE_MAX: usize = 40;
const UNTITLED: &str = "(untitled)";

/// Control chars and whole `ESC [ … final` sequences each become one space,
/// whitespace runs collapse to one space, and the ends are trimmed.
pub fn sanitize_line(s: &str) -> String {
    let mut spaced = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\x1b' && chars.peek() == Some(&'[') {
            chars.next();
            // ECMA-48 CSI: parameter/intermediate bytes 0x20–0x3F, then one final byte 0x40–0x7E.
            while chars.next_if(|p| ('\x20'..='\x3f').contains(p)).is_some() {}
            chars.next_if(|p| ('\x40'..='\x7e').contains(p));
            spaced.push(' ');
        } else if c.is_control() {
            spaced.push(' ');
        } else {
            spaced.push(c);
        }
    }
    spaced.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// At most `max` chars; when it cuts, the last of them is `…`.
pub fn truncate_chars(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    if max == 0 {
        return String::new();
    }
    let mut out: String = s.chars().take(max - 1).collect();
    out.push('…');
    out
}

pub fn title_from_prompt(text: &str) -> String {
    let mut lines = text.lines();
    if let TagParse::Tagged(_) = parse_tag(text) {
        lines.next();
    }
    lines
        .map(sanitize_line)
        .find(|l| !l.is_empty())
        .map_or_else(|| UNTITLED.to_string(), |l| truncate_chars(&l, TITLE_MAX))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn title_skips_tag_line() {
        assert_eq!(
            title_from_prompt("pelhaim-project: cabn\nFix the sprite atlas"),
            "Fix the sprite atlas"
        );
    }

    #[test]
    fn title_keeps_malformed_tag_line() {
        assert_eq!(
            title_from_prompt("pelhaim-project: Cabn\nFix it"),
            "pelhaim-project: Cabn"
        );
    }

    #[test]
    fn title_skips_blank_lines_after_tag() {
        assert_eq!(
            title_from_prompt("pelhaim-project: cabn\r\n\r\n  \x07\n  Do it  "),
            "Do it"
        );
    }

    #[test]
    fn tag_only_prompt_titles_untitled() {
        assert_eq!(title_from_prompt("pelhaim-project: cabn"), "(untitled)");
    }

    #[test]
    fn title_truncates_by_chars_not_bytes() {
        let t = title_from_prompt(&"é🚂漢".repeat(30));
        assert_eq!(t.chars().count(), 40);
        assert!(t.ends_with('…'));
    }

    #[test]
    fn title_strips_ansi_and_controls() {
        assert_eq!(
            title_from_prompt("\x1b[31mred\x1b[0m\tthing\u{7}"),
            "red thing"
        );
    }

    #[test]
    fn empty_prompt_titles_untitled() {
        assert_eq!(title_from_prompt("   \n\n"), "(untitled)");
        assert_eq!(title_from_prompt(""), "(untitled)");
    }

    #[test]
    fn sanitize_collapses_and_trims() {
        assert_eq!(sanitize_line("  a \t\r\n b\u{85}c  "), "a b c");
        assert_eq!(sanitize_line("\x1b[1;32mok\x1b[0m"), "ok");
        assert_eq!(sanitize_line("a\x1bb"), "a b");
        assert_eq!(sanitize_line("a\x1b[3~b"), "a b");
        assert_eq!(sanitize_line("cut\x1b[12"), "cut");
        assert_eq!(sanitize_line("“smart” 🚂"), "“smart” 🚂");
    }

    #[test]
    fn truncate_chars_bounds() {
        assert_eq!(truncate_chars("abc", 3), "abc");
        assert_eq!(truncate_chars("abcd", 3), "ab…");
        assert_eq!(truncate_chars("漢字漢字", 2), "漢…");
        assert_eq!(truncate_chars("abc", 1), "…");
        assert_eq!(truncate_chars("abc", 0), "");
        assert_eq!(truncate_chars("", 0), "");
    }
}
