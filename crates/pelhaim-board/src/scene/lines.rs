use pelhaim_core::{LineKey, LineView, Snapshot, Station, StationStatus, TrainId};

use super::fold::{LineTrain, line_trains};
use super::{BLOCK_GAP, BLOCK_LENGTHS, BUNDLE_GAP, Ctx, FOLDED_H, LABEL_PX, LABEL_ROW_H, Scene};
use super::{TRACK_THICK, TRACK_X0};
use crate::color::{Rgba, oklab_mute};
use crate::display::R;
use crate::font::{Cut, advance, draw_text, text_width};
use crate::palette::{LineColor, assign_line_colors, slug_hash};

/// Owner, 2026-11-24: lines run most of the way across the screen. Each starts this
/// fraction of the board width in from the left and ends as far in from the right.
pub const LINE_INSET_FRAC: f32 = 0.06;
/// Below this the board is laid out as if this wide, so labels keep some room.
pub const MIN_BOARD_W: f32 = 480.0;
/// A line's path steps between this many levels, `LEVEL_STEP` apart, on 45° diagonals.
pub const LEVELS: u64 = 3;
pub const LEVEL_STEP: f32 = 10.0;
/// From a line's top: the label row, then room for station labels above the top level.
pub const BASE_Y: f32 = LABEL_ROW_H + STATION_LABEL_UP;
pub const LINE_H: f32 = BASE_Y + (LEVELS - 1) as f32 * LEVEL_STEP + 9.0;
/// The station label (5x7 at 1.3, 9.1 tall) tops out 21 units above the track under it.
pub const STATION_LABEL_UP: f32 = 21.0;
pub const MARKER: f32 = 7.0;
const MARKER_HOLE: f32 = 3.0;
const FOLDED_THICK: f32 = 1.5;
const STAR_OFFSET: f32 = 7.0;
const STAR_HALF: f32 = 3.5;
const STAR_WIDTH: f32 = 1.4;

#[derive(Clone, PartialEq, Debug)]
pub struct LineBox {
    pub key: LineKey,
    pub top: f32,
    pub h: f32,
    pub color: LineColor,
}

/// A drawn line: its path, its block chain and its one train, all in board units.
/// Positions along the line are arc lengths `s` from its start.
#[derive(Clone, PartialEq, Debug)]
pub struct LineGeom {
    pub key: LineKey,
    pub color: LineColor,
    /// Vertices, x strictly increasing: level runs joined by 45° diagonals.
    pub pts: Vec<(f32, f32)>,
    pub blocks: Vec<(f32, f32)>,
    pub station_s: Vec<f32>,
    pub head_s: f32,
    pub train: LineTrain,
}

impl LineGeom {
    pub fn len(&self) -> f32 {
        self.pts.windows(2).map(|w| seg_len(w[0], w[1])).sum()
    }

    pub fn is_empty(&self) -> bool {
        self.pts.len() < 2
    }

    pub fn point_at(&self, s: f32) -> (f32, f32) {
        let mut left = s.max(0.0);
        for w in self.pts.windows(2) {
            let l = seg_len(w[0], w[1]);
            if left <= l {
                let f = if l > 0.0 { left / l } else { 0.0 };
                return (
                    w[0].0 + (w[1].0 - w[0].0) * f,
                    w[0].1 + (w[1].1 - w[0].1) * f,
                );
            }
            left -= l;
        }
        *self.pts.last().expect("a path has points")
    }

    pub fn y_at(&self, x: f32) -> f32 {
        for w in self.pts.windows(2) {
            if x <= w[1].0 {
                let dx = w[1].0 - w[0].0;
                let f = if dx > 0.0 {
                    ((x - w[0].0) / dx).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                return w[0].1 + (w[1].1 - w[0].1) * f;
            }
        }
        self.pts.last().map_or(0.0, |p| p.1)
    }

    /// The highest point (least y) of the path over `[x0, x1]`.
    pub fn min_y(&self, x0: f32, x1: f32) -> f32 {
        self.pts
            .iter()
            .filter(|p| p.0 > x0 && p.0 < x1)
            .map(|p| p.1)
            .fold(self.y_at(x0).min(self.y_at(x1)), f32::min)
    }

    pub fn x0(&self) -> f32 {
        self.pts[0].0
    }

    pub fn x1(&self) -> f32 {
        self.pts[self.pts.len() - 1].0
    }

    pub fn station_xy(&self, i: usize) -> (f32, f32) {
        self.point_at(self.station_s[i])
    }

    pub fn head(&self) -> (f32, f32) {
        self.point_at(self.head_s)
    }

    /// The straight pieces of the path between arc lengths `s0` and `s1`.
    pub fn pieces(&self, s0: f32, s1: f32) -> Vec<((f32, f32), (f32, f32))> {
        let mut out = Vec::new();
        let mut at = 0.0;
        for w in self.pts.windows(2) {
            let l = seg_len(w[0], w[1]);
            let (a, b) = (s0.max(at), s1.min(at + l));
            if b > a && l > 0.0 {
                let p = |s: f32| {
                    let f = (s - at) / l;
                    (
                        w[0].0 + (w[1].0 - w[0].0) * f,
                        w[0].1 + (w[1].1 - w[0].1) * f,
                    )
                };
                out.push((p(a), p(b)));
            }
            at += l;
        }
        out
    }
}

fn seg_len(a: (f32, f32), b: (f32, f32)) -> f32 {
    ((b.0 - a.0).powi(2) + (b.1 - a.1).powi(2)).sqrt()
}

/// Board units. `geoms` holds the drawn lines in board order; folded lines have none.
#[derive(Clone, PartialEq, Debug)]
pub struct Layout {
    pub lines: Vec<LineBox>,
    pub geoms: Vec<LineGeom>,
    pub width: f32,
    pub content: R,
}

impl Layout {
    pub fn geom(&self, key: &LineKey) -> Option<&LineGeom> {
        self.geoms.iter().find(|g| &g.key == key)
    }

    /// The line whose train folds in session train `id`.
    pub fn geom_of(&self, id: TrainId) -> Option<&LineGeom> {
        self.geoms.iter().find(|g| g.train.has(id))
    }

    pub fn line_x0(&self) -> f32 {
        self.width * LINE_INSET_FRAC
    }

    pub fn line_x1(&self) -> f32 {
        self.width * (1.0 - LINE_INSET_FRAC)
    }
}

fn seed(key: &LineKey) -> u64 {
    match key {
        LineKey::Project(s) => slug_hash(s),
        LineKey::Unsorted => slug_hash("~unsorted"),
        LineKey::Dispatch => slug_hash("~dispatch"),
    }
}

/// Every line's colour, from the whole set at once so no two projects share one.
pub fn line_colors(snap: &Snapshot) -> Vec<LineColor> {
    let assigned = assign_line_colors(snap.lines.iter().filter_map(|l| match &l.key {
        LineKey::Project(s) => Some((s.as_str(), l.color.as_deref())),
        _ => None,
    }));
    snap.lines
        .iter()
        .map(|l| match &l.key {
            LineKey::Project(s) => assigned[s],
            _ => LineColor::Neutral,
        })
        .collect()
}

/// SplitMix64: a tiny deterministic stream, so a line's turns depend on its slug alone.
fn next(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9e37_79b9_7f4a_7c15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^ (z >> 31)
}

/// 3–5 jogs between levels, each centred near an even division of the span, so the
/// diagonals stay apart and x keeps increasing.
pub fn line_path(key: &LineKey, x0: f32, x1: f32, base: f32) -> Vec<(f32, f32)> {
    let mut st = seed(key);
    let jogs = 3 + next(&mut st) % 3;
    let mut level = next(&mut st) % LEVELS;
    let y = |l: u64| base + l as f32 * LEVEL_STEP;
    let span = x1 - x0;
    let pitch = span / (jogs + 1) as f32;
    let mut pts = vec![(x0, y(level))];
    for i in 0..jogs {
        let jitter = ((next(&mut st) % 1000) as f32 / 1000.0 - 0.5) * pitch * 0.5;
        let c = x0 + pitch * (i + 1) as f32 + jitter;
        let to = (level + 1 + next(&mut st) % (LEVELS - 1)) % LEVELS;
        let dx = (to as f32 - level as f32).abs() * LEVEL_STEP;
        pts.push((c - dx / 2.0, y(level)));
        pts.push((c + dx / 2.0, y(to)));
        level = to;
    }
    pts.push((x1, y(level)));
    pts
}

fn blocks(key: &LineKey, len: f32) -> Vec<(f32, f32)> {
    let mut i = (seed(key) % BLOCK_LENGTHS.len() as u64) as usize;
    let mut s = 0.0;
    let mut out = Vec::new();
    while s < len {
        let w = BLOCK_LENGTHS[i % BLOCK_LENGTHS.len()].min(len - s);
        out.push((s, s + w));
        s += w + BLOCK_GAP;
        i += 1;
    }
    out
}

pub fn board_layout(snap: &Snapshot, width: f32) -> Layout {
    let width = width.max(MIN_BOARD_W);
    let (x0, x1) = (width * LINE_INSET_FRAC, width * (1.0 - LINE_INSET_FRAC));
    let mut trains: Vec<LineTrain> = line_trains(snap);
    let mut lines = Vec::with_capacity(snap.lines.len());
    let mut geoms = Vec::new();
    let mut top = 0.0;
    for (line, color) in snap.lines.iter().zip(line_colors(snap)) {
        let h = if line.key == LineKey::Dispatch {
            LABEL_ROW_H
        } else if let Some(i) = trains.iter().position(|t| t.line == line.key)
            && line.active
        {
            let train = trains.remove(i);
            let pts = line_path(&line.key, x0, x1, top + BASE_Y);
            let mut g = LineGeom {
                key: line.key.clone(),
                color,
                pts,
                blocks: Vec::new(),
                station_s: Vec::new(),
                head_s: 0.0,
                train,
            };
            let len = g.len();
            let legs = (g.train.stations.len() + 1) as f32;
            g.blocks = blocks(&line.key, len);
            g.station_s = (0..g.train.stations.len())
                .map(|i| len * (i + 1) as f32 / legs)
                .collect();
            g.head_s = len * (g.train.at / legs).clamp(0.0, 1.0);
            geoms.push(g);
            LINE_H
        } else {
            LABEL_ROW_H + FOLDED_H
        };
        lines.push(LineBox {
            key: line.key.clone(),
            top,
            h,
            color,
        });
        top += h + BUNDLE_GAP;
    }
    let bottom = lines.last().map_or(0.0, |l| l.top + l.h);
    let mut layout = Layout {
        lines,
        geoms,
        width,
        content: R {
            x: 0.0,
            y: 0.0,
            w: width,
            h: bottom,
        },
    };
    // A tag ends at the train's head, so a train at the line start can hang left of x = 0.
    let left = layout
        .geoms
        .iter()
        .map(|g| super::tags::tag_rect_in(g).x - TRACK_X0)
        .fold(0.0f32, f32::min);
    layout.content.x = left;
    layout.content.w -= left;
    layout
}

pub(super) fn draw(out: &mut Scene, cx: &Ctx) {
    for (line, lb) in cx.snap.lines.iter().zip(&cx.layout.lines) {
        draw_label(out, cx, line, lb.top);
        if line.key == LineKey::Dispatch {
            continue;
        }
        let base = cx.line_rgba(lb.color);
        match cx.layout.geom(&line.key) {
            Some(g) => draw_line(out, cx, g, base),
            None => {
                let y = lb.top + LABEL_ROW_H + FOLDED_H / 2.0;
                cx.rect(
                    &mut out.list,
                    R {
                        x: cx.layout.line_x0(),
                        y: y - FOLDED_THICK / 2.0,
                        w: cx.layout.line_x1() - cx.layout.line_x0(),
                        h: FOLDED_THICK,
                    },
                    base.alpha(0.5),
                );
            }
        }
    }
}

fn running_words(n: u32) -> String {
    match n {
        1 => "1 agent running".into(),
        n => format!("{n} agents running"),
    }
}

/// §7.10: a train-board clock, shown only on the station the train is running to.
pub(super) fn arrival_text(snap: &Snapshot, st: &Station) -> Option<String> {
    if st.status != StationStatus::Active {
        return None;
    }
    let offset = chrono::FixedOffset::east_opt(snap.utc_offset_secs)?;
    Some(format!(
        "ARR {}",
        st.eta?.with_timezone(&offset).format("%H:%M")
    ))
}

fn draw_label(out: &mut Scene, cx: &Ctx, line: &LineView, top: f32) {
    let p = &cx.paint.p;
    let space = advance(Cut::C7x9, LABEL_PX);
    let mut x = cx.layout.line_x0();
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
    if line.key == LineKey::Dispatch && line.active {
        let c = if line.has_open_alert { p.wait } else { p.glyph };
        put(&format!("· {}", running_words(line.running)), c);
    }
    if line.open_decisions > 0 {
        put(&format!("●{}", line.open_decisions), p.wait);
    }
    if line.format_untested {
        put("⚠", p.glyph);
    }
}

/// Ahead of the train the line's own colour is dimmed; behind it, full. Progress only:
/// state lives on the tag and the stations (owner, 2026-11-24).
pub fn ahead_rgba(base: Rgba) -> Rgba {
    oklab_mute(base, 0.62, 0.7)
}

pub(super) fn block_rgba(g: &LineGeom, base: Rgba) -> Vec<Rgba> {
    let ahead = ahead_rgba(base);
    g.blocks
        .iter()
        .map(|&(s0, s1)| {
            if (s0 + s1) / 2.0 <= g.head_s {
                base
            } else {
                ahead
            }
        })
        .collect()
}

/// A station's label: its title cut to the room before the next station (or the line
/// end), then the clock if it is the one the train runs to.
pub fn station_label(snap: &Snapshot, g: &LineGeom, i: usize) -> Option<(String, Option<String>)> {
    let st = snap.stations.get(&g.train.stations[i])?;
    let adv = advance(Cut::C5x7, LABEL_PX);
    let (sx, _) = g.station_xy(i);
    let room_end = match g.station_s.get(i + 1) {
        Some(_) => g.station_xy(i + 1).0 - MARKER / 2.0,
        None => g.x1() + STAR_OFFSET + STAR_HALF,
    };
    let arr = (g.train.next == Some(i))
        .then(|| arrival_text(snap, st))
        .flatten();
    let arr_w = arr
        .as_ref()
        .map_or(0.0, |a| text_width(a, Cut::C5x7, LABEL_PX) + adv);
    let room = room_end - (sx - MARKER / 2.0) - adv - arr_w;
    let fit = (room / adv).max(0.0) as usize;
    Some((pelhaim_core::truncate_chars(&st.title, fit), arr))
}

/// The label's top-left: lifted above the highest point of the path under it, so the
/// line never runs through its own labels.
pub fn station_label_at(snap: &Snapshot, g: &LineGeom, i: usize) -> Option<(f32, f32, f32)> {
    let (title, arr) = station_label(snap, g, i)?;
    let adv = advance(Cut::C5x7, LABEL_PX);
    let x = g.station_xy(i).0 - MARKER / 2.0;
    let w = text_width(&title, Cut::C5x7, LABEL_PX)
        + arr
            .as_ref()
            .map_or(0.0, |a| adv + text_width(a, Cut::C5x7, LABEL_PX));
    Some((x, g.min_y(x, x + w) - STATION_LABEL_UP, w))
}

pub fn earlier_label(g: &LineGeom) -> Option<(String, f32, f32, f32)> {
    let n = g.train.folded;
    if n == 0 {
        return None;
    }
    let s = format!("+{n} earlier");
    let x = g.x0();
    let w = text_width(&s, Cut::C5x7, LABEL_PX);
    let y = g.min_y(x, x + w) - STATION_LABEL_UP;
    Some((s, x, y, w))
}

fn draw_line(out: &mut Scene, cx: &Ctx, g: &LineGeom, base: Rgba) {
    let p = &cx.paint.p;
    let t = TRACK_THICK;
    for (&(s0, s1), c) in g.blocks.iter().zip(block_rgba(g, base)) {
        let pieces = g.pieces(s0, s1);
        for (i, &(a, b)) in pieces.iter().enumerate() {
            if a.1 == b.1 {
                cx.rect(
                    &mut out.list,
                    R {
                        x: a.0,
                        y: a.1 - t / 2.0,
                        w: b.0 - a.0,
                        h: t,
                    },
                    c,
                );
            } else {
                cx.seg(&mut out.list, a, b, t, c);
            }
            // A square cap where a block bends hides the notch between its pieces.
            if i > 0 {
                cx.rect(
                    &mut out.list,
                    R {
                        x: a.0 - t / 2.0,
                        y: a.1 - t / 2.0,
                        w: t,
                        h: t,
                    },
                    c,
                );
            }
        }
    }

    let (ex, ey) = (g.x1() + STAR_OFFSET, g.pts[g.pts.len() - 1].1);
    let h = STAR_HALF;
    cx.seg(
        &mut out.list,
        (ex - h, ey - h),
        (ex + h, ey + h),
        STAR_WIDTH,
        p.glyph,
    );
    cx.seg(
        &mut out.list,
        (ex - h, ey + h),
        (ex + h, ey - h),
        STAR_WIDTH,
        p.glyph,
    );

    if let Some((s, x, y, _)) = earlier_label(g) {
        draw_text(&mut out.list, cx.v, &s, x, y, Cut::C5x7, LABEL_PX, p.glyph);
    }
    let adv = advance(Cut::C5x7, LABEL_PX);
    let state = cx.paint.state(g.train.state(cx.snap));
    let last = g.station_s.len().checked_sub(1);
    for i in 0..g.station_s.len() {
        let Some(st) = cx.snap.stations.get(&g.train.stations[i]) else {
            continue;
        };
        let (sx, sy) = g.station_xy(i);
        let here = g.train.berthed && (g.train.next.or(last) == Some(i));
        let ring = if here {
            state
        } else if st.overdue(cx.snap.now) {
            p.wait
        } else if st.status == StationStatus::Done {
            p.done
        } else {
            p.idle
        };
        let m = R {
            x: sx - MARKER / 2.0,
            y: sy - MARKER / 2.0,
            w: MARKER,
            h: MARKER,
        };
        cx.rect(&mut out.list, m, ring);
        if !here {
            let d = (MARKER - MARKER_HOLE) / 2.0;
            cx.rect(
                &mut out.list,
                R {
                    x: m.x + d,
                    y: m.y + d,
                    w: MARKER_HOLE,
                    h: MARKER_HOLE,
                },
                p.bg,
            );
        }

        let (Some((title, arr)), Some((lx, ly, _))) = (
            station_label(cx.snap, g, i),
            station_label_at(cx.snap, g, i),
        ) else {
            continue;
        };
        draw_text(
            &mut out.list,
            cx.v,
            &title,
            lx,
            ly,
            Cut::C5x7,
            LABEL_PX,
            p.label,
        );
        if let Some(arr) = arr {
            let c = if st.overdue(cx.snap.now) {
                p.wait
            } else {
                p.glyph
            };
            draw_text(
                &mut out.list,
                cx.v,
                &arr,
                lx + text_width(&title, Cut::C5x7, LABEL_PX) + adv,
                ly,
                Cut::C5x7,
                LABEL_PX,
                c,
            );
        }
    }
}
