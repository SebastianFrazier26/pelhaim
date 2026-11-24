use pelhaim_core::sanitize_line;

use crate::color::Rgba;
use crate::palette::Palette;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mark {
    Done,
    Pending,
    Needs,
    Failed,
    Plain,
}

impl Mark {
    pub fn glyph(self) -> Option<char> {
        match self {
            Mark::Done => Some('☑'),
            Mark::Pending => Some('☐'),
            Mark::Needs => Some('⚠'),
            Mark::Failed => Some('✕'),
            Mark::Plain => None,
        }
    }

    pub fn color(self, p: &Palette) -> Rgba {
        match self {
            Mark::Done => p.work,
            Mark::Pending => p.idle,
            Mark::Needs => p.wait,
            Mark::Failed => p.fail,
            Mark::Plain => p.label,
        }
    }
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct CardRow {
    pub mark: Mark,
    pub text: String,
}

fn strip_list_marker(s: &str) -> Option<&str> {
    if let Some(rest) = s.strip_prefix("- ").or_else(|| s.strip_prefix("* ")) {
        return Some(rest);
    }
    let digits = s.chars().take_while(char::is_ascii_digit).count();
    if digits > 0 {
        return s[digits..].strip_prefix(". ");
    }
    None
}

pub fn parse_cards(text: &str) -> Vec<CardRow> {
    text.lines()
        .map(sanitize_line)
        .filter(|l| !l.is_empty())
        .map(|l| {
            let row = |mark, t: &str| CardRow {
                mark,
                text: t.to_string(),
            };
            if let Some(t) = l
                .strip_prefix("- [x] ")
                .or_else(|| l.strip_prefix("- [X] "))
            {
                row(Mark::Done, t)
            } else if let Some(t) = l.strip_prefix("- [ ] ") {
                row(Mark::Pending, t)
            } else if l.starts_with("Error") || l.starts_with("Failed") {
                row(Mark::Failed, &l)
            } else if let Some(t) = strip_list_marker(&l) {
                row(Mark::Plain, t)
            } else {
                row(Mark::Plain, &l)
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cards_parse_rules() {
        let r = parse_cards("- [x] judge run\n- [ ] corpus\nFailed: lint\n* note\n\nplain");
        assert_eq!(
            r.iter().map(|c| c.mark).collect::<Vec<_>>(),
            [
                Mark::Done,
                Mark::Pending,
                Mark::Failed,
                Mark::Plain,
                Mark::Plain
            ]
        );
        assert_eq!(r[3].text, "note");
    }

    #[test]
    fn markers_are_stripped_and_failures_keep_their_text() {
        let r = parse_cards("- [x] judge run\n12. twelfth\n  - indented\nError: boom\n- [X] caps");
        let texts: Vec<&str> = r.iter().map(|c| c.text.as_str()).collect();
        assert_eq!(
            texts,
            ["judge run", "twelfth", "indented", "Error: boom", "caps"]
        );
        assert_eq!(r[3].mark, Mark::Failed);
        assert_eq!(r[4].mark, Mark::Done);
    }

    #[test]
    fn control_chars_are_sanitised() {
        let r = parse_cards("- [ ] a\x1b[31mred\x07");
        assert_eq!(r[0].text, "a red");
    }
}
