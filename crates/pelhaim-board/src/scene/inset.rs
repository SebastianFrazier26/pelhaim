use pelhaim_core::{
    RuntimeRef, Snapshot, StationStatus, TagResolution, Train, format_train_number, truncate_chars,
};

use super::{
    Hit, InsetAction, Layout, Ov, Scene, SceneInput, block_state, build_board, popup, state_words,
    tag_rect, train_line,
};
use crate::color::Rgba;
use crate::display::{Prim, R};
use crate::font::Cut;
use crate::palette::{Palette, block_color};
use crate::view::View;

pub const W_FRAC: f32 = 0.55;
pub const H_FRAC: f32 = 0.5;
pub const BOARD_FRAC: f32 = 0.62;
pub const ZOOM: f32 = 2.4;
pub const TWEEN_S: f64 = 0.15;
pub const TAIL_LINES: usize = 8;
const LINE_H: f32 = 10.0;
const GUTTER: f32 = 8.0;

pub fn session_id(t: &Train) -> &str {
    match &t.runtime {
        RuntimeRef::ObservedClaude { session_id, .. }
        | RuntimeRef::ObservedCodex { session_id, .. } => session_id,
    }
}

/// The right-hand text, top to bottom, each line with its colour.
pub fn info_lines(snap: &Snapshot, t: &Train, p: &Palette) -> Vec<(String, Rgba)> {
    let mut out = vec![
        (
            format!(
                "{} · {}",
                format_train_number(t.number),
                train_line(snap, t)
            ),
            p.label,
        ),
        (t.label.clone(), p.label),
        (state_words(t, snap), block_color(p, block_state(&t.state))),
    ];
    // F31: a mis-tagged session shows the slug it asked for.
    if let TagResolution::MisTagged(slug) = &t.tag {
        out.push((format!("tag: {slug} (unknown project)"), p.fail));
    }
    out.push((
        format!("cwd {}", t.cwd.as_deref().unwrap_or("unknown")),
        p.glyph,
    ));
    out.push((format!("session {}", session_id(t)), p.glyph));
    if let Some(track) = snap.tracks.get(&t.track) {
        for st in track.stations.iter().filter_map(|id| snap.stations.get(id)) {
            let (mark, c) = match st.status {
                StationStatus::Done => ('☑', p.done),
                StationStatus::Active => ('▸', p.work),
            };
            out.push((format!("{mark} {}", st.title), c));
        }
    }
    for l in &t.tail[t.tail.len().saturating_sub(TAIL_LINES)..] {
        out.push((l.clone(), p.done));
    }
    out
}

/// Full motion tweens the zoom from the board's k to k × `ZOOM` over `TWEEN_S`.
pub fn inset_k(main_k: f32, opened_s: f64, now_s: f64, reduced_motion: bool) -> f32 {
    let f = if reduced_motion {
        1.0
    } else {
        ((now_s - opened_s) / TWEEN_S).clamp(0.0, 1.0) as f32
    };
    main_k + (main_k * ZOOM - main_k) * f
}

pub fn frame_rect(viewport: (f32, f32)) -> R {
    let (w, h) = super::overlay_size(viewport);
    R {
        x: w * (1.0 - W_FRAC) / 2.0,
        y: h * (1.0 - H_FRAC) / 2.0,
        w: w * W_FRAC,
        h: h * H_FRAC,
    }
}

fn intersect(a: R, b: R) -> Option<R> {
    let x0 = a.x.max(b.x);
    let y0 = a.y.max(b.y);
    let x1 = (a.x + a.w).min(b.x + b.w);
    let y1 = (a.y + a.h).min(b.y + b.h);
    (x1 > x0 && y1 > y0).then_some(R {
        x: x0,
        y: y0,
        w: x1 - x0,
        h: y1 - y0,
    })
}

pub fn draw(out: &mut Scene, inp: &SceneInput, layout: &Layout) {
    let Some(st) = &inp.view.inset else {
        return;
    };
    let Some(t) = inp.snap.trains.get(&st.train) else {
        return;
    };
    let ov = Ov::new(inp);
    let p = ov.p;
    let r = frame_rect(inp.viewport);
    let body = popup::frame(out, &ov, r);
    popup::tab(out, &ov, r, "Train");

    let left = R {
        w: body.w * BOARD_FRAC,
        ..body
    };
    let clip = ov.v.rect(left);
    let k = inset_k(inp.view.k, st.opened_s, inp.now_s, inp.reduced_motion);
    let tag = tag_rect(layout, t);
    let (cx, cy) = (tag.x + tag.w / 2.0, tag.y + tag.h / 2.0);
    let v = View {
        k,
        ox: clip.x + clip.w / 2.0 - cx * k,
        oy: clip.y + clip.h / 2.0 - cy * k,
    };
    out.list.prims.push(Prim::Clip(clip));
    ov.rect(&mut out.list, left, p.bg);
    let mut board = Scene::default();
    build_board(&mut board, inp, layout, &v);
    out.list.prims.extend(board.list.prims);
    out.list.prims.push(Prim::Unclip);
    // Hits are clipped too, so a tag zoomed past the inset's edge cannot be clicked there.
    out.hits.extend(
        board
            .hits
            .into_iter()
            .filter_map(|(hr, h)| intersect(hr, clip).map(|c| (c, h))),
    );

    let x = left.x + left.w + GUTTER;
    let w = body.x + body.w - x - 6.0;
    let wc = ov.chars_in(w, Cut::C5x7);
    let close = popup::close(out, &ov, body);
    ov.hit(out, close, Hit::InsetAction(InsetAction::Close));
    let buttons_y = body.y + body.h - LINE_H - 2.0;
    let mut y = body.y + 16.0;
    for (s, c) in info_lines(inp.snap, t, &p) {
        if y + LINE_H > buttons_y {
            break;
        }
        ov.text(&mut out.list, &truncate_chars(&s, wc), x, y, Cut::C5x7, c);
        y += LINE_H;
    }
    let b = ov.button(&mut out.list, "[Open cwd]", x, buttons_y, Cut::C5x7, p.idle);
    ov.hit(out, b, Hit::InsetAction(InsetAction::OpenCwd));
    let b2 = ov.button(
        &mut out.list,
        "[Copy session id]",
        b.x + b.w + 6.0,
        buttons_y,
        Cut::C5x7,
        p.idle,
    );
    ov.hit(out, b2, Hit::InsetAction(InsetAction::CopySessionId));
}
