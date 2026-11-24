use crate::color::{Rgba, oklab_mute};

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::oklab_l;

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
