use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::Arc;

use pelhaim_core::{
    Alert, AlertId, AlertKind, AlertState, Answer, Choice, LineKey, Origin, Snapshot, TrainId,
    TrainState,
};

use crate::cards::{CardRow, Mark};
use crate::console_cmd::{ConsoleCmd, parse_console};
use crate::display::R;
use crate::scene::popup::pick_projects;
use crate::scene::{AlertButton, Hit, InsetAction, Layout, Scene, TRACK_PITCH, inset};
use crate::textinput::TextInput;
use crate::view::{MAX_ZOOM, MIN_ZOOM, View, fit};

#[derive(Clone, PartialEq, Debug)]
pub struct ViewState {
    pub k: f32,
    pub ox: f32,
    pub oy: f32,
    pub selected: Option<TrainId>,
    pub expanded: BTreeSet<LineKey>,
    pub hover: Option<Hit>,
    pub popup: Option<PopupState>,
    pub inset: Option<InsetState>,
    pub console: TextInput,
    pub console_focused: bool,
    pub radio_log_open: bool,
    pub radio_log: Vec<CardRow>,
    pub paused: bool,
    /// The snapshot the board draws while paused; popups and the tray keep the live one.
    pub frozen: Option<Arc<Snapshot>>,
    /// Alerts already auto-opened once, so a dismissed one never pops up again by itself.
    pub shown: BTreeSet<AlertId>,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct PopupState {
    pub alert: AlertId,
    pub highlighted: Option<usize>,
    pub picking_project: bool,
}

impl PopupState {
    pub fn new(alert: AlertId) -> Self {
        PopupState {
            alert,
            highlighted: None,
            picking_project: false,
        }
    }
}

#[derive(Clone, PartialEq, Debug)]
pub struct InsetState {
    pub train: TrainId,
    pub opened_s: f64,
}

// By hand, not derived: a derived `k = 0` would collapse the whole board to a point (F30).
impl Default for ViewState {
    fn default() -> Self {
        ViewState {
            k: 1.0,
            ox: 0.0,
            oy: 0.0,
            selected: None,
            expanded: BTreeSet::new(),
            hover: None,
            popup: None,
            inset: None,
            console: TextInput::default(),
            console_focused: false,
            radio_log_open: false,
            radio_log: Vec::new(),
            paused: false,
            frozen: None,
            shown: BTreeSet::new(),
        }
    }
}

impl ViewState {
    pub fn view(&self) -> View {
        View {
            k: self.k,
            ox: self.ox,
            oy: self.oy,
        }
    }

    fn set_view(&mut self, v: View) {
        (self.k, self.ox, self.oy) = (v.k, v.ox, v.oy);
    }

    /// Zooms by `factor` keeping the board point under screen point `at` fixed.
    fn zoom_at(&mut self, factor: f32, at: (f32, f32)) {
        let k = (self.k * factor).clamp(MIN_ZOOM, MAX_ZOOM);
        let (u, w) = self.view().inv(at.0, at.1);
        self.set_view(View {
            k,
            ox: at.0 - u * k,
            oy: at.1 - w * k,
        });
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum UiKey {
    Char(char),
    Enter,
    Esc,
    Tab,
    Backspace,
    Left,
    Right,
    Home,
    End,
    Up,
    Down,
    Space,
}

#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub struct Mods {
    pub shift: bool,
    pub cmd: bool,
}

/// Printable console text arrives only as `Text` (F41); `Key(Char)` is for shortcuts.
#[derive(Clone, PartialEq, Debug)]
pub enum UiInput {
    Key(UiKey, Mods),
    Click { x: f32, y: f32, double: bool },
    Move { x: f32, y: f32 },
    Drag { dx: f32, dy: f32 },
    Scroll { dx: f32, dy: f32 },
    Zoom { factor: f32, at: (f32, f32) },
    Text(String),
    Preedit(Option<String>),
}

/// F40: no `Assign` action in M1; answering the Assign alert covers it.
/// F38: `Reveal` carries the raw cwd; the backend validates it before revealing.
#[derive(Clone, PartialEq, Debug)]
pub enum UiAction {
    Answer(Answer),
    Dismiss(AlertId),
    Reopen(AlertId),
    LogDecision {
        alert: AlertId,
        project: Option<String>,
    },
    ExportPending,
    PauseKeepAwake,
    CopyToClipboard(String),
    Reveal(PathBuf),
    Redraw,
}

pub const KEY_ZOOM: f32 = 1.25;
pub const DECISION_CARDS: usize = 10;

pub fn sync(view: &mut ViewState, snap: &Snapshot) {
    let present = |id: &AlertId| snap.alerts.iter().any(|a| &a.id == id);
    view.shown.retain(|id| present(id));
    if view.popup.as_ref().is_some_and(|p| !present(&p.alert)) {
        view.popup = None;
    }
    if view
        .inset
        .as_ref()
        .is_some_and(|i| !snap.trains.contains_key(&i.train))
    {
        view.inset = None;
    }
    if view.selected.is_some_and(|t| !snap.trains.contains_key(&t)) {
        view.selected = None;
    }
    if view.popup.is_none()
        && let Some(a) = snap
            .alerts
            .iter()
            .find(|a| a.state == AlertState::Open && !view.shown.contains(&a.id))
    {
        view.popup = Some(PopupState::new(a.id.clone()));
        view.shown.insert(a.id.clone());
    }
}

pub fn on_input(
    view: &mut ViewState,
    scene: &Scene,
    snap: &Snapshot,
    layout: &Layout,
    viewport: (f32, f32),
    now_s: f64,
    ev: UiInput,
) -> Vec<UiAction> {
    match ev {
        UiInput::Key(k, m) if view.console_focused => console_key(view, snap, layout, k, m),
        UiInput::Key(k, m) => board_key(view, snap, layout, viewport, now_s, k, m),
        UiInput::Text(s) => {
            if view.console_focused {
                view.console.preedit = None;
                view.console.insert(&s);
            }
            vec![]
        }
        UiInput::Preedit(p) => {
            if view.console_focused {
                view.console.preedit = p;
            }
            vec![]
        }
        UiInput::Click { x, y, double } => match scene.hit_at(x, y).cloned() {
            Some(h) => click(view, snap, now_s, h, double),
            None => vec![],
        },
        UiInput::Move { x, y } => {
            let h = scene.hit_at(x, y).cloned();
            if h == view.hover {
                vec![]
            } else {
                view.hover = h;
                vec![UiAction::Redraw]
            }
        }
        UiInput::Drag { dx, dy } | UiInput::Scroll { dx, dy } => {
            view.ox += dx;
            view.oy += dy;
            vec![]
        }
        UiInput::Zoom { factor, at } => {
            view.zoom_at(factor, at);
            vec![]
        }
    }
}

fn console_key(
    view: &mut ViewState,
    snap: &Snapshot,
    layout: &Layout,
    k: UiKey,
    m: Mods,
) -> Vec<UiAction> {
    let c = &mut view.console;
    match k {
        UiKey::Esc => view.console_focused = false,
        UiKey::Enter => {
            let s = c.take();
            return run_command(view, snap, layout, parse_console(&s));
        }
        UiKey::Backspace => c.backspace(),
        UiKey::Left => c.left(),
        UiKey::Right => c.right(),
        UiKey::Home => c.home(),
        UiKey::End => c.end(),
        UiKey::Up if m.cmd => view.radio_log_open = !view.radio_log_open,
        _ => {}
    }
    vec![]
}

fn run_command(
    view: &mut ViewState,
    snap: &Snapshot,
    layout: &Layout,
    cmd: ConsoleCmd,
) -> Vec<UiAction> {
    let card = |mark, text: &str| CardRow {
        mark,
        text: text.into(),
    };
    let cards = match cmd {
        ConsoleCmd::Empty => return vec![],
        ConsoleCmd::DecisionsExport => return vec![UiAction::ExportPending],
        ConsoleCmd::Goto(name) => match goto_top(snap, layout, &name) {
            Some(top) => {
                view.oy = -top * view.k;
                return vec![];
            }
            None => vec![card(Mark::Failed, &format!("no line named {name}"))],
        },
        ConsoleCmd::Decisions if snap.decisions_recent.is_empty() => {
            vec![card(Mark::Plain, "no decisions logged yet")]
        }
        ConsoleCmd::Decisions => snap
            .decisions_recent
            .iter()
            .take(DECISION_CARDS)
            .map(|d| {
                let mark = if d.written_to_md {
                    Mark::Done
                } else {
                    Mark::Pending
                };
                card(mark, &format!("{} {}: {}", d.date, d.project, d.title))
            })
            .collect(),
        ConsoleCmd::Mode => vec![
            card(Mark::Done, "observe"),
            card(Mark::Pending, "launch (M2)"),
            card(Mark::Pending, "direct (M3)"),
        ],
        ConsoleCmd::Help => [
            "/goto <line>",
            "/decisions",
            "/decisions export",
            "/mode",
            "@<train> <text>",
        ]
        .into_iter()
        .map(|s| card(Mark::Plain, s))
        .collect(),
        ConsoleCmd::Train { .. } => vec![card(
            Mark::Needs,
            "observed sessions are answered in their own terminal",
        )],
        ConsoleCmd::Unknown(s) => vec![card(Mark::Failed, &format!("unknown command: {s}"))],
    };
    view.radio_log.extend(cards);
    // The reply is only visible in the radio log, so a command that answers opens it.
    view.radio_log_open = true;
    vec![]
}

fn goto_top(snap: &Snapshot, layout: &Layout, name: &str) -> Option<f32> {
    let line = snap.lines.iter().find(|l| {
        l.name.eq_ignore_ascii_case(name)
            || match &l.key {
                LineKey::Project(slug) => slug.eq_ignore_ascii_case(name),
                LineKey::Dispatch => name.eq_ignore_ascii_case("dispatch"),
                LineKey::Unsorted => name.eq_ignore_ascii_case("unsorted"),
            }
    })?;
    layout
        .lines
        .iter()
        .find(|b| b.key == line.key)
        .map(|b| b.top)
}

fn board_key(
    view: &mut ViewState,
    snap: &Snapshot,
    layout: &Layout,
    viewport: (f32, f32),
    now_s: f64,
    k: UiKey,
    m: Mods,
) -> Vec<UiAction> {
    let centre = (viewport.0 / 2.0, viewport.1 / 2.0);
    match k {
        UiKey::Char('=' | '+') if m.cmd => view.zoom_at(KEY_ZOOM, centre),
        UiKey::Char('-') if m.cmd => view.zoom_at(1.0 / KEY_ZOOM, centre),
        UiKey::Up if m.cmd => view.radio_log_open = !view.radio_log_open,
        _ if m.cmd => {}
        UiKey::Char('/') => {
            view.console_focused = true;
            view.console.insert("/");
        }
        UiKey::Char(c @ '1'..='9') => {
            let i = c as usize - '1' as usize;
            if let Some(ps) = view.popup.as_mut()
                && i < option_count(snap, ps)
            {
                ps.highlighted = Some(i);
            }
        }
        UiKey::Char('0') => view.set_view(fit(layout.content, viewport)),
        UiKey::Char('f' | 'F') => {
            if let Some(r) = selection_rect(view, snap, layout) {
                view.set_view(fit(r, viewport));
            }
        }
        UiKey::Char('j' | 'J') => step_selection(view, snap, layout, 1),
        UiKey::Char('k' | 'K') => step_selection(view, snap, layout, -1),
        UiKey::Space => {
            view.paused = !view.paused;
            view.frozen = view.paused.then(|| Arc::new(snap.clone()));
        }
        UiKey::Tab => return cycle_alerts(view, snap, m.shift),
        UiKey::Enter if view.popup.is_some() => return send_popup(view, snap),
        UiKey::Enter => {
            if let Some(t) = view.selected.filter(|t| snap.trains.contains_key(t)) {
                view.inset = Some(InsetState {
                    train: t,
                    opened_s: now_s,
                });
            }
        }
        UiKey::Esc => {
            if view.popup.is_some() {
                return close_popup(view, snap);
            } else if view.inset.is_some() {
                view.inset = None;
            } else {
                view.selected = None;
            }
        }
        _ => {}
    }
    vec![]
}

fn find<'a>(snap: &'a Snapshot, id: &AlertId) -> Option<&'a Alert> {
    snap.alerts.iter().find(|a| &a.id == id)
}

/// Options an M1 popup can act on: an open, answerable Assign alert's targets, or the
/// project picker. Observed questions are read-only, so digits do nothing there.
fn option_count(snap: &Snapshot, ps: &PopupState) -> usize {
    if ps.picking_project {
        return pick_projects(snap).len();
    }
    match find(snap, &ps.alert) {
        Some(Alert {
            kind: AlertKind::AssignProject { options, .. },
            answerable: true,
            state: AlertState::Open | AlertState::Tray,
            ..
        }) => options.len(),
        _ => 0,
    }
}

fn send_popup(view: &mut ViewState, snap: &Snapshot) -> Vec<UiAction> {
    let Some(ps) = view.popup.as_mut() else {
        return vec![];
    };
    if ps.picking_project {
        return match ps.highlighted {
            Some(i) => pick_project(ps, snap, i),
            None => vec![],
        };
    }
    let prefill = match find(snap, &ps.alert).map(|a| &a.kind) {
        Some(AlertKind::AssignProject { prefill, .. }) => *prefill,
        _ => None,
    };
    match ps.highlighted.or(prefill) {
        Some(i) if i < option_count(snap, ps) => vec![answer(&ps.alert, i)],
        _ => vec![],
    }
}

fn answer(id: &AlertId, i: usize) -> UiAction {
    UiAction::Answer(Answer {
        alert_id: id.clone(),
        choice: Choice::Options(vec![i]),
        origin: Origin::Local,
    })
}

fn pick_project(ps: &mut PopupState, snap: &Snapshot, i: usize) -> Vec<UiAction> {
    let Some((slug, _)) = pick_projects(snap).into_iter().nth(i) else {
        return vec![];
    };
    ps.picking_project = false;
    ps.highlighted = None;
    vec![UiAction::LogDecision {
        alert: ps.alert.clone(),
        project: Some(slug),
    }]
}

/// ✕ and Esc park an open alert in the tray; anything else just closes.
fn close_popup(view: &mut ViewState, snap: &Snapshot) -> Vec<UiAction> {
    let Some(ps) = view.popup.take() else {
        return vec![];
    };
    match find(snap, &ps.alert) {
        Some(a) if a.state == AlertState::Open => vec![UiAction::Dismiss(ps.alert)],
        _ => vec![],
    }
}

fn open_alert(view: &mut ViewState, snap: &Snapshot, id: &AlertId) -> Vec<UiAction> {
    view.popup = Some(PopupState::new(id.clone()));
    view.shown.insert(id.clone());
    match find(snap, id) {
        Some(a) if a.state == AlertState::Tray => vec![UiAction::Reopen(id.clone())],
        _ => vec![],
    }
}

/// Trains with an open alert, each once, ordered by their oldest open alert.
fn trains_with_open_alerts(snap: &Snapshot) -> Vec<(TrainId, AlertId)> {
    let mut out: Vec<(TrainId, AlertId)> = Vec::new();
    for a in snap.alerts.iter().filter(|a| a.is_open()) {
        if let Some(t) = a.train
            && snap.trains.contains_key(&t)
            && !out.iter().any(|(x, _)| *x == t)
        {
            out.push((t, a.id.clone()));
        }
    }
    out
}

fn cycle_alerts(view: &mut ViewState, snap: &Snapshot, back: bool) -> Vec<UiAction> {
    let list = trains_with_open_alerts(snap);
    let Some(i) = step(list.iter().map(|(t, _)| *t), view.selected, back) else {
        return vec![];
    };
    let (t, id) = list[i].clone();
    view.selected = Some(t);
    open_alert(view, snap, &id)
}

/// The index after (or before) `current` in `items`, wrapping; from nothing, the first
/// (or the last).
fn step(
    items: impl Iterator<Item = TrainId>,
    current: Option<TrainId>,
    back: bool,
) -> Option<usize> {
    let items: Vec<TrainId> = items.collect();
    let n = items.len();
    if n == 0 {
        return None;
    }
    Some(
        match current.and_then(|c| items.iter().position(|&t| t == c)) {
            Some(i) if back => (i + n - 1) % n,
            Some(i) => (i + 1) % n,
            None if back => n - 1,
            None => 0,
        },
    )
}

/// Drawn trains in layout order: line order, then track order within the line.
fn layout_order(snap: &Snapshot, layout: &Layout) -> Vec<TrainId> {
    snap.lines
        .iter()
        .flat_map(|l| &l.tracks)
        .filter(|t| layout.tracks.contains_key(t))
        .flat_map(|track| snap.trains.values().filter(move |t| t.track == *track))
        .map(|t| t.id)
        .collect()
}

fn step_selection(view: &mut ViewState, snap: &Snapshot, layout: &Layout, dir: i32) {
    let order = layout_order(snap, layout);
    if let Some(i) = step(order.iter().copied(), view.selected, dir < 0) {
        view.selected = Some(order[i]);
    }
}

fn selection_rect(view: &ViewState, snap: &Snapshot, layout: &Layout) -> Option<R> {
    let t = snap.trains.get(&view.selected?)?;
    let g = layout.tracks.get(&t.track)?;
    let end = g.blocks.last().map_or(g.x0, |&(x, w)| x + w);
    Some(R {
        x: g.x0,
        y: g.y - TRACK_PITCH * 0.75,
        w: end - g.x0,
        h: TRACK_PITCH,
    })
}

/// The alert a yellow tag opens: the one it is waiting on, else its oldest open alert.
fn train_alert(snap: &Snapshot, t: TrainId) -> Option<AlertId> {
    let train = snap.trains.get(&t)?;
    if let TrainState::Waiting(id) = &train.state
        && find(snap, id).is_some_and(Alert::is_open)
    {
        return Some(id.clone());
    }
    snap.alerts
        .iter()
        .find(|a| a.train == Some(t) && a.is_open())
        .map(|a| a.id.clone())
}

fn click(view: &mut ViewState, snap: &Snapshot, now_s: f64, h: Hit, double: bool) -> Vec<UiAction> {
    match h {
        Hit::Train(t) => {
            view.selected = Some(t);
            if double {
                view.inset = Some(InsetState {
                    train: t,
                    opened_s: now_s,
                });
            } else if snap
                .trains
                .get(&t)
                .is_some_and(|tr| matches!(tr.state, TrainState::Waiting(_) | TrainState::Stalled))
                && let Some(id) = train_alert(snap, t)
            {
                return open_alert(view, snap, &id);
            }
            vec![]
        }
        Hit::Stub(key) => {
            if !view.expanded.remove(&key) {
                view.expanded.insert(key);
            }
            vec![]
        }
        Hit::Alert { id, button } => alert_click(view, snap, id, button),
        Hit::TrayItem(id) => open_alert(view, snap, &id),
        Hit::Console => {
            view.console_focused = true;
            vec![]
        }
        Hit::Lamp => vec![UiAction::PauseKeepAwake],
        Hit::InsetAction(a) => {
            let train = view.inset.as_ref().and_then(|i| snap.trains.get(&i.train));
            match a {
                InsetAction::Close => {
                    view.inset = None;
                    vec![]
                }
                InsetAction::OpenCwd => train
                    .and_then(|t| t.cwd.as_ref())
                    .map(|c| UiAction::Reveal(PathBuf::from(c)))
                    .into_iter()
                    .collect(),
                InsetAction::CopySessionId => train
                    .map(|t| UiAction::CopyToClipboard(inset::session_id(t).into()))
                    .into_iter()
                    .collect(),
            }
        }
        Hit::Blocker => vec![],
    }
}

fn alert_click(
    view: &mut ViewState,
    snap: &Snapshot,
    id: AlertId,
    button: AlertButton,
) -> Vec<UiAction> {
    let Some(ps) = view.popup.as_mut().filter(|p| p.alert == id) else {
        return vec![];
    };
    match button {
        AlertButton::Option(i) if i < option_count(snap, ps) && !ps.picking_project => {
            ps.highlighted = Some(i);
            vec![answer(&id, i)]
        }
        AlertButton::Option(_) => vec![],
        AlertButton::ProjectPick(i) => pick_project(ps, snap, i),
        AlertButton::LogDecision => {
            let Some(a) = find(snap, &id) else {
                return vec![];
            };
            if matches!(
                decision_line(snap, a),
                LineKey::Dispatch | LineKey::Unsorted
            ) {
                ps.picking_project = true;
                ps.highlighted = None;
                vec![]
            } else {
                vec![UiAction::LogDecision {
                    alert: id,
                    project: None,
                }]
            }
        }
        AlertButton::Close => close_popup(view, snap),
        AlertButton::CopySessionId => find(snap, &id)
            .and_then(|a| a.train)
            .and_then(|t| snap.trains.get(&t))
            .map(|t| UiAction::CopyToClipboard(inset::session_id(t).into()))
            .into_iter()
            .collect(),
    }
}

/// F39: the engine files a decision under the train's current line; `alert.line` is only
/// the fallback, since an Assign alert's line is still Unsorted after the answer.
fn decision_line<'a>(snap: &'a Snapshot, a: &'a Alert) -> &'a LineKey {
    a.train
        .and_then(|t| snap.trains.get(&t))
        .and_then(|t| snap.tracks.get(&t.track))
        .map_or(&a.line, |tr| &tr.line)
}
