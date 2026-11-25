use pelhaim_core::{TagResolution, Train, TrainId, format_train_number};

use super::lines::LineGeom;
use super::{Ctx, Hit, Layout, Scene};
use crate::display::{P, Prim, R};
use crate::font::{Cut, advance, draw_text};

const TAG_H: f32 = 9.0;
const TAG_PAD: f32 = 4.0;
const ARROW_W: f32 = 7.0;
const DIGIT_PX: f32 = 1.0;
const UNDERLINE_GAP: f32 = 1.6;
const HIT_PAD: f32 = 3.0;

fn body_w(number: u16) -> f32 {
    let digits = format_train_number(number).chars().count() as f32;
    TAG_PAD + digits * advance(Cut::C5x7, DIGIT_PX)
}

/// The tag of the line train that folds in `train`, whichever member it is: body plus
/// arrowhead in board units, its tip at the head. A train on a folded line gets an empty
/// rect at the origin, which no hit test matches.
pub fn tag_rect(layout: &Layout, train: &Train) -> R {
    layout.geom_of(train.id).map_or(
        R {
            x: 0.0,
            y: 0.0,
            w: 0.0,
            h: 0.0,
        },
        tag_rect_in,
    )
}

pub(crate) fn tag_rect_in(g: &LineGeom) -> R {
    let (x, y) = g.head();
    let tw = body_w(g.train.number) + ARROW_W;
    R {
        x: x - tw,
        y: y - TAG_H / 2.0,
        w: tw,
        h: TAG_H,
    }
}

pub(super) fn draw(out: &mut Scene, cx: &Ctx, selected: Option<TrainId>) {
    let p = &cx.paint.p;
    for g in &cx.layout.geoms {
        let lt = &g.train;
        let r = tag_rect_in(g);
        let color = cx.paint.state(lt.state(cx.snap));
        let bw = body_w(lt.number);
        let ring = R {
            x: r.x - 1.0,
            y: r.y - 1.0,
            w: r.w + 2.0,
            h: r.h + 2.0,
        };
        cx.rect(&mut out.list, ring, p.bg);
        let mistagged = lt.members.iter().any(|id| {
            cx.snap
                .trains
                .get(id)
                .is_some_and(|t| matches!(t.tag, TagResolution::MisTagged(_)))
        });
        if mistagged {
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
            &format_train_number(lt.number),
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
        out.hits.push((cx.v.rect(hit), Hit::Train(lt.lead)));
    }

    if let Some(g) = selected.and_then(|id| cx.layout.geom_of(id)) {
        let r = tag_rect_in(g);
        let sel = R {
            x: r.x - 4.0,
            y: r.y - 3.5,
            w: r.w + 8.0,
            h: r.h + 9.0,
        };
        cx.outline(&mut out.list, sel, 1.0, p.wait);
    }
}
