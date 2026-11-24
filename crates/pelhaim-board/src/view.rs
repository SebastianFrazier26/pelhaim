use crate::display::R;

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct View {
    pub k: f32,
    pub ox: f32,
    pub oy: f32,
}

impl View {
    pub fn x(&self, u: f32) -> f32 {
        u * self.k + self.ox
    }

    pub fn y(&self, u: f32) -> f32 {
        u * self.k + self.oy
    }

    pub fn inv(&self, sx: f32, sy: f32) -> (f32, f32) {
        ((sx - self.ox) / self.k, (sy - self.oy) / self.k)
    }

    pub fn rect(&self, r: R) -> R {
        R {
            x: self.x(r.x),
            y: self.y(r.y),
            w: r.w * self.k,
            h: r.h * self.k,
        }
    }
}

pub const MIN_ZOOM: f32 = 0.25;
pub const MAX_ZOOM: f32 = 8.0;

pub fn fit(content: R, viewport: (f32, f32)) -> View {
    let (vw, vh) = viewport;
    let k = (vw / content.w).min(vh / content.h);
    // 0/0 (empty content in an empty viewport) is NaN, which clamp would pass through.
    let k = if k.is_nan() {
        1.0
    } else {
        k.clamp(MIN_ZOOM, MAX_ZOOM)
    };
    View {
        k,
        ox: (vw - content.w * k) / 2.0 - content.x * k,
        oy: (vh - content.h * k) / 2.0 - content.y * k,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-3
    }

    #[test]
    fn view_inverse_round_trip() {
        let v = View {
            k: 2.5,
            ox: -40.0,
            oy: 13.0,
        };
        for (u, w) in [(0.0, 0.0), (12.5, -7.0), (960.0, 540.0)] {
            let (bu, bw) = v.inv(v.x(u), v.y(w));
            assert!(close(bu, u) && close(bw, w), "({u},{w}) -> ({bu},{bw})");
        }
    }

    #[test]
    fn rect_scales_and_offsets() {
        let v = View {
            k: 2.0,
            ox: 10.0,
            oy: 20.0,
        };
        assert_eq!(
            v.rect(R {
                x: 1.0,
                y: 2.0,
                w: 3.0,
                h: 4.0
            }),
            R {
                x: 12.0,
                y: 24.0,
                w: 6.0,
                h: 8.0
            }
        );
    }

    #[test]
    fn fit_centres_content() {
        let v = fit(
            R {
                x: 100.0,
                y: 50.0,
                w: 400.0,
                h: 100.0,
            },
            (800.0, 600.0),
        );
        assert!(close(v.k, 2.0));
        assert!(close(v.x(100.0), 0.0) && close(v.x(500.0), 800.0));
        assert!(close(v.y(50.0), 200.0) && close(v.y(150.0), 400.0));
    }

    #[test]
    fn fit_clamps_zoom() {
        assert_eq!(
            fit(
                R {
                    x: 0.0,
                    y: 0.0,
                    w: 10.0,
                    h: 10.0
                },
                (10_000.0, 10_000.0)
            )
            .k,
            8.0
        );
        assert_eq!(
            fit(
                R {
                    x: 0.0,
                    y: 0.0,
                    w: 100_000.0,
                    h: 10.0
                },
                (100.0, 100.0)
            )
            .k,
            0.25
        );
        let big = fit(
            R {
                x: 0.0,
                y: 0.0,
                w: 1000.0,
                h: 1000.0,
            },
            (100.0, 100.0),
        );
        assert!(close(big.x(500.0), 50.0) && close(big.y(500.0), 50.0));
    }

    #[test]
    fn fit_empty_content_is_finite() {
        let v = fit(
            R {
                x: 5.0,
                y: 5.0,
                w: 0.0,
                h: 0.0,
            },
            (0.0, 0.0),
        );
        assert!(v.k.is_finite() && v.ox.is_finite() && v.oy.is_finite());
    }
}
