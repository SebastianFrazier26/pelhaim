mod lines;
mod tags;

use std::collections::BTreeMap;

use pelhaim_core::{AlertId, LineKey, Snapshot, TrackId, Train, TrainId, TrainState};

use crate::color::Rgba;
use crate::display::{DisplayList, P, Prim, R};
use crate::input::ViewState;
use crate::palette::{BlockState, Palette, PaletteKind, block_color, palette};
use crate::view::View;

pub use lines::{Layout, LineBox, TrackGeom, board_layout};
pub use tags::tag_rect;

pub const TRACK_X0: f32 = 8.0;
pub const LABEL_ROW_H: f32 = 16.0;
pub const TRACK_PITCH: f32 = 28.0;
pub const BUNDLE_GAP: f32 = 36.0;
pub const TRACK_THICK: f32 = 2.5;
pub const BLOCK_GAP: f32 = 4.0;
pub const MAX_VISIBLE_TRACKS: usize = 4;
pub const FOLDED_H: f32 = 10.0;
pub const BLOCK_LENGTHS: [f32; 7] = [14.0, 18.0, 24.0, 18.0, 14.0, 24.0, 18.0];
/// The row under a line's 4 visible tracks that holds its "+N" stub.
pub const STUB_ROW_H: f32 = 12.0;
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

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Hit {
    Train(TrainId),
    Stub(LineKey),
    Alert { id: AlertId, button: AlertButton },
    TrayItem(AlertId),
    Console,
    Lamp,
    InsetAction(InsetAction),
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum AlertButton {
    Option(usize),
    Send,
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
    let layout = board_layout(inp.snap, &inp.view.expanded);
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
    out
}

/// The board alone (no background fill), drawn through `v`. The zoom inset calls this with
/// its own view inside a clip, so the inset is the same scene.
pub fn build_board(out: &mut Scene, inp: &SceneInput, layout: &Layout, v: &View) {
    let cx = Ctx {
        snap: inp.snap,
        layout,
        v,
        paint: Paint::new(palette(inp.palette), inp.now_s, inp.reduced_motion),
        train_on: inp.snap.trains.values().map(|t| (t.track, t)).collect(),
    };
    lines::draw(out, &cx);
    tags::draw(out, &cx, inp.view.selected);
}

pub(crate) struct Ctx<'a> {
    pub snap: &'a Snapshot,
    pub layout: &'a Layout,
    pub v: &'a View,
    pub paint: Paint,
    pub train_on: BTreeMap<TrackId, &'a Train>,
}

impl Ctx<'_> {
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
