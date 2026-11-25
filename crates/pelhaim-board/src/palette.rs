use crate::color::{Rgba, oklab_mute, oklch};

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum PaletteKind {
    #[default]
    Matte,
    Film,
    HighContrast,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Palette {
    pub bg: Rgba,
    pub idle: Rgba,
    pub work: Rgba,
    pub route: Rgba,
    pub wait: Rgba,
    pub wait_dim: Rgba,
    pub fail: Rgba,
    pub fork: Rgba,
    pub done: Rgba,
    pub label: Rgba,
    pub glyph: Rgba,
    pub frame_navy: Rgba,
    pub frame_dash: Rgba,
    pub frame_white: Rgba,
    pub frame_yellow: Rgba,
    pub body: Rgba,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BlockState {
    Idle,
    Work,
    Route,
    Wait,
    Fail,
    Fork,
    Done,
}

const FILM: Palette = {
    let wait = Rgba::hex(0xF7F06E);
    Palette {
        bg: Rgba::hex(0x0D0B21),
        idle: Rgba::hex(0xA9DADD),
        work: Rgba::hex(0x75D068),
        // route and wait_dim are derived in `with_derived`.
        route: Rgba::hex(0x000000),
        wait,
        wait_dim: Rgba::hex(0x000000),
        fail: Rgba::hex(0xFD6160),
        fork: Rgba::hex(0xD868F5),
        done: Rgba::hex(0x85889A),
        label: Rgba::hex(0xD5D6DA),
        glyph: Rgba::hex(0x74A6A9),
        frame_navy: Rgba::hex(0x0A1837),
        frame_dash: Rgba::hex(0x4FA8C8),
        frame_white: Rgba::hex(0xFFFFFF),
        frame_yellow: wait,
        body: Rgba::hex(0x060A1A),
    }
};

fn with_derived(p: Palette) -> Palette {
    Palette {
        route: oklab_mute(p.work, 0.72, 0.8),
        wait_dim: oklab_mute(p.wait, 0.7, 1.0),
        ..p
    }
}

fn matte() -> Palette {
    let m = |c| oklab_mute(c, 0.88, 0.6);
    let f = FILM;
    let label = m(f.label);
    let wait = m(f.wait);
    Palette {
        idle: m(f.idle),
        work: m(f.work),
        wait,
        fail: m(f.fail),
        fork: m(f.fork),
        label,
        glyph: m(f.glyph),
        frame_dash: m(f.frame_dash),
        frame_white: label,
        frame_yellow: wait,
        ..f
    }
}

pub fn palette(k: PaletteKind) -> Palette {
    with_derived(match k {
        PaletteKind::Matte => matte(),
        PaletteKind::Film => FILM,
        PaletteKind::HighContrast => Palette {
            bg: Rgba::hex(0x000000),
            label: Rgba::hex(0xFFFFFF),
            body: Rgba::hex(0x000000),
            ..FILM
        },
    })
}

pub fn block_color(p: &Palette, s: BlockState) -> Rgba {
    match s {
        BlockState::Idle => p.idle,
        BlockState::Work => p.work,
        BlockState::Route => p.route,
        BlockState::Wait => p.wait,
        BlockState::Fail => p.fail,
        BlockState::Fork => p.fork,
        BlockState::Done => p.done,
    }
}

/// §9.1 (owner, 2026-11-24): every line has its own base colour, like a subway map.
pub const LINE_COLOR_NAMES: [&str; 12] = [
    "rose", "orange", "ochre", "olive", "green", "jade", "teal", "cyan", "blue", "indigo",
    "violet", "magenta",
];
const LINE_HUES: [f32; 12] = [
    10.0, 45.0, 75.0, 105.0, 135.0, 165.0, 195.0, 225.0, 255.0, 285.0, 315.0, 345.0,
];

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum LineColor {
    Palette(usize),
    Rgb(u32),
    /// UNSORTED: not a project, so no hue of its own.
    Neutral,
}

/// A `[[project]].color`: a palette name or `#RRGGBB`.
pub fn parse_line_color(s: &str) -> Option<LineColor> {
    if let Some(hex) = s.strip_prefix('#') {
        return (hex.len() == 6 && hex.bytes().all(|b| b.is_ascii_hexdigit()))
            .then(|| u32::from_str_radix(hex, 16).ok())
            .flatten()
            .map(LineColor::Rgb);
    }
    LINE_COLOR_NAMES
        .iter()
        .position(|n| n.eq_ignore_ascii_case(s))
        .map(LineColor::Palette)
}

/// FNV-1a: stable across Rust versions and platforms, unlike `DefaultHasher`.
pub fn slug_hash(slug: &str) -> u64 {
    slug.bytes().fold(0xcbf2_9ce4_8422_2325, |h, b| {
        (h ^ u64::from(b)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

/// The override if it parses, else a palette slot picked by the slug's hash. The high
/// half: FNV's low bits mix poorly, and short slugs bunched onto a few hues with them.
pub fn line_color(slug: &str, color: Option<&str>) -> LineColor {
    color.and_then(parse_line_color).unwrap_or_else(|| {
        LineColor::Palette(((slug_hash(slug) >> 32) % LINE_COLOR_NAMES.len() as u64) as usize)
    })
}

/// Collision-free colours for a board's project lines (owner, 2026-11-25).
///
/// A parsed `color` wins and reserves its palette slot. The rest, sorted by slug, start
/// at their hashed slot and probe forward to the next free one; only once all 12 are
/// taken does a fresh round start, reusing colours. Sorting by slug (not first-seen
/// order) makes the result a pure function of the set of lines, so it is stable across
/// frames and restarts; a new line moves only lines that sort after it and whose probe
/// runs through its slot.
pub fn assign_line_colors<'a>(
    lines: impl IntoIterator<Item = (&'a str, Option<&'a str>)>,
) -> std::collections::BTreeMap<String, LineColor> {
    let n = LINE_COLOR_NAMES.len();
    let mut out = std::collections::BTreeMap::new();
    let mut taken = vec![false; n];
    let mut free: Vec<&str> = Vec::new();
    for (slug, color) in lines {
        match color.and_then(parse_line_color) {
            Some(c) => {
                if let LineColor::Palette(i) = c {
                    taken[i % n] = true;
                }
                out.insert(slug.to_string(), c);
            }
            None => free.push(slug),
        }
    }
    free.sort_unstable();
    free.dedup();
    for slug in free {
        if taken.iter().all(|&t| t) {
            taken.fill(false);
        }
        let start = ((slug_hash(slug) >> 32) % n as u64) as usize;
        let i = (0..n)
            .map(|k| (start + k) % n)
            .find(|&i| !taken[i])
            .expect("a free slot after the reset");
        taken[i] = true;
        out.insert(slug.to_string(), LineColor::Palette(i));
    }
    out
}

/// Line colours sit darker than the state colours, so a state tag or station never reads
/// as part of its line; red `fail` is nearly as dark, so it is kept apart by distance. High contrast lifts them for the black bg.
pub fn line_rgba(k: PaletteKind, c: LineColor) -> Rgba {
    let (l, ch) = match k {
        PaletteKind::Matte => (0.57, 0.095),
        PaletteKind::Film => (0.64, 0.13),
        PaletteKind::HighContrast => (0.74, 0.14),
    };
    match c {
        LineColor::Palette(i) => oklch(l, ch, LINE_HUES[i % LINE_HUES.len()]),
        LineColor::Rgb(rgb) => Rgba::hex(rgb),
        LineColor::Neutral => oklch(l - 0.06, 0.012, 280.0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::{oklab_dist, oklab_l};

    #[test]
    fn line_palette_is_distinct_readable_and_apart_from_state() {
        for k in [
            PaletteKind::Matte,
            PaletteKind::Film,
            PaletteKind::HighContrast,
        ] {
            let p = palette(k);
            let cs: Vec<Rgba> = (0..LINE_COLOR_NAMES.len())
                .map(|i| line_rgba(k, LineColor::Palette(i)))
                .collect();
            for (i, a) in cs.iter().enumerate() {
                assert!(
                    oklab_l(*a) - oklab_l(p.bg) > 0.35,
                    "{k:?} {i} readable on bg"
                );
                for b in &cs[i + 1..] {
                    assert!(oklab_dist(*a, *b) > 0.04, "{k:?} {i}: {a:?} vs {b:?}");
                }
            }
            if k == PaletteKind::Matte {
                for st in [p.work, p.wait, p.idle] {
                    assert!(
                        cs.iter().all(|c| oklab_l(st) - oklab_l(*c) > 0.08),
                        "state {st:?} is lighter than every line"
                    );
                }
                for st in [p.fail, p.done] {
                    assert!(
                        cs.iter().all(|c| oklab_dist(st, *c) > 0.05),
                        "state {st:?} is apart from every line"
                    );
                }
            }
        }
    }

    const REAL: [&str; 8] = [
        "safeascent",
        "stringcan",
        "g-l-a-s-s",
        "pelhaim",
        "ethica",
        "cabn",
        "loosy-goose",
        "mlb-fansent",
    ];

    #[test]
    fn real_slugs_get_distinct_colours_in_any_order() {
        let a = assign_line_colors(REAL.iter().map(|s| (*s, None)));
        let distinct: std::collections::BTreeSet<_> = a.values().collect();
        assert_eq!(distinct.len(), REAL.len(), "{a:?}");
        let b = assign_line_colors(REAL.iter().rev().map(|s| (*s, None)));
        assert_eq!(a, b, "input order does not matter");
    }

    #[test]
    fn configured_colour_wins_and_reserves_its_slot() {
        let LineColor::Palette(hashed) = line_color("cabn", None) else {
            unreachable!()
        };
        let name = LINE_COLOR_NAMES[hashed];
        let a = assign_line_colors([("cabn", None), ("other", Some(name))]);
        assert_eq!(a["other"], LineColor::Palette(hashed));
        assert_eq!(
            a["cabn"],
            LineColor::Palette((hashed + 1) % 12),
            "probes past it"
        );
        let rgb = assign_line_colors([("cabn", None), ("x", Some("#010203"))]);
        assert_eq!(
            rgb["cabn"],
            LineColor::Palette(hashed),
            "a hex colour reserves no slot"
        );
    }

    #[test]
    fn wraps_only_after_all_twelve() {
        let slugs: Vec<String> = (0..20).map(|i| format!("p{i:02}")).collect();
        let a = assign_line_colors(slugs.iter().map(|s| (s.as_str(), None)));
        let first: std::collections::BTreeSet<_> = slugs[..12].iter().map(|s| a[s]).collect();
        assert_eq!(first.len(), 12, "the first 12 by slug use every colour");
        let second: std::collections::BTreeSet<_> = slugs[12..].iter().map(|s| a[s]).collect();
        assert_eq!(second.len(), 8, "the next round is distinct again");
    }

    #[test]
    fn a_new_line_disturbs_at_most_later_lines_on_its_probe() {
        let base = assign_line_colors(REAL.iter().map(|s| (*s, None)));
        let more = assign_line_colors(REAL.iter().map(|s| (*s, None)).chain([("zzz", None)]));
        for s in REAL {
            assert_eq!(base[s], more[s], "{s}: a slug sorting last moves nobody");
        }
    }

    #[test]
    fn line_color_is_deterministic_and_overridable() {
        assert_eq!(
            line_color("safeascent", None),
            line_color("safeascent", None)
        );
        assert_eq!(slug_hash("a"), 0xaf63_dc4c_8601_ec8c);
        assert_eq!(
            line_color("safeascent", Some("teal")),
            LineColor::Palette(6)
        );
        assert_eq!(line_color("x", Some("#1a2B3c")), LineColor::Rgb(0x1A2B3C));
        assert_eq!(line_color("x", Some("chartreuse")), line_color("x", None));
        for bad in ["#12345", "#12345g", "", "#"] {
            assert_eq!(parse_line_color(bad), None, "{bad:?}");
        }
        let mut seen = std::collections::BTreeSet::new();
        for slug in [
            "safeascent",
            "ethica",
            "cabn",
            "loosy-goose",
            "stringcan",
            "pelhaim",
        ] {
            if let LineColor::Palette(i) = line_color(slug, None) {
                seen.insert(i);
            }
        }
        assert!(seen.len() >= 5, "the hash spreads real slugs: {seen:?}");
    }

    #[test]
    fn matte_matches_spec_table() {
        let m = palette(PaletteKind::Matte);
        assert_eq!(m.idle, Rgba::hex(0x99B5B7));
        assert_eq!(m.work, Rgba::hex(0x76A96F));
        assert_eq!(m.route, Rgba::hex(0x466C41));
        assert_eq!(m.wait, Rgba::hex(0xCCC984));
        assert_eq!(m.fail, Rgba::hex(0xBF6661));
        assert_eq!(m.fork, Rgba::hex(0xA86AB8));
        assert_eq!(m.done, Rgba::hex(0x85889A));
        assert_eq!(m.label, Rgba::hex(0xB4B4B7));
        assert_eq!(m.glyph, Rgba::hex(0x6D898A));
        assert_eq!(m.frame_dash, Rgba::hex(0x5B8A9C));
        assert_eq!(m.bg, Rgba::hex(0x0D0B21));
        assert_eq!(m.frame_navy, Rgba::hex(0x0A1837));
    }

    #[test]
    fn matte_derived_frame_and_dim_tokens() {
        let m = palette(PaletteKind::Matte);
        assert_eq!(m.frame_white, m.label);
        assert_eq!(m.frame_yellow, m.wait);
        assert_eq!(m.body, Rgba::hex(0x060A1A));
        assert_eq!(m.wait_dim, oklab_mute(m.wait, 0.7, 1.0));
    }

    #[test]
    fn film_values() {
        let f = palette(PaletteKind::Film);
        assert_eq!(f.bg, Rgba::hex(0x0D0B21));
        assert_eq!(f.idle, Rgba::hex(0xA9DADD));
        assert_eq!(f.work, Rgba::hex(0x75D068));
        assert_eq!(f.wait, Rgba::hex(0xF7F06E));
        assert_eq!(f.fail, Rgba::hex(0xFD6160));
        assert_eq!(f.fork, Rgba::hex(0xD868F5));
        assert_eq!(f.done, Rgba::hex(0x85889A));
        assert_eq!(f.label, Rgba::hex(0xD5D6DA));
        assert_eq!(f.glyph, Rgba::hex(0x74A6A9));
        assert_eq!(f.frame_navy, Rgba::hex(0x0A1837));
        assert_eq!(f.frame_dash, Rgba::hex(0x4FA8C8));
        assert_eq!(f.frame_white, Rgba::hex(0xFFFFFF));
        assert_eq!(f.frame_yellow, f.wait);
        assert_eq!(f.body, Rgba::hex(0x060A1A));
        assert_eq!(f.route, oklab_mute(f.work, 0.72, 0.8));
        assert_eq!(f.wait_dim, oklab_mute(f.wait, 0.7, 1.0));
    }

    #[test]
    fn high_contrast_is_film_on_black_with_white_labels() {
        let f = palette(PaletteKind::Film);
        let h = palette(PaletteKind::HighContrast);
        assert_eq!(h.bg, Rgba::hex(0x000000));
        assert_eq!(h.label, Rgba::hex(0xFFFFFF));
        assert_eq!(h.body, Rgba::hex(0x000000));
        assert_eq!(
            Palette {
                bg: f.bg,
                label: f.label,
                body: f.body,
                ..h
            },
            f
        );
    }

    #[test]
    fn done_distinct_from_idle() {
        let m = palette(PaletteKind::Matte);
        assert!((oklab_l(m.idle) - oklab_l(m.done)).abs() > 0.1);
    }

    #[test]
    fn block_color_maps_each_state() {
        let p = palette(PaletteKind::Matte);
        let cases = [
            (BlockState::Idle, p.idle),
            (BlockState::Work, p.work),
            (BlockState::Route, p.route),
            (BlockState::Wait, p.wait),
            (BlockState::Fail, p.fail),
            (BlockState::Fork, p.fork),
            (BlockState::Done, p.done),
        ];
        for (s, c) in cases {
            assert_eq!(block_color(&p, s), c, "{s:?}");
        }
    }
}
