use pelhaim_board::scene::{console, inset, lamp, popup, tray, wrap};
use pelhaim_board::{
    AlertButton, CardRow, Hit, InsetAction, InsetState, Layout, Mark, PaletteKind, PopupState,
    Prim, Scene, SceneInput, ViewState, board_layout, build_scene, palette, tag_rect,
};
use pelhaim_core::testkit::{sample_board, secs, ts};
use pelhaim_core::{
    Alert, AlertId, AlertKind, AlertState, AnsweredBy, LineKey, QOption, Question, Snapshot,
    TagResolution, TrainId,
};

const VP: (f32, f32) = (1600.0, 900.0);
const OV: (f32, f32) = (VP.0 / 1.5, VP.1 / 1.5);

const QUESTION: &str = "q:claude:S1:toolu_q";
const ASSIGN: &str = "assign:claude:U1a2b3c4d5";
const S1_TRAIN: TrainId = TrainId(7);

fn input<'a>(snap: &'a Snapshot, view: &'a ViewState) -> SceneInput<'a> {
    SceneInput {
        snap,
        view,
        viewport: VP,
        now_s: 0.0,
        palette: PaletteKind::Matte,
        reduced_motion: false,
    }
}

fn layout(snap: &Snapshot, view: &ViewState) -> Layout {
    board_layout(snap, &view.expanded)
}

fn with_popup(id: &str) -> ViewState {
    ViewState {
        popup: Some(PopupState::new(AlertId(id.into()))),
        ..ViewState::default()
    }
}

fn plan(snap: &Snapshot, view: &ViewState) -> popup::PopupPlan {
    popup::plan(&input(snap, view), &layout(snap, view)).expect("popup plan")
}

fn popup_scene(snap: &Snapshot, view: &ViewState) -> Scene {
    let mut s = Scene::default();
    popup::draw(&mut s, &input(snap, view), &layout(snap, view));
    s
}

fn alert_buttons(s: &Scene) -> Vec<AlertButton> {
    s.hits
        .iter()
        .filter_map(|(_, h)| match h {
            Hit::Alert { button, .. } => Some(button.clone()),
            _ => None,
        })
        .collect()
}

fn texts(plan: &popup::PopupPlan) -> Vec<String> {
    plan.rows.iter().map(popup::Row::text).collect()
}

fn alert(id: &str, kind: AlertKind, created: &str, state: AlertState) -> Alert {
    Alert {
        id: AlertId(id.into()),
        train: Some(S1_TRAIN),
        line: LineKey::Project("safeascent".into()),
        kind,
        created: ts(created),
        state,
        answerable: false,
        decision: None,
    }
}

fn question(text: &str) -> AlertKind {
    AlertKind::Question {
        questions: vec![Question {
            header: "H".into(),
            question: text.into(),
            multi_select: false,
            options: vec![
                QOption {
                    label: "Yes".into(),
                    description: "go".into(),
                },
                QOption {
                    label: "No".into(),
                    description: String::new(),
                },
            ],
        }],
    }
}

/// The popup's tag rect in overlay units, for placement checks.
fn tag_overlay(snap: &Snapshot, view: &ViewState) -> pelhaim_board::R {
    let r = view
        .view()
        .rect(tag_rect(&layout(snap, view), &snap.trains[&S1_TRAIN]));
    pelhaim_board::R {
        x: r.x / 1.5,
        y: r.y / 1.5,
        w: r.w / 1.5,
        h: r.h / 1.5,
    }
}

#[test]
fn observed_question_popup_snapshot() {
    let snap = sample_board();
    let view = with_popup(QUESTION);
    let p = plan(&snap, &view);
    assert_eq!(p.tab, "Question");
    assert_eq!(
        texts(&p),
        [
            "1423 · SAFEASCENT / Stage C1 rehearsal",
            "STATUS: WAITING ON YOU",
            "Proceed to C1?",
            "[1] ☐ Proceed to C1 rehearsal — Proceed to C1 r…",
            "[2] ☐ Hold at B — Hold at B description",
            "Answer in terminal  [Copy session id]",
        ]
    );
    insta::assert_ron_snapshot!(popup_scene(&snap, &view).list);
}

#[test]
fn observed_question_has_no_option_hits() {
    let snap = sample_board();
    let view = with_popup(QUESTION);
    let s = popup_scene(&snap, &view);
    assert_eq!(
        alert_buttons(&s),
        [AlertButton::Close, AlertButton::CopySessionId]
    );
    let p = plan(&snap, &view);
    let dim = palette(PaletteKind::Matte).done;
    for r in p.rows.iter().filter(|r| r.text().starts_with('[')) {
        assert!(r.hit.is_none());
        assert_eq!(r.spans[0].color, dim, "{}", r.text());
    }
}

#[test]
fn assign_popup_has_option_hits_and_prefill_highlight() {
    let mut snap = sample_board();
    let a = snap.alerts.iter_mut().find(|a| a.id.0 == ASSIGN).unwrap();
    if let AlertKind::AssignProject { prefill, .. } = &mut a.kind {
        *prefill = Some(2);
    }
    let view = with_popup(ASSIGN);
    let p = plan(&snap, &view);
    assert_eq!(p.tab, "Assign");
    let opts: Vec<_> = p.rows.iter().filter(|r| r.hit.is_some()).collect();
    assert_eq!(opts.len(), 6);
    assert_eq!(opts[2].text(), "[3] ☐ CABN");
    let lit: Vec<_> = p
        .rows
        .iter()
        .filter(|r| r.highlight)
        .map(|r| r.text())
        .collect();
    assert_eq!(lit, ["[3] ☐ CABN"]);

    let s = popup_scene(&snap, &view);
    let mut b = alert_buttons(&s);
    b.sort_by_key(|b| format!("{b:?}"));
    assert_eq!(
        b,
        [
            AlertButton::Close,
            AlertButton::Option(0),
            AlertButton::Option(1),
            AlertButton::Option(2),
            AlertButton::Option(3),
            AlertButton::Option(4),
            AlertButton::Option(5),
        ]
    );

    let mut view = view;
    view.popup.as_mut().unwrap().highlighted = Some(0);
    let p = plan(&snap, &view);
    let lit: Vec<_> = p
        .rows
        .iter()
        .filter(|r| r.highlight)
        .map(|r| r.text())
        .collect();
    assert_eq!(lit, ["[1] ☐ SAFEASCENT"]);
}

#[test]
fn project_picker_lists_configured_projects() {
    let snap = sample_board();
    let mut view = with_popup(QUESTION);
    view.popup.as_mut().unwrap().picking_project = true;
    let s = popup_scene(&snap, &view);
    let picks: Vec<_> = alert_buttons(&s)
        .into_iter()
        .filter(|b| matches!(b, AlertButton::ProjectPick(_)))
        .collect();
    assert_eq!(picks.len(), 5);
    assert_eq!(popup::pick_projects(&snap)[1].0, "ethica");
    assert!(texts(&plan(&snap, &view)).contains(&"[2] ☐ ETHICA".to_string()));
}

#[test]
fn popup_sits_below_the_tag_when_it_fits() {
    let snap = sample_board();
    let view = with_popup(QUESTION);
    let p = plan(&snap, &view);
    let tag = tag_overlay(&snap, &view);
    assert!(p.rect.y >= tag.y + tag.h, "{:?} vs {tag:?}", p.rect);
}

#[test]
fn popup_flips_above_when_no_room_below() {
    let snap = sample_board();
    let mut view = with_popup(QUESTION);
    let ty = tag_rect(&layout(&snap, &view), &snap.trains[&S1_TRAIN]).y;
    view.oy = 820.0 - ty;
    let p = plan(&snap, &view);
    let tag = tag_overlay(&snap, &view);
    assert!(p.rect.y + p.rect.h <= tag.y, "{:?} vs {tag:?}", p.rect);
    assert!(p.rect.y >= 10.0);
}

#[test]
fn popup_clamped_on_screen_for_offscreen_tag() {
    let snap = sample_board();
    for (ox, oy) in [(-10_000.0, 5_000.0), (10_000.0, -5_000.0)] {
        let mut view = with_popup(QUESTION);
        view.ox = ox;
        view.oy = oy;
        let p = plan(&snap, &view);
        let r = p.rect;
        assert!(r.x >= 10.0 && r.x + r.w <= OV.0 - 10.0, "{r:?}");
        assert!(r.y >= 10.0 && r.y + r.h <= OV.1 - 10.0, "{r:?}");
        let t = p.tag.expect("connector end");
        assert!((0.0..=OV.0).contains(&t.x) && (0.0..=OV.1).contains(&t.y));
        let s = popup_scene(&snap, &view);
        let seg = s
            .list
            .prims
            .iter()
            .find_map(|p| match p {
                Prim::Seg { a, b, .. } => Some((*a, *b)),
                _ => None,
            })
            .expect("connector");
        for pt in [seg.0, seg.1] {
            assert!((0.0..=VP.0).contains(&pt.x) && (0.0..=VP.1).contains(&pt.y));
        }
    }
}

#[test]
fn long_question_wraps_and_is_capped() {
    let mut snap = sample_board();
    let long = format!(
        "{} {} 長い質問がここに続きます長い質問がここに続きます長い質問がここに続きます長い質問がここに続きます",
        "word ".repeat(400),
        "x".repeat(120)
    );
    snap.alerts.push(alert(
        "q:long",
        question(&long),
        "2026-11-23T14:29:00Z",
        AlertState::Open,
    ));
    let view = with_popup("q:long");
    let p = plan(&snap, &view);
    let wc = popup::body_chars();
    for r in &p.rows {
        assert!(r.text().chars().count() <= wc, "{:?}", r.text());
    }
    assert!(p.rect.h <= OV.1 * 0.6, "{}", p.rect.h);
    let t = texts(&p);
    let text_rows: Vec<_> = p
        .rows
        .iter()
        .filter(|r| r.flow == popup::Flow::Text)
        .collect();
    assert!(text_rows.len() > 1);
    assert!(text_rows.last().unwrap().text().ends_with(popup::MORE));
    assert!(t.iter().any(|l| l.starts_with("[1] ☐ Yes — go")));
    assert!(t.iter().any(|l| l.starts_with("[2] ☐ No")));
}

#[test]
fn wrap_breaks_words_longer_than_the_line() {
    assert_eq!(wrap("ab cd ef", 5), ["ab cd", "ef"]);
    assert_eq!(wrap("abcdefghij k", 4), ["abcd", "efgh", "ij k"]);
    assert_eq!(wrap("長い質問です", 4), ["長い質問", "です"]);
    assert_eq!(wrap("one\n\ntwo\x1b[1m!", 20), ["one", "two !"]);
}

#[test]
fn tray_lists_newest_first_with_age() {
    let mut snap = sample_board();
    snap.alerts.clear();
    let answered = |at: &str| AlertState::Answered {
        answer: "Yes".into(),
        by: AnsweredBy::Terminal,
        at: ts(at),
    };
    snap.alerts = vec![
        alert(
            "shown",
            question("q"),
            "2026-11-23T14:29:59Z",
            AlertState::Open,
        ),
        alert(
            "old",
            question("q"),
            "2026-11-23T12:00:00Z",
            AlertState::Tray,
        ),
        alert(
            "recent",
            question("q"),
            "2026-11-23T14:29:15Z",
            AlertState::Open,
        ),
        alert(
            "ans",
            question("q"),
            "2026-11-23T13:00:00Z",
            answered("2026-11-23T14:20:00Z"),
        ),
        alert(
            "stale",
            question("q"),
            "2026-11-23T12:00:00Z",
            answered("2026-11-23T13:20:00Z"),
        ),
        alert(
            "gone",
            AlertKind::Stalled,
            "2026-11-23T14:00:00Z",
            AlertState::Expired,
        ),
        alert(
            "fail",
            AlertKind::Failure { reason: "x".into() },
            "2026-11-23T14:25:00Z",
            AlertState::Tray,
        ),
    ];
    let view = with_popup("shown");
    let rows = tray::rows(&snap, &view);
    let t: Vec<_> = rows.iter().map(tray::TrayRow::text).collect();
    assert_eq!(t, ["? 1423 45s", "! 1423 5m", "✓ 1423 10m", "? 1423 2h"]);
    assert_eq!(tray::queued_count(&snap, &view), 3);

    let mut s = Scene::default();
    tray::draw(&mut s, &input(&snap, &view));
    let ids: Vec<_> = s
        .hits
        .iter()
        .filter_map(|(_, h)| match h {
            Hit::TrayItem(id) => Some(id.0.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(ids, ["recent", "fail", "ans", "old"]);

    for i in 0..10 {
        snap.alerts.push(alert(
            &format!("extra{i}"),
            question("q"),
            "2026-11-23T14:00:00Z",
            AlertState::Tray,
        ));
    }
    let mut s = Scene::default();
    tray::draw(&mut s, &input(&snap, &view));
    assert_eq!(
        s.hits
            .iter()
            .filter(|(_, h)| matches!(h, Hit::TrayItem(_)))
            .count(),
        tray::MAX_ROWS
    );
    assert_eq!(tray::rows(&snap, &view).len(), 14);
}

#[test]
fn tray_age_units() {
    assert_eq!(tray::age(-3), "0s");
    assert_eq!(tray::age(59), "59s");
    assert_eq!(tray::age(60), "1m");
    assert_eq!(tray::age(3599), "59m");
    assert_eq!(tray::age(7300), "2h");
}

fn console_scene(snap: &Snapshot, view: &ViewState) -> Scene {
    let mut s = Scene::default();
    let inp = input(snap, view);
    console::draw(&mut s, &inp);
    lamp::draw_tooltip(&mut s, &inp);
    s
}

#[test]
fn console_and_lamp_snapshot() {
    let mut snap = sample_board();
    let now = snap.now;
    let ka = &mut snap.keep_awake;
    assert_eq!(lamp::label(ka, now), "AWAKE 2");
    assert_eq!(lamp::tooltip(ka), ["held by 2 working trains"]);
    ka.held = false;
    ka.count = 0;
    ka.on_battery = Some(true);
    assert_eq!(
        lamp::tooltip(ka),
        ["not held", "on battery: lid-close will sleep"]
    );
    ka.paused_until = Some(now + secs(600));
    assert_eq!(lamp::label(ka, now), "AWAKE paused");
    ka.paused_until = Some(now - secs(1));
    assert_eq!(lamp::label(ka, now), "AWAKE 0");
    ka.mechanism = None;
    assert_eq!(lamp::label(ka, now), "AWAKE n/a");

    let p = palette(PaletteKind::Matte);
    let lamp_color = |s: &Scene| {
        s.list
            .prims
            .iter()
            .find_map(|pr| match pr {
                Prim::Rect { r, color, .. } if r.w == 9.0 && r.h == 9.0 => Some(*color),
                _ => None,
            })
            .expect("lamp square")
    };
    let view = ViewState {
        hover: Some(Hit::Lamp),
        ..ViewState::default()
    };
    let unheld = console_scene(&snap, &view);
    assert_eq!(lamp_color(&unheld), p.glyph.alpha(0.35));
    let held = console_scene(&sample_board(), &view);
    assert_eq!(lamp_color(&held), p.glyph);
    for s in [&held, &unheld] {
        assert!(s.hits.iter().any(|(_, h)| *h == Hit::Lamp));
        assert!(s.hits.iter().any(|(_, h)| *h == Hit::Console));
    }
    let no_hover = console_scene(&snap, &ViewState::default());
    assert!(no_hover.list.prims.len() < unheld.list.prims.len());

    let mut snap = sample_board();
    snap.keep_awake.on_battery = Some(true);
    snap.decisions_unexported = 3;
    let mut view = view;
    view.console.insert("/goto eth");
    view.console.preedit = Some("ica".into());
    view.console_focused = true;
    let items: Vec<_> = console::status_items(&snap, &view)
        .into_iter()
        .map(|(s, _)| s)
        .collect();
    assert_eq!(
        items,
        ["ALERTS 3", "UNEXPORTED 3", "[obs]", "[launch]", "[direct]"]
    );
    insta::assert_ron_snapshot!(console_scene(&snap, &view).list);
}

#[test]
fn console_grows_to_six_rows_and_shows_selection_status() {
    let snap = sample_board();
    let mut view = ViewState {
        selected: Some(S1_TRAIN),
        ..ViewState::default()
    };
    let g1 = console::geom(&input(&snap, &view));
    assert_eq!(g1.rows, 1);
    assert_eq!(
        console::selection_status(&snap, &view).unwrap(),
        "1423 · SAFEASCENT · waiting on you: Proceed to C1? · Stage C1 rehearsal"
    );
    view.console.insert(&"x".repeat(5000));
    let g6 = console::geom(&input(&snap, &view));
    assert_eq!(g6.rows, 6);
    assert!(g6.strip.h > g1.strip.h && g6.strip.y + g6.strip.h == g1.strip.y + g1.strip.h);
}

#[test]
fn radio_log_cards_draw_above_the_strip() {
    let snap = sample_board();
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
        ..ViewState::default()
    };
    let closed = console_scene(&snap, &view).list.prims.len();
    view.radio_log_open = true;
    let s = console_scene(&snap, &view);
    assert!(s.list.prims.len() > closed);
    let strip_top = console::geom(&input(&snap, &view)).strip.y * 1.5;
    let needs = palette(PaletteKind::Matte).wait;
    assert!(s.list.prims.iter().any(|p| matches!(
        p,
        Prim::Rect { r, color, .. } if *color == needs && r.y < strip_top
    )));
}

fn inset_view() -> ViewState {
    ViewState {
        inset: Some(InsetState {
            train: S1_TRAIN,
            opened_s: 0.0,
        }),
        ..ViewState::default()
    }
}

#[test]
fn inset_snapshot_contains_clip_and_state_word() {
    let mut snap = sample_board();
    let t = snap.trains.get_mut(&S1_TRAIN).unwrap();
    t.cwd = Some("/Users/me/Developer/safeascent".into());
    t.tail = (0..12).map(|i| format!("tail line {i}")).collect();
    let view = inset_view();
    let inp = SceneInput {
        now_s: 1.0,
        ..input(&snap, &view)
    };
    let l = layout(&snap, &view);
    let mut s = Scene::default();
    inset::draw(&mut s, &inp, &l);

    let clip = s
        .list
        .prims
        .iter()
        .position(|p| matches!(p, Prim::Clip(_)))
        .expect("Clip");
    let unclip = s
        .list
        .prims
        .iter()
        .position(|p| matches!(p, Prim::Unclip))
        .expect("Unclip");
    assert!(clip + 100 < unclip, "the board is drawn inside the clip");

    let lines: Vec<String> =
        inset::info_lines(&snap, &snap.trains[&S1_TRAIN], &palette(PaletteKind::Matte))
            .into_iter()
            .map(|(s, _)| s)
            .collect();
    assert!(lines.contains(&"waiting on you: Proceed to C1?".to_string()));
    assert!(lines.contains(&"tail line 4".to_string()));
    assert!(!lines.contains(&"tail line 3".to_string()));
    assert!(lines.contains(&"▸ Stage C1 rehearsal".to_string()));

    let Prim::Clip(cr) = s.list.prims[clip] else {
        unreachable!()
    };
    for (r, _) in s.hits.iter().filter(|(_, h)| matches!(h, Hit::Train(_))) {
        assert!(
            r.x >= cr.x && r.x + r.w <= cr.x + cr.w + 0.01,
            "{r:?} outside {cr:?}"
        );
    }
    let actions: Vec<_> = s
        .hits
        .iter()
        .filter_map(|(_, h)| match h {
            Hit::InsetAction(a) => Some(*a),
            _ => None,
        })
        .collect();
    assert_eq!(
        actions,
        [
            InsetAction::Close,
            InsetAction::OpenCwd,
            InsetAction::CopySessionId
        ]
    );
    let hits: Vec<_> = s
        .hits
        .iter()
        .filter(|(_, h)| !matches!(h, Hit::Train(_)))
        .map(|(r, h)| (*r, format!("{h:?}")))
        .collect();
    insta::assert_ron_snapshot!((cr, lines, hits));
}

#[test]
fn inset_shows_mistagged_slug() {
    let mut snap = sample_board();
    snap.trains.get_mut(&S1_TRAIN).unwrap().tag = TagResolution::MisTagged("safeacsent".into());
    let lines = inset::info_lines(&snap, &snap.trains[&S1_TRAIN], &palette(PaletteKind::Matte));
    let p = palette(PaletteKind::Matte);
    assert!(lines.contains(&("tag: safeacsent (unknown project)".to_string(), p.fail)));
}

#[test]
fn inset_zoom_tweens_unless_reduced_motion() {
    assert_eq!(inset::inset_k(2.0, 10.0, 10.0, false), 2.0);
    assert!((inset::inset_k(2.0, 10.0, 10.075, false) - 3.4).abs() < 1e-4);
    assert!((inset::inset_k(2.0, 10.0, 11.0, false) - 4.8).abs() < 1e-4);
    assert!((inset::inset_k(2.0, 10.0, 10.0, true) - 4.8).abs() < 1e-4);
}

#[test]
fn result_alert_shows_cards_without_log_decision() {
    let mut snap = sample_board();
    snap.alerts.push(Alert {
        answerable: false,
        ..alert(
            "r:1",
            AlertKind::Result {
                summary: "- [x] judge run\n- [ ] corpus\nFailed: lint".into(),
            },
            "2026-11-23T14:29:00Z",
            AlertState::Answered {
                answer: "done".into(),
                by: AnsweredBy::Terminal,
                at: ts("2026-11-23T14:29:30Z"),
            },
        )
    });
    let view = with_popup("r:1");
    let p = plan(&snap, &view);
    let t = texts(&p);
    assert!(t.contains(&"☑ judge run".to_string()));
    assert!(t.contains(&"☐ corpus".to_string()));
    assert!(t.contains(&"✕ Failed: lint".to_string()));
    let pal = palette(PaletteKind::Matte);
    let failed = p
        .rows
        .iter()
        .find(|r| r.text() == "✕ Failed: lint")
        .unwrap();
    assert_eq!(failed.spans[0].color, pal.fail);
    assert!(!alert_buttons(&popup_scene(&snap, &view)).contains(&AlertButton::LogDecision));
}

#[test]
fn answered_question_offers_log_decision_and_stalled_reads_unknown() {
    let mut snap = sample_board();
    let q = snap.alerts.iter_mut().find(|a| a.id.0 == QUESTION).unwrap();
    q.state = AlertState::Answered {
        answer: "Hold at B".into(),
        by: AnsweredBy::Terminal,
        at: ts("2026-11-23T14:29:00Z"),
    };
    let view = with_popup(QUESTION);
    let t = texts(&plan(&snap, &view));
    assert_eq!(t[1], "STATUS: ANSWERED");
    assert!(alert_buttons(&popup_scene(&snap, &view)).contains(&AlertButton::LogDecision));

    let view = with_popup("stalled:claude:E1:0");
    let t = texts(&plan(&snap, &view));
    assert_eq!(
        &t[1..4],
        [
            "STATUS: UNKNOWN",
            "COMMUNICATION: NONE",
            "Trying to contact train 1350…"
        ]
    );
}

#[test]
fn overlays_come_after_the_board_and_popup_hits_are_on_top() {
    let snap = sample_board();
    let view = with_popup(QUESTION);
    let s = build_scene(&input(&snap, &view));
    let first_train = s
        .hits
        .iter()
        .position(|(_, h)| matches!(h, Hit::Train(_)))
        .unwrap();
    let last = s
        .hits
        .iter()
        .rposition(|(_, h)| matches!(h, Hit::Alert { .. }))
        .unwrap();
    let console = s.hits.iter().position(|(_, h)| *h == Hit::Console).unwrap();
    assert!(first_train < console && console < last);
}

#[test]
fn multi_select_question_renders_single_click_rows() {
    let mut snap = sample_board();
    let mut kind = question("Pick some");
    if let AlertKind::Question { questions } = &mut kind {
        questions[0].multi_select = true;
    }
    snap.alerts.push(Alert {
        answerable: true,
        ..alert("q:multi", kind, "2026-11-23T14:29:00Z", AlertState::Open)
    });
    let view = with_popup("q:multi");
    let p = plan(&snap, &view);
    let opts: Vec<_> = p.rows.iter().filter(|r| r.hit.is_some()).collect();
    assert_eq!(opts.len(), 2);
    assert!(opts.iter().all(|r| r.text().contains('☐')));
    let mut b = alert_buttons(&popup_scene(&snap, &view));
    b.sort_by_key(|b| format!("{b:?}"));
    assert_eq!(
        b,
        [
            AlertButton::Close,
            AlertButton::Option(0),
            AlertButton::Option(1)
        ]
    );
}
