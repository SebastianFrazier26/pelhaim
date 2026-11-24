use std::path::PathBuf;

use pelhaim_board::scene::{popup, tray};
use pelhaim_board::{
    A11yRole, AlertButton, Hit, InsetAction, InsetState, Layout, Mark, Mods, PaletteKind,
    PopupState, R, Scene, SceneInput, UiAction, UiInput, UiKey, ViewState, a11y_tree, board_layout,
    build_scene, fit, on_input, sync,
};
use pelhaim_core::testkit::{sample_board, secs};
use pelhaim_core::{
    AlertId, AlertState, Answer, AnsweredBy, Choice, LineKey, Origin, Snapshot, TagResolution,
    TrainId, TrainState,
};

const VP: (f32, f32) = (1600.0, 900.0);
const QUESTION: &str = "q:claude:S1:toolu_q";
const ASSIGN: &str = "assign:claude:U1a2b3c4d5";
const STALLED: &str = "stalled:claude:E1:0";

fn id(s: &str) -> AlertId {
    AlertId(s.into())
}

fn layout(snap: &Snapshot, view: &ViewState) -> Layout {
    board_layout(snap, &view.expanded)
}

fn scene(snap: &Snapshot, view: &ViewState) -> Scene {
    build_scene(&SceneInput {
        snap,
        view,
        viewport: VP,
        now_s: 0.0,
        palette: PaletteKind::Matte,
        reduced_motion: false,
    })
}

/// Sends one input against a freshly built scene, as the app does each frame.
fn send(view: &mut ViewState, snap: &Snapshot, ev: UiInput) -> Vec<UiAction> {
    let s = scene(snap, view);
    let l = layout(snap, view);
    on_input(view, &s, snap, &l, VP, 5.0, ev)
}

fn key(view: &mut ViewState, snap: &Snapshot, k: UiKey) -> Vec<UiAction> {
    send(view, snap, UiInput::Key(k, Mods::default()))
}

fn cmd(view: &mut ViewState, snap: &Snapshot, k: UiKey) -> Vec<UiAction> {
    send(
        view,
        snap,
        UiInput::Key(
            k,
            Mods {
                shift: false,
                cmd: true,
            },
        ),
    )
}

fn centre(r: R) -> (f32, f32) {
    (r.x + r.w / 2.0, r.y + r.h / 2.0)
}

fn hit_rect(s: &Scene, h: &Hit) -> R {
    s.hits
        .iter()
        .rev()
        .find(|(_, x)| x == h)
        .unwrap_or_else(|| panic!("no hit {h:?}"))
        .0
}

fn click_hit(view: &mut ViewState, snap: &Snapshot, h: Hit, double: bool) -> Vec<UiAction> {
    let s = scene(snap, view);
    let (x, y) = centre(hit_rect(&s, &h));
    assert_eq!(s.hit_at(x, y), Some(&h), "{h:?} is covered");
    send(view, snap, UiInput::Click { x, y, double })
}

fn popup_id(view: &ViewState) -> Option<&str> {
    view.popup.as_ref().map(|p| p.alert.0.as_str())
}

fn set_state(snap: &mut Snapshot, alert: &str, state: AlertState) {
    snap.alerts
        .iter_mut()
        .find(|a| a.id.0 == alert)
        .unwrap()
        .state = state;
}

fn answered() -> AlertState {
    AlertState::Answered {
        answer: "x".into(),
        by: AnsweredBy::Local,
        at: sample_board().now,
    }
}

#[test]
fn only_one_popup_and_no_focus_steal() {
    let mut snap = sample_board();
    let base = snap
        .alerts
        .iter()
        .find(|a| a.id.0 == ASSIGN)
        .unwrap()
        .clone();
    snap.alerts = (0..30)
        .map(|i| {
            let mut a = base.clone();
            a.id = id(&format!("assign:{i:02}"));
            a.created = base.created + secs(i);
            a
        })
        .collect();
    let mut view = ViewState {
        console_focused: true,
        ..ViewState::default()
    };
    sync(&mut view, &snap);
    assert_eq!(popup_id(&view), Some("assign:00"));
    assert!(view.console_focused);
    assert_eq!(view.shown.len(), 1);
    sync(&mut view, &snap);
    assert_eq!(popup_id(&view), Some("assign:00"));
    assert_eq!(tray::queued_count(&snap, &view), 29);

    assert!(send(&mut view, &snap, UiInput::Text("j".into())).is_empty());
    key(&mut view, &snap, UiKey::Char('j'));
    key(&mut view, &snap, UiKey::Char('1'));
    assert_eq!(view.console.text(), "j");
    assert_eq!(view.selected, None);
    assert_eq!(view.popup.as_ref().unwrap().highlighted, None);
    assert!(view.console_focused);

    // Dismissing one surfaces exactly the next, still without touching focus.
    view.console_focused = false;
    assert_eq!(
        key(&mut view, &snap, UiKey::Esc),
        [UiAction::Dismiss(id("assign:00"))]
    );
    set_state(&mut snap, "assign:00", AlertState::Tray);
    view.console_focused = true;
    sync(&mut view, &snap);
    assert_eq!(popup_id(&view), Some("assign:01"));
    assert!(view.console_focused);
}

#[test]
fn esc_on_popup_dismisses_to_tray() {
    let mut snap = sample_board();
    let mut view = ViewState::default();
    sync(&mut view, &snap);
    assert_eq!(popup_id(&view), Some(ASSIGN));
    assert_eq!(
        key(&mut view, &snap, UiKey::Esc),
        [UiAction::Dismiss(id(ASSIGN))]
    );
    assert_eq!(view.popup, None);

    set_state(&mut snap, ASSIGN, AlertState::Tray);
    sync(&mut view, &snap);
    assert_eq!(
        popup_id(&view),
        Some(QUESTION),
        "the dismissed one stays in the tray"
    );

    // ✕ does the same.
    let close = Hit::Alert {
        id: id(QUESTION),
        button: AlertButton::Close,
    };
    assert_eq!(
        click_hit(&mut view, &snap, close, false),
        [UiAction::Dismiss(id(QUESTION))]
    );
    assert_eq!(view.popup, None);

    // Clicking the tray entry reopens it.
    assert_eq!(
        click_hit(&mut view, &snap, Hit::TrayItem(id(ASSIGN)), false),
        [UiAction::Reopen(id(ASSIGN))]
    );
    assert_eq!(popup_id(&view), Some(ASSIGN));

    // With no popup, Esc closes the inset, then clears the selection.
    view.popup = None;
    view.selected = Some(TrainId(1));
    view.inset = Some(InsetState {
        train: TrainId(1),
        opened_s: 0.0,
    });
    assert!(key(&mut view, &snap, UiKey::Esc).is_empty());
    assert_eq!(
        (view.inset.is_none(), view.selected),
        (true, Some(TrainId(1)))
    );
    key(&mut view, &snap, UiKey::Esc);
    assert_eq!(view.selected, None);
}

#[test]
fn closing_an_answered_popup_sends_no_dismiss() {
    let mut snap = sample_board();
    set_state(&mut snap, QUESTION, answered());
    let mut view = ViewState {
        popup: Some(PopupState::new(id(QUESTION))),
        ..ViewState::default()
    };
    assert!(key(&mut view, &snap, UiKey::Esc).is_empty());
    assert_eq!(view.popup, None);
}

#[test]
fn digit_enter_answers_assign_locally() {
    let snap = sample_board();
    let mut view = ViewState::default();
    sync(&mut view, &snap);
    assert_eq!(popup_id(&view), Some(ASSIGN));
    key(&mut view, &snap, UiKey::Char('2'));
    key(&mut view, &snap, UiKey::Char('9'));
    assert_eq!(view.popup.as_ref().unwrap().highlighted, Some(1));
    assert_eq!(
        key(&mut view, &snap, UiKey::Enter),
        [UiAction::Answer(Answer {
            alert_id: id(ASSIGN),
            choice: Choice::Options(vec![1]),
            origin: Origin::Local,
        })]
    );

    // A click on an option answers in one go.
    let opt = Hit::Alert {
        id: id(ASSIGN),
        button: AlertButton::Option(5),
    };
    assert_eq!(
        click_hit(&mut view, &snap, opt, false),
        [UiAction::Answer(Answer {
            alert_id: id(ASSIGN),
            choice: Choice::Options(vec![5]),
            origin: Origin::Local,
        })]
    );
}

#[test]
fn digits_ignored_on_observed_question() {
    let snap = sample_board();
    let mut view = ViewState {
        popup: Some(PopupState::new(id(QUESTION))),
        ..ViewState::default()
    };
    key(&mut view, &snap, UiKey::Char('1'));
    assert_eq!(view.popup.as_ref().unwrap().highlighted, None);
    assert!(key(&mut view, &snap, UiKey::Enter).is_empty());
    assert_eq!(view.inset, None, "Enter on a popup never opens the inset");
}

#[test]
fn tab_cycles_open_alerts_oldest_first() {
    let mut snap = sample_board();
    set_state(&mut snap, STALLED, AlertState::Tray);
    let mut view = ViewState::default();
    let mut seen = Vec::new();
    let mut actions = Vec::new();
    for _ in 0..4 {
        actions.push(key(&mut view, &snap, UiKey::Tab));
        seen.push((
            view.selected.unwrap().0,
            popup_id(&view).unwrap().to_string(),
        ));
    }
    assert_eq!(
        seen,
        [
            (6, ASSIGN.to_string()),
            (7, QUESTION.to_string()),
            (4, STALLED.to_string()),
            (6, ASSIGN.to_string()),
        ]
    );
    assert_eq!(
        actions[2],
        [UiAction::Reopen(id(STALLED))],
        "a tray alert reopens"
    );
    let back = send(
        &mut view,
        &snap,
        UiInput::Key(
            UiKey::Tab,
            Mods {
                shift: true,
                cmd: false,
            },
        ),
    );
    assert_eq!(view.selected, Some(TrainId(4)));
    assert_eq!(back, [UiAction::Reopen(id(STALLED))]);
}

#[test]
fn jk_next_prev_in_layout_order() {
    let snap = sample_board();
    let mut view = ViewState::default();
    let mut order = Vec::new();
    for _ in 0..8 {
        key(&mut view, &snap, UiKey::Char('j'));
        order.push(view.selected.unwrap().0);
    }
    assert_eq!(order, [1, 7, 4, 2, 3, 5, 6, 1]);
    key(&mut view, &snap, UiKey::Char('k'));
    assert_eq!(view.selected, Some(TrainId(6)));
    view.selected = None;
    key(&mut view, &snap, UiKey::Char('K'));
    assert_eq!(view.selected, Some(TrainId(6)));
}

#[test]
fn click_yellow_tag_opens_alert_double_click_opens_inset() {
    let snap = sample_board();
    let mut view = ViewState::default();
    click_hit(&mut view, &snap, Hit::Train(TrainId(1)), false);
    assert_eq!(
        (view.selected, view.popup.is_none()),
        (Some(TrainId(1)), true)
    );

    click_hit(&mut view, &snap, Hit::Train(TrainId(7)), false);
    assert_eq!(view.selected, Some(TrainId(7)));
    assert_eq!(popup_id(&view), Some(QUESTION));

    view.popup = None;
    click_hit(&mut view, &snap, Hit::Train(TrainId(4)), false);
    assert_eq!(popup_id(&view), Some(STALLED), "stalled is yellow too");

    view.popup = None;
    click_hit(&mut view, &snap, Hit::Train(TrainId(1)), true);
    assert_eq!(
        view.inset,
        Some(InsetState {
            train: TrainId(1),
            opened_s: 5.0
        })
    );
    assert!(view.popup.is_none());

    // Enter with a selection and no popup opens the inset too.
    view.inset = None;
    view.selected = Some(TrainId(3));
    key(&mut view, &snap, UiKey::Enter);
    assert_eq!(view.inset.as_ref().map(|i| i.train), Some(TrainId(3)));
}

#[test]
fn popup_background_click_is_swallowed() {
    let snap = sample_board();
    let mut view = ViewState {
        popup: Some(PopupState::new(id(QUESTION))),
        ..ViewState::default()
    };
    view.selected = Some(TrainId(1));
    let s = scene(&snap, &view);
    let rect = popup::plan(
        &SceneInput {
            snap: &snap,
            view: &view,
            viewport: VP,
            now_s: 0.0,
            palette: PaletteKind::Matte,
            reduced_motion: false,
        },
        &layout(&snap, &view),
    )
    .unwrap()
    .rect;
    let k = 1.5;
    // The navy band, and the tab 4 units above the rect.
    for (ux, uy) in [(rect.x + 3.0, rect.y + 3.0), (rect.x + 14.0, rect.y - 2.0)] {
        let (x, y) = (ux * k, uy * k);
        assert_eq!(s.hit_at(x, y), Some(&Hit::Blocker));
        assert!(
            send(
                &mut view,
                &snap,
                UiInput::Click {
                    x,
                    y,
                    double: false
                }
            )
            .is_empty()
        );
    }
    assert_eq!(view.selected, Some(TrainId(1)));
    assert_eq!(popup_id(&view), Some(QUESTION));

    // A tag under the popup cannot be reached through it.
    let mut under = 0;
    for (r, h) in &s.hits {
        let Hit::Train(_) = h else { continue };
        let (x, y) = centre(*r);
        let ext = R {
            x: (rect.x - 7.0) * k,
            y: (rect.y - 9.0) * k,
            w: (rect.w + 7.0) * k,
            h: (rect.h + 9.0) * k,
        };
        if ext.contains(x, y) {
            under += 1;
            assert!(!matches!(s.hit_at(x, y), Some(Hit::Train(_))));
        }
    }
    assert!(under > 0, "the sample popup covers at least one tag");

    // Its buttons still work.
    let copy = Hit::Alert {
        id: id(QUESTION),
        button: AlertButton::CopySessionId,
    };
    assert_eq!(
        click_hit(&mut view, &snap, copy, false),
        [UiAction::CopyToClipboard("S1".into())]
    );
}

#[test]
fn inset_background_blocks_and_actions_work() {
    let mut snap = sample_board();
    snap.trains.get_mut(&TrainId(1)).unwrap().cwd = Some("/work/pelhaim".into());
    let mut view = ViewState {
        selected: Some(TrainId(2)),
        inset: Some(InsetState {
            train: TrainId(1),
            opened_s: 0.0,
        }),
        ..ViewState::default()
    };
    let s = scene(&snap, &view);
    let blocker = hit_rect(&s, &Hit::Blocker);
    let (x, y) = (blocker.x + 2.0, blocker.y + 2.0);
    assert_eq!(s.hit_at(x, y), Some(&Hit::Blocker));
    assert!(
        send(
            &mut view,
            &snap,
            UiInput::Click {
                x,
                y,
                double: false
            }
        )
        .is_empty()
    );
    assert_eq!(view.selected, Some(TrainId(2)));

    assert_eq!(
        click_hit(
            &mut view,
            &snap,
            Hit::InsetAction(InsetAction::OpenCwd),
            false
        ),
        [UiAction::Reveal(PathBuf::from("/work/pelhaim"))]
    );
    assert_eq!(
        click_hit(
            &mut view,
            &snap,
            Hit::InsetAction(InsetAction::CopySessionId),
            false
        ),
        [UiAction::CopyToClipboard("D1".into())]
    );
    click_hit(
        &mut view,
        &snap,
        Hit::InsetAction(InsetAction::Close),
        false,
    );
    assert_eq!(view.inset, None);

    snap.trains.get_mut(&TrainId(1)).unwrap().cwd = None;
    view.inset = Some(InsetState {
        train: TrainId(1),
        opened_s: 0.0,
    });
    assert!(
        click_hit(
            &mut view,
            &snap,
            Hit::InsetAction(InsetAction::OpenCwd),
            false
        )
        .is_empty()
    );
}

#[test]
fn log_decision_on_dispatch_asks_project_first() {
    let mut snap = sample_board();
    set_state(&mut snap, ASSIGN, answered());
    let mut view = ViewState {
        popup: Some(PopupState::new(id(ASSIGN))),
        ..ViewState::default()
    };
    let log = Hit::Alert {
        id: id(ASSIGN),
        button: AlertButton::LogDecision,
    };
    assert!(click_hit(&mut view, &snap, log.clone(), false).is_empty());
    assert!(view.popup.as_ref().unwrap().picking_project);
    let pick = Hit::Alert {
        id: id(ASSIGN),
        button: AlertButton::ProjectPick(1),
    };
    assert_eq!(
        click_hit(&mut view, &snap, pick, false),
        [UiAction::LogDecision {
            alert: id(ASSIGN),
            project: Some("ethica".into()),
        }]
    );
    assert!(!view.popup.as_ref().unwrap().picking_project);

    // By keyboard: a digit picks, Enter logs.
    click_hit(&mut view, &snap, log.clone(), false);
    key(&mut view, &snap, UiKey::Char('3'));
    assert_eq!(
        key(&mut view, &snap, UiKey::Enter),
        [UiAction::LogDecision {
            alert: id(ASSIGN),
            project: Some("cabn".into()),
        }]
    );

    // F39: the train's current line decides, not the alert's (still Unsorted) line.
    let track = snap.trains[&TrainId(6)].track;
    snap.tracks.get_mut(&track).unwrap().line = LineKey::Project("cabn".into());
    view.popup = Some(PopupState::new(id(ASSIGN)));
    assert_eq!(
        click_hit(&mut view, &snap, log, false),
        [UiAction::LogDecision {
            alert: id(ASSIGN),
            project: None,
        }]
    );
    assert!(!view.popup.as_ref().unwrap().picking_project);
}

#[test]
fn space_freezes_board_snapshot_only() {
    let snap = sample_board();
    let mut view = ViewState::default();
    key(&mut view, &snap, UiKey::Space);
    assert!(view.paused);
    assert_eq!(view.frozen.as_deref(), Some(&snap));

    let mut live = snap.clone();
    live.version += 1;
    live.trains.get_mut(&TrainId(1)).unwrap().state = TrainState::Done;
    sync(&mut view, &live);
    assert_eq!(
        view.frozen.as_deref(),
        Some(&snap),
        "the board stays frozen"
    );
    assert_eq!(
        popup_id(&view),
        Some(ASSIGN),
        "popups follow the live snapshot"
    );

    key(&mut view, &live, UiKey::Space);
    assert!(!view.paused);
    assert_eq!(view.frozen, None);
}

#[test]
fn zero_fits_and_cmd_plus_zooms_clamped() {
    let snap = sample_board();
    let mut view = ViewState {
        k: 3.0,
        ox: -500.0,
        oy: 40.0,
        ..ViewState::default()
    };
    key(&mut view, &snap, UiKey::Char('0'));
    let f = fit(layout(&snap, &view).content, VP);
    assert_eq!((view.k, view.ox, view.oy), (f.k, f.ox, f.oy));

    let c = (VP.0 / 2.0, VP.1 / 2.0);
    let before = view.view().inv(c.0, c.1);
    let k0 = view.k;
    cmd(&mut view, &snap, UiKey::Char('='));
    assert!((view.k - k0 * 1.25).abs() < 1e-4);
    let after = view.view().inv(c.0, c.1);
    assert!((before.0 - after.0).abs() < 1e-3 && (before.1 - after.1).abs() < 1e-3);
    for _ in 0..40 {
        cmd(&mut view, &snap, UiKey::Char('='));
    }
    assert_eq!(view.k, 8.0);
    for _ in 0..40 {
        cmd(&mut view, &snap, UiKey::Char('-'));
    }
    assert_eq!(view.k, 0.25);

    // F fits the selected train's track; with no selection it does nothing.
    let unchanged = view.clone();
    key(&mut view, &snap, UiKey::Char('f'));
    assert_eq!(view, unchanged);
    view.selected = Some(TrainId(7));
    key(&mut view, &snap, UiKey::Char('F'));
    let l = layout(&snap, &view);
    let g = &l.tracks[&snap.trains[&TrainId(7)].track];
    let y = view.view().y(g.y);
    assert!(view.k > 0.25 && y > 0.0 && y < VP.1);

    // Pointer zoom keeps the point under the cursor.
    let at = (300.0, 200.0);
    let before = view.view().inv(at.0, at.1);
    send(&mut view, &snap, UiInput::Zoom { factor: 0.5, at });
    let after = view.view().inv(at.0, at.1);
    assert!((before.0 - after.0).abs() < 1e-3 && (before.1 - after.1).abs() < 1e-3);
    let (ox, oy) = (view.ox, view.oy);
    send(&mut view, &snap, UiInput::Drag { dx: 10.0, dy: -4.0 });
    send(&mut view, &snap, UiInput::Scroll { dx: 1.0, dy: 2.0 });
    assert_eq!((view.ox, view.oy), (ox + 11.0, oy - 2.0));
}

#[test]
fn console_commands_reply_in_the_radio_log() {
    let snap = sample_board();
    let mut view = ViewState::default();
    key(&mut view, &snap, UiKey::Char('/'));
    assert!(view.console_focused);
    assert_eq!(view.console.text(), "/");
    send(&mut view, &snap, UiInput::Text("mode".into()));
    assert!(key(&mut view, &snap, UiKey::Enter).is_empty());
    assert_eq!(view.console.text(), "");
    assert!(view.radio_log_open);
    let log: Vec<_> = view
        .radio_log
        .iter()
        .map(|c| (c.mark, c.text.as_str()))
        .collect();
    assert_eq!(
        log,
        [
            (Mark::Done, "observe"),
            (Mark::Pending, "launch (M2)"),
            (Mark::Pending, "direct (M3)"),
        ]
    );

    let run = |view: &mut ViewState, s: &str| {
        send(view, &snap, UiInput::Text(s.into()));
        key(view, &snap, UiKey::Enter)
    };
    run(&mut view, "@1148 hi");
    run(&mut view, "/bogus");
    let tail: Vec<_> = view.radio_log[3..]
        .iter()
        .map(|c| (c.mark, c.text.clone()))
        .collect();
    assert_eq!(
        tail,
        [
            (
                Mark::Needs,
                "observed sessions are answered in their own terminal".to_string()
            ),
            (Mark::Failed, "unknown command: /bogus".to_string()),
        ]
    );
    assert_eq!(
        run(&mut view, "/decisions export"),
        [UiAction::ExportPending]
    );

    view.oy = 123.0;
    view.k = 2.0;
    run(&mut view, "/goto ethica");
    let l = layout(&snap, &view);
    let top = l
        .lines
        .iter()
        .find(|b| b.key == LineKey::Project("ethica".into()))
        .unwrap()
        .top;
    assert_eq!(view.view().y(top), 0.0);

    // Editing keys, then Esc unfocuses; cmd+Up toggles the radio log.
    send(&mut view, &snap, UiInput::Text("ab".into()));
    key(&mut view, &snap, UiKey::Left);
    key(&mut view, &snap, UiKey::Backspace);
    assert_eq!(view.console.text(), "b");
    send(&mut view, &snap, UiInput::Preedit(Some("に".into())));
    assert_eq!(view.console.preedit.as_deref(), Some("に"));
    cmd(&mut view, &snap, UiKey::Up);
    assert!(!view.radio_log_open);
    key(&mut view, &snap, UiKey::Esc);
    assert!(!view.console_focused);
    send(&mut view, &snap, UiInput::Text("zz".into()));
    assert_eq!(
        view.console.text(),
        "b",
        "text never lands in an unfocused console"
    );

    let s = scene(&snap, &view);
    let (x, y) = centre(hit_rect(&s, &Hit::Console));
    send(
        &mut view,
        &snap,
        UiInput::Click {
            x,
            y,
            double: false,
        },
    );
    assert!(view.console_focused);
}

#[test]
fn decisions_command_lists_up_to_ten() {
    let mut snap = sample_board();
    let mut view = ViewState {
        console_focused: true,
        ..ViewState::default()
    };
    let d = pelhaim_core::Decision {
        id: pelhaim_core::DecisionId(1),
        project: "ethica".into(),
        date: sample_board().now.date_naive(),
        title: "Ship v0".into(),
        why: "ok".into(),
        who: pelhaim_core::Who::Owner,
        origin: Origin::Local,
        source_alert: None,
        written_to_md: true,
    };
    snap.decisions_recent = (0..12)
        .map(|i| pelhaim_core::Decision {
            written_to_md: i != 0,
            ..d.clone()
        })
        .collect();
    send(&mut view, &snap, UiInput::Text("/decisions".into()));
    key(&mut view, &snap, UiKey::Enter);
    assert_eq!(view.radio_log.len(), 10);
    assert_eq!(view.radio_log[0].mark, Mark::Pending);
    assert_eq!(view.radio_log[1].mark, Mark::Done);
    assert!(view.radio_log[0].text.contains("ethica: Ship v0"));
}

#[test]
fn lamp_pauses_keep_awake_and_stub_toggles_expanded() {
    let mut snap = sample_board();
    let mut view = ViewState::default();
    assert_eq!(
        click_hit(&mut view, &snap, Hit::Lamp, false),
        [UiAction::PauseKeepAwake]
    );

    let ethica = snap
        .lines
        .iter_mut()
        .find(|l| l.key == LineKey::Project("ethica".into()))
        .unwrap();
    ethica.tracks.extend((100..104).map(pelhaim_core::TrackId));
    let key = LineKey::Project("ethica".into());
    click_hit(&mut view, &snap, Hit::Stub(key.clone()), false);
    assert!(view.expanded.contains(&key));
    assert!(
        !scene(&snap, &view)
            .hits
            .iter()
            .any(|(_, h)| h == &Hit::Stub(key.clone()))
    );
}

#[test]
fn move_sets_hover_and_redraws_on_change() {
    let snap = sample_board();
    let mut view = ViewState::default();
    let s = scene(&snap, &view);
    let (x, y) = centre(hit_rect(&s, &Hit::Lamp));
    assert_eq!(
        send(&mut view, &snap, UiInput::Move { x, y }),
        [UiAction::Redraw]
    );
    assert_eq!(view.hover, Some(Hit::Lamp));
    assert!(send(&mut view, &snap, UiInput::Move { x, y }).is_empty());
    send(&mut view, &snap, UiInput::Move { x: -5.0, y: -5.0 });
    assert_eq!(view.hover, None);
}

#[test]
fn sync_closes_popups_for_gone_alerts() {
    let mut snap = sample_board();
    let mut view = ViewState::default();
    sync(&mut view, &snap);
    assert_eq!(popup_id(&view), Some(ASSIGN));
    snap.alerts.retain(|a| a.id.0 != ASSIGN);
    sync(&mut view, &snap);
    assert_eq!(popup_id(&view), Some(QUESTION));
    assert!(!view.shown.contains(&id(ASSIGN)));
}

#[test]
fn a11y_train_name_format() {
    let mut snap = sample_board();
    let view = ViewState {
        popup: Some(PopupState {
            highlighted: Some(0),
            ..PopupState::new(id(QUESTION))
        }),
        ..ViewState::default()
    };
    let tree = a11y_tree(&snap, &view);
    let by_id = |i: &str| tree.iter().find(|n| n.id == i).unwrap();
    assert_eq!(tree[0].role, A11yRole::Window);
    let t7 = by_id("train:7");
    assert_eq!(t7.role, A11yRole::Train);
    assert_eq!(
        t7.name,
        "Train 1423, SAFEASCENT, waiting on you: Proceed to C1?"
    );
    assert_eq!(
        by_id("train:5").name,
        "Train 1400, LOOSY-GOOSE, failed: aborted"
    );

    let groups: Vec<_> = tree[0]
        .children
        .iter()
        .map(|c| by_id(c))
        .filter(|n| n.role == A11yRole::Group)
        .collect();
    assert_eq!(groups.len(), snap.lines.len());
    let ethica = groups.iter().find(|g| g.name == "ETHICA").unwrap();
    assert_eq!(ethica.children, ["train:4", "train:2"]);

    let dialog = tree.iter().find(|n| n.role == A11yRole::Dialog).unwrap();
    assert!(dialog.name.contains("Proceed to C1?"), "{}", dialog.name);
    let opts: Vec<_> = dialog
        .children
        .iter()
        .map(|c| by_id(c))
        .filter(|n| n.role == A11yRole::Checkbox)
        .map(|n| n.name.as_str())
        .collect();
    assert_eq!(opts, ["1 Proceed to C1 rehearsal, selected", "2 Hold at B"]);
    assert!(tree[0].children.contains(&dialog.id));
    let console = by_id(tree[0].children.last().unwrap());
    assert_eq!(console.role, A11yRole::TextInput);

    // F31: a mis-tag names the slug it asked for.
    snap.trains.get_mut(&TrainId(6)).unwrap().tag = TagResolution::MisTagged("ethca".into());
    let tree = a11y_tree(&snap, &ViewState::default());
    let t6 = tree.iter().find(|n| n.id == "train:6").unwrap();
    assert_eq!(
        t6.name,
        "Train 1410, UNSORTED, idle, tag: ethca (unknown project)"
    );
    assert!(tree.iter().all(|n| n.role != A11yRole::Dialog));
}
