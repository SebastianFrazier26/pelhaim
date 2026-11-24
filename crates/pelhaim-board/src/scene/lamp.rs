use pelhaim_core::{KeepAwakeStatus, Ts};

use super::{Hit, Ov, Scene, SceneInput, console, pad};
use crate::display::R;
use crate::font::{Cut, text_width};

const SQUARE: f32 = 6.0;
const TEXT_DX: f32 = 9.0;
const NOT_HELD_ALPHA: f32 = 0.35;

pub fn label(ka: &KeepAwakeStatus, now: Ts) -> String {
    if ka.mechanism.is_none() {
        "AWAKE n/a".into()
    } else if ka.paused_until.is_some_and(|t| t > now) {
        "AWAKE paused".into()
    } else {
        format!("AWAKE {}", ka.count)
    }
}

pub fn tooltip(ka: &KeepAwakeStatus) -> Vec<String> {
    let mut out = vec![if ka.held {
        format!("held by {} working trains", ka.count)
    } else {
        "not held".into()
    }];
    if ka.on_battery == Some(true) {
        out.push("on battery: lid-close will sleep".into());
    }
    out
}

pub fn width(ka: &KeepAwakeStatus, now: Ts) -> f32 {
    TEXT_DX + text_width(&label(ka, now), Cut::C5x7, 1.0)
}

/// `top` is the top of the 7-unit text row; returns the lamp's extent.
pub(crate) fn draw(out: &mut Scene, ov: &Ov, x: f32, top: f32) -> R {
    let ka = &ov.inp.snap.keep_awake;
    let color = if ka.held {
        ov.p.glyph
    } else {
        ov.p.glyph.alpha(NOT_HELD_ALPHA)
    };
    ov.rect(
        &mut out.list,
        R {
            x,
            y: top + 0.5,
            w: SQUARE,
            h: SQUARE,
        },
        color,
    );
    let text = label(ka, ov.inp.snap.now);
    ov.text(&mut out.list, &text, x + TEXT_DX, top, Cut::C5x7, color);
    let r = pad(
        R {
            x,
            y: top,
            w: width(ka, ov.inp.snap.now),
            h: 7.0,
        },
        2.0,
    );
    ov.hit(out, r, Hit::Lamp);
    r
}

pub fn draw_tooltip(out: &mut Scene, inp: &SceneInput) {
    if inp.view.hover != Some(Hit::Lamp) {
        return;
    }
    let ov = Ov::new(inp);
    let lamp = console::geom(inp).lamp;
    let lines = tooltip(&inp.snap.keep_awake);
    let w = lines
        .iter()
        .map(|l| text_width(l, Cut::C5x7, 1.0))
        .fold(0.0, f32::max)
        + 10.0;
    let h = lines.len() as f32 * 10.0 + 6.0;
    let r = R {
        x: lamp.x.min(ov.w - 8.0 - w).max(0.0),
        y: lamp.y - 4.0 - h,
        w,
        h,
    };
    ov.rect(&mut out.list, r, ov.p.body);
    ov.outline(&mut out.list, r, 1.0, ov.p.frame_dash);
    for (i, l) in lines.iter().enumerate() {
        ov.text(
            &mut out.list,
            l,
            r.x + 5.0,
            r.y + 4.0 + i as f32 * 10.0,
            Cut::C5x7,
            ov.p.label,
        );
    }
}
