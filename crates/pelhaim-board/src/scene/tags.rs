use pelhaim_core::{TagResolution, Train, TrainId, format_train_number};

use super::lines::head_block;
use super::{Ctx, Hit, Layout, Scene};
use crate::display::{P, Prim, R};
use crate::font::{Cut, advance, draw_text};

const TAG_H: f32 = 9.0;
const TAG_PAD: f32 = 4.0;
const ARROW_W: f32 = 7.0;
const DIGIT_PX: f32 = 1.0;
const UNDERLINE_GAP: f32 = 1.6;
const HIT_PAD: f32 = 3.0;

fn body_w(train: &Train) -> f32 {
    let digits = format_train_number(train.number).chars().count() as f32;
    TAG_PAD + digits * advance(Cut::C5x7, DIGIT_PX)
}

/// The tag body plus arrowhead, in board units. A train whose track is not laid out (folded
/// line, or behind a "+N" stub) gets an empty rect at the origin, which no hit test matches.
pub fn tag_rect(layout: &Layout, train: &Train) -> R {
    let Some(g) = layout.tracks.get(&train.track) else {
        return R {
            x: 0.0,
            y: 0.0,
            w: 0.0,
            h: 0.0,
        };
    };
    let (x, w) = head_block(g, train);
    let tw = body_w(train) + ARROW_W;
    R {
        x: x + w - tw,
        y: g.y - TAG_H / 2.0,
        w: tw,
        h: TAG_H,
    }
}

pub(super) fn draw(out: &mut Scene, cx: &Ctx, selected: Option<TrainId>) {
    let p = &cx.paint.p;
    for t in cx.snap.trains.values() {
        if !cx.layout.tracks.contains_key(&t.track) {
            continue;
        }
        let r = tag_rect(cx.layout, t);
        let color = cx.paint.state(&t.state);
        let bw = body_w(t);
        let ring = R {
            x: r.x - 1.0,
            y: r.y - 1.0,
            w: r.w + 2.0,
            h: r.h + 2.0,
        };
        cx.rect(&mut out.list, ring, p.bg);
        if matches!(t.tag, TagResolution::MisTagged(_)) {
            cx.outline(&mut out.list, ring, 1.0, p.fail);
        }
        cx.rect(&mut out.list, R { w: bw, ..r }, color);
        let tip_x = r.x + bw;
        let pt = |x: f32, y: f32| P {
            x: cx.v.x(x),
            y: cx.v.y(y),
        };
        out.list.prims.push(Prim::Tri {
            p: [
                pt(tip_x + ARROW_W, r.y + TAG_H / 2.0),
                pt(tip_x, r.y - 0.5),
                pt(tip_x, r.y + TAG_H + 0.5),
            ],
            color,
        });
        draw_text(
            &mut out.list,
            cx.v,
            &format_train_number(t.number),
            r.x + 2.0,
            r.y + 1.0,
            Cut::C5x7,
            DIGIT_PX,
            p.bg,
        );
        // Context-use meter: the fill is unknown in M1, so every dot is the empty glyph colour.
        let mut dx = r.x + 0.5;
        while dx < r.x + bw - 0.5 {
            cx.rect(
                &mut out.list,
                R {
                    x: dx,
                    y: r.y + TAG_H + UNDERLINE_GAP,
                    w: 1.0,
                    h: 1.0,
                },
                p.glyph,
            );
            dx += 2.0;
        }
        let hit = R {
            x: r.x - HIT_PAD,
            y: r.y - HIT_PAD,
            w: r.w + 2.0 * HIT_PAD,
            h: r.h + 2.0 * HIT_PAD,
        };
        out.hits.push((cx.v.rect(hit), Hit::Train(t.id)));
    }

    if let Some(t) = selected.and_then(|id| cx.snap.trains.get(&id))
        && cx.layout.tracks.contains_key(&t.track)
    {
        let r = tag_rect(cx.layout, t);
        let sel = R {
            x: r.x - 4.0,
            y: r.y - 3.5,
            w: r.w + 8.0,
            h: r.h + 9.0,
        };
        cx.outline(&mut out.list, sel, 1.0, p.wait);
    }
}
