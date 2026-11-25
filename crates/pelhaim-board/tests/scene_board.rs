use pelhaim_board::font::{Cut, draw_text, text_width};
use pelhaim_board::scene::{LABEL_PX, TRACK_X0};
use pelhaim_board::{
    DisplayList, Hit, Layout, PaletteKind, Prim, R, Rgba, Scene, SceneInput, View, ViewState,
    board_layout, build_scene, palette, tag_rect,
};
use pelhaim_core::testkit::{self, sample_board, ts};
use pelhaim_core::{
    LEG_BLOCKS, LineKey, Position, SessionKey, Snapshot, TagResolution, TrackId, TrainId,
    TrainState,
};

fn input<'a>(snap: &'a Snapshot, view: &'a ViewState) -> SceneInput<'a> {
    SceneInput {
        snap,
        view,
        viewport: (1600.0, 900.0),
        now_s: 0.0,
        palette: PaletteKind::Matte,
        reduced_motion: false,
    }
}

fn scene(snap: &Snapshot) -> Scene {
    build_scene(&input(snap, &ViewState::default()))
}

fn layout(snap: &Snapshot) -> Layout {
    board_layout(snap, &Default::default())
}

fn rect_color(list: &DisplayList, r: R) -> Option<Rgba> {
    list.prims.iter().find_map(|p| match p {
        Prim::Rect { r: pr, color, .. } if *pr == r => Some(*color),
        _ => None,
    })
}

fn block_colors(list: &DisplayList, layout: &Layout, track: TrackId) -> Vec<Rgba> {
    let g = &layout.tracks[&track];
    g.blocks
        .iter()
        .map(|&(x, w)| {
            rect_color(
                list,
                R {
                    x,
                    y: g.y - 1.25,
                    w,
                    h: 2.5,
                },
            )
            .unwrap_or_else(|| panic!("no block rect at x={x}"))
        })
        .collect()
}

fn tag_color(list: &DisplayList, layout: &Layout, snap: &Snapshot, id: TrainId) -> Rgba {
    let r = tag_rect(layout, &snap.trains[&id]);
    list.prims
        .iter()
        .find_map(|p| match p {
            Prim::Tri { p, color } if p[0].x == r.x + r.w => Some(*color),
            _ => None,
        })
        .expect("tag arrowhead")
}

/// Compared through serde, which rounds coordinates to 0.01, so summed text advances match.
fn contains_run(hay: &[Prim], needle: &[Prim]) -> bool {
    let j = |ps: &[Prim]| -> Vec<serde_json::Value> {
        ps.iter()
            .map(|p| serde_json::to_value(p).unwrap())
            .collect()
    };
    let (hay, needle) = (j(hay), j(needle));
    !needle.is_empty() && hay.windows(needle.len()).any(|w| w == needle)
}

/// One Claude main session on the `safeascent` line after `calls` tool calls in its first leg.
fn one_train(calls: usize) -> (Snapshot, TrainId, TrackId) {
    let mut e = testkit::engine(&["safeascent"]);
    let k = SessionKey::claude("W1");
    let t = ts("2026-11-23T10:00:00Z");
    testkit::discover_in(&mut e, &k, None, t);
    testkit::tag(&mut e, &k, "pelhaim-project: safeascent\nShip it");
    testkit::prompt(&mut e, &k, "Ship it", t);
    for i in 0..calls {
        let call = format!("toolu_{i}");
        testkit::tool_open(&mut e, &k, &call, "Read", t);
        testkit::tool_close(&mut e, &k, &call, t);
    }
    let snap = e.snapshot();
    let tr = snap.trains.values().next().expect("train");
    let (id, track) = (tr.id, tr.track);
    (snap, id, track)
}

#[test]
fn sample_board_scene_snapshot() {
    insta::assert_ron_snapshot!(scene(&sample_board()).list);
}

#[test]
fn blocks_behind_train_are_done_occupied_take_state_route_ahead() {
    let (snap, id, track) = one_train(6);
    let tr = &snap.trains[&id];
    assert_eq!(tr.state, TrainState::Working);
    assert_eq!(tr.position.block, 6);
    let p = palette(PaletteKind::Matte);
    let got = block_colors(&scene(&snap).list, &layout(&snap), track);
    let mut want = vec![p.done; 2];
    want.extend([p.work; 5]);
    want.extend([p.route; 3]);
    want.extend([p.idle; 2]);
    assert_eq!(got.len(), usize::from(LEG_BLOCKS) + 2);
    assert_eq!(got, want);
}

#[test]
fn cleared_route_only_while_working() {
    let p = palette(PaletteKind::Matte);
    let (mut snap, id, track) = one_train(3);
    let working = block_colors(&scene(&snap).list, &layout(&snap), track);
    assert!(working.contains(&p.route));
    for state in [
        TrainState::Idle,
        TrainState::ProbablyWaiting,
        TrainState::Stalled,
        TrainState::Failed("x".into()),
    ] {
        snap.trains.get_mut(&id).unwrap().state = state.clone();
        let c = block_colors(&scene(&snap).list, &layout(&snap), track);
        assert!(!c.contains(&p.route), "{state:?}: {c:?}");
        let occupied = c.iter().filter(|&&c| c != p.done && c != p.idle).count();
        assert!(occupied <= 3, "{state:?}: a non-working train is 3 blocks");
    }
}

#[test]
fn platform_bar_shows_done_overdue_and_arrival() {
    let p = palette(PaletteKind::Matte);
    let snap = sample_board();
    let bar = |snap: &Snapshot, track: u32, i: usize| {
        let l = layout(snap);
        let g = &l.tracks[&TrackId(track)];
        rect_color(
            &scene(snap).list,
            R {
                x: g.station_x[i],
                y: g.y - 9.0,
                w: 33.0,
                h: 5.0,
            },
        )
        .expect("platform bar")
    };
    // Track 7: SafeAscent passed "plan" and runs to "rehearse", due 15:23.
    assert_eq!(bar(&snap, 7, 0), p.done);
    assert_eq!(bar(&snap, 7, 1), p.idle);
    // Track 2: the ethica subagent is past its 14:10 arrival on "corpus pass".
    assert_eq!(bar(&snap, 2, 1), p.wait);

    let mut snap = snap;
    let t = snap.trains.get_mut(&TrainId(2)).unwrap();
    t.position = Position {
        leg: 1,
        block: LEG_BLOCKS - 1,
        berthed: true,
    };
    assert_eq!(
        bar(&snap, 2, 1),
        p.work,
        "a train at the platform lights it"
    );
}

fn has_text(snap: &Snapshot, s: &str, x: f32, y: f32, cut: Cut, color: Rgba) -> bool {
    let v = View {
        k: 1.0,
        ox: 0.0,
        oy: 0.0,
    };
    let mut want = DisplayList::default();
    draw_text(&mut want, &v, s, x, y, cut, LABEL_PX, color);
    contains_run(&scene(snap).list.prims, &want.prims)
}

#[test]
fn arrival_clock_follows_the_running_station() {
    let p = palette(PaletteKind::Matte);
    let snap = sample_board();
    let l = layout(&snap);
    let adv = pelhaim_board::font::advance(Cut::C5x7, LABEL_PX);
    let at = |track: u32, title: &str| {
        let g = &l.tracks[&TrackId(track)];
        (
            g.station_x[1] + text_width(title, Cut::C5x7, LABEL_PX) + adv,
            g.y - 21.0,
        )
    };
    let (x, y) = at(7, "rehearse");
    assert!(has_text(&snap, "ARR 15:23", x, y, Cut::C5x7, p.glyph));
    let (x, y) = at(2, "corpus pass");
    assert!(
        has_text(&snap, "ARR 14:10", x, y, Cut::C5x7, p.wait),
        "overdue is amber"
    );
    // A station without an estimate shows no clock.
    let g5 = &l.tracks[&TrackId(5)];
    assert!(has_text(
        &snap,
        "review",
        g5.station_x[4],
        g5.y - 21.0,
        Cut::C5x7,
        p.label
    ));
    assert!(!has_text(
        &snap,
        "ARR",
        g5.station_x[4] + text_width("review", Cut::C5x7, LABEL_PX) + adv,
        g5.y - 21.0,
        Cut::C5x7,
        p.glyph
    ));
}

#[test]
fn folded_stations_show_earlier_count() {
    let p = palette(PaletteKind::Matte);
    let snap = sample_board();
    assert_eq!(snap.tracks[&TrackId(5)].folded, 2);
    assert_eq!(snap.tracks[&TrackId(5)].stations.len(), 5);
    let l = layout(&snap);
    let g = &l.tracks[&TrackId(5)];
    assert_eq!(g.blocks.len(), 5 * usize::from(LEG_BLOCKS) + 2);
    assert!(has_text(
        &snap,
        "+2 earlier",
        g.x0,
        g.y - 21.0,
        Cut::C5x7,
        p.glyph
    ));
}

#[test]
fn dispatch_is_a_one_row_count_strip() {
    let p = palette(PaletteKind::Matte);
    let snap = sample_board();
    let l = layout(&snap);
    let d = &l.lines[0];
    assert_eq!(d.key, LineKey::Dispatch);
    assert_eq!(d.h, 16.0);
    assert!(!l.tracks.contains_key(&TrackId(1)), "no orchestrator track");
    let after = TRACK_X0
        + text_width("DISPATCH", Cut::C7x9, LABEL_PX)
        + pelhaim_board::font::advance(Cut::C7x9, LABEL_PX);
    assert!(has_text(
        &snap,
        "DISPATCH",
        TRACK_X0,
        d.top + 2.0,
        Cut::C7x9,
        p.label
    ));
    assert!(has_text(
        &snap,
        "· 1 agent running",
        after,
        d.top + 2.0,
        Cut::C7x9,
        p.glyph
    ));
}

#[test]
fn waiting_pulses_unless_reduced_motion() {
    let p = palette(PaletteKind::Matte);
    let snap = sample_board();
    let l = layout(&snap);
    let view = ViewState::default();
    let at = |now_s, reduced_motion| {
        let list = build_scene(&SceneInput {
            now_s,
            reduced_motion,
            ..input(&snap, &view)
        })
        .list;
        // Train 7 is Waiting on a question; train 4 is Stalled, which reads as waiting too.
        (
            tag_color(&list, &l, &snap, TrainId(7)),
            tag_color(&list, &l, &snap, TrainId(4)),
            block_colors(&list, &l, TrackId(7))[usize::from(LEG_BLOCKS)],
        )
    };
    assert_eq!(at(0.0, false), (p.wait, p.wait, p.wait));
    assert_eq!(at(0.5, false), (p.wait_dim, p.wait_dim, p.wait_dim));
    assert_ne!(at(0.0, false), at(0.5, false));
    assert_eq!(at(0.5, true), (p.wait, p.wait, p.wait));
    assert_eq!(at(0.25, true), (p.wait, p.wait, p.wait));
}

#[test]
fn fork_diagonal_is_60_degrees() {
    let snap = sample_board();
    let list = scene(&snap).list;
    let diagonals: Vec<f32> = list
        .prims
        .iter()
        .filter_map(|p| match p {
            Prim::Seg { a, b, width, .. } if *width == 2.5 && a.y != b.y => {
                Some((b.y - a.y).abs().atan2((b.x - a.x).abs()).to_degrees())
            }
            _ => None,
        })
        .collect();
    assert_eq!(
        diagonals.len(),
        1,
        "one diagonal per drawn fork; none off DISPATCH"
    );
    for d in diagonals {
        assert!((d - 60.0).abs() < 0.01, "{d}");
    }
    let l = layout(&snap);
    let (parent, child) = (&l.tracks[&TrackId(7)], &l.tracks[&TrackId(8)]);
    let f = &snap.forks[0];
    let at = usize::from(f.at.leg) * usize::from(LEG_BLOCKS) + usize::from(f.at.block);
    let (fx, fw) = parent.blocks[at];
    let dy = (child.y - parent.y).abs();
    let x0 = fx + fw + 6.0 + dy / 60f32.to_radians().tan() + 4.0;
    assert!((child.x0 - x0).abs() < 1e-3, "{} vs {x0}", child.x0);
}

#[test]
fn fork_takes_child_colour_and_done_child_is_grey() {
    let p = palette(PaletteKind::Matte);
    let snap = sample_board();
    let list = scene(&snap).list;
    let l = layout(&snap);
    let diag_color = |child: u32| {
        let cy = l.tracks[&TrackId(child)].y;
        list.prims
            .iter()
            .find_map(|pr| match pr {
                Prim::Seg { a, b, color, .. } if a.y != b.y && b.y == cy => Some(*color),
                _ => None,
            })
            .expect("fork diagonal")
    };
    assert_eq!(diag_color(8), p.work);
    assert!(
        block_colors(&list, &l, TrackId(3))
            .iter()
            .all(|&c| c == p.done)
    );
    // No rejoin "X" in M1: red means failed, so no red crossover segments are drawn.
    assert!(
        !list
            .prims
            .iter()
            .any(|pr| matches!(pr, Prim::Seg { color, .. } if *color == p.fail))
    );
}

fn crowded(n: usize) -> Snapshot {
    let mut e = testkit::engine(&["safeascent"]);
    for i in 0..n {
        let k = SessionKey::claude(&format!("C{i}"));
        let t = ts(&format!("2026-11-23T10:{i:02}:00Z"));
        testkit::discover_in(&mut e, &k, None, t);
        testkit::tag(&mut e, &k, "pelhaim-project: safeascent\nWork");
        testkit::prompt(&mut e, &k, "Work", t);
    }
    e.snapshot()
}

fn train_hits(s: &Scene) -> Vec<TrainId> {
    s.hits
        .iter()
        .filter_map(|(_, h)| match h {
            Hit::Train(id) => Some(*id),
            _ => None,
        })
        .collect()
}

#[test]
fn more_than_four_tracks_collapse_to_stub_and_expand() {
    let snap = crowded(6);
    let key = LineKey::Project("safeascent".into());
    let s = scene(&snap);
    assert_eq!(train_hits(&s).len(), 4);
    let stubs: Vec<_> = s
        .hits
        .iter()
        .filter(|(_, h)| *h == Hit::Stub(key.clone()))
        .collect();
    assert_eq!(stubs.len(), 1);
    let l = layout(&snap);
    let line = l.lines.iter().find(|b| b.key == key).unwrap();
    let stub_top = line.top + line.h - 12.0 + 1.0;
    let mut want = DisplayList::default();
    let p = palette(PaletteKind::Matte);
    let v = View {
        k: 1.0,
        ox: 0.0,
        oy: 0.0,
    };
    draw_text(
        &mut want,
        &v,
        "+2",
        TRACK_X0,
        stub_top,
        Cut::C5x7,
        LABEL_PX,
        p.glyph,
    );
    assert!(contains_run(&s.list.prims, &want.prims));
    let shown = &snap.lines.iter().find(|lv| lv.key == key).unwrap().tracks[..4];
    assert!(shown.iter().all(|t| l.tracks.contains_key(t)));

    let mut view = ViewState::default();
    view.expanded.insert(key.clone());
    let s = build_scene(&input(&snap, &view));
    assert_eq!(train_hits(&s).len(), 6);
    assert!(!s.hits.iter().any(|(_, h)| matches!(h, Hit::Stub(_))));
    let el = board_layout(&snap, &view.expanded);
    let eline = el.lines.iter().find(|b| b.key == key).unwrap();
    assert!(eline.h > line.h);
}

#[test]
fn inactive_line_folds() {
    let snap = sample_board();
    let key = LineKey::Project("stringcan".into());
    assert!(!snap.lines.iter().find(|l| l.key == key).unwrap().active);
    let l = layout(&snap);
    let b = l.lines.iter().find(|b| b.key == key).unwrap();
    assert_eq!(b.h, 16.0 + 10.0);
    let list = scene(&snap).list;
    let inside = |y: f32| y >= b.top && y < b.top + b.h;
    let tracks: Vec<_> = list
        .prims
        .iter()
        .filter_map(|p| match p {
            Prim::Rect { r, color, .. } if inside(r.y) && r.w > 100.0 => Some((*r, *color)),
            _ => None,
        })
        .collect();
    let glyph = palette(PaletteKind::Matte).glyph;
    assert_eq!(tracks.len(), 1);
    assert_eq!(tracks[0].0.w, 600.0);
    assert_eq!(tracks[0].0.h, 1.5);
    assert_eq!(tracks[0].1, glyph.alpha(0.5));
    assert!(
        !list
            .prims
            .iter()
            .any(|p| matches!(p, Prim::Tri { p, .. } if inside(p[0].y)))
    );
}

#[test]
fn badges_on_line_label() {
    let mut snap = sample_board();
    let key = LineKey::Project("safeascent".into());
    let v = View {
        k: 1.0,
        ox: 0.0,
        oy: 0.0,
    };
    let p = palette(PaletteKind::Matte);
    let badge = |snap: &Snapshot, s: &str, x: f32, color| {
        let top = layout(snap)
            .lines
            .iter()
            .find(|b| b.key == key)
            .unwrap()
            .top;
        let mut want = DisplayList::default();
        draw_text(&mut want, &v, s, x, top + 2.0, Cut::C7x9, LABEL_PX, color);
        contains_run(&scene(snap).list.prims, &want.prims)
    };
    let after = |s: &str| TRACK_X0 + text_width(s, Cut::C7x9, LABEL_PX);
    assert!(badge(&snap, "SAFEASCENT", TRACK_X0, p.label));
    assert!(badge(&snap, "●2", after("SAFEASCENT "), p.wait));
    assert!(!badge(&snap, "⚠", after("SAFEASCENT ●2 "), p.glyph));

    let line = snap.lines.iter_mut().find(|l| l.key == key).unwrap();
    line.format_untested = true;
    assert!(badge(&snap, "⚠", after("SAFEASCENT ●2 "), p.glyph));
    let line = snap.lines.iter_mut().find(|l| l.key == key).unwrap();
    line.open_decisions = 0;
    assert!(!badge(&snap, "●0", after("SAFEASCENT "), p.wait));
    assert!(badge(&snap, "⚠", after("SAFEASCENT "), p.glyph));
}

#[test]
fn selection_outline_drawn() {
    let snap = sample_board();
    let l = layout(&snap);
    let wait = palette(PaletteKind::Matte).wait;
    let r = tag_rect(&l, &snap.trains[&TrainId(5)]);
    let top_edge = R {
        x: r.x - 4.0,
        y: r.y - 3.5,
        w: r.w + 8.0,
        h: 1.0,
    };
    assert_eq!(rect_color(&scene(&snap).list, top_edge), None);
    let view = ViewState {
        selected: Some(TrainId(5)),
        ..ViewState::default()
    };
    let list = build_scene(&input(&snap, &view)).list;
    assert_eq!(rect_color(&list, top_edge), Some(wait));
    let bottom_edge = R {
        y: r.y - 3.5 + r.h + 9.0 - 1.0,
        ..top_edge
    };
    assert_eq!(rect_color(&list, bottom_edge), Some(wait));
}

#[test]
fn mistagged_tag_has_fail_outline() {
    let mut snap = sample_board();
    let fail = palette(PaletteKind::Matte).fail;
    let l = layout(&snap);
    let r = tag_rect(&l, &snap.trains[&TrainId(6)]);
    let top_edge = R {
        x: r.x - 1.0,
        y: r.y - 1.0,
        w: r.w + 2.0,
        h: 1.0,
    };
    assert_eq!(rect_color(&scene(&snap).list, top_edge), None);
    snap.trains.get_mut(&TrainId(6)).unwrap().tag = TagResolution::MisTagged("safeascnet".into());
    assert_eq!(rect_color(&scene(&snap).list, top_edge), Some(fail));
}

#[test]
fn track_without_stations_draws_one_leg() {
    let snap = sample_board();
    assert!(snap.tracks[&TrackId(4)].stations.is_empty());
    let l = layout(&snap);
    let g = &l.tracks[&TrackId(4)];
    assert_eq!(g.blocks.len(), usize::from(LEG_BLOCKS) + 2);
    assert!(g.station_x.is_empty());
    assert_eq!(g.x0, TRACK_X0);
}

#[test]
fn hit_map_has_every_visible_train() {
    let snap = sample_board();
    let l = layout(&snap);
    let view = ViewState {
        k: 2.0,
        ox: 10.0,
        oy: -5.0,
        ..ViewState::default()
    };
    let s = build_scene(&input(&snap, &view));
    let mut ids = train_hits(&s);
    ids.sort();
    let want: Vec<TrainId> = snap
        .trains
        .values()
        .filter(|t| snap.tracks[&t.track].line != LineKey::Dispatch)
        .map(|t| t.id)
        .collect();
    assert!(!want.contains(&TrainId(1)));
    assert_eq!(ids, want);
    let v = view.view();
    for (r, h) in &s.hits {
        if let Hit::Train(id) = h {
            let t = v.rect(tag_rect(&l, &snap.trains[id]));
            assert!(r.x < t.x && r.y < t.y && r.x + r.w > t.x + t.w && r.y + r.h > t.y + t.h);
            assert!((r.w - t.w - 12.0).abs() < 1e-3, "3-unit pad at k = 2");
        }
    }
    let c = l.content;
    for t in snap.trains.values() {
        let r = tag_rect(&l, t);
        assert!(
            r.x >= c.x && r.x + r.w <= c.x + c.w,
            "content covers tag {:?}",
            t.id
        );
    }
}
