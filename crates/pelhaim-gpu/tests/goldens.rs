#![cfg(feature = "gpu-goldens")]

use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard, PoisonError};

use pelhaim_board::{
    CardRow, DisplayList, InsetState, Mark, PaletteKind, PopupState, Prim, R, SceneInput,
    ViewState, build_scene, palette,
};
use pelhaim_core::testkit::sample_board;
use pelhaim_core::{AlertId, Snapshot, TrainId};
use pelhaim_gpu::{adapter_info, render_offscreen};

const W: u32 = 1600;
const H: u32 = 900;
/// A pixel differs when any channel is off by more than this.
const CHANNEL_TOL: u8 = 2;
/// At most this fraction of pixels may differ.
const MAX_DIFF_FRACTION: f64 = 0.001;

// Lavapipe (CI's software Vulkan) segfaults intermittently when test threads create and drop
// wgpu instances and devices at the same time, so every test holds this for its GPU work.
static GPU: Mutex<()> = Mutex::new(());

fn gpu() -> MutexGuard<'static, ()> {
    GPU.lock().unwrap_or_else(PoisonError::into_inner)
}

fn render_scene(snap: &Snapshot, view: &ViewState, now_s: f64) -> Vec<u8> {
    let scene = build_scene(&SceneInput {
        snap,
        view,
        viewport: (W as f32, H as f32),
        now_s,
        palette: PaletteKind::Matte,
        reduced_motion: false,
    });
    let bg = palette(PaletteKind::Matte).bg;
    render_offscreen(&scene.list, W, H, 1.0, bg).expect("offscreen render")
}

fn golden_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/goldens")
        .join(format!("{name}.png"))
}

fn write_png(path: &PathBuf, rgba: &[u8], w: u32, h: u32) {
    let file = std::fs::File::create(path).expect("create png");
    let mut enc = png::Encoder::new(std::io::BufWriter::new(file), w, h);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    let mut wr = enc.write_header().expect("png header");
    wr.write_image_data(rgba).expect("png data");
}

fn read_png(path: &PathBuf) -> (Vec<u8>, u32, u32) {
    let file = std::fs::File::open(path)
        .unwrap_or_else(|e| panic!("{}: {e} (bless with PELHAIM_BLESS=1)", path.display()));
    let mut dec = png::Decoder::new(std::io::BufReader::new(file));
    dec.set_transformations(png::Transformations::EXPAND);
    let mut rd = dec.read_info().expect("png info");
    let mut buf = vec![0; rd.output_buffer_size().expect("png size")];
    let info = rd.next_frame(&mut buf).expect("png frame");
    assert_eq!(
        (info.color_type, info.bit_depth),
        (png::ColorType::Rgba, png::BitDepth::Eight)
    );
    buf.truncate(info.buffer_size());
    (buf, info.width, info.height)
}

fn check_golden(name: &str, actual: &[u8]) {
    let path = golden_path(name);
    if std::env::var_os("PELHAIM_BLESS").is_some_and(|v| v == "1") {
        write_png(&path, actual, W, H);
        return;
    }
    let (want, w, h) = read_png(&path);
    assert_eq!((w, h), (W, H), "{name}: golden size");
    let differing = want
        .as_chunks::<4>()
        .0
        .iter()
        .zip(actual.as_chunks::<4>().0)
        .filter(|(a, b)| {
            a.iter()
                .zip(b.iter())
                .any(|(x, y)| x.abs_diff(*y) > CHANNEL_TOL)
        })
        .count();
    let total = (W * H) as usize;
    if differing as f64 > total as f64 * MAX_DIFF_FRACTION {
        let out = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("{name}-actual.png"));
        write_png(&out, actual, W, H);
        panic!(
            "{name}: {differing} of {total} pixels differ by more than {CHANNEL_TOL}; actual written to {}",
            out.display()
        );
    }
}

#[test]
fn adapter_is_reported() {
    let _gpu = gpu();
    let info = adapter_info().expect("an adapter");
    assert!(info.ends_with(')') && info.contains(" ("), "{info}");
    eprintln!("adapter: {info}");
}

#[test]
fn work_colour_is_exact() {
    let _gpu = gpu();
    let p = palette(PaletteKind::Matte);
    let dl = DisplayList {
        prims: vec![Prim::Rect {
            r: R {
                x: 0.0,
                y: 0.0,
                w: 64.0,
                h: 64.0,
            },
            color: p.work,
            aa: true,
        }],
    };
    let px = render_offscreen(&dl, 64, 64, 1.0, p.bg).expect("offscreen render");
    let i = (32 * 64 + 32) * 4;
    let centre = &px[i..i + 4];
    for (got, want) in centre.iter().zip([0x76u8, 0xA9, 0x6F, 255]) {
        assert!(got.abs_diff(want) <= 1, "centre {centre:02X?}");
    }
}

#[test]
fn board_golden() {
    let _gpu = gpu();
    let px = render_scene(&sample_board(), &ViewState::default(), 0.0);
    check_golden("board", &px);
}

#[test]
fn alert_golden() {
    let _gpu = gpu();
    let view = ViewState {
        popup: Some(PopupState::new(AlertId("q:claude:S1:toolu_q".into()))),
        ..ViewState::default()
    };
    let px = render_scene(&sample_board(), &view, 0.0);
    check_golden("alert", &px);
}

#[test]
fn inset_golden() {
    let _gpu = gpu();
    let mut snap = sample_board();
    let train = TrainId(7);
    let t = snap.trains.get_mut(&train).expect("train 7");
    t.cwd = Some("/Users/me/Developer/safeascent".into());
    t.tail = (0..12).map(|i| format!("tail line {i}")).collect();
    let view = ViewState {
        inset: Some(InsetState {
            train,
            opened_s: 0.0,
        }),
        ..ViewState::default()
    };
    // Past the inset's open animation, so the golden shows it settled.
    let px = render_scene(&snap, &view, 1.0);
    check_golden("inset", &px);
}

#[test]
fn console_golden() {
    let _gpu = gpu();
    let mut view = ViewState {
        radio_log: vec![
            CardRow {
                mark: Mark::Done,
                text: "observe".into(),
            },
            CardRow {
                mark: Mark::Needs,
                text: "observed sessions are answered in their own terminal".into(),
            },
        ],
        radio_log_open: true,
        console_focused: true,
        ..ViewState::default()
    };
    view.console.insert("/goto eth");
    view.console.preedit = Some("ica".into());
    let px = render_scene(&sample_board(), &view, 0.0);
    check_golden("console", &px);
}
