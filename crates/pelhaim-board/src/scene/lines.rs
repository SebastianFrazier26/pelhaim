use std::collections::{BTreeMap, BTreeSet};

use pelhaim_core::{LEG_BLOCKS, LineKey, LineView, Snapshot, TrackId, Train, TrainState};

use super::{
    BLOCK_GAP, BLOCK_LENGTHS, BUNDLE_GAP, Ctx, FOLDED_H, Hit, LABEL_PX, LABEL_ROW_H,
    MAX_VISIBLE_TRACKS, STUB_ROW_H, Scene, TRACK_PITCH, TRACK_THICK, TRACK_X0,
};
use crate::color::Rgba;
use crate::display::R;
use crate::font::{Cut, advance, draw_text, line_height, text_width};

#[derive(Clone, PartialEq, Debug)]
pub struct LineBox {
    pub key: LineKey,
    pub top: f32,
    pub h: f32,
}

#[derive(Clone, PartialEq, Debug)]
pub struct TrackGeom {
    pub y: f32,
    pub x0: f32,
    pub blocks: Vec<(f32, f32)>,
    pub station_x: Vec<f32>,
}

/// Board units. `tracks` holds only the tracks drawn: folded lines and tracks behind a
/// "+N" stub have no geometry.
#[derive(Clone, PartialEq, Debug)]
pub struct Layout {
    pub lines: Vec<LineBox>,
    pub tracks: BTreeMap<TrackId, TrackGeom>,
    pub content: R,
}

// The station label (5x7 at 1.3, 9.1 tall) tops out 21 units above its track.
const TRACK_Y_IN_SLOT: f32 = 21.0;
const STATION_LABEL_UP: f32 = 9.0 + 12.0;
const PLATFORM_UP: f32 = 9.0;
const PLATFORM_W: f32 = 33.0;
const PLATFORM_H: f32 = 5.0;
const FOLDED_LEN: f32 = 600.0;
const FOLDED_THICK: f32 = 1.5;
const STAR_OFFSET: f32 = 7.0;
const STAR_HALF: f32 = 3.5;
const STAR_WIDTH: f32 = 1.4;
const FORK_STUB: f32 = 6.0;
const FORK_TO_TRACK: f32 = 4.0;
const TRAILING_IDLE_BLOCKS: usize = 2;

fn visible(line: &LineView, expanded: &BTreeSet<LineKey>) -> usize {
    if expanded.contains(&line.key) {
        line.tracks.len()
    } else {
        line.tracks.len().min(MAX_VISIBLE_TRACKS)
    }
}

fn tan60() -> f32 {
    60f32.to_radians().tan()
}

fn block_count(snap: &Snapshot, id: TrackId) -> usize {
    let legs = snap.tracks.get(&id).map_or(0, |t| t.stations.len()).max(1);
    legs * usize::from(LEG_BLOCKS) + TRAILING_IDLE_BLOCKS
}

fn head_index(train: &Train, n: usize) -> usize {
    let p = train.position;
    (usize::from(p.leg) * usize::from(LEG_BLOCKS) + usize::from(p.block)).min(n - 1)
}

pub fn board_layout(snap: &Snapshot, expanded: &BTreeSet<LineKey>) -> Layout {
    let mut lines = Vec::with_capacity(snap.lines.len());
    let mut ys = BTreeMap::new();
    let mut top = 0.0;
    let mut right: f32 = 0.0;
    for line in &snap.lines {
        let name_w = text_width(&line.name, Cut::C7x9, LABEL_PX);
        right = right.max(TRACK_X0 + name_w);
        let h = if line.active {
            let shown = visible(line, expanded);
            for (i, &t) in line.tracks.iter().take(shown).enumerate() {
                ys.insert(
                    t,
                    top + LABEL_ROW_H + i as f32 * TRACK_PITCH + TRACK_Y_IN_SLOT,
                );
            }
            let stub = if shown < line.tracks.len() {
                STUB_ROW_H
            } else {
                0.0
            };
            LABEL_ROW_H + shown as f32 * TRACK_PITCH + stub
        } else {
            right = right.max(TRACK_X0 + FOLDED_LEN);
            LABEL_ROW_H + FOLDED_H
        };
        lines.push(LineBox {
            key: line.key.clone(),
            top,
            h,
        });
        top += h + BUNDLE_GAP;
    }

    let fork_of: BTreeMap<TrackId, _> = snap.forks.iter().map(|f| (f.child_track, f)).collect();
    let mut tracks = BTreeMap::new();
    for &id in ys.keys() {
        place(snap, id, &ys, &fork_of, &mut tracks, 0);
    }
    for (id, g) in &tracks {
        let end = g.blocks.last().map_or(g.x0, |&(x, w)| x + w);
        right = right.max(end + STAR_OFFSET + STAR_HALF);
        if let (Some(&sx), Some(t)) = (g.station_x.last(), snap.tracks.get(id))
            && let Some(st) = t.stations.last().and_then(|s| snap.stations.get(s))
        {
            right = right.max(sx + text_width(&st.title, Cut::C5x7, LABEL_PX));
        }
    }
    let bottom = lines.last().map_or(0.0, |l| l.top + l.h);
    let mut layout = Layout {
        lines,
        tracks,
        content: R {
            x: 0.0,
            y: 0.0,
            w: right + TRACK_X0,
            h: bottom,
        },
    };
    // A tag ends at its head block, so a train near the track start hangs left of x = 0.
    let left = snap
        .trains
        .values()
        .filter(|t| layout.tracks.contains_key(&t.track))
        .map(|t| super::tag_rect(&layout, t).x - TRACK_X0)
        .fold(0.0f32, f32::min);
    layout.content.x = left;
    layout.content.w -= left;
    layout
}

/// A child track starts where its fork's diagonal lands, so its parent is placed first.
fn place(
    snap: &Snapshot,
    id: TrackId,
    ys: &BTreeMap<TrackId, f32>,
    fork_of: &BTreeMap<TrackId, &pelhaim_core::Fork>,
    out: &mut BTreeMap<TrackId, TrackGeom>,
    depth: usize,
) {
    if out.contains_key(&id) {
        return;
    }
    let y = ys[&id];
    let mut x0 = TRACK_X0;
    // The depth bound only guards against a malformed snapshot with a fork cycle.
    if let Some(f) = fork_of.get(&id)
        && ys.contains_key(&f.parent_track)
        && depth < 64
    {
        place(snap, f.parent_track, ys, fork_of, out, depth + 1);
        let pg = &out[&f.parent_track];
        let fx = fork_x(pg, f);
        x0 = fx + FORK_STUB + (y - pg.y).abs() / tan60() + FORK_TO_TRACK;
    }
    let n = block_count(snap, id);
    let mut x = x0;
    let blocks: Vec<(f32, f32)> = (0..n)
        .map(|i| {
            let w = BLOCK_LENGTHS[(id.0 as usize + i) % BLOCK_LENGTHS.len()];
            let b = (x, w);
            x += w + BLOCK_GAP;
            b
        })
        .collect();
    let stations = snap.tracks.get(&id).map_or(0, |t| t.stations.len());
    let station_x = (0..stations)
        .map(|s| {
            let (bx, bw) = blocks[(s + 1) * usize::from(LEG_BLOCKS) - 1];
            bx + bw
        })
        .collect();
    out.insert(
        id,
        TrackGeom {
            y,
            x0,
            blocks,
            station_x,
        },
    );
}

fn fork_x(parent: &TrackGeom, f: &pelhaim_core::Fork) -> f32 {
    let at = usize::from(f.at.leg) * usize::from(LEG_BLOCKS) + usize::from(f.at.block);
    let (x, w) = parent.blocks[at.min(parent.blocks.len() - 1)];
    x + w
}

pub(super) fn draw(out: &mut Scene, cx: &Ctx) {
    let p = &cx.paint.p;
    for (line, lb) in cx.snap.lines.iter().zip(&cx.layout.lines) {
        draw_label(out, cx, line, lb.top);
        if !line.active {
            let y = lb.top + LABEL_ROW_H + FOLDED_H / 2.0;
            cx.rect(
                &mut out.list,
                R {
                    x: TRACK_X0,
                    y: y - FOLDED_THICK / 2.0,
                    w: FOLDED_LEN,
                    h: FOLDED_THICK,
                },
                p.glyph.alpha(0.5),
            );
            continue;
        }
        for t in &line.tracks {
            if let Some(g) = cx.layout.tracks.get(t) {
                draw_track(out, cx, *t, g);
            }
        }
        let hidden = line
            .tracks
            .iter()
            .filter(|t| !cx.layout.tracks.contains_key(t))
            .count();
        if hidden > 0 {
            let s = format!("+{hidden}");
            let top = lb.top + lb.h - STUB_ROW_H + 1.0;
            draw_text(
                &mut out.list,
                cx.v,
                &s,
                TRACK_X0,
                top,
                Cut::C5x7,
                LABEL_PX,
                p.glyph,
            );
            let r = R {
                x: TRACK_X0 - 3.0,
                y: top - 3.0,
                w: text_width(&s, Cut::C5x7, LABEL_PX) + 6.0,
                h: line_height(Cut::C5x7, LABEL_PX) + 6.0,
            };
            out.hits.push((cx.v.rect(r), Hit::Stub(line.key.clone())));
        }
    }
    for f in &cx.snap.forks {
        draw_fork(out, cx, f);
    }
}

fn draw_label(out: &mut Scene, cx: &Ctx, line: &LineView, top: f32) {
    let p = &cx.paint.p;
    let space = advance(Cut::C7x9, LABEL_PX);
    let mut x = TRACK_X0;
    let mut put = |s: &str, color: Rgba| {
        draw_text(
            &mut out.list,
            cx.v,
            s,
            x,
            top + 2.0,
            Cut::C7x9,
            LABEL_PX,
            color,
        );
        x += text_width(s, Cut::C7x9, LABEL_PX) + space;
    };
    put(&line.name, p.label);
    if line.open_decisions > 0 {
        put(&format!("●{}", line.open_decisions), p.wait);
    }
    if line.format_untested {
        put("⚠", p.glyph);
    }
}

pub(super) fn block_colors(cx: &Ctx, train: Option<&Train>, n: usize) -> Vec<Rgba> {
    let p = &cx.paint.p;
    let Some(t) = train else {
        return vec![p.idle; n];
    };
    if t.parent.is_some() && t.state == TrainState::Done {
        return vec![p.done; n];
    }
    let working = t.state == TrainState::Working;
    let head = head_index(t, n) as isize;
    let tail = head - if working { 5 } else { 3 } + 1;
    let leg_end = (usize::from(t.position.leg) + 1) * usize::from(LEG_BLOCKS) - 1;
    let own = cx.paint.state(&t.state);
    (0..n)
        .map(|i| {
            let b = i as isize;
            if b < tail {
                p.done
            } else if b <= head {
                own
            } else if working && i <= leg_end {
                p.route
            } else {
                p.idle
            }
        })
        .collect()
}

fn draw_track(out: &mut Scene, cx: &Ctx, id: TrackId, g: &TrackGeom) {
    let p = &cx.paint.p;
    let train = cx.train_on.get(&id).copied();
    let colors = block_colors(cx, train, g.blocks.len());
    for (&(x, w), &c) in g.blocks.iter().zip(&colors) {
        cx.rect(
            &mut out.list,
            R {
                x,
                y: g.y - TRACK_THICK / 2.0,
                w,
                h: TRACK_THICK,
            },
            c,
        );
    }

    let stations = cx.snap.tracks.get(&id).map_or(&[][..], |t| &t.stations[..]);
    let adv = advance(Cut::C5x7, LABEL_PX);
    for (i, (&sx, sid)) in g.station_x.iter().zip(stations).enumerate() {
        if let Some(st) = cx.snap.stations.get(sid) {
            let title = match g.station_x.get(i + 1) {
                Some(&next) => {
                    let fit = ((next - sx - adv) / adv).max(0.0) as usize;
                    pelhaim_core::truncate_chars(&st.title, fit)
                }
                None => st.title.clone(),
            };
            draw_text(
                &mut out.list,
                cx.v,
                &title,
                sx,
                g.y - STATION_LABEL_UP,
                Cut::C5x7,
                LABEL_PX,
                p.label,
            );
        }
        let berthed = train.filter(|t| t.position.berthed && usize::from(t.position.leg) == i);
        cx.rect(
            &mut out.list,
            R {
                x: sx,
                y: g.y - PLATFORM_UP,
                w: PLATFORM_W,
                h: PLATFORM_H,
            },
            berthed.map_or(p.idle, |t| cx.paint.state(&t.state)),
        );
    }

    let end = g.blocks.last().map_or(g.x0, |&(x, w)| x + w);
    let (sx, sy) = (end + STAR_OFFSET, g.y);
    let h = STAR_HALF;
    cx.seg(
        &mut out.list,
        (sx - h, sy - h),
        (sx + h, sy + h),
        STAR_WIDTH,
        p.glyph,
    );
    cx.seg(
        &mut out.list,
        (sx - h, sy + h),
        (sx + h, sy - h),
        STAR_WIDTH,
        p.glyph,
    );
}

fn draw_fork(out: &mut Scene, cx: &Ctx, f: &pelhaim_core::Fork) {
    let (Some(pg), Some(cg)) = (
        cx.layout.tracks.get(&f.parent_track),
        cx.layout.tracks.get(&f.child_track),
    ) else {
        return;
    };
    let color = cx
        .train_on
        .get(&f.child_track)
        .map_or(cx.paint.p.idle, |t| cx.paint.state(&t.state));
    let fx = fork_x(pg, f);
    let (py, cy) = (pg.y, cg.y);
    let knee = fx + FORK_STUB;
    let pts = [
        (fx, py),
        (knee, py),
        (knee + (cy - py).abs() / tan60(), cy),
        (cg.x0, cy),
    ];
    for w in pts.windows(2) {
        cx.seg(&mut out.list, w[0], w[1], TRACK_THICK, color);
    }
    // Square caps at each vertex hide the butt-end notches between segments.
    let t = TRACK_THICK;
    for (x, y) in pts {
        cx.rect(
            &mut out.list,
            R {
                x: x - t / 2.0,
                y: y - t / 2.0,
                w: t,
                h: t,
            },
            color,
        );
    }
}

pub(super) fn head_block(g: &TrackGeom, train: &Train) -> (f32, f32) {
    g.blocks[head_index(train, g.blocks.len())]
}
