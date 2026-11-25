//! Draws the pelhaim app icon: the front of a metro car with a glowing LED "1" route sign.
//!
//! `cargo run -p pelhaim-board --example icon` rewrites `assets/icon/png/` and
//! `assets/icon/pelhaim.iconset/`; `tools/icon/build.sh` also rebuilds `pelhaim.icns`.
//!
//! 16 and 32 px come from the hand-tuned grids `assets/icon/grid-{16,32}.txt`. 64 px and up
//! draw a 64-cell scene at a whole number of pixels per cell, so every body edge lands on a
//! pixel boundary; only the tile corners, the LED dots and their glow are anti-aliased. The
//! LED "1" is drawn here, not taken from the board font: the font's short hook and foot read as
//! an "L" at icon size (owner, 2026-11-24), so the icon's "1" has a long diagonal flag and no foot.

use std::fs;
use std::path::{Path, PathBuf};

use pelhaim_board::{PaletteKind, Rgba, palette};

const CELLS: f32 = 64.0;
const TILE: (f32, f32) = (6.0, 58.0);
// Apple's 1024 grid: an 824 px tile with a ~185 px corner radius.
const TILE_RADIUS: f32 = 11.6;
const SIGN_CENTER: (f32, f32) = (32.0, 19.0);
const PITCH: f32 = 1.0;
const ONE: [u16; 7] = [
    0b00100, 0b01100, 0b10100, 0b00100, 0b00100, 0b00100, 0b00100,
];
const RING_RADIUS: f32 = 5.6;
const DOT_RADIUS: f32 = 0.38;

struct Colors {
    tile_top: Rgba,
    tile_bottom: Rgba,
    ground: Rgba,
    highlight: Rgba,
    steel: Rgba,
    shadow: Rgba,
    bumper: Rgba,
    under: Rgba,
    bezel: Rgba,
    panel: Rgba,
    led: Rgba,
    led_dim: Rgba,
    led_core: Rgba,
    glow: Rgba,
    glass_top: Rgba,
    glass: Rgba,
    glint: Rgba,
    lamp: Rgba,
}

fn colors() -> Colors {
    let matte = palette(PaletteKind::Matte);
    let film = palette(PaletteKind::Film);
    Colors {
        tile_top: Rgba::hex(0x24205A),
        tile_bottom: matte.bg,
        ground: Rgba::hex(0x332F6E),
        highlight: matte.label,
        steel: matte.done,
        shadow: Rgba::hex(0x5B5E72),
        bumper: Rgba::hex(0x44475A),
        under: Rgba::hex(0x1A1B2A),
        bezel: Rgba::hex(0x34364A),
        panel: matte.body,
        // The one saturated accent: the film palette's red, not the muted matte one.
        led: film.fail,
        led_dim: matte.fail,
        led_core: Rgba::hex(0xFFC4BA),
        glow: Rgba::hex(0x3A121C),
        glass_top: Rgba::hex(0x2A3760),
        glass: Rgba::hex(0x18213C),
        glint: Rgba::hex(0x3C4C78),
        lamp: Rgba::hex(0xEEE7CF),
    }
}

/// Premultiplied RGBA, blended in sRGB space: plenty for flat pixel art.
struct Canvas {
    n: usize,
    px: Vec<[f32; 4]>,
}

fn rgb(c: Rgba) -> [f32; 3] {
    [0, 1, 2].map(|i| f32::from(c.0[i]) / 255.0)
}

fn mix(a: Rgba, b: Rgba, t: f32) -> Rgba {
    let (a, b) = (rgb(a), rgb(b));
    let c = [0, 1, 2].map(|i| ((a[i] + (b[i] - a[i]) * t) * 255.0).round() as u8);
    Rgba([c[0], c[1], c[2], 255])
}

impl Canvas {
    fn new(n: usize) -> Canvas {
        Canvas {
            n,
            px: vec![[0.0; 4]; n * n],
        }
    }

    fn over(&mut self, x: usize, y: usize, c: Rgba, a: f32) {
        if a <= 0.0 {
            return;
        }
        let a = a.min(1.0);
        let s = rgb(c);
        let p = &mut self.px[y * self.n + x];
        for i in 0..3 {
            p[i] = s[i] * a + p[i] * (1.0 - a);
        }
        p[3] = a + p[3] * (1.0 - a);
    }

    fn add(&mut self, x: usize, y: usize, c: Rgba, k: f32) {
        let s = rgb(c);
        let p = &mut self.px[y * self.n + x];
        for i in 0..3 {
            p[i] = (p[i] + s[i] * k).min(p[3]);
        }
    }

    /// Paints `c` with coverage from a signed distance in pixels (negative inside), over the
    /// pixel box `[x0, x1) x [y0, y1)`.
    fn sdf(&mut self, b: [f32; 4], c: Rgba, d: impl Fn(f32, f32) -> f32) {
        let n = self.n as f32;
        let x0 = b[0].floor().clamp(0.0, n) as usize;
        let y0 = b[1].floor().clamp(0.0, n) as usize;
        let x1 = b[2].ceil().clamp(0.0, n) as usize;
        let y1 = b[3].ceil().clamp(0.0, n) as usize;
        for y in y0..y1 {
            for x in x0..x1 {
                let cov = (0.5 - d(x as f32 + 0.5, y as f32 + 0.5)).clamp(0.0, 1.0);
                self.over(x, y, c, cov);
            }
        }
    }

    fn png(&self) -> Vec<u8> {
        let mut out = Vec::new();
        {
            let mut enc = png::Encoder::new(&mut out, self.n as u32, self.n as u32);
            enc.set_color(png::ColorType::Rgba);
            enc.set_depth(png::BitDepth::Eight);
            let mut w = enc.write_header().expect("png header");
            let data: Vec<u8> = self
                .px
                .iter()
                .flat_map(|p| {
                    let a = p[3];
                    let un = |v: f32| {
                        if a > 0.0 {
                            ((v / a).clamp(0.0, 1.0) * 255.0).round() as u8
                        } else {
                            0
                        }
                    };
                    [un(p[0]), un(p[1]), un(p[2]), (a * 255.0).round() as u8]
                })
                .collect();
            w.write_image_data(&data).expect("png data");
        }
        out
    }
}

fn rounded_box(px: f32, py: f32, c: (f32, f32), half: (f32, f32), r: f32) -> f32 {
    let qx = (px - c.0).abs() - half.0 + r;
    let qy = (py - c.1).abs() - half.1 + r;
    let out = (qx.max(0.0).powi(2) + qy.max(0.0).powi(2)).sqrt();
    out + qx.max(qy).min(0.0) - r
}

struct Scene<'a> {
    cv: Canvas,
    u: f32,
    k: &'a Colors,
}

impl Scene<'_> {
    /// A cell-aligned rect; `x1`, `y1` exclusive.
    fn rect(&mut self, x0: f32, y0: f32, x1: f32, y1: f32, c: Rgba) {
        let u = self.u;
        let (a, b) = ((x0 * u).round() as usize, (y0 * u).round() as usize);
        let (e, f) = ((x1 * u).round() as usize, (y1 * u).round() as usize);
        for y in b..f.min(self.cv.n) {
            for x in a..e.min(self.cv.n) {
                self.cv.over(x, y, c, 1.0);
            }
        }
    }

    /// Mirrors a rect across the car's centre line (x = 32).
    fn pair(&mut self, x0: f32, y0: f32, x1: f32, y1: f32, c: Rgba) {
        self.rect(x0, y0, x1, y1, c);
        self.rect(64.0 - x1, y0, 64.0 - x0, y1, c);
    }

    fn dot(&mut self, cx: f32, cy: f32, r: f32, c: Rgba) {
        let u = self.u;
        let (cx, cy, r) = (cx * u, cy * u, (r * u).max(0.5));
        self.cv.sdf(
            [cx - r - 1.0, cy - r - 1.0, cx + r + 1.0, cy + r + 1.0],
            c,
            |x, y| ((x - cx).powi(2) + (y - cy).powi(2)).sqrt() - r,
        );
    }

    fn tile(&mut self) {
        let u = self.u;
        let mid = (TILE.0 + TILE.1) / 2.0 * u;
        let half = (TILE.1 - TILE.0) / 2.0 * u;
        let r = TILE_RADIUS * u;
        // Soft drop shadow inside the grid's margin, as macOS icons carry.
        let (dy, blur) = (0.6 * u, 1.6 * u);
        for y in 0..self.cv.n {
            for x in 0..self.cv.n {
                let d = rounded_box(
                    x as f32 + 0.5,
                    y as f32 + 0.5 - dy,
                    (mid, mid),
                    (half, half),
                    r,
                );
                let a = 0.35 * (1.0 - (d / blur).clamp(0.0, 1.0)).powi(2);
                self.cv.over(x, y, Rgba::hex(0x000000), a);
            }
        }
        let (top, bottom) = (TILE.0 * u, TILE.1 * u);
        for y in 0..self.cv.n {
            let t = ((y as f32 + 0.5 - top) / (bottom - top)).clamp(0.0, 1.0);
            let c = mix(self.k.tile_top, self.k.tile_bottom, t);
            for x in 0..self.cv.n {
                let d = rounded_box(x as f32 + 0.5, y as f32 + 0.5, (mid, mid), (half, half), r);
                self.cv.over(x, y, c, (0.5 - d).clamp(0.0, 1.0));
            }
        }
    }

    fn clip_to_tile(&mut self, from: &Canvas) {
        let u = self.u;
        let mid = (TILE.0 + TILE.1) / 2.0 * u;
        let half = (TILE.1 - TILE.0) / 2.0 * u;
        let r = TILE_RADIUS * u;
        for y in 0..self.cv.n {
            for x in 0..self.cv.n {
                let d = rounded_box(x as f32 + 0.5, y as f32 + 0.5, (mid, mid), (half, half), r);
                if (0.5 - d) < 1.0 {
                    self.cv.px[y * self.cv.n + x] = from.px[y * self.cv.n + x];
                }
            }
        }
    }

    fn car(&mut self) {
        let k = self.k;
        let (hi, st, sh) = (k.highlight, k.steel, k.shadow);

        // Rounded cab roof, stepped like pixel art.
        for (y, x0) in [(9.0, 19.0), (10.0, 17.0), (11.0, 16.0), (12.0, 15.0)] {
            self.rect(x0, y, 64.0 - x0, y + 1.0, st);
        }
        self.rect(14.0, 13.0, 50.0, 49.0, st);
        self.rect(19.0, 9.0, 45.0, 10.0, hi);
        self.pair(17.0, 10.0, 19.0, 11.0, hi);
        self.rect(16.0, 11.0, 17.0, 12.0, hi);
        self.rect(15.0, 12.0, 16.0, 13.0, hi);
        self.rect(14.0, 13.0, 16.0, 49.0, hi);
        self.rect(48.0, 13.0, 50.0, 49.0, sh);
        self.rect(47.0, 11.0, 49.0, 13.0, sh);
        self.rect(45.0, 10.0, 47.0, 11.0, sh);

        self.rect(23.0, 11.0, 41.0, 27.0, k.bezel);
        self.rect(24.0, 12.0, 40.0, 26.0, k.panel);
        self.rect(16.0, 28.0, 48.0, 29.0, sh);

        self.rect(18.0, 30.0, 46.0, 43.0, k.glass);
        self.rect(17.0, 31.0, 47.0, 42.0, k.glass);
        self.rect(18.0, 30.0, 46.0, 32.0, k.glass_top);
        self.rect(17.0, 31.0, 18.0, 32.0, k.glass_top);
        self.rect(46.0, 31.0, 47.0, 32.0, k.glass_top);
        for i in 0..9 {
            let y = 32.0 + i as f32;
            let x = 22.0 - i as f32 * 0.5;
            self.rect(x.round(), y, x.round() + 2.0, y + 1.0, k.glint);
            let x = 34.0 - i as f32 * 0.5;
            if i < 7 {
                self.rect(x.round(), y, x.round() + 1.0, y + 1.0, k.glint);
            }
        }
        // Three-pane front: the centre pane is the cab's end door.
        self.pair(25.0, 30.0, 26.0, 43.0, st);

        self.pair(18.0, 44.0, 24.0, 47.0, k.bezel);
        self.pair(19.0, 45.0, 23.0, 46.0, k.lamp);
        self.rect(14.0, 47.0, 50.0, 49.0, sh);

        self.rect(12.0, 49.0, 52.0, 52.0, k.bumper);
        self.rect(12.0, 49.0, 52.0, 50.0, sh);
        self.pair(15.0, 52.0, 26.0, 55.0, k.under);
        self.rect(31.0, 52.0, 33.0, 54.0, k.under);
        self.rect(28.0, 53.0, 36.0, 55.0, sh);
        self.rect(6.0, 55.0, 58.0, 56.0, k.ground);
        self.pair(17.0, 55.0, 22.0, 56.0, k.shadow);
    }

    fn sign(&mut self, one: &[u16]) {
        let k = self.k;
        let (cx, cy) = SIGN_CENTER;
        let mut lit: Vec<(f32, f32)> = Vec::new();
        for (r, bits) in one.iter().enumerate() {
            for c in 0..5 {
                if bits & (1 << (4 - c)) != 0 {
                    lit.push((cx + (c as f32 - 1.5) * PITCH, cy + (r as f32 - 3.0) * PITCH));
                }
            }
        }
        let n = (std::f32::consts::TAU * RING_RADIUS / PITCH).round() as usize;
        for i in 0..n {
            let a = std::f32::consts::TAU * i as f32 / n as f32;
            lit.push((cx + RING_RADIUS * a.sin(), cy - RING_RADIUS * a.cos()));
        }

        let u = self.u;
        let (px0, py0, px1, py1) = (24.0 * u, 12.0 * u, 40.0 * u, 26.0 * u);
        for y in py0 as usize..py1 as usize {
            for x in px0 as usize..px1 as usize {
                let (fx, fy) = ((x as f32 + 0.5) / u, (y as f32 + 0.5) / u);
                let mut g = 0.0;
                for &(lx, ly) in &lit {
                    let d2 = (fx - lx).powi(2) + (fy - ly).powi(2);
                    g += 0.32 * (-d2 / (2.0 * 0.6f32.powi(2))).exp()
                        + 0.035 * (-d2 / (2.0 * 2.2f32.powi(2))).exp();
                }
                self.cv.add(x, y, k.led, g.min(0.9));
            }
        }
        for &(x, y) in &lit {
            self.dot(x, y, DOT_RADIUS, k.led);
            if u >= 8.0 {
                self.dot(x - 0.06, y - 0.06, DOT_RADIUS * 0.45, k.led_core);
            }
        }
    }
}

fn draw_scene(n: usize, one: &[u16], k: &Colors) -> Canvas {
    let mut s = Scene {
        cv: Canvas::new(n),
        u: n as f32 / CELLS,
        k,
    };
    s.tile();
    let backdrop = Canvas {
        n,
        px: s.cv.px.clone(),
    };
    s.car();
    s.sign(one);
    s.clip_to_tile(&backdrop);
    s.cv
}

fn draw_grid(src: &str, k: &Colors) -> Canvas {
    let rows: Vec<&str> = src
        .lines()
        .filter(|l| !l.starts_with(';') && !l.is_empty())
        .collect();
    let n = rows.len();
    let mut cv = Canvas::new(n);
    let (top, bottom) = (rows.iter().position(|r| r.contains('T')).unwrap_or(0), n);
    for (y, row) in rows.iter().enumerate() {
        assert_eq!(row.len(), n, "grid row {y} is not {n} wide");
        let t = (y.saturating_sub(top)) as f32 / (bottom - top) as f32;
        for (x, ch) in row.chars().enumerate() {
            let c = match ch {
                '.' => continue,
                'T' => mix(k.tile_top, k.tile_bottom, t),
                'G' => k.ground,
                'H' => k.highlight,
                'S' => k.steel,
                'D' => k.shadow,
                'B' => k.bumper,
                'K' => k.under,
                'P' => k.panel,
                'q' => k.glow,
                'R' => k.led,
                'r' => k.led_dim,
                'W' => k.glass,
                'w' => k.glint,
                'L' => k.lamp,
                other => panic!("grid ({x}, {y}): unknown pixel {other:?}"),
            };
            cv.over(x, y, c, 1.0);
        }
    }
    cv
}

fn main() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let read = |p: &str| fs::read_to_string(root.join(p)).unwrap_or_else(|e| panic!("{p}: {e}"));
    let one = ONE;
    let k = colors();

    let png_dir = root.join("assets/icon/png");
    let set_dir = root.join("assets/icon/pelhaim.iconset");
    fs::create_dir_all(&png_dir).expect("png dir");
    fs::create_dir_all(&set_dir).expect("iconset dir");

    let mut sizes: Vec<(usize, Vec<u8>)> = Vec::new();
    for n in [16, 32] {
        sizes.push((
            n,
            draw_grid(&read(&format!("assets/icon/grid-{n}.txt")), &k).png(),
        ));
    }
    for n in [64, 128, 256, 512, 1024] {
        sizes.push((n, draw_scene(n, &one, &k).png()));
    }
    let get = |n: usize| &sizes.iter().find(|(s, _)| *s == n).expect("size").1;
    let write =
        |p: &Path, b: &[u8]| fs::write(p, b).unwrap_or_else(|e| panic!("{}: {e}", p.display()));
    for (n, bytes) in &sizes {
        write(&png_dir.join(format!("pelhaim-{n}.png")), bytes);
    }
    for n in [16, 32, 128, 256, 512] {
        write(&set_dir.join(format!("icon_{n}x{n}.png")), get(n));
        write(&set_dir.join(format!("icon_{n}x{n}@2x.png")), get(2 * n));
    }
    println!("wrote {} and {}", png_dir.display(), set_dir.display());
}
