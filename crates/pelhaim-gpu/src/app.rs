use std::sync::Arc;
use std::time::Instant;

use anyhow::Context;
use pelhaim_board::scene::{TRACK_PITCH, block_state, inset};
use pelhaim_board::{
    A11yNode, BlockState, Layout, PaletteKind, Prim, R, Scene, SceneInput, UiAction, UiInput,
    ViewState, a11y_tree, board_layout, build_board, build_overlays, on_input, palette, sync,
};
use pelhaim_core::Snapshot;
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{ElementState, Ime, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop, EventLoopProxy};
use winit::keyboard::ModifiersState;
use winit::window::{Window, WindowId};

use crate::Renderer;
use crate::a11y_bridge::{focus_id, node_id, tree_update};
use crate::keymap::{Platform, press_inputs};

pub trait UiBackend: Send + 'static {
    fn snapshot(&self) -> Arc<Snapshot>;
    fn perform(&self, a: UiAction);
    fn set_waker(&self, wake: Box<dyn Fn() + Send + Sync>);
}

#[derive(Clone, Debug)]
pub struct AppOptions {
    pub palette: PaletteKind,
    pub reduced_motion: bool,
    pub title: String,
}

impl Default for AppOptions {
    fn default() -> Self {
        AppOptions {
            palette: PaletteKind::Matte,
            reduced_motion: false,
            title: "pelhaim".into(),
        }
    }
}

pub const DOUBLE_CLICK_S: f64 = 0.35;
pub const DOUBLE_CLICK_PX: f32 = 6.0;

/// Device pixels per scene unit. Always a whole number: the pixel font is snapped per rect,
/// so a fractional scale gives glyph pixels of uneven width.
pub fn text_scale(device_pixel_ratio: f64) -> f32 {
    if device_pixel_ratio.is_finite() {
        device_pixel_ratio.round().max(1.0) as f32
    } else {
        1.0
    }
}

#[derive(Default, Debug)]
pub struct ClickTracker {
    last: Option<(f64, f32, f32)>,
}

impl ClickTracker {
    /// Records a left press and says whether it completes a double-click. A third press
    /// starts a new pair.
    pub fn press(&mut self, now_s: f64, x: f32, y: f32) -> bool {
        let double = self.last.is_some_and(|(t, lx, ly)| {
            now_s - t <= DOUBLE_CLICK_S
                && (x - lx).abs() <= DOUBLE_CLICK_PX
                && (y - ly).abs() <= DOUBLE_CLICK_PX
        });
        self.last = if double { None } else { Some((now_s, x, y)) };
        double
    }
}

/// The board from `view.frozen` while paused, overlays always from `live`. The returned
/// layout is the live one, which `on_input` needs to act on the live snapshot.
pub fn frame_scene(
    live: &Snapshot,
    view: &ViewState,
    viewport: (f32, f32),
    now_s: f64,
    opts: &AppOptions,
) -> (Scene, Layout) {
    let input = |snap| SceneInput {
        snap,
        view,
        viewport,
        now_s,
        palette: opts.palette,
        reduced_motion: opts.reduced_motion,
    };
    let board_snap = view.frozen.as_deref().unwrap_or(live);
    let mut out = Scene::default();
    out.list.prims.push(Prim::Rect {
        r: R {
            x: 0.0,
            y: 0.0,
            w: viewport.0,
            h: viewport.1,
        },
        color: palette(opts.palette).bg,
        aa: false,
    });
    build_board(
        &mut out,
        &input(board_snap),
        &board_layout(board_snap, viewport.0),
        &view.view(),
    );
    let live_layout = board_layout(live, viewport.0);
    build_overlays(&mut out, &input(live), &live_layout);
    (out, live_layout)
}

/// §9.1: only a waiting train's pulse and the inset's opening zoom animate.
pub fn animating(live: &Snapshot, view: &ViewState, now_s: f64, reduced_motion: bool) -> bool {
    if reduced_motion {
        return false;
    }
    let pulsing = |s: &Snapshot| {
        s.trains
            .values()
            .any(|t| block_state(&t.state) == BlockState::Wait)
    };
    let tweening = view
        .inset
        .as_ref()
        .is_some_and(|i| now_s - i.opened_s < inset::TWEEN_S);
    tweening || pulsing(live) || view.frozen.as_deref().is_some_and(pulsing)
}

pub fn run_app(backend: Box<dyn UiBackend>, opts: AppOptions) -> anyhow::Result<()> {
    let event_loop = EventLoop::<AppEvent>::with_user_event()
        .build()
        .context("creating the event loop")?;
    let proxy = event_loop.create_proxy();
    let waker = proxy.clone();
    backend.set_waker(Box::new(move || {
        let _ = waker.send_event(AppEvent::Wake);
    }));
    let mut app = App {
        backend,
        opts,
        proxy,
        start: Instant::now(),
        view: ViewState::default(),
        win: None,
        frame: None,
        mods: ModifiersState::empty(),
        cursor: (0.0, 0.0),
        clicks: ClickTracker::default(),
        dragging: false,
        composing: false,
        a11y_last: None,
        error: None,
    };
    event_loop
        .run_app(&mut app)
        .context("running the event loop")?;
    match app.error {
        Some(e) => Err(e),
        None => Ok(()),
    }
}

#[derive(Debug)]
enum AppEvent {
    Wake,
    AccessKit(accesskit_winit::Event),
}

impl From<accesskit_winit::Event> for AppEvent {
    fn from(e: accesskit_winit::Event) -> Self {
        AppEvent::AccessKit(e)
    }
}

struct Gpu {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    view_format: wgpu::TextureFormat,
    renderer: Renderer,
}

struct Win {
    window: Arc<Window>,
    a11y: accesskit_winit::Adapter,
    gpu: Gpu,
    ime_allowed: bool,
}

/// What the owner last saw: input is resolved against it.
struct Frame {
    scene: Scene,
    layout: Layout,
    live: Arc<Snapshot>,
    viewport: (f32, f32),
}

struct App {
    backend: Box<dyn UiBackend>,
    opts: AppOptions,
    proxy: EventLoopProxy<AppEvent>,
    start: Instant,
    view: ViewState,
    win: Option<Win>,
    frame: Option<Frame>,
    mods: ModifiersState,
    /// In scene units.
    cursor: (f32, f32),
    clicks: ClickTracker,
    dragging: bool,
    composing: bool,
    a11y_last: Option<(Vec<A11yNode>, String)>,
    error: Option<anyhow::Error>,
}

impl App {
    fn now_s(&self) -> f64 {
        self.start.elapsed().as_secs_f64()
    }

    fn scale(&self) -> f32 {
        self.win
            .as_ref()
            .map_or(1.0, |w| text_scale(w.window.scale_factor()))
    }

    fn viewport(&self) -> Option<((u32, u32), (f32, f32))> {
        let w = self.win.as_ref()?;
        let size = w.window.inner_size();
        let s = self.scale();
        Some((
            (size.width, size.height),
            (size.width as f32 / s, size.height as f32 / s),
        ))
    }

    fn build_frame(&mut self) -> Option<&Frame> {
        let (_, viewport) = self.viewport()?;
        let live = self.backend.snapshot();
        sync(&mut self.view, &live);
        let (scene, layout) = frame_scene(&live, &self.view, viewport, self.now_s(), &self.opts);
        self.frame = Some(Frame {
            scene,
            layout,
            live,
            viewport,
        });
        self.frame.as_ref()
    }

    fn input(&mut self, ev: UiInput) {
        if self.frame.is_none() && self.build_frame().is_none() {
            return;
        }
        let now_s = self.now_s();
        let Some(f) = &self.frame else { return };
        // Pointer moves arrive at display rate; most change nothing and need no frame.
        let before = self.view.clone();
        let actions = on_input(
            &mut self.view,
            &f.scene,
            &f.live,
            &f.layout,
            f.viewport,
            now_s,
            ev,
        );
        let changed = !actions.is_empty() || self.view != before;
        for a in actions {
            self.backend.perform(a);
        }
        self.update_ime();
        if changed {
            self.request_redraw();
        }
    }

    fn update_ime(&mut self) {
        let focused = self.view.console_focused;
        if let Some(w) = &mut self.win
            && w.ime_allowed != focused
        {
            w.window.set_ime_allowed(focused);
            w.ime_allowed = focused;
            if !focused {
                self.composing = false;
            }
        }
    }

    fn request_redraw(&self) {
        if let Some(w) = &self.win {
            w.window.request_redraw();
        }
    }

    fn redraw(&mut self) {
        let Some(((pw, ph), _)) = self.viewport() else {
            return;
        };
        if pw == 0 || ph == 0 {
            return;
        }
        let scale = self.scale();
        let now_s = self.now_s();
        let bg = palette(self.opts.palette).bg;
        if self.build_frame().is_none() {
            return;
        }
        let (Some(f), Some(w)) = (&self.frame, &mut self.win) else {
            return;
        };
        let g = &mut w.gpu;
        if (g.config.width, g.config.height) != (pw, ph) {
            g.config.width = pw;
            g.config.height = ph;
            g.surface.configure(&g.device, &g.config);
        }
        let (texture, suboptimal) = match g.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(t) => (t, false),
            wgpu::CurrentSurfaceTexture::Suboptimal(t) => (t, true),
            wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                g.surface.configure(&g.device, &g.config);
                w.window.request_redraw();
                return;
            }
            wgpu::CurrentSurfaceTexture::Timeout
            | wgpu::CurrentSurfaceTexture::Occluded
            | wgpu::CurrentSurfaceTexture::Validation => return,
        };
        let target = texture.texture.create_view(&wgpu::TextureViewDescriptor {
            format: Some(g.view_format),
            ..Default::default()
        });
        // The surface texture is the single-sample resolve target, at the surface's size.
        g.renderer.render(
            &g.device,
            &g.queue,
            &target,
            &f.scene.list,
            (pw, ph),
            scale,
            bg,
        );
        w.window.pre_present_notify();
        g.queue.present(texture);
        if suboptimal {
            g.surface.configure(&g.device, &g.config);
        }

        let nodes = a11y_tree(&f.live, &self.view);
        let focus = focus_id(&self.view, &nodes);
        if self
            .a11y_last
            .as_ref()
            .is_none_or(|(n, fo)| *n != nodes || *fo != focus)
        {
            w.a11y.update_if_active(|| tree_update(&nodes, &focus));
            self.a11y_last = Some((nodes, focus));
        }
        if animating(&f.live, &self.view, now_s, self.opts.reduced_motion) {
            w.window.request_redraw();
        }
        self.update_ime();
    }

    fn open(&mut self, el: &ActiveEventLoop) -> anyhow::Result<()> {
        let attrs = Window::default_attributes()
            .with_title(self.opts.title.clone())
            .with_inner_size(LogicalSize::new(1280.0, 800.0))
            .with_visible(false);
        let window = Arc::new(el.create_window(attrs).context("creating the window")?);
        // accesskit_winit panics if the window has already been shown.
        let a11y = accesskit_winit::Adapter::with_event_loop_proxy(el, &window, self.proxy.clone());
        let gpu = gpu(el, window.clone())?;
        window.set_ime_allowed(false);
        window.set_visible(true);
        window.request_redraw();
        self.win = Some(Win {
            window,
            a11y,
            gpu,
            ime_allowed: false,
        });
        Ok(())
    }

    fn mouse(&mut self, ev: WindowEvent) {
        let scale = self.scale();
        match ev {
            WindowEvent::CursorMoved { position, .. } => {
                let (x, y) = (position.x as f32 / scale, position.y as f32 / scale);
                let (dx, dy) = (x - self.cursor.0, y - self.cursor.1);
                self.cursor = (x, y);
                if self.dragging {
                    self.input(UiInput::Drag { dx, dy });
                }
                self.input(UiInput::Move { x, y });
            }
            WindowEvent::MouseInput {
                state,
                button: MouseButton::Left,
                ..
            } => match state {
                ElementState::Pressed => {
                    let (x, y) = self.cursor;
                    let double = self.clicks.press(self.now_s(), x, y);
                    self.dragging = self
                        .frame
                        .as_ref()
                        .is_none_or(|f| f.scene.hit_at(x, y).is_none());
                    self.input(UiInput::Click { x, y, double });
                }
                ElementState::Released => self.dragging = false,
            },
            WindowEvent::MouseWheel { delta, .. } => {
                let (dx, dy) = match delta {
                    MouseScrollDelta::PixelDelta(p) => (p.x as f32 / scale, p.y as f32 / scale),
                    // A wheel notch moves the board one track.
                    MouseScrollDelta::LineDelta(x, y) => {
                        let step = TRACK_PITCH * self.view.k;
                        (x * step, y * step)
                    }
                };
                self.input(UiInput::Scroll { dx, dy });
            }
            WindowEvent::PinchGesture { delta, .. } => {
                let factor = (1.0 + delta) as f32;
                if factor.is_finite() && factor > 0.0 {
                    self.input(UiInput::Zoom {
                        factor,
                        at: self.cursor,
                    });
                }
            }
            _ => {}
        }
    }

    fn a11y_event(&mut self, ev: accesskit_winit::WindowEvent) {
        match ev {
            accesskit_winit::WindowEvent::InitialTreeRequested => {
                self.a11y_last = None;
                self.request_redraw();
            }
            accesskit_winit::WindowEvent::ActionRequested(req) => {
                if req.action == accesskit::Action::Focus
                    && req.target_node == node_id(pelhaim_board::a11y::CONSOLE_ID)
                {
                    self.view.console_focused = true;
                    self.update_ime();
                    self.request_redraw();
                }
            }
            accesskit_winit::WindowEvent::AccessibilityDeactivated => {}
        }
    }
}

fn gpu(el: &ActiveEventLoop, window: Arc<Window>) -> anyhow::Result<Gpu> {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_with_display_handle_from_env(
        Box::new(el.owned_display_handle()),
    ));
    let size = window.inner_size();
    let surface = instance
        .create_surface(window)
        .context("creating the surface")?;
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        compatible_surface: Some(&surface),
        ..Default::default()
    }))
    .context("no wgpu adapter for the window")?;
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("pelhaim window"),
        ..Default::default()
    }))?;
    let mut config = surface
        .get_default_config(&adapter, size.width.max(1), size.height.max(1))
        .context("the surface does not fit this adapter")?;
    let caps = surface.get_capabilities(&adapter);
    // The renderer writes linear colour, so it needs an sRGB view to encode it.
    if let Some(f) = caps.formats.iter().find(|f| f.is_srgb()) {
        config.format = *f;
    }
    let view_format = config.format.add_srgb_suffix();
    if view_format != config.format {
        config.view_formats = vec![view_format];
    }
    config.present_mode = wgpu::PresentMode::AutoVsync;
    surface.configure(&device, &config);
    let renderer = Renderer::new(&device, view_format);
    Ok(Gpu {
        surface,
        device,
        queue,
        config,
        view_format,
        renderer,
    })
}

impl ApplicationHandler<AppEvent> for App {
    fn resumed(&mut self, el: &ActiveEventLoop) {
        if self.win.is_some() {
            return;
        }
        if let Err(e) = self.open(el) {
            self.error = Some(e);
            el.exit();
        }
    }

    fn user_event(&mut self, _el: &ActiveEventLoop, ev: AppEvent) {
        match ev {
            AppEvent::Wake => self.request_redraw(),
            AppEvent::AccessKit(e) => self.a11y_event(e.window_event),
        }
    }

    fn window_event(&mut self, el: &ActiveEventLoop, _id: WindowId, ev: WindowEvent) {
        if let Some(w) = &mut self.win {
            w.a11y.process_event(&w.window, &ev);
        }
        match ev {
            WindowEvent::CloseRequested => el.exit(),
            WindowEvent::RedrawRequested => self.redraw(),
            WindowEvent::Resized(_) | WindowEvent::ScaleFactorChanged { .. } => {
                self.request_redraw()
            }
            WindowEvent::ModifiersChanged(m) => self.mods = m.state(),
            WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed => {
                let inputs = press_inputs(
                    &event.logical_key,
                    event.text.as_deref(),
                    self.mods,
                    Platform::current(),
                    self.view.console_focused,
                    self.composing,
                );
                for ev in inputs {
                    self.input(ev);
                }
            }
            WindowEvent::Ime(ime) => match ime {
                Ime::Commit(s) => {
                    self.composing = false;
                    self.input(UiInput::Text(s));
                }
                Ime::Preedit(s, _) => {
                    self.composing = !s.is_empty();
                    self.input(UiInput::Preedit((!s.is_empty()).then_some(s)));
                }
                Ime::Disabled => {
                    self.composing = false;
                    self.input(UiInput::Preedit(None));
                }
                Ime::Enabled => {}
            },
            ev @ (WindowEvent::CursorMoved { .. }
            | WindowEvent::MouseInput { .. }
            | WindowEvent::MouseWheel { .. }
            | WindowEvent::PinchGesture { .. }) => self.mouse(ev),
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pelhaim_board::{Hit, InsetState, build_scene};
    use pelhaim_core::testkit::sample_board;

    const VP: (f32, f32) = (1600.0, 900.0);

    #[test]
    fn text_scale_is_a_whole_number_of_device_pixels() {
        assert_eq!(text_scale(1.0), 1.0);
        assert_eq!(text_scale(2.0), 2.0);
        assert_eq!(text_scale(3.0), 3.0);
        assert_eq!(text_scale(1.25), 1.0);
        assert_eq!(text_scale(1.75), 2.0);
        assert_eq!(text_scale(0.5), 1.0);
        assert_eq!(text_scale(f64::NAN), 1.0);
    }

    #[test]
    fn double_click_needs_350ms_and_6px() {
        let mut c = ClickTracker::default();
        assert!(!c.press(0.0, 10.0, 10.0));
        assert!(c.press(0.35, 16.0, 4.0));
        assert!(!c.press(0.4, 16.0, 4.0), "a third press starts a new pair");
        assert!(!c.press(0.8, 16.0, 4.0), "too slow");
        assert!(!c.press(0.9, 23.0, 4.0), "too far");
        assert!(c.press(1.0, 23.0, 4.0));
    }

    fn train_hits(s: &Scene) -> usize {
        s.hits
            .iter()
            .filter(|(_, h)| matches!(h, Hit::Train(_)))
            .count()
    }

    #[test]
    fn live_frame_is_the_board_scene() {
        let snap = sample_board();
        let view = ViewState::default();
        let opts = AppOptions::default();
        let (scene, _) = frame_scene(&snap, &view, VP, 0.0, &opts);
        let want = build_scene(&SceneInput {
            snap: &snap,
            view: &view,
            viewport: VP,
            now_s: 0.0,
            palette: opts.palette,
            reduced_motion: opts.reduced_motion,
        });
        assert_eq!(scene, want);
    }

    #[test]
    fn paused_board_draws_the_frozen_snapshot() {
        let frozen = sample_board();
        let mut live = frozen.clone();
        // A line's only session, so its whole line train goes with it.
        let gone = pelhaim_board::line_trains(&live)
            .into_iter()
            .find(|lt| lt.members.len() == 1)
            .unwrap()
            .lead;
        live.trains.remove(&gone);
        let view = ViewState {
            paused: true,
            frozen: Some(Arc::new(frozen.clone())),
            ..ViewState::default()
        };
        let opts = AppOptions::default();
        let (paused, _) = frame_scene(&live, &view, VP, 0.0, &opts);
        let (unpaused, _) = frame_scene(&live, &ViewState::default(), VP, 0.0, &opts);
        assert!(paused.hits.iter().any(|(_, h)| *h == Hit::Train(gone)));
        assert!(train_hits(&paused) > train_hits(&unpaused));
    }

    #[test]
    fn only_pulse_and_inset_tween_animate() {
        let snap = sample_board();
        let view = ViewState::default();
        assert!(
            animating(&snap, &view, 0.0, false),
            "sample_board has a waiting train"
        );
        assert!(!animating(&snap, &view, 0.0, true));

        let mut calm = snap.clone();
        for t in calm.trains.values_mut() {
            t.state = pelhaim_core::TrainState::Working;
        }
        assert!(!animating(&calm, &view, 0.0, false));
        let t = *calm.trains.keys().next().unwrap();
        let opening = ViewState {
            inset: Some(InsetState {
                train: t,
                opened_s: 1.0,
            }),
            ..ViewState::default()
        };
        assert!(animating(&calm, &opening, 1.1, false));
        assert!(!animating(&calm, &opening, 1.01 + inset::TWEEN_S, false));
    }
}
