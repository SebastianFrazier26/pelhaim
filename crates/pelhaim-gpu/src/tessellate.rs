use std::ops::Range;

use bytemuck::{Pod, Zeroable};
use pelhaim_board::{DisplayList, Prim, Rgba};

#[repr(C)]
#[derive(Pod, Zeroable, Clone, Copy, PartialEq, Debug)]
pub struct Vertex {
    /// Device pixels, origin top-left.
    pub pos: [f32; 2],
    /// Linear, premultiplied.
    pub color: [f32; 4],
    /// Position relative to the shape's centre, in its own (possibly rotated) frame.
    pub local: [f32; 2],
    pub half: [f32; 2],
    /// 1 runs the SDF edge coverage; 0 draws the shape solid.
    pub aa: f32,
}

#[derive(Clone, PartialEq, Debug)]
pub struct Batch {
    /// `[x, y, w, h]` in device pixels, already clamped to the viewport.
    pub scissor: Option<[u32; 4]>,
    /// A range of the index buffer.
    pub range: Range<u32>,
}

fn srgb_to_linear(c: u8) -> f32 {
    let c = f32::from(c) / 255.0;
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

/// sRGB bytes to the linear, premultiplied colour the shader blends with, so an `*Srgb`
/// target encodes back to the exact hex.
pub fn linear_premul(c: Rgba) -> [f32; 4] {
    let [r, g, b, a] = c.0;
    let a = f32::from(a) / 255.0;
    [
        srgb_to_linear(r) * a,
        srgb_to_linear(g) * a,
        srgb_to_linear(b) * a,
        a,
    ]
}

/// Inclusive-exclusive `[x0, y0, x1, y1]` device-pixel box.
type PxBox = [u32; 4];

struct Builder {
    vs: Vec<Vertex>,
    is: Vec<u32>,
    batches: Vec<Batch>,
    batch_start: u32,
    clips: Vec<PxBox>,
}

impl Builder {
    fn close_batch(&mut self) {
        let end = self.is.len() as u32;
        if end > self.batch_start {
            self.batches.push(Batch {
                scissor: self
                    .clips
                    .last()
                    .map(|&[x0, y0, x1, y1]| [x0, y0, x1 - x0, y1 - y0]),
                range: self.batch_start..end,
            });
        }
        self.batch_start = end;
    }

    fn quad(
        &mut self,
        corners: [[f32; 2]; 4],
        locals: [[f32; 2]; 4],
        half: [f32; 2],
        color: [f32; 4],
        aa: f32,
    ) {
        let base = self.vs.len() as u32;
        for (pos, local) in corners.into_iter().zip(locals) {
            self.vs.push(Vertex {
                pos,
                color,
                local,
                half,
                aa,
            });
        }
        self.is
            .extend([base, base + 1, base + 2, base, base + 2, base + 3]);
    }

    /// A box centred on `c` with half-extents `half` along the unit axis `u` and its normal,
    /// grown by one device pixel on every side so the SDF has room to fade the edge out.
    fn sdf_box(&mut self, c: [f32; 2], u: [f32; 2], half: [f32; 2], color: [f32; 4]) {
        let n = [-u[1], u[0]];
        let (gx, gy) = (half[0] + 1.0, half[1] + 1.0);
        let locals = [[-gx, -gy], [gx, -gy], [gx, gy], [-gx, gy]];
        let corners =
            locals.map(|[lx, ly]| [c[0] + u[0] * lx + n[0] * ly, c[1] + u[1] * lx + n[1] * ly]);
        self.quad(corners, locals, half, color, 1.0);
    }
}

fn snap_span(a: f32, len: f32, scale: f32) -> (f32, f32) {
    let a0 = (a * scale).round();
    (a0, (a0 + 1.0).max(((a + len) * scale).round()))
}

pub fn tessellate(
    dl: &DisplayList,
    scale: f32,
    viewport_px: (u32, u32),
) -> (Vec<Vertex>, Vec<u32>, Vec<Batch>) {
    let mut b = Builder {
        vs: Vec::new(),
        is: Vec::new(),
        batches: Vec::new(),
        batch_start: 0,
        clips: Vec::new(),
    };
    let (vw, vh) = viewport_px;
    for prim in &dl.prims {
        match prim {
            Prim::Rect {
                r,
                color,
                aa: false,
            } => {
                let (x0, x1) = snap_span(r.x, r.w, scale);
                let (y0, y1) = snap_span(r.y, r.h, scale);
                let corners = [[x0, y0], [x1, y0], [x1, y1], [x0, y1]];
                b.quad(corners, [[0.0; 2]; 4], [0.0; 2], linear_premul(*color), 0.0);
            }
            Prim::Rect { r, color, aa: true } => {
                let half = [r.w * scale / 2.0, r.h * scale / 2.0];
                let c = [r.x * scale + half[0], r.y * scale + half[1]];
                b.sdf_box(c, [1.0, 0.0], half, linear_premul(*color));
            }
            Prim::Seg {
                a,
                b: e,
                width,
                color,
            } => {
                let (ax, ay, ex, ey) = (a.x * scale, a.y * scale, e.x * scale, e.y * scale);
                let (dx, dy) = (ex - ax, ey - ay);
                let len = dx.hypot(dy);
                let u = if len > 0.0 {
                    [dx / len, dy / len]
                } else {
                    [1.0, 0.0]
                };
                let w = width * scale;
                let half = [(len + w) / 2.0, w / 2.0];
                b.sdf_box(
                    [(ax + ex) / 2.0, (ay + ey) / 2.0],
                    u,
                    half,
                    linear_premul(*color),
                );
            }
            Prim::Tri { p, color } => {
                let base = b.vs.len() as u32;
                let color = linear_premul(*color);
                for q in p {
                    b.vs.push(Vertex {
                        pos: [q.x * scale, q.y * scale],
                        color,
                        local: [0.0; 2],
                        half: [0.0; 2],
                        aa: 0.0,
                    });
                }
                b.is.extend([base, base + 1, base + 2]);
            }
            Prim::Clip(r) => {
                b.close_batch();
                let px = |v: f32, max: u32| (v.max(0.0) as u32).min(max);
                let x0 = px((r.x * scale).floor(), vw);
                let y0 = px((r.y * scale).floor(), vh);
                let x1 = px(((r.x + r.w) * scale).ceil(), vw).max(x0);
                let y1 = px(((r.y + r.h) * scale).ceil(), vh).max(y0);
                let mut clip = [x0, y0, x1, y1];
                if let Some(&[ox0, oy0, ox1, oy1]) = b.clips.last() {
                    let (nx0, ny0) = (x0.max(ox0), y0.max(oy0));
                    clip = [nx0, ny0, x1.min(ox1).max(nx0), y1.min(oy1).max(ny0)];
                }
                b.clips.push(clip);
            }
            Prim::Unclip => {
                b.close_batch();
                b.clips.pop();
            }
        }
    }
    b.close_batch();
    (b.vs, b.is, b.batches)
}

#[cfg(test)]
mod tests {
    use super::*;
    use pelhaim_board::{P, Prim, R};

    fn r(x: f32, y: f32, w: f32, h: f32) -> R {
        R { x, y, w, h }
    }

    fn list(prims: Vec<Prim>) -> DisplayList {
        DisplayList { prims }
    }

    fn bounds(vs: &[Vertex]) -> ([f32; 2], [f32; 2]) {
        let mut lo = [f32::MAX; 2];
        let mut hi = [f32::MIN; 2];
        for v in vs {
            for i in 0..2 {
                lo[i] = lo[i].min(v.pos[i]);
                hi[i] = hi[i].max(v.pos[i]);
            }
        }
        (lo, hi)
    }

    fn close(a: [f32; 2], b: [f32; 2]) -> bool {
        (a[0] - b[0]).abs() < 1e-4 && (a[1] - b[1]).abs() < 1e-4
    }

    const WHITE: Rgba = Rgba([255, 255, 255, 255]);

    #[test]
    fn snapped_rect_lands_on_device_pixels() {
        let dl = list(vec![Prim::Rect {
            r: r(1.3, 1.3, 1.0, 1.0),
            color: WHITE,
            aa: false,
        }]);
        let (vs, is, bs) = tessellate(&dl, 2.0, (100, 100));
        assert_eq!(vs.len(), 4);
        assert_eq!(is.len(), 6);
        assert_eq!(bounds(&vs), ([3.0, 3.0], [5.0, 5.0]));
        assert!(vs.iter().all(|v| v.aa == 0.0));
        assert_eq!(
            bs,
            [Batch {
                scissor: None,
                range: 0..6
            }]
        );
    }

    #[test]
    fn snapped_rect_is_at_least_one_device_pixel() {
        let dl = list(vec![Prim::Rect {
            r: r(4.0, 4.0, 0.1, 0.1),
            color: WHITE,
            aa: false,
        }]);
        let (vs, _, _) = tessellate(&dl, 1.0, (100, 100));
        assert_eq!(bounds(&vs), ([4.0, 4.0], [5.0, 5.0]));
    }

    #[test]
    fn aa_rect_grows_one_device_pixel_with_centred_local() {
        let dl = list(vec![Prim::Rect {
            r: r(10.0, 20.0, 4.0, 2.0),
            color: WHITE,
            aa: true,
        }]);
        let (vs, _, _) = tessellate(&dl, 2.0, (100, 100));
        assert_eq!(bounds(&vs), ([19.0, 39.0], [29.0, 45.0]));
        for v in &vs {
            assert_eq!(v.aa, 1.0);
            assert_eq!(v.half, [4.0, 2.0]);
            assert!(close(v.local, [v.pos[0] - 24.0, v.pos[1] - 42.0]));
        }
    }

    #[test]
    fn segment_becomes_box_with_square_caps() {
        let dl = list(vec![Prim::Seg {
            a: P { x: 10.0, y: 10.0 },
            b: P { x: 10.0, y: 30.0 },
            width: 2.0,
            color: WHITE,
        }]);
        let (vs, is, _) = tessellate(&dl, 1.0, (100, 100));
        assert_eq!((vs.len(), is.len()), (4, 6));
        // Length |b - a| + width runs along the segment, thickness `width` across it.
        for v in &vs {
            assert_eq!(v.half, [11.0, 1.0]);
            assert_eq!(v.aa, 1.0);
            assert_eq!(v.local[0].abs(), 12.0);
            assert_eq!(v.local[1].abs(), 2.0);
        }
        let (lo, hi) = bounds(&vs);
        assert!(
            close(lo, [8.0, 8.0]) && close(hi, [12.0, 32.0]),
            "{lo:?} {hi:?}"
        );
    }

    #[test]
    fn diagonal_segment_local_frame_follows_the_segment() {
        let dl = list(vec![Prim::Seg {
            a: P { x: 0.0, y: 0.0 },
            b: P { x: 30.0, y: 40.0 },
            width: 2.0,
            color: WHITE,
        }]);
        let (vs, _, _) = tessellate(&dl, 1.0, (100, 100));
        let (dx, dy) = (0.6, 0.8);
        for v in &vs {
            let (px, py) = (v.pos[0] - 15.0, v.pos[1] - 20.0);
            let along = px * dx + py * dy;
            let across = -px * dy + py * dx;
            assert!(close(v.local, [along, across]), "{:?}", v.local);
        }
    }

    #[test]
    fn triangle_is_solid_and_scaled() {
        let dl = list(vec![Prim::Tri {
            p: [
                P { x: 1.0, y: 1.0 },
                P { x: 5.0, y: 1.0 },
                P { x: 1.0, y: 3.0 },
            ],
            color: WHITE,
        }]);
        let (vs, is, _) = tessellate(&dl, 2.0, (100, 100));
        assert_eq!(is, [0, 1, 2]);
        let pos: Vec<_> = vs.iter().map(|v| v.pos).collect();
        assert_eq!(pos, [[2.0, 2.0], [10.0, 2.0], [2.0, 6.0]]);
        assert!(vs.iter().all(|v| v.aa == 0.0));
    }

    #[test]
    fn clip_starts_new_batch_with_scissor() {
        let rect = |x| Prim::Rect {
            r: r(x, 0.0, 1.0, 1.0),
            color: WHITE,
            aa: false,
        };
        let dl = list(vec![
            rect(0.0),
            Prim::Clip(r(10.2, 5.5, 20.0, 100.0)),
            rect(1.0),
            rect(2.0),
            Prim::Unclip,
            rect(3.0),
        ]);
        let (_, is, bs) = tessellate(&dl, 2.0, (50, 60));
        assert_eq!(is.len(), 24);
        assert_eq!(
            bs,
            [
                Batch {
                    scissor: None,
                    range: 0..6
                },
                // floor(20.4), floor(11) .. ceil(60.4) clamped to 50, ceil(211) clamped to 60.
                Batch {
                    scissor: Some([20, 11, 30, 49]),
                    range: 6..18
                },
                Batch {
                    scissor: None,
                    range: 18..24
                },
            ]
        );
    }

    #[test]
    fn clip_outside_the_viewport_is_empty_and_empty_batches_drop() {
        let dl = list(vec![
            Prim::Clip(r(-50.0, -50.0, 10.0, 10.0)),
            Prim::Unclip,
            Prim::Clip(r(200.0, 0.0, 10.0, 10.0)),
            Prim::Rect {
                r: r(0.0, 0.0, 1.0, 1.0),
                color: WHITE,
                aa: false,
            },
            Prim::Unclip,
        ]);
        let (_, _, bs) = tessellate(&dl, 1.0, (100, 100));
        assert_eq!(
            bs,
            [Batch {
                scissor: Some([100, 0, 0, 10]),
                range: 0..6
            }]
        );
    }

    #[test]
    fn nested_clip_intersects_and_unclip_restores_the_outer_one() {
        let rect = Prim::Rect {
            r: r(0.0, 0.0, 1.0, 1.0),
            color: WHITE,
            aa: false,
        };
        let dl = list(vec![
            Prim::Clip(r(0.0, 0.0, 50.0, 50.0)),
            Prim::Clip(r(40.0, 40.0, 50.0, 50.0)),
            rect.clone(),
            Prim::Unclip,
            rect,
            Prim::Unclip,
        ]);
        let (_, _, bs) = tessellate(&dl, 1.0, (100, 100));
        let sc: Vec<_> = bs.iter().map(|b| b.scissor).collect();
        assert_eq!(sc, [Some([40, 40, 10, 10]), Some([0, 0, 50, 50])]);
    }

    #[test]
    fn colour_is_linear_and_premultiplied() {
        let [r, g, b, a] = linear_premul(Rgba([0x76, 0xA9, 0x6F, 255]));
        assert!((r - 0.181_164).abs() < 1e-4, "{r}");
        assert!((g - 0.396_755).abs() < 1e-4, "{g}");
        assert!((b - 0.158_961).abs() < 1e-4, "{b}");
        assert_eq!(a, 1.0);
        let [r, _, _, a] = linear_premul(Rgba([255, 0, 0, 51]));
        assert!((a - 0.2).abs() < 1e-6 && (r - 0.2).abs() < 1e-6);
        assert_eq!(linear_premul(Rgba([0, 0, 0, 0])), [0.0; 4]);
    }

    #[test]
    fn vertex_packs_to_44_bytes_in_field_order() {
        let v = Vertex {
            pos: [1.0, 2.0],
            color: [3.0, 4.0, 5.0, 6.0],
            local: [7.0, 8.0],
            half: [9.0, 10.0],
            aa: 11.0,
        };
        let bytes = bytemuck::bytes_of(&v);
        assert_eq!(bytes.len(), 44);
        let floats: Vec<f32> = bytes
            .as_chunks::<4>()
            .0
            .iter()
            .map(|c| f32::from_ne_bytes(*c))
            .collect();
        assert_eq!(floats, (1..=11).map(|i| i as f32).collect::<Vec<_>>());
    }
}
