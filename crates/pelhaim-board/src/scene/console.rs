use pelhaim_core::{Snapshot, format_train_number, truncate_chars};

use super::{
    Hit, Ov, Scene, SceneInput, lamp, popup, state_words, station_title, train_line, tray,
};
use crate::color::Rgba;
use crate::display::R;
use crate::font::{Cut, advance, text_width};
use crate::input::ViewState;
use crate::textinput::TextInput;

pub const PROMPT: &str = "DISPATCH ▸ ";
const PAD_X: f32 = 8.0;
const PAD_Y: f32 = 5.0;
const ROW: f32 = 12.0;
const ITEM_GAP: f32 = 8.0;
const STATUS_GAP: f32 = 12.0;
const LOG_W: f32 = 520.0;
const LOG_ROW: f32 = 11.0;
const LOG_MAX_FRAC: f32 = 0.4;
const DIM_ALPHA: f32 = 0.35;

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct ConsoleGeom {
    pub strip: R,
    pub input: R,
    pub width_chars: usize,
    pub rows: usize,
    pub status_x: f32,
    pub lamp: R,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum StatusTone {
    Lit,
    Dim,
    Warn,
}

/// The right-hand status items before the lamp, left to right.
pub fn status_items(snap: &Snapshot, view: &ViewState) -> Vec<(String, StatusTone)> {
    let mut out = vec![(
        format!("ALERTS {}", tray::queued_count(snap, view)),
        StatusTone::Lit,
    )];
    if snap.decisions_unexported > 0 {
        out.push((
            format!("UNEXPORTED {}", snap.decisions_unexported),
            StatusTone::Warn,
        ));
    }
    out.push(("[obs]".into(), StatusTone::Lit));
    out.push(("[launch]".into(), StatusTone::Dim));
    out.push(("[direct]".into(), StatusTone::Dim));
    out
}

/// The one-line status of the selected train, shown in the empty console (F34).
pub fn selection_status(snap: &Snapshot, view: &ViewState) -> Option<String> {
    let t = snap.trains.get(&view.selected?)?;
    let mut s = format!(
        "{} · {} · {}",
        format_train_number(t.number),
        train_line(snap, t),
        state_words(t, snap)
    );
    if let Some(st) = station_title(snap, t) {
        s.push_str(" · ");
        s.push_str(&st);
    }
    Some(s)
}

fn item_w(s: &str) -> f32 {
    text_width(s, Cut::C5x7, 1.0)
}

pub fn geom(inp: &SceneInput) -> ConsoleGeom {
    let ov = Ov::new(inp);
    let items = status_items(inp.snap, inp.view);
    let lamp_w = lamp::width(&inp.snap.keep_awake, inp.snap.now);
    let status_w: f32 = items.iter().map(|(s, _)| item_w(s) + ITEM_GAP).sum::<f32>() + lamp_w;
    let status_x = ov.w - PAD_X - status_w;
    let input_x = PAD_X + text_width(PROMPT, Cut::C7x9, 1.0);
    let input_w = (status_x - STATUS_GAP - input_x).max(0.0);
    let width_chars = ov.chars_in(input_w, Cut::C7x9).max(1);
    let rows = inp.view.console.rows(width_chars);
    let h = rows as f32 * ROW + 2.0 * PAD_Y;
    let strip = R {
        x: 0.0,
        y: ov.h - h,
        w: ov.w,
        h,
    };
    ConsoleGeom {
        strip,
        input: R {
            x: input_x,
            y: strip.y + PAD_Y,
            w: input_w,
            h: rows as f32 * ROW,
        },
        width_chars,
        rows,
        status_x,
        lamp: R {
            x: ov.w - PAD_X - lamp_w,
            y: strip.y + PAD_Y + 2.5,
            w: lamp_w,
            h: 7.0,
        },
    }
}

pub fn draw(out: &mut Scene, inp: &SceneInput) {
    let ov = Ov::new(inp);
    let p = ov.p;
    let g = geom(inp);
    if inp.view.radio_log_open {
        draw_radio_log(out, &ov, g.strip.y);
    }
    ov.rect(&mut out.list, g.strip, p.frame_navy);
    ov.dashed_h(
        &mut out.list,
        0.0,
        ov.w,
        g.strip.y,
        popup::DASH,
        popup::GAP,
        popup::DASH_T,
        p.frame_dash,
    );
    let text_top = g.input.y + 1.5;
    ov.text(&mut out.list, PROMPT, PAD_X, text_top, Cut::C7x9, p.idle);
    draw_input(out, &ov, &g, &inp.view.console, inp.view.console_focused);
    let c = &inp.view.console;
    if c.text().is_empty()
        && c.preedit.is_none()
        && let Some(s) = selection_status(inp.snap, inp.view)
    {
        let s = truncate_chars(&s, g.width_chars);
        ov.text(&mut out.list, &s, g.input.x, text_top, Cut::C7x9, p.glyph);
    }
    ov.hit(
        out,
        R {
            x: 0.0,
            y: g.strip.y,
            w: g.status_x - STATUS_GAP / 2.0,
            h: g.strip.h,
        },
        Hit::Console,
    );

    let mut x = g.status_x;
    for (s, tone) in status_items(inp.snap, inp.view) {
        let color = match tone {
            StatusTone::Lit => p.label,
            StatusTone::Dim => p.label.alpha(DIM_ALPHA),
            StatusTone::Warn => p.wait,
        };
        ov.text(&mut out.list, &s, x, g.lamp.y, Cut::C5x7, color);
        x += item_w(&s) + ITEM_GAP;
    }
    lamp::draw(out, &ov, g.lamp.x, g.lamp.y);
}

fn draw_input(out: &mut Scene, ov: &Ov, g: &ConsoleGeom, c: &TextInput, focused: bool) {
    let p = &ov.p;
    let (before, after) = c.text().split_at(c.cursor());
    let pre = c.preedit.as_deref().unwrap_or("");
    let chars: Vec<char> = before
        .chars()
        .chain(pre.chars())
        .chain(after.chars())
        .collect();
    let pre_start = before.chars().count();
    let pre_end = pre_start + pre.chars().count();
    let wc = g.width_chars;
    let caret = pre_end;
    // A caret right after a full last row stays at that row's end rather than opening a row
    // `TextInput::rows` did not count.
    let (caret_row, caret_col) = if caret > 0 && caret.is_multiple_of(wc) && caret == chars.len() {
        (caret / wc - 1, wc)
    } else {
        (caret / wc, caret % wc)
    };
    // Scroll so the caret's row is always one of the visible rows.
    let first_row = (caret_row + 1).saturating_sub(g.rows);
    let adv = advance(Cut::C7x9, 1.0);
    for (row, chunk) in chars.chunks(wc).enumerate().skip(first_row).take(g.rows) {
        let y = g.input.y + (row - first_row) as f32 * ROW + 1.5;
        let s: String = chunk.iter().collect();
        ov.text(&mut out.list, &s, g.input.x, y, Cut::C7x9, p.label);
        for col in 0..chunk.len() {
            let i = row * wc + col;
            if (pre_start..pre_end).contains(&i) {
                ov.rect(
                    &mut out.list,
                    R {
                        x: g.input.x + col as f32 * adv,
                        y: y + 10.0,
                        w: adv,
                        h: 1.0,
                    },
                    p.label,
                );
            }
        }
    }
    if focused {
        let row = caret_row - first_row;
        ov.rect(
            &mut out.list,
            R {
                x: g.input.x + caret_col as f32 * adv - 1.0,
                y: g.input.y + row as f32 * ROW + 1.0,
                w: 1.5,
                h: 10.0,
            },
            p.label,
        );
    }
}

fn draw_radio_log(out: &mut Scene, ov: &Ov, strip_y: f32) {
    let p = &ov.p;
    let log = &ov.inp.view.radio_log;
    let w = LOG_W.min(ov.w);
    let max_rows = ((ov.h * LOG_MAX_FRAC - 8.0) / LOG_ROW).floor().max(1.0) as usize;
    let shown = &log[log.len().saturating_sub(max_rows)..];
    let h = shown.len().max(1) as f32 * LOG_ROW + 8.0;
    let r = R {
        x: 0.0,
        y: strip_y - h,
        w,
        h,
    };
    ov.rect(&mut out.list, r, p.body);
    ov.dashed_h(
        &mut out.list,
        0.0,
        w,
        r.y,
        popup::DASH,
        popup::GAP,
        1.0,
        p.frame_dash,
    );
    let wc = ov.chars_in(w - 30.0, Cut::C5x7);
    for (i, card) in shown.iter().enumerate() {
        let y = r.y + 5.0 + i as f32 * LOG_ROW;
        draw_card(out, ov, card, PAD_X, y, wc, p.label);
    }
}

pub(crate) fn draw_card(
    out: &mut Scene,
    ov: &Ov,
    card: &crate::cards::CardRow,
    x: f32,
    y: f32,
    width_chars: usize,
    text_color: Rgba,
) {
    if let Some(g) = card.mark.glyph() {
        ov.text(
            &mut out.list,
            &g.to_string(),
            x,
            y,
            Cut::C5x7,
            card.mark.color(&ov.p),
        );
    }
    let text = truncate_chars(&card.text, width_chars);
    ov.text(&mut out.list, &text, x + 12.0, y, Cut::C5x7, text_color);
}
