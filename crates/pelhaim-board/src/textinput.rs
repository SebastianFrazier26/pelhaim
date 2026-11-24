use unicode_segmentation::UnicodeSegmentation;

pub const MAX_ROWS: usize = 6;

/// The console's edit buffer. `cursor` is private so it can only ever sit on a grapheme
/// boundary; IME composition text lives in `preedit` until the platform commits it.
#[derive(Default, Clone, PartialEq, Eq, Debug)]
pub struct TextInput {
    text: String,
    cursor: usize,
    pub preedit: Option<String>,
}

impl TextInput {
    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn cursor(&self) -> usize {
        self.cursor
    }

    pub fn insert(&mut self, s: &str) {
        self.text.insert_str(self.cursor, s);
        self.cursor += s.len();
    }

    pub fn backspace(&mut self) {
        let prev = self.prev_boundary();
        self.text.replace_range(prev..self.cursor, "");
        self.cursor = prev;
    }

    pub fn left(&mut self) {
        self.cursor = self.prev_boundary();
    }

    pub fn right(&mut self) {
        self.cursor = self.text[self.cursor..]
            .grapheme_indices(true)
            .nth(1)
            .map_or(self.text.len(), |(i, _)| self.cursor + i);
    }

    pub fn home(&mut self) {
        self.cursor = 0;
    }

    pub fn end(&mut self) {
        self.cursor = self.text.len();
    }

    pub fn take(&mut self) -> String {
        self.cursor = 0;
        self.preedit = None;
        std::mem::take(&mut self.text)
    }

    /// Rows the text plus any preedit wraps to at `width_chars` (by char, as the font draws).
    pub fn rows(&self, width_chars: usize) -> usize {
        let n =
            self.text.chars().count() + self.preedit.as_deref().map_or(0, |p| p.chars().count());
        n.div_ceil(width_chars.max(1)).clamp(1, MAX_ROWS)
    }

    fn prev_boundary(&self) -> usize {
        self.text[..self.cursor]
            .grapheme_indices(true)
            .next_back()
            .map_or(0, |(i, _)| i)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn textinput_grapheme_backspace() {
        let mut t = TextInput::default();
        t.insert("a👍🏽b");
        t.left();
        t.backspace();
        assert_eq!(t.text(), "ab");
        assert_eq!(t.cursor(), 1);
        t.home();
        t.backspace();
        assert_eq!(t.text(), "ab");
    }

    #[test]
    fn moves_by_grapheme() {
        let mut t = TextInput::default();
        t.insert("e\u{301}x");
        t.home();
        t.right();
        assert_eq!(t.cursor(), "e\u{301}".len());
        t.right();
        t.right();
        assert_eq!(t.cursor(), t.text().len());
        t.left();
        t.insert("y");
        assert_eq!(t.text(), "e\u{301}yx");
        t.end();
        assert_eq!(t.cursor(), t.text().len());
    }

    #[test]
    fn take_empties_and_rows_cap_at_six() {
        let mut t = TextInput::default();
        assert_eq!(t.rows(10), 1);
        t.insert(&"x".repeat(25));
        assert_eq!(t.rows(10), 3);
        t.preedit = Some("abcde".into());
        assert_eq!(t.rows(10), 3);
        t.insert(&"x".repeat(200));
        assert_eq!(t.rows(10), MAX_ROWS);
        assert_eq!(t.take().len(), 225);
        assert_eq!((t.text(), t.cursor(), t.preedit.clone()), ("", 0, None));
    }
}
