use pelhaim_core::{AlertId, AlertKind, AlertState, Snapshot, format_train_number};

use super::{Hit, Ov, Scene, SceneInput, console};
use crate::display::R;
use crate::font::Cut;
use crate::input::ViewState;

pub const TRAY_W: f32 = 170.0;
pub const ROW_H: f32 = 14.0;
pub const MAX_ROWS: usize = 8;
const MARGIN: f32 = 10.0;
const RECENT_ANSWERED_S: i64 = 3600;

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct TrayRow {
    pub id: AlertId,
    pub mark: char,
    pub number: String,
    pub age: String,
}

impl TrayRow {
    pub fn text(&self) -> String {
        format!("{} {} {}", self.mark, self.number, self.age)
    }
}

pub fn age(secs: i64) -> String {
    let s = secs.max(0);
    if s < 60 {
        format!("{s}s")
    } else if s < 3600 {
        format!("{}m", s / 60)
    } else {
        format!("{}h", s / 3600)
    }
}

fn shown(view: &ViewState) -> Option<&AlertId> {
    view.popup.as_ref().map(|p| &p.alert)
}

/// Open alerts waiting behind the popup, plus the ones dismissed to the tray.
pub fn queued_count(snap: &Snapshot, view: &ViewState) -> usize {
    snap.alerts
        .iter()
        .filter(|a| a.is_open() && Some(&a.id) != shown(view))
        .count()
}

/// Every tray entry, newest first (an answered alert dates from its answer).
pub fn rows(snap: &Snapshot, view: &ViewState) -> Vec<TrayRow> {
    let mut items: Vec<_> = snap
        .alerts
        .iter()
        .filter_map(|a| {
            let (at, mark) = match &a.state {
                AlertState::Open if Some(&a.id) == shown(view) => return None,
                AlertState::Open | AlertState::Tray => (
                    a.created,
                    match a.kind {
                        AlertKind::Question { .. } => '?',
                        AlertKind::Result { .. } => '✓',
                        AlertKind::Stalled | AlertKind::Failure { .. } => '!',
                        AlertKind::AssignProject { .. } => '+',
                    },
                ),
                AlertState::Answered { at, .. }
                    if (snap.now - *at).num_seconds() <= RECENT_ANSWERED_S =>
                {
                    (*at, '✓')
                }
                AlertState::Answered { .. } | AlertState::Expired => return None,
            };
            let number = a
                .train
                .and_then(|t| snap.trains.get(&t))
                .map_or_else(|| "----".into(), |t| format_train_number(t.number));
            Some((
                at,
                TrayRow {
                    id: a.id.clone(),
                    mark,
                    number,
                    age: age((snap.now - at).num_seconds()),
                },
            ))
        })
        .collect();
    items.sort_by_key(|(at, _)| std::cmp::Reverse(*at));
    items.into_iter().map(|(_, r)| r).collect()
}

pub fn draw(out: &mut Scene, inp: &SceneInput) {
    let all = rows(inp.snap, inp.view);
    if all.is_empty() {
        return;
    }
    let ov = Ov::new(inp);
    let p = &ov.p;
    let more = all.len().saturating_sub(MAX_ROWS);
    let n = all.len().min(MAX_ROWS) + usize::from(more > 0);
    let h = n as f32 * ROW_H + 6.0;
    let r = R {
        x: ov.w - MARGIN - TRAY_W,
        y: console::geom(inp).strip.y - MARGIN - h,
        w: TRAY_W,
        h,
    };
    ov.rect(&mut out.list, r, p.body);
    ov.outline(&mut out.list, r, 1.0, p.frame_dash);
    for (i, row) in all.iter().take(MAX_ROWS).enumerate() {
        let y = r.y + 3.0 + i as f32 * ROW_H;
        let mark_color = match row.mark {
            '?' => p.wait,
            '✓' => p.work,
            '!' => p.fail,
            _ => p.idle,
        };
        let ty = y + 3.5;
        ov.text(
            &mut out.list,
            &row.mark.to_string(),
            r.x + 8.0,
            ty,
            Cut::C5x7,
            mark_color,
        );
        ov.text(
            &mut out.list,
            &format!("{} {}", row.number, row.age),
            r.x + 20.0,
            ty,
            Cut::C5x7,
            p.label,
        );
        ov.hit(
            out,
            R {
                x: r.x,
                y,
                w: TRAY_W,
                h: ROW_H,
            },
            Hit::TrayItem(row.id.clone()),
        );
    }
    if more > 0 {
        let y = r.y + 3.0 + MAX_ROWS as f32 * ROW_H + 3.5;
        ov.text(
            &mut out.list,
            &format!("+{more} more"),
            r.x + 8.0,
            y,
            Cut::C5x7,
            p.glyph,
        );
    }
}
