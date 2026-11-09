use gallery::mines::{Mines, Status};
use std::time::Duration;

#[test]
fn presets_and_custom_density_are_bounded() {
    assert_eq!(Mines::suggested_mines(9, 9), 10);
    assert_eq!(Mines::suggested_mines(16, 16), 40);
    assert_eq!(Mines::suggested_mines(30, 16), 99);
    for c in 1..=64 {
        for r in 1..=64 {
            assert!(Mines::suggested_mines(c, r) <= Mines::maximum_mines(c, r));
        }
    }
    let mut g = Mines::default();
    g.configure(0, 1000, usize::MAX);
    assert_eq!((g.columns, g.rows, g.mine_count), (1, 64, 55));
}
#[test]
fn first_reveal_is_safe_at_edges_and_center_and_seed_is_replayable() {
    for (c, r, m) in [
        (9, 9, 10),
        (16, 16, 40),
        (30, 16, 99),
        (2, 2, 3),
        (64, 64, 4087),
    ] {
        for i in [0, c * r / 2, c * r - 1] {
            let mut g = Mines::default();
            g.configure(c, r, m);
            let mut replay = g.clone();
            assert!(g.reveal(i));
            assert!(replay.reveal(i));
            assert_eq!(g, replay);
            assert_ne!(g.status, Status::Lost);
            assert!(!g.cells[i].mine);
            assert_eq!(g.cells.iter().filter(|c| c.mine).count(), m);
            if c * r >= m + 9 {
                assert_eq!(g.cells[i].adjacent, 0);
            }
            for j in 0..g.cells.len() {
                let adjacent = (0..g.cells.len())
                    .filter(|&k| {
                        k != j
                            && (k % c).abs_diff(j % c) <= 1
                            && (k / c).abs_diff(j / c) <= 1
                            && g.cells[k].mine
                    })
                    .count();
                assert_eq!(g.cells[j].adjacent as usize, adjacent);
            }
        }
    }
}
#[test]
fn flags_do_not_start_timer_and_empty_field_floods_and_wins() {
    let mut g = Mines::default();
    g.configure(9, 9, 0);
    assert!(g.flag(0));
    assert!(!g.reveal(0));
    assert_eq!(g.status, Status::Ready);
    g.time = Duration::from_secs(50);
    assert_eq!(g.elapsed(), Duration::ZERO);
    assert!(g.flag(0));
    assert!(g.reveal(40));
    assert_eq!(g.status, Status::Won);
    assert!(g.cells.iter().all(|c| c.open));
    assert!(!g.flag(0));
    assert!(!g.reveal(0));
}
#[test]
fn loss_reveals_unflagged_mines_and_freezes_timer() {
    let mut g = Mines::default();
    g.configure(9, 9, 10);
    g.reveal(40);
    let mine = g.cells.iter().position(|c| c.mine).unwrap();
    g.time += Duration::from_secs(12);
    assert!(g.reveal(mine));
    assert_eq!(g.status, Status::Lost);
    assert!(
        g.cells
            .iter()
            .filter(|c| c.mine && !c.flagged)
            .all(|c| c.open)
    );
    g.time += Duration::from_secs(20);
    assert_eq!(g.elapsed(), Duration::from_secs(12));
    assert!(!g.flag(1));
    g.restart();
    assert_eq!(g.status, Status::Ready);
    assert_eq!(g.flags(), 0);
}
#[test]
fn chord_requires_exact_flags_and_wrong_flags_can_explode() {
    let mut g = Mines::default();
    g.configure(9, 9, 10);
    g.reveal(40);
    let i = (0..81)
        .find(|&i| {
            g.cells[i].open
                && g.cells[i].adjacent > 0
                && (0..81).any(|j| {
                    j != i
                        && (j % 9).abs_diff(i % 9) <= 1
                        && (j / 9).abs_diff(i / 9) <= 1
                        && !g.cells[j].mine
                        && !g.cells[j].open
                })
        })
        .unwrap();
    assert!(!g.chord(i));
    let neighbors = (0usize..81)
        .filter(|&j| j != i && (j % 9).abs_diff(i % 9) <= 1 && (j / 9).abs_diff(i / 9) <= 1)
        .collect::<Vec<_>>();
    for &j in &neighbors {
        if g.cells[j].mine {
            g.flag(j);
        }
    }
    assert!(g.chord(i));
    assert_ne!(g.status, Status::Lost);
    // A constructed small field exercises the incorrect-flag branch exactly.
    let mut g = Mines::default();
    g.configure(2, 2, 1);
    g.status = Status::Running;
    g.cells[0].open = true;
    g.cells[0].adjacent = 1;
    g.cells[1].mine = true;
    g.cells[2].flagged = true;
    assert!(g.chord(0));
    assert_eq!(g.status, Status::Lost);
}
#[test]
fn opening_all_safe_cells_auto_flags_and_roundtrips() {
    let mut g = Mines::default();
    g.configure(9, 9, 10);
    g.reveal(40);
    for i in 0..81 {
        if !g.cells[i].mine {
            g.reveal(i);
        }
    }
    assert_eq!(g.status, Status::Won);
    assert_eq!(g.flags(), 10);
    let restored: Mines = serde_json::from_str(&serde_json::to_string(&g).unwrap()).unwrap();
    assert_eq!(restored.status, g.status);
    assert_eq!(restored.seed, g.seed);
    for (a, b) in restored.cells.iter().zip(&g.cells) {
        assert_eq!(
            (a.mine, a.open, a.flagged, a.adjacent),
            (b.mine, b.open, b.flagged, b.adjacent)
        );
        assert!((a.reveal_at - b.reveal_at).abs() < 1e-12);
    }
}

#[cfg(feature = "host")]
mod ui {
    use super::*;
    use gallery::{Gallery, Page};
    use plico::VirtualClock;
    use plico_desktop::Source;
    use plico_renderer::{
        FrameSource, KeyEvent, KeyLocation, KeyPhase, Modifiers, Point, PointerButton,
        PointerEvent, PointerPhase, PointerProperties, SemanticAction, SemanticRole, TextContext,
        UiEvent,
    };
    fn frame(s: &mut Source<Gallery>, t: &mut TextContext, width: u32, height: u32) {
        s.build_frame(t, width, height);
        assert!(s.last_error.is_none(), "{:?}", s.last_error);
    }
    fn key(key: &str) -> UiEvent {
        UiEvent::Key(KeyEvent {
            phase: KeyPhase::Down,
            key: key.into(),
            key_without_modifiers: key.into(),
            code: key.into(),
            text: None,
            text_with_all_modifiers: None,
            location: KeyLocation::Standard,
            modifiers: Modifiers::empty(),
            repeat: false,
            synthetic: false,
        })
    }
    fn activate(s: &mut Source<Gallery>, label: &str) {
        let id = s
            .adapter
            .semantics(&s.engine, 0)
            .node_by_role(SemanticRole::Button, label, None)
            .unwrap()
            .id;
        assert!(s.handle_semantic_action(SemanticAction::Click { target: id }));
    }
    fn app() -> Gallery {
        Gallery {
            router: plico::Router::new(Page::Mines),
            ..Default::default()
        }
    }
    #[test]
    fn presets_right_click_and_keyboard_navigation_use_real_runtime() {
        let mut s = Source::new(app());
        let mut t = TextContext::new();
        frame(&mut s, &mut t, 1120, 820);
        activate(&mut s, "Easy");
        frame(&mut s, &mut t, 1120, 820);
        assert_eq!(
            (
                s.view.mines.columns,
                s.view.mines.rows,
                s.view.mines.mine_count
            ),
            (9, 9, 10)
        );
        let cell = s
            .adapter
            .semantics(&s.engine, 0)
            .node_by_role(SemanticRole::Button, "Row 5, column 5: hidden", None)
            .unwrap()
            .clone();
        for phase in [PointerPhase::Down, PointerPhase::Up] {
            s.handle_event(UiEvent::Pointer(PointerEvent {
                phase,
                position: Point {
                    x: ((cell.bounds[0] + cell.bounds[2]) / 2.) as f64,
                    y: ((cell.bounds[1] + cell.bounds[3]) / 2.) as f64,
                },
                button: Some(PointerButton::Secondary),
                buttons: if phase == PointerPhase::Down { 2 } else { 0 },
                modifiers: Modifiers::empty(),
                properties: PointerProperties::default(),
            }));
            frame(&mut s, &mut t, 1120, 820);
        }
        assert!(s.view.mines.cells[40].flagged);
        assert_eq!(s.view.mines.status, Status::Ready);
        // No pointer is needed for keyboard navigation or revealing a focused cell.
        s.handle_event(UiEvent::Pointer(PointerEvent {
            phase: PointerPhase::Leave,
            position: Point { x: 0., y: 0. },
            button: None,
            buttons: 0,
            modifiers: Modifiers::empty(),
            properties: PointerProperties::default(),
        }));
        assert!(s.handle_event(key("ArrowRight")).handled);
        frame(&mut s, &mut t, 1120, 820);
        assert!(s.handle_event(key("Enter")).handled);
        frame(&mut s, &mut t, 1120, 820);
        assert!(s.view.mines.cells[1].open);
        assert_eq!(s.view.mines.status, Status::Running);
        s.handle_event(key("r"));
        frame(&mut s, &mut t, 1120, 820);
        assert_eq!(s.view.mines.flags(), 0);
        assert_eq!(s.view.mines.status, Status::Ready);
        s.handle_event(key("Escape"));
        frame(&mut s, &mut t, 1120, 820);
        assert!(!s.view.mines.playing);
    }
    #[test]
    fn numeric_fields_accept_native_text_navigation_and_submit() {
        let mut s = Source::new(app());
        let mut t = TextContext::new();
        frame(&mut s, &mut t, 1120, 820);
        let id = s
            .adapter
            .semantics(&s.engine, 0)
            .node_by_role(SemanticRole::TextInput, "COLUMNS", None)
            .unwrap()
            .id;
        assert!(s.handle_semantic_action(SemanticAction::Focus { target: id }));
        frame(&mut s, &mut t, 1120, 820);
        s.handle_event(key("Home"));
        s.handle_event(key("Delete"));
        s.handle_event(key("Delete"));
        s.handle_event(UiEvent::TextInput("9".into()));
        frame(&mut s, &mut t, 1120, 820);
        assert_eq!(s.view.mines.columns, 9);
        s.handle_event(key("ArrowUp"));
        frame(&mut s, &mut t, 1120, 820);
        assert_eq!(s.view.mines.columns, 10);
        s.handle_event(key("Enter"));
        frame(&mut s, &mut t, 1120, 820);
        assert!(s.view.mines.playing);
    }
    #[test]
    fn small_window_and_dense_board_fit_and_result_stops_animating() {
        for (c, r, m) in [(9, 9, 10), (30, 16, 99), (64, 64, 100)] {
            let mut app = app();
            app.sidebar_collapsed = true;
            app.mines.configure(c, r, m);
            let mut s = Source::new(app);
            let mut t = TextContext::new();
            frame(&mut s, &mut t, 640, 740);
            for node in s
                .adapter
                .semantics(&s.engine, 0)
                .exposed_nodes()
                .into_iter()
                .filter(|n| n.label.as_deref().is_some_and(|x| x.starts_with("Row ")))
            {
                assert!(
                    node.bounds[0] >= 0.
                        && node.bounds[1] >= 0.
                        && node.bounds[2] <= 640.
                        && node.bounds[3] <= 740.,
                    "{:?}",
                    node.bounds
                );
            }
        }
        let clock = VirtualClock::default();
        let mut app = app();
        app.mines.configure(9, 9, 0);
        let mut s = Source::with_clock(app, clock.clone());
        let mut t = TextContext::new();
        frame(&mut s, &mut t, 1120, 820);
        activate(&mut s, "Row 5, column 5: hidden");
        frame(&mut s, &mut t, 1120, 820);
        assert!(s.has_anim());
        clock.advance(Duration::from_secs(5)).unwrap();
        frame(&mut s, &mut t, 1120, 820);
        assert_eq!(s.view.mines.status, Status::Won);
        assert!(!s.has_anim());
        activate(&mut s, "Play again");
        frame(&mut s, &mut t, 1120, 820);
        assert_eq!(s.view.mines.status, Status::Ready);
    }
}
