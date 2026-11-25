use pelhaim_board::font::{Cut, advance, draw_text, line_height, text_width};
use pelhaim_board::scene::lines::{
    LINE_INSET_FRAC, MARKER, earlier_label, station_label, station_label_at,
};
use pelhaim_board::scene::{LABEL_PX, TRACK_THICK};
use pelhaim_board::{
    DisplayList, Hit, Layout, LineColor, PaletteKind, Prim, R, Rgba, Scene, SceneInput, View,
    ViewState, assign_line_colors, board_layout, build_scene, line_rgba, line_train_of,
    line_trains, palette, state_rank, tag_rect,
};
use pelhaim_core::testkit::{self, sample_board, ts};
use pelhaim_core::{LineKey, SessionKey, Snapshot, TagResolution, TrainId, TrainState};

const W: f32 = 1600.0;

fn input<'a>(snap: &'a Snapshot, view: &'a ViewState) -> SceneInput<'a> {
    SceneInput {
        snap,
        view,
        viewport: (W, 900.0),
        now_s: 0.0,
        palette: PaletteKind::Matte,
        reduced_motion: false,
    }
}

fn scene(snap: &Snapshot) -> Scene {
    build_scene(&input(snap, &ViewState::default()))
}

fn layout(snap: &Snapshot) -> Layout {
    board_layout(snap, W)
}

fn project(s: &str) -> LineKey {
    LineKey::Project(s.into())
}

fn rect_color(list: &DisplayList, r: R) -> Option<Rgba> {
    list.prims.iter().find_map(|p| match p {
        Prim::Rect { r: pr, color, .. } if *pr == r => Some(*color),
        _ => None,
    })
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

fn train_hits(s: &Scene) -> Vec<TrainId> {
    s.hits
        .iter()
        .filter_map(|(_, h)| match h {
            Hit::Train(id) => Some(*id),
            _ => None,
        })
        .collect()
}

/// `n` Claude main sessions on `safeascent`, a minute apart.
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

fn ids_by_start(snap: &Snapshot) -> Vec<TrainId> {
    let mut v: Vec<_> = snap.trains.values().collect();
    v.sort_by_key(|t| t.counters.started);
    v.into_iter().map(|t| t.id).collect()
}

#[test]
fn sample_board_scene_snapshot() {
    insta::assert_ron_snapshot!(scene(&sample_board()).list);
}

#[test]
fn one_train_per_line_no_matter_how_many_sessions() {
    let snap = crowded(6);
    let lts = line_trains(&snap);
    assert_eq!(lts.len(), 1);
    assert_eq!(lts[0].members.len(), 6);
    let s = scene(&snap);
    assert_eq!(train_hits(&s), vec![lts[0].lead]);
    let tags = s
        .list
        .prims
        .iter()
        .filter(|p| matches!(p, Prim::Tri { .. }))
        .count();
    assert_eq!(tags, 1, "one tag, no sub-tracks");

    // The sample board: one hit per drawn line, each the line's lead.
    let snap = sample_board();
    let l = layout(&snap);
    let mut hits = train_hits(&scene(&snap));
    hits.sort();
    let mut leads: Vec<TrainId> = l.geoms.iter().map(|g| g.train.lead).collect();
    leads.sort();
    assert_eq!(hits, leads);
    assert_eq!(
        l.geoms.len(),
        5,
        "safeascent, ethica, cabn, loosy-goose, unsorted"
    );
    let ethica = l.geom(&project("ethica")).unwrap();
    assert_eq!(
        ethica.train.members.len(),
        2,
        "the subagent and E1 share one train"
    );
    let sa = l.geom(&project("safeascent")).unwrap();
    assert_eq!(sa.train.members.len(), 2, "S1 and its subagent b1");
}

#[test]
fn state_precedence_waiting_failed_working_stalled_idle_done() {
    let base = crowded(6);
    let ids = ids_by_start(&base);
    let states = [
        TrainState::Done,
        TrainState::Idle,
        TrainState::Stalled,
        TrainState::Working,
        TrainState::Failed("x".into()),
        TrainState::Waiting(pelhaim_core::AlertId("a".into())),
    ];
    let ranks: Vec<u8> = states.iter().map(state_rank).collect();
    assert_eq!(ranks, [5, 4, 3, 2, 1, 0]);
    assert_eq!(state_rank(&TrainState::ProbablyWaiting), 0);
    // Add states one by one, highest last: each new one must win.
    for n in 1..=states.len() {
        let mut snap = base.clone();
        for (id, st) in ids.iter().zip(states.iter().take(n)) {
            snap.trains.get_mut(id).unwrap().state = st.clone();
        }
        for id in &ids[n..] {
            snap.trains.get_mut(id).unwrap().state = TrainState::Done;
        }
        let lt = line_train_of(&snap, ids[0]).unwrap();
        assert_eq!(lt.state(&snap), &states[n - 1], "with {n} states");
    }
}

#[test]
fn tag_number_is_the_earliest_active_session() {
    let mut snap = crowded(3);
    let ids = ids_by_start(&snap);
    let n = |snap: &Snapshot| line_trains(snap)[0].number;
    assert_eq!(n(&snap), snap.trains[&ids[0]].number);
    snap.trains.get_mut(&ids[0]).unwrap().state = TrainState::Done;
    assert_eq!(
        n(&snap),
        snap.trains[&ids[1]].number,
        "a done session gives way"
    );
    for id in &ids {
        snap.trains.get_mut(id).unwrap().state = TrainState::Done;
    }
    assert_eq!(
        n(&snap),
        snap.trains[&ids[0]].number,
        "all done: the earliest"
    );
}

/// Two sessions on one line whose milestones interleave in time.
fn interleaved(max_stations: usize) -> Snapshot {
    let mut c = testkit::cfg(&["safeascent"]);
    c.max_stations = max_stations;
    let mut e = pelhaim_core::engine::Engine::new(c, Default::default());
    let (a, b) = (SessionKey::claude("A"), SessionKey::claude("B"));
    let t = |m: u32| ts(&format!("2026-11-23T10:{m:02}:00Z"));
    for (k, m) in [(&a, 0), (&b, 1)] {
        testkit::discover_in(&mut e, k, None, t(m));
        testkit::tag(&mut e, k, "pelhaim-project: safeascent\nWork");
        testkit::prompt(&mut e, k, "Work", t(m));
    }
    for (k, name, m) in [
        (&a, "a1", 2),
        (&b, "b1", 3),
        (&a, "a2 ~1h", 4),
        (&b, "b2 ~2h", 5),
    ] {
        testkit::milestone(&mut e, k, &format!("pelhaim-milestone: {name}"), t(m));
    }
    e.snapshot()
}

fn titles(snap: &Snapshot) -> Vec<String> {
    line_trains(snap)[0]
        .stations
        .iter()
        .map(|s| snap.stations[s].title.clone())
        .collect()
}

#[test]
fn stations_from_every_agent_in_time_order_with_fold() {
    let snap = interleaved(5);
    assert_eq!(titles(&snap), ["a1", "b1", "a2", "b2"]);
    let lt = &line_trains(&snap)[0];
    assert_eq!(lt.folded, 0);
    assert_eq!(lt.next, Some(2), "a2 is the oldest station still running");

    let snap = interleaved(3);
    assert_eq!(titles(&snap), ["b1", "a2", "b2"]);
    let l = layout(&snap);
    let g = &l.geoms[0];
    assert_eq!(g.train.folded, 1);
    let (s, x, y, _) = earlier_label(g).unwrap();
    assert_eq!(s, "+1 earlier");
    assert!(has_text(
        &snap,
        "+1 earlier",
        x,
        y,
        Cut::C5x7,
        palette(PaletteKind::Matte).glyph
    ));
}

#[test]
fn arrival_clock_only_on_the_next_station() {
    let snap = interleaved(5);
    let l = layout(&snap);
    let g = &l.geoms[0];
    let with_clock: Vec<usize> = (0..g.station_s.len())
        .filter(|&i| station_label(&snap, g, i).unwrap().1.is_some())
        .collect();
    assert_eq!(with_clock, [2], "b2 has an estimate but is not next");

    let p = palette(PaletteKind::Matte);
    let snap = sample_board();
    let l = layout(&snap);
    let adv = advance(Cut::C5x7, LABEL_PX);
    let clock = |key: &str, title: &str, arr: &str, color: Rgba| {
        let g = l.geom(&project(key)).unwrap();
        let i = g.train.next.unwrap();
        let (t, a) = station_label(&snap, g, i).unwrap();
        assert_eq!((t.as_str(), a.as_deref()), (title, Some(arr)));
        let (x, y, _) = station_label_at(&snap, g, i).unwrap();
        has_text(
            &snap,
            arr,
            x + text_width(title, Cut::C5x7, LABEL_PX) + adv,
            y,
            Cut::C5x7,
            color,
        )
    };
    assert!(clock("safeascent", "rehearse", "ARR 15:23", p.glyph));
    assert!(
        clock("ethica", "corpus pass", "ARR 14:10", p.wait),
        "overdue is amber"
    );
}

#[test]
fn station_markers_show_done_overdue_and_arrival() {
    let p = palette(PaletteKind::Matte);
    let snap = sample_board();
    let marker = |snap: &Snapshot, key: &str, i: usize| {
        let l = layout(snap);
        let (x, y) = l.geom(&project(key)).unwrap().station_xy(i);
        rect_color(
            &scene(snap).list,
            R {
                x: x - MARKER / 2.0,
                y: y - MARKER / 2.0,
                w: MARKER,
                h: MARKER,
            },
        )
        .expect("station marker")
    };
    assert_eq!(marker(&snap, "safeascent", 0), p.done);
    assert_eq!(marker(&snap, "safeascent", 1), p.idle);
    assert_eq!(marker(&snap, "ethica", 1), p.wait, "overdue");

    let mut snap = snap;
    let sa = line_train_of(&snap, TrainId(7)).unwrap();
    assert_eq!(sa.next, Some(1));
    let t = snap.trains.get_mut(&TrainId(7)).unwrap();
    t.position.block = pelhaim_core::LEG_BLOCKS - 1;
    t.position.berthed = true;
    t.state = TrainState::Working;
    snap.trains.get_mut(&TrainId(8)).unwrap().state = TrainState::Idle;
    assert_eq!(
        marker(&snap, "safeascent", 1),
        p.work,
        "a train at the station lights it"
    );
}

#[test]
fn line_colour_is_per_slug_with_config_override() {
    let snap = sample_board();
    let l = layout(&snap);
    let assigned = assign_line_colors(snap.lines.iter().filter_map(|lv| match &lv.key {
        LineKey::Project(s) => Some((s.as_str(), None)),
        _ => None,
    }));
    for b in &l.lines {
        let want = match &b.key {
            LineKey::Project(s) => assigned[s],
            _ => LineColor::Neutral,
        };
        assert_eq!(b.color, want);
    }
    let projects: Vec<_> = l
        .lines
        .iter()
        .filter(|b| matches!(b.key, LineKey::Project(_)))
        .map(|b| b.color)
        .collect();
    let distinct: std::collections::BTreeSet<_> = projects.iter().collect();
    assert_eq!(
        distinct.len(),
        projects.len(),
        "no two lines share a colour"
    );
    assert_eq!(layout(&snap), layout(&snap), "stable across frames");

    let mut snap = snap;
    let sa = snap
        .lines
        .iter_mut()
        .find(|lv| lv.key == project("safeascent"))
        .unwrap();
    sa.color = Some("#123456".into());
    let l = layout(&snap);
    let g = l.geom(&project("safeascent")).unwrap();
    assert_eq!(g.color, LineColor::Rgb(0x123456));
    let list = scene(&snap).list;
    let (s0, _) = g.blocks[0];
    let (a, _) = g.pieces(s0, g.blocks[0].1)[0];
    let first = list.prims.iter().find_map(|p| match p {
        Prim::Rect { r, color, .. } if r.x == a.0 && r.h == TRACK_THICK => Some(*color),
        _ => None,
    });
    assert_eq!(
        first,
        Some(Rgba::hex(0x123456)),
        "behind the train: the line colour"
    );

    // State is on the tag, never the line: a waiting line keeps its own colour.
    let p = palette(PaletteKind::Matte);
    assert_eq!(tag_color(&list, &l, &snap, TrainId(7)), p.wait);
    assert!(!list.prims.iter().any(|pr| matches!(
        pr,
        Prim::Rect { r, color, .. } if r.h == TRACK_THICK && *color == p.wait
    )));

    let hc = line_rgba(PaletteKind::HighContrast, LineColor::Palette(0));
    assert_ne!(hc, line_rgba(PaletteKind::Matte, LineColor::Palette(0)));
}

#[test]
fn geometry_is_deterministic_with_45_degree_turns() {
    let snap = sample_board();
    let a = layout(&snap);
    let b = layout(&sample_board());
    assert_eq!(a.geoms, b.geoms);
    let paths: Vec<_> = a.geoms.iter().map(|g| g.pts.clone()).collect();
    let mut turns = 0;
    for g in &a.geoms {
        for w in g.pts.windows(2) {
            let (dx, dy) = (w[1].0 - w[0].0, w[1].1 - w[0].1);
            assert!(dx > 0.0, "x increases along {:?}", g.key);
            if dy != 0.0 {
                assert!((dy.abs() - dx).abs() < 1e-3, "45°: {dx} {dy}");
                turns += 1;
            }
        }
    }
    assert!(turns >= 3 * a.geoms.len(), "every line has at least 3 jogs");
    // Two lines with the same slug-relative position still get different shapes.
    let rel = |g: &pelhaim_board::LineGeom| -> Vec<(i32, i32)> {
        g.pts
            .iter()
            .map(|p| (p.0.round() as i32, (p.1 - g.pts[0].1).round() as i32))
            .collect()
    };
    let shapes: std::collections::BTreeSet<_> = a.geoms.iter().map(rel).collect();
    assert!(shapes.len() > 1, "{paths:?}");
}

fn seg_hits_rect(a: (f32, f32), b: (f32, f32), r: R) -> bool {
    (0..=64).any(|i| {
        let f = i as f32 / 64.0;
        let (x, y) = (a.0 + (b.0 - a.0) * f, a.1 + (b.1 - a.1) * f);
        let t = TRACK_THICK / 2.0;
        x + t > r.x && x - t < r.x + r.w && y + t > r.y && y - t < r.y + r.h
    })
}

#[test]
fn no_line_crosses_any_label() {
    for snap in [sample_board(), interleaved(5), crowded(4)] {
        let l = layout(&snap);
        let h = line_height(Cut::C5x7, LABEL_PX);
        let mut labels: Vec<(String, R)> = Vec::new();
        for (lv, lb) in snap.lines.iter().zip(&l.lines) {
            labels.push((
                lv.name.clone(),
                R {
                    x: l.line_x0(),
                    y: lb.top + 2.0,
                    w: text_width(&lv.name, Cut::C7x9, LABEL_PX) + 40.0,
                    h: line_height(Cut::C7x9, LABEL_PX),
                },
            ));
        }
        for g in &l.geoms {
            for i in 0..g.station_s.len() {
                let (x, y, w) = station_label_at(&snap, g, i).unwrap();
                labels.push((format!("station {i} of {:?}", g.key), R { x, y, w, h }));
            }
            if let Some((s, x, y, w)) = earlier_label(g) {
                labels.push((s, R { x, y, w, h }));
            }
        }
        for g in &l.geoms {
            for w in g.pts.windows(2) {
                for (name, r) in &labels {
                    assert!(!seg_hits_rect(w[0], w[1], *r), "{:?} crosses {name}", g.key);
                }
            }
        }
    }
}

#[test]
fn lines_span_most_of_the_width_and_the_train_maps_along_it() {
    for w in [800.0, 1600.0, 2560.0] {
        for snap in [sample_board(), crowded(1)] {
            let l = board_layout(&snap, w);
            for g in &l.geoms {
                let span = (g.x1() - g.x0()) / w;
                assert!(
                    (span - (1.0 - 2.0 * LINE_INSET_FRAC)).abs() < 1e-4,
                    "{span}"
                );
                assert!((0.85..=0.9).contains(&span));
                let n = g.station_s.len();
                for (i, &s) in g.station_s.iter().enumerate() {
                    let want = g.len() * (i + 1) as f32 / (n + 1) as f32;
                    assert!(
                        (s - want).abs() < 1e-2,
                        "stations spread over the full length"
                    );
                }
                let at = g.train.at / (n + 1) as f32;
                assert!((g.head_s - g.len() * at).abs() < 1e-2);
            }
        }
    }
    // A line with no stations still spans the width; its train is part way along.
    let snap = crowded(1);
    let l = layout(&snap);
    let g = &l.geoms[0];
    assert!(g.station_s.is_empty());
    assert!(g.x1() - g.x0() > 0.85 * W);
}

#[test]
fn forks_are_not_drawn() {
    let snap = sample_board();
    assert!(!snap.forks.is_empty(), "the engine still records the fork");
    let l = layout(&snap);
    let list = scene(&snap).list;
    let on_a_path = |p: (f32, f32)| {
        l.geoms.iter().any(|g| {
            g.pts.windows(2).any(|w| {
                let (dx, dy) = (w[1].0 - w[0].0, w[1].1 - w[0].1);
                let f = ((p.0 - w[0].0) / dx).clamp(0.0, 1.0);
                ((w[0].0 + dx * f - p.0).abs() + (w[0].1 + dy * f - p.1).abs()) < 0.05
            })
        })
    };
    for pr in &list.prims {
        if let Prim::Seg { a, b, width, .. } = pr
            && *width == TRACK_THICK
        {
            assert!(
                on_a_path((a.x, a.y)) && on_a_path((b.x, b.y)),
                "{a:?}–{b:?}"
            );
        }
    }
}

#[test]
fn dispatch_is_a_one_row_count_strip() {
    let p = palette(PaletteKind::Matte);
    let snap = sample_board();
    let l = layout(&snap);
    let d = &l.lines[0];
    assert_eq!(d.key, LineKey::Dispatch);
    assert_eq!(d.h, 16.0);
    assert!(l.geom(&LineKey::Dispatch).is_none());
    let x0 = l.line_x0();
    let after = x0 + text_width("DISPATCH", Cut::C7x9, LABEL_PX) + advance(Cut::C7x9, LABEL_PX);
    assert!(has_text(
        &snap,
        "DISPATCH",
        x0,
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
        tag_color(&list, &l, &snap, TrainId(7))
    };
    assert_eq!(at(0.0, false), p.wait);
    assert_eq!(at(0.5, false), p.wait_dim);
    assert_eq!(at(0.5, true), p.wait);
    assert_eq!(at(0.25, true), p.wait);
}

#[test]
fn inactive_line_folds_to_a_full_width_thin_line() {
    let snap = sample_board();
    let key = project("stringcan");
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
    assert_eq!(tracks.len(), 1);
    assert!((tracks[0].0.w - (l.line_x1() - l.line_x0())).abs() < 1e-3);
    assert_eq!(tracks[0].0.h, 1.5);
    let c = line_rgba(PaletteKind::Matte, b.color);
    assert_eq!(tracks[0].1, c.alpha(0.5));
}

#[test]
fn badges_on_line_label() {
    let mut snap = sample_board();
    let key = project("safeascent");
    let v = View {
        k: 1.0,
        ox: 0.0,
        oy: 0.0,
    };
    let p = palette(PaletteKind::Matte);
    let x0 = layout(&snap).line_x0();
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
    let after = |s: &str| x0 + text_width(s, Cut::C7x9, LABEL_PX);
    assert!(badge(&snap, "SAFEASCENT", x0, p.label));
    assert!(badge(&snap, "●2", after("SAFEASCENT "), p.wait));
    let line = snap.lines.iter_mut().find(|l| l.key == key).unwrap();
    line.format_untested = true;
    assert!(badge(&snap, "⚠", after("SAFEASCENT ●2 "), p.glyph));
}

#[test]
fn selecting_any_session_outlines_its_line_train() {
    let snap = sample_board();
    let l = layout(&snap);
    let wait = palette(PaletteKind::Matte).wait;
    let r = tag_rect(&l, &snap.trains[&TrainId(7)]);
    assert_eq!(
        r,
        tag_rect(&l, &snap.trains[&TrainId(8)]),
        "members share the tag"
    );
    let top_edge = R {
        x: r.x - 4.0,
        y: r.y - 3.5,
        w: r.w + 8.0,
        h: 1.0,
    };
    assert_eq!(rect_color(&scene(&snap).list, top_edge), None);
    for id in [TrainId(7), TrainId(8)] {
        let view = ViewState {
            selected: Some(id),
            ..ViewState::default()
        };
        let list = build_scene(&input(&snap, &view)).list;
        assert_eq!(rect_color(&list, top_edge), Some(wait), "{id:?}");
    }
}

#[test]
fn any_mistagged_member_outlines_the_tag() {
    let mut snap = sample_board();
    let fail = palette(PaletteKind::Matte).fail;
    let l = layout(&snap);
    let r = tag_rect(&l, &snap.trains[&TrainId(8)]);
    let top_edge = R {
        x: r.x - 1.0,
        y: r.y - 1.0,
        w: r.w + 2.0,
        h: 1.0,
    };
    assert_eq!(rect_color(&scene(&snap).list, top_edge), None);
    snap.trains.get_mut(&TrainId(8)).unwrap().tag = TagResolution::MisTagged("safeascnet".into());
    assert_eq!(rect_color(&scene(&snap).list, top_edge), Some(fail));
}

#[test]
fn hit_map_pads_every_tag_and_content_covers_it() {
    let snap = sample_board();
    let l = layout(&snap);
    let view = ViewState {
        k: 2.0,
        ox: 10.0,
        oy: -5.0,
        ..ViewState::default()
    };
    let s = build_scene(&input(&snap, &view));
    assert!(
        !train_hits(&s).contains(&TrainId(1)),
        "the orchestrator draws no train"
    );
    let v = view.view();
    for (r, h) in &s.hits {
        if let Hit::Train(id) = h {
            let t = v.rect(tag_rect(&l, &snap.trains[id]));
            assert!((r.w - t.w - 12.0).abs() < 1e-3, "3-unit pad at k = 2");
        }
    }
    let c = l.content;
    for g in &l.geoms {
        let r = tag_rect(&l, &snap.trains[&g.train.lead]);
        assert!(
            r.x >= c.x && r.x + r.w <= c.x + c.w,
            "content covers {:?}",
            g.key
        );
    }
}
