pub mod grid;

use crate::color::Rgba;
use crate::display::{DisplayList, Prim, R};
use crate::view::View;

include!(concat!(env!("OUT_DIR"), "/glyphs.rs"));

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Cut {
    C5x7,
    C7x9,
}

impl Cut {
    fn cell(self) -> (u8, u8) {
        match self {
            Cut::C5x7 => (5, 7),
            Cut::C7x9 => (7, 9),
        }
    }
}

pub fn fold(c: char) -> char {
    match c {
        '\u{2018}' | '\u{2019}' => '\'',
        '\u{201C}' | '\u{201D}' => '"',
        '\u{2013}' | '\u{2014}' => '-',
        '\u{A0}' => ' ',
        '\u{D7}' => 'x',
        c => c,
    }
}

pub fn advance(cut: Cut, px: f32) -> f32 {
    f32::from(cut.cell().0 + 1) * px
}

pub fn line_height(cut: Cut, px: f32) -> f32 {
    match cut {
        Cut::C5x7 => 10.0 * px,
        Cut::C7x9 => 12.0 * px,
    }
}

pub fn text_width(s: &str, cut: Cut, px: f32) -> f32 {
    s.chars().count() as f32 * advance(cut, px)
}

fn glyph(cut: Cut, c: char) -> Option<&'static [u16]> {
    fn find<const H: usize>(t: &'static [(char, [u16; H])], c: char) -> Option<&'static [u16]> {
        t.binary_search_by_key(&c, |(k, _)| *k)
            .ok()
            .map(|i| &t[i].1[..])
    }
    match cut {
        Cut::C5x7 => find(CUT_5X7, c),
        Cut::C7x9 => find(CUT_7X9, c),
    }
}

fn tofu(cut: Cut) -> [u16; 9] {
    let (w, h) = cut.cell();
    let mut rows = [0; 9];
    for (r, row) in rows.iter_mut().enumerate().take(usize::from(h)) {
        *row = if r == 0 || r + 1 == usize::from(h) {
            (1 << w) - 1
        } else {
            1 << (w - 1) | 1
        };
    }
    rows
}

// The argument list is the plan's interface (Task 13), shared by every label caller.
#[allow(clippy::too_many_arguments)]
pub fn draw_text(
    dl: &mut DisplayList,
    v: &View,
    s: &str,
    x: f32,
    top: f32,
    cut: Cut,
    px: f32,
    color: Rgba,
) {
    let (w, h) = cut.cell();
    let adv = advance(cut, px);
    let tofu = tofu(cut);
    for (i, ch) in s.chars().enumerate() {
        let gx = x + i as f32 * adv;
        let rows = glyph(cut, fold(ch)).unwrap_or(&tofu[..usize::from(h)]);
        for (r, &bits) in rows.iter().enumerate() {
            let on = |col: u8| bits >> (w - 1 - col) & 1 == 1;
            let mut col = 0;
            while col < w {
                if !on(col) {
                    col += 1;
                    continue;
                }
                let start = col;
                while col < w && on(col) {
                    col += 1;
                }
                dl.prims.push(Prim::Rect {
                    r: v.rect(R {
                        x: gx + f32::from(start) * px,
                        y: top + r as f32 * px,
                        w: f32::from(col - start) * px,
                        h: px,
                    }),
                    color,
                    aa: false,
                });
            }
        }
    }
}

#[cfg(test)]
fn bitmap(cut: Cut, c: char) -> Vec<String> {
    let (w, _) = cut.cell();
    glyph(cut, c)
        .unwrap_or_else(|| panic!("no glyph for {c:?} in {cut:?}"))
        .iter()
        .map(|bits| {
            (0..w)
                .map(|i| {
                    if bits >> (w - 1 - i) & 1 == 1 {
                        '#'
                    } else {
                        '.'
                    }
                })
                .collect()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const UI_SYMBOLS: &str = "▶◀▸☐☑✓✕⚠●·…";
    const INK: Rgba = Rgba::hex(0xD5D6DA);

    fn rects(dl: &DisplayList) -> Vec<(R, bool)> {
        dl.prims
            .iter()
            .map(|p| match p {
                Prim::Rect { r, aa, color } => {
                    assert_eq!(*color, INK);
                    (*r, *aa)
                }
                other => panic!("unexpected prim {other:?}"),
            })
            .collect()
    }

    #[test]
    fn hooked_one_matches_spec() {
        assert_eq!(
            bitmap(Cut::C5x7, '1'),
            [
                ".##..", "..#..", "..#..", "..#..", "..#..", "..#..", "..###"
            ]
        );
        assert_eq!(bitmap(Cut::C7x9, '1')[8], "...####");
        assert_eq!(
            bitmap(Cut::C7x9, '1'),
            [
                "..##...", "...#...", "...#...", "...#...", "...#...", "...#...", "...#...",
                "...#...", "...####"
            ]
        );
    }

    #[test]
    fn ascii_and_symbols_covered_in_both_cuts() {
        let latin1 = ('\u{A1}'..='\u{FF}').map(fold);
        let wanted: Vec<char> = (' '..='~')
            .chain(UI_SYMBOLS.chars())
            .chain(latin1)
            .collect();
        for cut in [Cut::C5x7, Cut::C7x9] {
            let missing: Vec<char> = wanted
                .iter()
                .copied()
                .filter(|&c| glyph(cut, c).is_none())
                .collect();
            assert!(missing.is_empty(), "{cut:?} is missing {missing:?}");
            assert!(glyph(cut, ' ').unwrap().iter().all(|&r| r == 0));
            assert!(glyph(cut, 'O').unwrap().iter().any(|&r| r != 0));
        }
    }

    #[test]
    fn metrics_follow_cell_size() {
        assert_eq!(advance(Cut::C5x7, 1.0), 6.0);
        assert_eq!(advance(Cut::C7x9, 2.0), 16.0);
        assert_eq!(line_height(Cut::C5x7, 1.5), 15.0);
        assert_eq!(line_height(Cut::C7x9, 1.0), 12.0);
        assert_eq!(text_width("", Cut::C7x9, 1.0), 0.0);
        assert_eq!(text_width("abc", Cut::C7x9, 1.0), 24.0);
    }

    #[test]
    fn glyph_pixels_are_unantialiased_and_runs_merged() {
        let v = View {
            k: 2.0,
            ox: 10.0,
            oy: 20.0,
        };
        let mut dl = DisplayList::default();
        draw_text(&mut dl, &v, "1", 0.0, 0.0, Cut::C5x7, 1.0, INK);
        let got = rects(&dl);
        assert_eq!(got.len(), 7, "{got:?}");
        assert!(got.iter().all(|(_, aa)| !aa));
        assert_eq!(
            got[0].0,
            R {
                x: 12.0,
                y: 20.0,
                w: 4.0,
                h: 2.0
            }
        );
        assert_eq!(
            got[6].0,
            R {
                x: 14.0,
                y: 32.0,
                w: 6.0,
                h: 2.0
            }
        );

        let mut dl = DisplayList::default();
        draw_text(&mut dl, &v, " 1", 5.0, 1.0, Cut::C7x9, 0.5, INK);
        let got = rects(&dl);
        assert_eq!(got.len(), 9);
        assert_eq!(
            got[8].0,
            v.rect(R {
                x: 5.0 + advance(Cut::C7x9, 0.5) + 1.5,
                y: 1.0 + 4.0,
                w: 2.0,
                h: 0.5
            })
        );
    }

    #[test]
    fn unknown_char_draws_tofu_with_fixed_advance() {
        assert_eq!(
            text_width("a漢b", Cut::C5x7, 1.0),
            3.0 * advance(Cut::C5x7, 1.0)
        );
        assert_eq!(
            text_width("\u{0}\n\t", Cut::C7x9, 2.0),
            3.0 * advance(Cut::C7x9, 2.0)
        );

        let v = View {
            k: 1.0,
            ox: 0.0,
            oy: 0.0,
        };
        for (cut, s) in [
            (Cut::C5x7, "漢"),
            (Cut::C7x9, "\u{7}"),
            (Cut::C5x7, "\u{10FFFF}"),
            (Cut::C7x9, "\u{FEFF}"),
        ] {
            let (w, h) = cut.cell();
            let (w, h) = (f32::from(w), f32::from(h));
            let mut dl = DisplayList::default();
            draw_text(&mut dl, &v, s, 0.0, 0.0, cut, 1.0, INK);
            let got = rects(&dl);
            assert_eq!(
                got.len(),
                2 + 2 * (h as usize - 2),
                "{cut:?} {s:?}: {got:?}"
            );
            assert_eq!(
                got[0].0,
                R {
                    x: 0.0,
                    y: 0.0,
                    w,
                    h: 1.0
                }
            );
            assert_eq!(
                got[got.len() - 1].0,
                R {
                    x: 0.0,
                    y: h - 1.0,
                    w,
                    h: 1.0
                }
            );
            for (r, _) in &got[1..got.len() - 1] {
                assert!(r.w == 1.0 && (r.x == 0.0 || r.x == w - 1.0), "{r:?}");
            }
        }

        let mut dl = DisplayList::default();
        draw_text(&mut dl, &v, "漢1", 0.0, 0.0, Cut::C5x7, 1.0, INK);
        let one = rects(&dl).last().unwrap().0;
        assert_eq!(one.x, advance(Cut::C5x7, 1.0) + 2.0);
    }

    #[test]
    fn typographic_quotes_fold_to_ascii() {
        assert_eq!(fold('“'), '"');
        assert_eq!(fold('—'), '-');
        for (from, to) in [
            ('‘', '\''),
            ('’', '\''),
            ('“', '"'),
            ('”', '"'),
            ('–', '-'),
            ('—', '-'),
            ('\u{A0}', ' '),
            ('×', 'x'),
            ('a', 'a'),
            ('é', 'é'),
            ('漢', '漢'),
        ] {
            assert_eq!(fold(from), to, "{from:?}");
        }
        let mut dl = DisplayList::default();
        let v = View {
            k: 1.0,
            ox: 0.0,
            oy: 0.0,
        };
        draw_text(&mut dl, &v, "—", 0.0, 0.0, Cut::C5x7, 1.0, INK);
        let mut dash = DisplayList::default();
        draw_text(&mut dash, &v, "-", 0.0, 0.0, Cut::C5x7, 1.0, INK);
        assert_eq!(dl, dash);
    }
}
