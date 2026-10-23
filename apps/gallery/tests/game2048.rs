use plico_backend::Source;
use plico_core::{Clock, VirtualClock};
use plico_gallery::{
    Gallery, Page,
    game2048::{Direction, Game},
};
use plico_renderer::{
    FrameSource, KeyEvent, KeyLocation, KeyPhase, Modifiers, SemanticAction, SemanticRole,
    TextContext, UiEvent,
};
use std::time::Duration;
fn key(value: &str) -> UiEvent {
    UiEvent::Key(KeyEvent {
        phase: KeyPhase::Down,
        key: value.into(),
        key_without_modifiers: value.into(),
        code: value.into(),
        text: None,
        text_with_all_modifiers: None,
        location: KeyLocation::Standard,
        modifiers: Modifiers::empty(),
        repeat: false,
        synthetic: false,
    })
}
fn frame(s: &mut Source<Gallery>, tcx: &mut TextContext, w: u32, h: u32) {
    s.build_frame(tcx, w, h);
    assert!(s.last_error.is_none(), "{:?}", s.last_error);
}
#[test]
fn keyboard_slide_retains_entity_and_stops_requesting_frames() {
    let clock = VirtualClock::default();
    let mut source = Source::with_clock(
        Gallery {
            router: plico_core::Router::new(Page::Game2048),
            game2048: Game::from_cells([[0, 0, 0, 8], [0; 4], [0; 4], [0; 4]]),
            ..Default::default()
        },
        clock.clone(),
    );
    let mut tcx = TextContext::new();
    frame(&mut source, &mut tcx, 1120, 820);
    clock.advance(Duration::from_secs(1)).unwrap();
    frame(&mut source, &mut tcx, 1120, 820);
    let initial = source
        .adapter
        .placed
        .iter()
        .find(|n| n.paint.text.as_deref() == Some("8"))
        .unwrap()
        .clone();
    assert!(source.handle_event(key("ArrowLeft")).handled);
    frame(&mut source, &mut tcx, 1120, 820);
    assert_eq!(source.view.game2048.cells()[0][0], 8);
    let rect = |s: &Source<Gallery>| {
        s.adapter
            .placed
            .iter()
            .find(|n| n.node_id == initial.node_id)
            .and_then(|n| s.adapter.visual_bounds(n.node_id))
            .unwrap()
    };
    let start = rect(&source);
    clock.advance(Duration::from_millis(50)).unwrap();
    frame(&mut source, &mut tcx, 1120, 820);
    let middle = rect(&source);
    assert_eq!(source.adapter.stats.layout_runs, 0);
    assert_eq!(source.adapter.stats.style_writes, 0);
    assert_eq!(source.adapter.stats.flatten_visits, 0);
    clock.advance(Duration::from_millis(550)).unwrap();
    frame(&mut source, &mut tcx, 1120, 820);
    let end = rect(&source);
    assert!(start[0] > middle[0] && middle[0] > end[0]);
    assert!(!source.has_anim());
    assert_eq!(source.adapter.stats.created, 0);
    let snapshot = serde_json::to_value(source.engine.snapshot()).unwrap();
    frame(&mut source, &mut tcx, 1120, 820);
    assert_eq!(
        snapshot,
        serde_json::to_value(source.engine.snapshot()).unwrap()
    );
    assert_eq!(clock.now(), Duration::from_millis(1600));
}
#[test]
fn gallery_buttons_play_and_restart_and_board_fits_small_window() {
    let mut s = Source::new(Gallery {
        router: plico_core::Router::new(Page::Game2048),
        ..Default::default()
    });
    let mut tcx = TextContext::new();
    frame(&mut s, &mut tcx, 640, 740);
    let tree = s.adapter.semantics(&s.engine, 0);
    let right = tree
        .node_by_role(SemanticRole::Button, "→", None)
        .unwrap()
        .id;
    assert!(s.handle_semantic_action(SemanticAction::Click { target: right }));
    frame(&mut s, &mut tcx, 640, 740);
    s.view.game2048.score = 128;
    s.view.game2048.best = 512;
    let new_game = s
        .adapter
        .semantics(&s.engine, 0)
        .node_by_role(SemanticRole::Button, "New game", None)
        .unwrap()
        .id;
    s.handle_semantic_action(SemanticAction::Click { target: new_game });
    frame(&mut s, &mut tcx, 640, 740);
    assert_eq!(s.view.game2048.score, 0);
    assert_eq!(s.view.game2048.best, 512);
    for node in &s.adapter.placed {
        if node.paint.background.is_some_and(|c| {
            let c = c.to_rgba8();
            c.r == 187 && c.g == 173 && c.b == 160
        }) {
            assert!(node.rect[0] >= 164. && node.rect[2] <= 640.);
        }
    }
    s.view.router.go(Page::Controls);
    frame(&mut s, &mut tcx, 640, 740);
    let before = s.view.game2048.clone();
    s.handle_event(key("ArrowLeft"));
    assert_eq!(before.cells(), s.view.game2048.cells());
    assert_eq!(before.score, s.view.game2048.score);
}
#[test]
fn game_over_and_win_present_actions_after_motion() {
    for (cells, message) in [
        (
            [[2, 4, 2, 4], [4, 2, 4, 2], [2, 4, 2, 4], [4, 2, 4, 2]],
            "Game over!",
        ),
        ([[1024, 1024, 0, 0], [0; 4], [0; 4], [0; 4]], "You win!"),
    ] {
        let clock = VirtualClock::default();
        let mut game = Game::from_cells(cells);
        if !game.over {
            game.move_dir(Direction::Left);
        }
        let mut source = Source::with_clock(
            Gallery {
                router: plico_core::Router::new(Page::Game2048),
                game2048: game,
                ..Default::default()
            },
            clock.clone(),
        );
        let mut tcx = TextContext::new();
        frame(&mut source, &mut tcx, 920, 820);
        clock.advance(Duration::from_secs(1)).unwrap();
        frame(&mut source, &mut tcx, 920, 820);
        assert!(
            source
                .adapter
                .placed
                .iter()
                .any(|n| n.paint.text.as_deref() == Some(message))
        );
        let tree = source.adapter.semantics(&source.engine, 0);
        assert!(
            tree.node_by_role(SemanticRole::Button, "Try again", None)
                .is_some()
        );
    }
}

#[test]
fn portable_module_keys_use_injected_clock_viewport_and_preserve_game_state() {
    let app = Gallery {
        router: plico_core::Router::new(Page::Game2048),
        game2048: Game::from_cells([[2, 2, 4, 0], [0; 4], [0; 4], [0; 4]]),
        ..Default::default()
    };
    let frame = plico_dev::render::<Gallery>(plico_dev::Request {
        state: serde_json::to_value(app).unwrap(),
        keys: vec!["ArrowLeft".into()],
        viewport: Some([640., 740.]),
        time: plico_core::FrameTime {
            now: Duration::from_secs(10),
            delta: Duration::ZERO,
        },
        ..Default::default()
    })
    .unwrap();
    assert!(frame.key_handled);
    let moved: Gallery = serde_json::from_value(frame.state.clone()).unwrap();
    assert_eq!(moved.game2048.cells()[0][..2], [4, 4]);
    assert_eq!(moved.game2048.score, 4);
    assert_eq!(moved.game2048.time, Duration::from_secs(10));
    fn board(nodes: &[plico_core::ViewNode]) -> bool {
        nodes.iter().any(|n| {
            (n.style.width == Some(340.) && n.style.height == Some(340.)) || board(&n.children)
        })
    }
    assert!(board(&frame.nodes));
    let next = plico_dev::render::<Gallery>(plico_dev::Request {
        state: frame.state,
        viewport: Some([640., 740.]),
        time: plico_core::FrameTime {
            now: Duration::from_millis(10600),
            delta: Duration::from_millis(600),
        },
        ..Default::default()
    })
    .unwrap();
    assert!(!next.key_handled);
    assert!(!next.requests.contains(plico_core::Changes::ANIMATE));
    let restored: Gallery = serde_json::from_value(next.state).unwrap();
    assert_eq!(moved.game2048.cells(), restored.game2048.cells());
    assert_eq!(moved.game2048.best, restored.game2048.best);
    // The same application handler is reached through the normal native App runner.
    let mut source = Source::new(plico_backend::NativeApp(restored));
    let mut tcx = TextContext::new();
    source.build_frame(&mut tcx, 640, 740);
    assert!(source.handle_event(key("r")).handled);
    source.build_frame(&mut tcx, 640, 740);
    assert_eq!(source.view.0.game2048.tiles.len(), 2);
    assert_eq!(source.view.0.game2048.score, 0);
}
