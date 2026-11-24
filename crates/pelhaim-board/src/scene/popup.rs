use pelhaim_core::{
    Alert, AlertId, AlertKind, AlertState, LineKey, Snapshot, Train, format_train_number,
    truncate_chars,
};

use super::{
    AlertButton, Hit, Layout, Ov, Scene, SceneInput, console, line_name, pad, station_title,
    tag_rect, wrap,
};
use crate::cards::{Mark, parse_cards};
use crate::color::Rgba;
use crate::display::{P, Prim, R};
use crate::font::{Cut, text_width};
use crate::input::PopupState;
use crate::palette::Palette;

pub const POPUP_W: f32 = 320.0;
pub const BAND: f32 = 8.0;
pub const DASH: f32 = 10.0;
pub const GAP: f32 = 6.0;
pub const DASH_T: f32 = 2.5;
pub const HEIGHT_CAP: f32 = 0.6;
pub const MORE: &str = "… (more in terminal)";
const ROW_H: f32 = 11.0;
const PAD: f32 = 6.0;
const TOP: f32 = 16.0;
const MARGIN: f32 = 10.0;
const TAG_GAP: f32 = 14.0;
const HIGHLIGHT_ALPHA: f32 = 0.35;

#[derive(Clone, PartialEq, Debug)]
pub struct Span {
    pub text: String,
    pub color: Rgba,
    pub hit: Option<AlertButton>,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Flow {
    /// Always shown.
    Fixed,
    /// Wrapped question text: cut first at the height cap.
    Text,
    /// Option or card rows: cut only when the text alone cannot make room.
    List,
}

#[derive(Clone, PartialEq, Debug)]
pub struct Row {
    pub spans: Vec<Span>,
    /// The whole row is clickable.
    pub hit: Option<AlertButton>,
    pub highlight: bool,
    pub flow: Flow,
}

impl Row {
    pub fn text(&self) -> String {
        self.spans.iter().map(|s| s.text.as_str()).collect()
    }

    fn plain(text: impl Into<String>, color: Rgba, flow: Flow) -> Row {
        Row {
            spans: vec![Span {
                text: text.into(),
                color,
                hit: None,
            }],
            hit: None,
            highlight: false,
            flow,
        }
    }
}

#[derive(Clone, PartialEq, Debug)]
pub struct PopupPlan {
    pub alert: AlertId,
    pub tab: &'static str,
    /// The navy band's outer rect, in overlay units.
    pub rect: R,
    pub rows: Vec<Row>,
    /// Connector end: the tag centre clamped to the viewport, in overlay units.
    pub tag: Option<P>,
}

pub fn body_chars() -> usize {
    ((POPUP_W - 2.0 * BAND - 2.0 * PAD) / crate::font::advance(Cut::C5x7, 1.0)) as usize
}

/// The configured projects in board order; `AlertButton::ProjectPick(i)` indexes this.
pub fn pick_projects(snap: &Snapshot) -> Vec<(String, String)> {
    snap.lines
        .iter()
        .filter_map(|l| match &l.key {
            LineKey::Project(slug) => Some((slug.clone(), l.name.clone())),
            _ => None,
        })
        .collect()
}

fn option_row(
    n: usize,
    label: &str,
    desc: &str,
    checked: bool,
    clickable: Option<AlertButton>,
    highlight: bool,
    p: &Palette,
) -> Row {
    let mut text = format!("[{}] {} {label}", n + 1, if checked { '☑' } else { '☐' });
    if !desc.is_empty() {
        text.push_str(" — ");
        text.push_str(desc);
    }
    let color = if clickable.is_some() { p.label } else { p.done };
    Row {
        hit: clickable,
        highlight,
        ..Row::plain(truncate_chars(&text, body_chars()), color, Flow::List)
    }
}

fn button(text: &str, color: Rgba, b: AlertButton) -> Span {
    Span {
        text: text.into(),
        color,
        hit: Some(b),
    }
}

fn content(
    snap: &Snapshot,
    a: &Alert,
    train: Option<&Train>,
    ps: &PopupState,
    p: &Palette,
) -> Vec<Row> {
    let wc = body_chars();
    let line = line_name(snap, &a.line);
    let title = match train {
        Some(t) => match station_title(snap, t) {
            Some(st) => format!("{} · {line} / {st}", format_train_number(t.number)),
            None => format!("{} · {line}", format_train_number(t.number)),
        },
        None => line,
    };
    let mut rows = vec![Row::plain(truncate_chars(&title, wc), p.label, Flow::Fixed)];
    let answered = match &a.state {
        AlertState::Answered { answer, .. } => Some(answer.as_str()),
        _ => None,
    };
    let status = |s: &str, c| Row::plain(s, c, Flow::Fixed);
    match (&a.kind, answered) {
        (_, Some(_)) => rows.push(status("STATUS: ANSWERED", p.work)),
        (_, None) if a.state == AlertState::Expired => rows.push(status("STATUS: EXPIRED", p.done)),
        (AlertKind::Question { .. } | AlertKind::AssignProject { .. }, None) => {
            rows.push(status("STATUS: WAITING ON YOU", p.wait));
        }
        (AlertKind::Stalled, None) => {
            rows.push(status("STATUS: UNKNOWN", p.wait));
            rows.push(status("COMMUNICATION: NONE", p.wait));
            let n = train.map_or_else(|| "----".into(), |t| format_train_number(t.number));
            rows.push(status(&format!("Trying to contact train {n}…"), p.label));
        }
        (AlertKind::Result { .. }, None) => rows.push(status("STATUS: DONE", p.work)),
        (AlertKind::Failure { .. }, None) => rows.push(status("STATUS: FAILED", p.fail)),
    }

    let open = answered.is_none() && a.is_open();
    let clickable = |b: AlertButton| (open && a.answerable).then_some(b);
    if ps.picking_project {
        rows.push(Row::plain(
            "Log the decision under which project?",
            p.label,
            Flow::Text,
        ));
        for (i, (_, name)) in pick_projects(snap).iter().enumerate() {
            rows.push(option_row(
                i,
                name,
                "",
                false,
                Some(AlertButton::ProjectPick(i)),
                ps.highlighted == Some(i),
                p,
            ));
        }
        return rows;
    }
    match &a.kind {
        AlertKind::Question { questions } => {
            for q in questions {
                for l in wrap(&q.question, wc) {
                    rows.push(Row::plain(l, p.label, Flow::Text));
                }
                for (i, o) in q.options.iter().enumerate() {
                    rows.push(option_row(
                        i,
                        &o.label,
                        &o.description,
                        ps.checked.contains(&i),
                        clickable(AlertButton::Option(i)),
                        open && ps.highlighted == Some(i),
                        p,
                    ));
                }
            }
            if open && a.answerable && questions.iter().any(|q| q.multi_select) {
                rows.push(Row {
                    spans: vec![button("[Send]", p.idle, AlertButton::Send)],
                    ..Row::plain("", p.label, Flow::Fixed)
                });
            }
        }
        AlertKind::AssignProject {
            options, prefill, ..
        } => {
            rows.push(Row::plain(
                "Which project is this train for?",
                p.label,
                Flow::Text,
            ));
            let highlighted = ps.highlighted.or(*prefill);
            for (i, o) in options.iter().enumerate() {
                rows.push(option_row(
                    i,
                    &o.label,
                    "",
                    false,
                    clickable(AlertButton::Option(i)),
                    open && highlighted == Some(i),
                    p,
                ));
            }
        }
        AlertKind::Result { summary } => {
            for c in parse_cards(summary) {
                let mark = match c.mark.glyph() {
                    Some(g) => format!("{g} "),
                    None => "  ".into(),
                };
                rows.push(Row {
                    spans: vec![
                        Span {
                            text: mark,
                            color: c.mark.color(p),
                            hit: None,
                        },
                        Span {
                            text: truncate_chars(&c.text, wc - 2),
                            color: if c.mark == Mark::Plain {
                                p.label
                            } else {
                                c.mark.color(p)
                            },
                            hit: None,
                        },
                    ],
                    ..Row::plain("", p.label, Flow::List)
                });
            }
        }
        AlertKind::Failure { reason } => {
            for l in wrap(reason, wc) {
                rows.push(Row::plain(l, p.label, Flow::Text));
            }
        }
        AlertKind::Stalled => {}
    }

    // F36: Result alerts get no [Log decision] in M1.
    let is_result = matches!(a.kind, AlertKind::Result { .. });
    if let Some(ans) = answered {
        rows.push(Row::plain(
            truncate_chars(&format!("Answer: {ans}"), wc),
            p.done,
            Flow::Fixed,
        ));
        if !is_result && a.decision.is_none() {
            rows.push(Row {
                spans: vec![button("[Log decision]", p.idle, AlertButton::LogDecision)],
                ..Row::plain("", p.label, Flow::Fixed)
            });
        }
    } else if open && !a.answerable && matches!(a.kind, AlertKind::Question { .. }) {
        let mut spans = vec![Span {
            text: "Answer in terminal  ".into(),
            color: p.wait,
            hit: None,
        }];
        if train.is_some() {
            spans.push(button(
                "[Copy session id]",
                p.idle,
                AlertButton::CopySessionId,
            ));
        }
        rows.push(Row {
            spans,
            ..Row::plain("", p.label, Flow::Fixed)
        });
    }
    rows
}

fn ends_more(row: &mut Row, wc: usize) {
    let keep = wc.saturating_sub(MORE.chars().count() + 1);
    let text: String = row.text().chars().take(keep).collect();
    let color = row.spans.first().map_or(Rgba::hex(0), |s| s.color);
    *row = Row::plain(format!("{} {MORE}", text.trim_end()), color, row.flow);
}

/// Cuts rows to `cap`: question text first (from its end, keeping one line), then list rows.
/// The last line that remains before the cut ends with `MORE`.
fn cap_rows(rows: &mut Vec<Row>, cap: usize) {
    if rows.len() <= cap {
        return;
    }
    let mut over = rows.len() - cap;
    let mut cut_text = false;
    while over > 0 && rows.iter().filter(|r| r.flow == Flow::Text).count() > 1 {
        let i = rows.iter().rposition(|r| r.flow == Flow::Text).unwrap_or(0);
        rows.remove(i);
        over -= 1;
        cut_text = true;
    }
    let mut cut_list = false;
    while over > 0 {
        let Some(i) = rows.iter().rposition(|r| r.flow == Flow::List) else {
            break;
        };
        rows.remove(i);
        over -= 1;
        cut_list = true;
    }
    let flow = if cut_list { Flow::List } else { Flow::Text };
    if (cut_text || cut_list)
        && let Some(i) = rows
            .iter()
            .rposition(|r| r.flow == flow)
            .or_else(|| rows.iter().rposition(|r| r.flow == Flow::Text))
    {
        ends_more(&mut rows[i], body_chars());
    }
    rows.truncate(cap.max(1));
}

fn chrome_h() -> f32 {
    2.0 * BAND + TOP + PAD
}

pub fn plan(inp: &SceneInput, layout: &Layout) -> Option<PopupPlan> {
    let ps = inp.view.popup.as_ref()?;
    let snap = inp.snap;
    let a = snap.alerts.iter().find(|a| a.id == ps.alert)?;
    let train = a.train.and_then(|id| snap.trains.get(&id));
    let ov = Ov::new(inp);
    let mut rows = content(snap, a, train, ps, &ov.p);
    let cap = ((ov.h * HEIGHT_CAP - chrome_h()) / ROW_H).floor().max(1.0) as usize;
    cap_rows(&mut rows, cap);
    let h = chrome_h() + rows.len() as f32 * ROW_H;

    let (left, top) = (MARGIN, MARGIN);
    let right = ov.w - MARGIN;
    let bottom = console::geom(inp).strip.y - MARGIN;
    let tag = train
        .filter(|t| layout.tracks.contains_key(&t.track))
        .map(|t| {
            let s = inp.view.view().rect(tag_rect(layout, t));
            let k = ov.v.k;
            R {
                x: s.x / k,
                y: s.y / k,
                w: s.w / k,
                h: s.h / k,
            }
        });
    let (x, y) = match tag {
        Some(tr) => {
            let below = tr.y + tr.h + TAG_GAP;
            let y = if below + h <= bottom {
                below
            } else {
                tr.y - TAG_GAP - h
            };
            (tr.x - 24.0, y)
        }
        None => ((ov.w - POPUP_W) / 2.0, (bottom - h) / 2.0),
    };
    let rect = R {
        x: x.min(right - POPUP_W).max(left),
        y: y.min(bottom - h).max(top),
        w: POPUP_W,
        h,
    };
    Some(PopupPlan {
        alert: a.id.clone(),
        tab: a.kind.tab(),
        rect,
        rows,
        tag: tag.map(|tr| P {
            x: (tr.x + tr.w / 2.0).clamp(0.0, ov.w),
            y: (tr.y + tr.h / 2.0).clamp(0.0, ov.h),
        }),
    })
}

/// The popup's frame stack, shared with the inset: offset white and yellow lines, a navy
/// band with a dashed cyan border, and the body. Returns the body rect.
pub(crate) fn frame(out: &mut Scene, ov: &Ov, r: R) -> R {
    let p = &ov.p;
    let shift = |dx: f32, dy: f32| R {
        x: r.x + dx,
        y: r.y + dy,
        ..r
    };
    ov.outline(&mut out.list, shift(-7.0, -5.0), 1.0, p.frame_white);
    ov.outline(&mut out.list, shift(-3.0, -9.0), 1.0, p.frame_yellow);
    ov.rect(&mut out.list, r, p.frame_navy);
    ov.dashed(&mut out.list, pad(r, -1.5), DASH, GAP, DASH_T, p.frame_dash);
    let body = pad(r, -BAND);
    ov.rect(&mut out.list, body, p.body);
    body
}

pub(crate) fn tab(out: &mut Scene, ov: &Ov, r: R, text: &str) {
    let t = R {
        x: r.x + 10.0,
        y: r.y - 4.0,
        w: text_width(text, Cut::C7x9, 1.0) + 8.0,
        h: 13.0,
    };
    ov.rect(&mut out.list, t, ov.p.idle);
    ov.text(
        &mut out.list,
        text,
        t.x + 4.0,
        t.y + 2.0,
        Cut::C7x9,
        ov.p.bg,
    );
}

/// The ✕ in the body's top-right corner; returns its hit rect.
pub(crate) fn close(out: &mut Scene, ov: &Ov, body: R) -> R {
    let x = body.x + body.w - PAD - 7.0;
    ov.button(&mut out.list, "✕", x, body.y + 3.0, Cut::C7x9, ov.p.label)
}

pub fn draw(out: &mut Scene, inp: &SceneInput, layout: &Layout) {
    let Some(plan) = plan(inp, layout) else {
        return;
    };
    let ov = Ov::new(inp);
    let p = ov.p;
    let r = plan.rect;
    if let Some(t) = plan.tag {
        let ex = t.x.clamp(r.x + 8.0, r.x + r.w - 8.0);
        let ey = if t.y < r.y { r.y } else { r.y + r.h };
        out.list.prims.push(Prim::Seg {
            a: P {
                x: ov.v.x(ex),
                y: ov.v.y(ey),
            },
            b: P {
                x: ov.v.x(t.x),
                y: ov.v.y(t.y),
            },
            width: ov.v.k,
            color: p.glyph,
        });
    }
    let body = frame(out, &ov, r);
    tab(out, &ov, r, plan.tab);
    let id = plan.alert.clone();
    let hit = |out: &mut Scene, rr: R, button: AlertButton| {
        ov.hit(
            out,
            rr,
            Hit::Alert {
                id: id.clone(),
                button,
            },
        );
    };
    let c = close(out, &ov, body);
    hit(out, c, AlertButton::Close);
    for (i, row) in plan.rows.iter().enumerate() {
        let y = body.y + TOP + i as f32 * ROW_H;
        let rr = R {
            x: body.x + 2.0,
            y,
            w: body.w - 4.0,
            h: ROW_H,
        };
        if row.highlight {
            ov.rect(&mut out.list, rr, p.frame_dash.alpha(HIGHLIGHT_ALPHA));
        }
        let mut x = body.x + PAD;
        for s in &row.spans {
            let b = ov.button(&mut out.list, &s.text, x, y + 2.0, Cut::C5x7, s.color);
            if let Some(btn) = &s.hit {
                hit(out, b, btn.clone());
            }
            x += text_width(&s.text, Cut::C5x7, 1.0);
        }
        if let Some(btn) = &row.hit {
            hit(out, rr, btn.clone());
        }
    }
}
