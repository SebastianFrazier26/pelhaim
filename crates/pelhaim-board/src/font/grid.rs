#[derive(Clone, Debug, PartialEq)]
pub struct Glyphs {
    pub w: u8,
    pub h: u8,
    pub map: Vec<(char, Vec<u16>)>,
}

struct Open {
    c: char,
    rows: Vec<u16>,
}

fn is_row(line: &str) -> bool {
    !line.is_empty() && line.chars().all(|c| c == '#' || c == '.')
}

fn cut_header(line: &str) -> Option<(u8, u8)> {
    let (w, h) = line.strip_prefix('[')?.strip_suffix(']')?.split_once('x')?;
    Some((w.parse().ok()?, h.parse().ok()?))
}

fn glyph_header(line: &str, n: usize) -> Result<Option<char>, String> {
    let mut it = line.chars();
    let Some(c) = it.next() else {
        return Ok(None);
    };
    let Some(hex) = it.as_str().strip_prefix(" U+") else {
        return Ok(None);
    };
    let cp = (4..=6)
        .contains(&hex.len())
        .then(|| u32::from_str_radix(hex, 16).ok())
        .flatten()
        .and_then(char::from_u32)
        .ok_or_else(|| format!("line {n}: bad codepoint U+{hex}"))?;
    if cp != c {
        return Err(format!("line {n}: {c:?} is not U+{hex}"));
    }
    Ok(Some(c))
}

fn id(c: char) -> String {
    format!("U+{:04X}", c as u32)
}

// A header is tried before the comment rule because the '#' glyph's own header, "# U+0023",
// starts with "# ".
pub fn parse(src: &str) -> Result<Vec<Glyphs>, String> {
    let mut cuts: Vec<Glyphs> = Vec::new();
    let mut open: Option<Open> = None;
    let mut last: Option<char> = None;
    for (i, raw) in src.lines().enumerate() {
        let n = i + 1;
        let line = raw.strip_suffix('\r').unwrap_or(raw);
        if let Some(g) = open.as_mut() {
            let cut = cuts
                .last_mut()
                .expect("a glyph is only opened inside a cut");
            if !is_row(line) {
                return Err(format!(
                    "line {n}: {} has {} rows, want {}",
                    id(g.c),
                    g.rows.len(),
                    cut.h
                ));
            }
            if line.len() != usize::from(cut.w) {
                return Err(format!(
                    "line {n}: {} row is {} wide, want {}",
                    id(g.c),
                    line.len(),
                    cut.w
                ));
            }
            let bits = line
                .bytes()
                .fold(0u16, |acc, b| (acc << 1) | u16::from(b == b'#'));
            g.rows.push(bits);
            if g.rows.len() == usize::from(cut.h) {
                let g = open.take().expect("checked above");
                last = Some(g.c);
                cut.map.push((g.c, g.rows));
            }
            continue;
        }
        if line.is_empty() {
            last = None;
            continue;
        }
        if let Some(c) = glyph_header(line, n)? {
            let Some(cut) = cuts.last() else {
                return Err(format!("line {n}: {} before any [WxH] cut", id(c)));
            };
            if cut.map.iter().any(|(k, _)| *k == c) {
                return Err(format!(
                    "line {n}: duplicate {} in [{}x{}]",
                    id(c),
                    cut.w,
                    cut.h
                ));
            }
            open = Some(Open {
                c,
                rows: Vec::new(),
            });
        } else if let Some((w, h)) = cut_header(line) {
            if !(1..=16).contains(&w) || h == 0 {
                return Err(format!(
                    "line {n}: cut [{w}x{h}] must be 1-16 wide and 1+ high"
                ));
            }
            if cuts.iter().any(|g| g.w == w && g.h == h) {
                return Err(format!("line {n}: duplicate cut [{w}x{h}]"));
            }
            cuts.push(Glyphs {
                w,
                h,
                map: Vec::new(),
            });
        } else if line.starts_with("# ") {
        } else if let (true, Some(c)) = (is_row(line), last) {
            return Err(format!("line {n}: {} has more rows than its cut", id(c)));
        } else {
            return Err(format!("line {n}: unexpected {line:?}"));
        }
        last = None;
    }
    if let Some(g) = open {
        let h = cuts.last().map_or(0, |c| c.h);
        return Err(format!(
            "end of file: {} has {} rows, want {h}",
            id(g.c),
            g.rows.len()
        ));
    }
    Ok(cuts)
}

#[cfg(test)]
mod tests {
    use super::*;

    const ONE: &str = "\
# a comment
[3x2]
a U+0061
#.#
.#.

# U+0023
###
#..
";

    #[test]
    fn grid_parser_rejects_wrong_width_and_duplicates() {
        let ok = parse(ONE).unwrap();
        assert_eq!(
            ok,
            vec![Glyphs {
                w: 3,
                h: 2,
                map: vec![('a', vec![0b101, 0b010]), ('#', vec![0b111, 0b100])],
            }]
        );

        let wide = parse("[3x2]\na U+0061\n#.#.\n...\n").unwrap_err();
        assert!(wide.contains("line 3"), "{wide}");
        let narrow = parse("[3x2]\na U+0061\n#.\n...\n").unwrap_err();
        assert!(narrow.contains("line 3"), "{narrow}");

        let short = parse("[3x2]\na U+0061\n#.#\n\nb U+0062\n...\n...\n").unwrap_err();
        assert!(short.contains("U+0061"), "{short}");
        let short_eof = parse("[3x2]\na U+0061\n#.#\n").unwrap_err();
        assert!(short_eof.contains("U+0061"), "{short_eof}");
        let tall = parse("[3x2]\na U+0061\n#.#\n...\n###\n").unwrap_err();
        assert!(tall.contains("U+0061"), "{tall}");

        let dup = parse("[3x2]\na U+0061\n...\n...\na U+0061\n###\n###\n").unwrap_err();
        assert!(dup.contains("duplicate") && dup.contains("U+0061"), "{dup}");
        let dup_cut = parse("[3x2]\n[3x2]\n").unwrap_err();
        assert!(dup_cut.contains("duplicate"), "{dup_cut}");

        assert!(
            parse("a U+0062\n...\n...\n").is_err(),
            "char/codepoint mismatch"
        );
        assert!(
            parse("a U+0061\n...\n...\n").is_err(),
            "glyph before any cut"
        );
        assert!(parse("[17x2]\n").is_err(), "wider than u16");
        assert!(parse("[3x2]\nstray text\n").is_err());
    }
}
