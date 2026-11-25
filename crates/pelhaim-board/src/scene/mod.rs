pub mod console;
pub mod fold;
pub mod inset;
pub mod lamp;
pub mod lines;
pub mod popup;
mod tags;
pub mod tray;

use pelhaim_core::{AlertId, AlertKind, LineKey, Snapshot, Train, TrainId, TrainState};

use crate::color::Rgba;
use crate::display::{DisplayList, P, Prim, R};
use crate::font::{Cut, advance, draw_text, text_width};
use crate::input::ViewState;
use crate::palette::{
    BlockState, LineColor, Palette, PaletteKind, block_color, line_rgba, palette,
};
use crate::view::View;

pub use fold::{LineTrain, line_train_of, line_trains, state_rank};
pub use lines::{Layout, LineBox, LineGeom, board_layout};
pub use tags::tag_rect;

/// The left margin a tag hanging off the line start keeps.
pub const TRACK_X0: f32 = 8.0;
pub const LABEL_ROW_H: f32 = 16.0;
/// The arrow-key pan step and the height `F` fits around a line.
pub const TRACK_PITCH: f32 = 28.0;
pub const BUNDLE_GAP: f32 = 20.0;
pub const TRACK_THICK: f32 = 2.5;
pub const BLOCK_GAP: f32 = 4.0;
pub const FOLDED_H: f32 = 10.0;
pub const BLOCK_LENGTHS: [f32; 7] = [14.0, 18.0, 24.0, 18.0, 14.0, 24.0, 18.0];
pub const LABEL_PX: f32 = 1.3;

pub struct SceneInput<'a> {
    pub snap: &'a Snapshot,
    pub view: &'a ViewState,
    pub viewport: (f32, f32),
    pub now_s: f64,
    pub palette: PaletteKind,
    pub reduced_motion: bool,
}

#[derive(Default, Clone, PartialEq, Debug)]
pub struct Scene {
    pub list: DisplayList,
    /// Screen-space hit rects, in draw order (later entries are on top).
    pub hits: Vec<(R, Hit)>,
}

impl Scene {
    /// The topmost hit under a screen point: the last one pushed wins.
    pub fn hit_at(&self, x: f32, y: f32) -> Option<&Hit> {
        self.hits
            .iter()
            .rev()
            .find(|(r, _)| r.contains(x, y))
            .map(|(_, h)| h)
    }
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Hit {
    /// A line train's tag, by its lead session's train.
    Train(TrainId),
    Alert {
        id: AlertId,
        button: AlertButton,
    },
    TrayItem(AlertId),
    Console,
    Lamp,
    InsetAction(InsetAction),
    /// A popup's or the inset's background: swallows the click so it cannot reach the board.
    Blocker,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum AlertButton {
    Option(usize),
    LogDecision,
    Close,
    CopySessionId,
    ProjectPick(usize),
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum InsetAction {
    OpenCwd,
    CopySessionId,
    Close,
}

pub fn build_scene(inp: &SceneInput) -> Scene {
    let layout = board_layout(inp.snap, inp.viewport.0);
    let p = palette(inp.palette);
    let mut out = Scene::default();
    out.list.prims.push(Prim::Rect {
        r: R {
            x: 0.0,
            y: 0.0,
            w: inp.viewport.0,
            h: inp.viewport.1,
        },
        color: p.bg,
        aa: false,
    });
    build_board(&mut out, inp, &layout, &inp.view.view());
    build_overlays(&mut out, inp, &layout);
    out
}

/// Screen-space overlays, drawn after (over) the board and never zoomed with it. Hits are
/// pushed in the same order, so a popup's buttons sit on top of the tray and the board.
pub fn build_overlays(out: &mut Scene, inp: &SceneInput, layout: &Layout) {
    tray::draw(out, inp);
    console::draw(out, inp);
    inset::draw(out, inp, layout);
    popup::draw(out, inp, layout);
    lamp::draw_tooltip(out, inp);
}

/// The board alone (no background fill), drawn through `v`. The zoom inset calls this with
/// its own view inside a clip, so the inset is the same scene.
pub fn build_board(out: &mut Scene, inp: &SceneInput, layout: &Layout, v: &View) {
    let cx = Ctx {
        snap: inp.snap,
        layout,
        v,
        paint: Paint::new(palette(inp.palette), inp.now_s, inp.reduced_motion),
        kind: inp.palette,
    };
    lines::draw(out, &cx);
    tags::draw(out, &cx, inp.view.selected);
}

pub(crate) struct Ctx<'a> {
    pub snap: &'a Snapshot,
    pub layout: &'a Layout,
    pub v: &'a View,
    pub paint: Paint,
    pub kind: PaletteKind,
}

impl Ctx<'_> {
    pub fn line_rgba(&self, c: LineColor) -> Rgba {
        line_rgba(self.kind, c)
    }

    pub fn rect(&self, dl: &mut DisplayList, r: R, color: Rgba) {
        dl.prims.push(Prim::Rect {
            r: self.v.rect(r),
            color,
            aa: true,
        });
    }

    pub fn seg(&self, dl: &mut DisplayList, a: (f32, f32), b: (f32, f32), width: f32, color: Rgba) {
        let p = |(x, y): (f32, f32)| P {
            x: self.v.x(x),
            y: self.v.y(y),
        };
        dl.prims.push(Prim::Seg {
            a: p(a),
            b: p(b),
            width: width * self.v.k,
            color,
        });
    }

    pub fn outline(&self, dl: &mut DisplayList, r: R, t: f32, color: Rgba) {
        for side in [
            R { h: t, ..r },
            R {
                y: r.y + r.h - t,
                h: t,
                ..r
            },
            R { w: t, ..r },
            R {
                x: r.x + r.w - t,
                w: t,
                ..r
            },
        ] {
            self.rect(dl, side, color);
        }
    }
}

pub(crate) struct Paint {
    pub p: Palette,
    wait_now: Rgba,
}

impl Paint {
    fn new(p: Palette, now_s: f64, reduced_motion: bool) -> Self {
        let wait_now = if reduced_motion {
            p.wait
        } else {
            let f = 0.5 - 0.5 * (std::f64::consts::TAU * now_s).cos();
            lerp(p.wait, p.wait_dim, f)
        };
        Paint { p, wait_now }
    }

    pub fn state(&self, s: &TrainState) -> Rgba {
        match block_state(s) {
            BlockState::Wait => self.wait_now,
            b => block_color(&self.p, b),
        }
    }
}

/// Stalled reads as waiting (yellow): the owner is the one who has to look.
pub fn block_state(s: &TrainState) -> BlockState {
    match s {
        TrainState::Working => BlockState::Work,
        TrainState::Waiting(_) | TrainState::ProbablyWaiting | TrainState::Stalled => {
            BlockState::Wait
        }
        TrainState::Failed(_) => BlockState::Fail,
        TrainState::Done => BlockState::Done,
        TrainState::Idle => BlockState::Idle,
    }
}

fn lerp(a: Rgba, b: Rgba, f: f64) -> Rgba {
    let mut out = [0u8; 4];
    for (o, (&x, &y)) in out.iter_mut().zip(a.0.iter().zip(b.0.iter())) {
        *o = (f64::from(x) + (f64::from(y) - f64::from(x)) * f).round() as u8;
    }
    Rgba(out)
}

/// Overlay units are screen pixels / `OVERLAY_K`.
pub const OVERLAY_K: f32 = 1.5;

pub fn overlay_view() -> View {
    View {
        k: OVERLAY_K,
        ox: 0.0,
        oy: 0.0,
    }
}

/// The viewport in overlay units.
pub fn overlay_size(viewport: (f32, f32)) -> (f32, f32) {
    (viewport.0 / OVERLAY_K, viewport.1 / OVERLAY_K)
}

pub(crate) struct Ov<'a> {
    pub inp: &'a SceneInput<'a>,
    pub p: Palette,
    pub v: View,
    pub w: f32,
    pub h: f32,
}

impl<'a> Ov<'a> {
    pub fn new(inp: &'a SceneInput<'a>) -> Self {
        let (w, h) = overlay_size(inp.viewport);
        Ov {
            inp,
            p: palette(inp.palette),
            v: overlay_view(),
            w,
            h,
        }
    }

    pub fn rect(&self, dl: &mut DisplayList, r: R, color: Rgba) {
        dl.prims.push(Prim::Rect {
            r: self.v.rect(r),
            color,
            aa: true,
        });
    }

    pub fn outline(&self, dl: &mut DisplayList, r: R, t: f32, color: Rgba) {
        for side in [
            R { h: t, ..r },
            R {
                y: r.y + r.h - t,
                h: t,
                ..r
            },
            R { w: t, ..r },
            R {
                x: r.x + r.w - t,
                w: t,
                ..r
            },
        ] {
            self.rect(dl, side, color);
        }
    }

    /// Dashes along all four sides, inside `r`.
    pub fn dashed(&self, dl: &mut DisplayList, r: R, dash: f32, gap: f32, t: f32, color: Rgba) {
        self.dashed_h(dl, r.x, r.x + r.w, r.y, dash, gap, t, color);
        self.dashed_h(dl, r.x, r.x + r.w, r.y + r.h - t, dash, gap, t, color);
        let mut y = r.y + t + gap;
        while y < r.y + r.h - t - gap {
            let len = dash.min(r.y + r.h - t - gap - y);
            for x in [r.x, r.x + r.w - t] {
                self.rect(dl, R { x, y, w: t, h: len }, color);
            }
            y += dash + gap;
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn dashed_h(
        &self,
        dl: &mut DisplayList,
        x0: f32,
        x1: f32,
        y: f32,
        dash: f32,
        gap: f32,
        t: f32,
        color: Rgba,
    ) {
        let mut x = x0;
        while x < x1 {
            let w = dash.min(x1 - x);
            self.rect(dl, R { x, y, w, h: t }, color);
            x += dash + gap;
        }
    }

    pub fn text(&self, dl: &mut DisplayList, s: &str, x: f32, y: f32, cut: Cut, color: Rgba) {
        draw_text(dl, &self.v, s, x, y, cut, 1.0, color);
    }

    /// Draws `s` and returns its padded extent, for a button hit.
    pub fn button(
        &self,
        dl: &mut DisplayList,
        s: &str,
        x: f32,
        y: f32,
        cut: Cut,
        color: Rgba,
    ) -> R {
        self.text(dl, s, x, y, cut, color);
        pad(
            R {
                x,
                y,
                w: text_width(s, cut, 1.0),
                h: cut_h(cut),
            },
            2.0,
        )
    }

    pub fn hit(&self, out: &mut Scene, r: R, h: Hit) {
        out.hits.push((self.v.rect(r), h));
    }

    pub fn chars_in(&self, w: f32, cut: Cut) -> usize {
        (w / advance(cut, 1.0)).floor().max(0.0) as usize
    }
}

pub(crate) fn cut_h(cut: Cut) -> f32 {
    match cut {
        Cut::C5x7 => 7.0,
        Cut::C7x9 => 9.0,
    }
}

pub(crate) fn pad(r: R, d: f32) -> R {
    R {
        x: r.x - d,
        y: r.y - d,
        w: r.w + 2.0 * d,
        h: r.h + 2.0 * d,
    }
}

pub fn line_name(snap: &Snapshot, key: &LineKey) -> String {
    if let Some(l) = snap.lines.iter().find(|l| &l.key == key) {
        return l.name.clone();
    }
    match key {
        LineKey::Dispatch => "Dispatch".into(),
        LineKey::Unsorted => "Unsorted".into(),
        LineKey::Project(s) => s.clone(),
    }
}

pub fn train_line(snap: &Snapshot, t: &Train) -> String {
    snap.tracks
        .get(&t.track)
        .map_or_else(|| "?".into(), |tr| line_name(snap, &tr.line))
}

/// The station a session's train is at (or the last one, once it is past every leg).
pub fn current_station(snap: &Snapshot, t: &Train) -> Option<pelhaim_core::StationId> {
    let st = &snap.tracks.get(&t.track)?.stations;
    st.get(usize::from(t.position.leg)).or(st.last()).copied()
}

pub fn station_title(snap: &Snapshot, t: &Train) -> Option<String> {
    current_station(snap, t).and_then(|id| snap.stations.get(&id).map(|s| s.title.clone()))
}

pub fn agents_words(n: usize) -> String {
    match n {
        1 => "1 agent".into(),
        n => format!("{n} agents"),
    }
}

/// The words for a train's state, so state is never colour-only. Shared by the inset and
/// the a11y names (F37).
pub fn state_words(t: &Train, snap: &Snapshot) -> String {
    state_text(&t.state, snap)
}

pub fn state_text(s: &TrainState, snap: &Snapshot) -> String {
    match s {
        TrainState::Working => "working".into(),
        TrainState::Idle => "idle".into(),
        TrainState::Waiting(id) => {
            let q = snap
                .alerts
                .iter()
                .find(|a| &a.id == id)
                .and_then(|a| match &a.kind {
                    AlertKind::Question { questions } => {
                        questions.first().map(|q| q.question.clone())
                    }
                    _ => None,
                });
            match q {
                Some(q) => format!("waiting on you: {q}"),
                None => "waiting on you".into(),
            }
        }
        TrainState::ProbablyWaiting => "probably waiting on you in the terminal".into(),
        TrainState::Failed(r) => format!("failed: {r}"),
        TrainState::Done => "done".into(),
        TrainState::Stalled => "stalled".into(),
    }
}

/// Word wrap at `width` chars. A word wider than the line is broken by chars (F35): CJK and
/// long tokens have no spaces to break at.
pub fn wrap(text: &str, width: usize) -> Vec<String> {
    let width = width.max(1);
    let mut out = Vec::new();
    for para in text.lines() {
        let para = pelhaim_core::sanitize_line(para);
        if para.is_empty() {
            continue;
        }
        let mut line = String::new();
        let mut n = 0;
        for word in para.split(' ') {
            let mut chars: Vec<char> = word.chars().collect();
            if n > 0 && n + 1 + chars.len() <= width {
                line.push(' ');
                line.extend(&chars);
                n += 1 + chars.len();
                continue;
            }
            if n > 0 {
                out.push(std::mem::take(&mut line));
            }
            while chars.len() > width {
                out.push(chars.drain(..width).collect());
            }
            line.extend(&chars);
            n = chars.len();
        }
        if n > 0 {
            out.push(line);
        }
    }
    out
}
