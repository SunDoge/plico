use plico_backend::{Source, View};
use plico_core::prelude::*;
use plico_core::{Clock, VirtualClock};
use plico_renderer::{
    FrameSource, GesturePhase, Modifiers, Point, Scene, SemanticAction, SemanticRole, TextContext,
    UiEvent, WheelDeltaMode, WheelEvent,
};
use std::time::Duration;

#[derive(Default)]
struct Demo {
    calls: usize,
    observe: bool,
    virtualized: bool,
    first: usize,
    clicks: usize,
}
impl View for Demo {
    fn show(&mut self, cx: &mut UiContext) {
        self.calls += 1;
        if self.virtualized {
            let mut first = None;
            virtual_list(cx, 10_000, 24.)
                .key("port")
                .width(300.)
                .height(240.)
                .smooth_scroll(true)
                .rows(
                    |i| i,
                    |cx, i| {
                        first.get_or_insert(i);
                        text!(cx, "Row {i}");
                    },
                );
            self.first = first.unwrap();
        } else {
            column(cx)
                .key("port")
                .width(300.)
                .height(240.)
                .scroll_y(true)
                .smooth_scroll(true)
                .children(|cx| {
                    if self.observe {
                        let _ = cx.scroll_viewport();
                    }
                    for i in 0..1000usize {
                        if button(cx)
                            .text(format!("Row {i}"))
                            .key(i)
                            .height(24.)
                            .motion(Motion::None)
                            .clicked()
                        {
                            self.clicks += 1;
                        }
                    }
                });
        }
    }
}
fn wheel(delta: f64) -> UiEvent {
    UiEvent::Wheel(WheelEvent {
        position: Point { x: 20., y: 20. },
        delta_x: 0.,
        delta_y: delta,
        delta_mode: WheelDeltaMode::Pixel,
        phase: GesturePhase::Changed,
        modifiers: Modifiers::empty(),
    })
}
fn sample(s: &mut Source<Demo>, text: &mut TextContext) {
    s.render_retained_frame(&mut Scene::new(), text, (600, 400), 1.)
        .unwrap();
    assert!(s.last_error.is_none());
}
fn setup(observe: bool, virtualized: bool) -> (Source<Demo>, VirtualClock, TextContext) {
    let clock = VirtualClock::default();
    let mut source = Source::with_clock(
        Demo {
            observe,
            virtualized,
            ..Default::default()
        },
        clock.clone(),
    );
    let mut text = TextContext::new();
    sample(&mut source, &mut text);
    assert!(source.handle_event(wheel(480.)).handled);
    sample(&mut source, &mut text);
    (source, clock, text)
}
fn offset(source: &Source<Demo>) -> f32 {
    source
        .adapter
        .placed
        .iter()
        .find(|p| p.scroll.scrollable[1])
        .unwrap()
        .scroll
        .offset[1]
}
#[test]
fn ordinary_scroll_moves_geometry_without_redeclaring_or_relayout() {
    let (mut source, clock, mut text) = setup(false, false);
    let calls = source.view.calls;
    let before = source.batch_stats();
    for _ in 0..10 {
        clock.advance(Duration::from_millis(16)).unwrap();
        sample(&mut source, &mut text);
        assert_eq!(source.view.calls, calls);
        assert_eq!(source.adapter.stats.layout_runs, 0);
        assert!(source.adapter.stats.flatten_visits > 0);
    }
    assert_eq!(offset(&source), 480.);
    assert!(!source.has_anim());
    assert_eq!(
        source.batch_stats().declaration_batches,
        before.declaration_batches
    );
    sample(&mut source, &mut text);
    assert_eq!(source.adapter.stats.scene_reuses, 1);
    // Target the actually presented row, not a stale pre-scroll position.
    let id = source
        .adapter
        .semantics(&source.engine, 0)
        .node_by_role(SemanticRole::Button, "Row 20", None)
        .unwrap()
        .id;
    assert!(source.handle_semantic_action(SemanticAction::Click { target: id }));
    sample(&mut source, &mut text);
    assert_eq!(source.view.clicks, 1);
}
#[test]
fn reading_scroll_geometry_invalidates_and_stops_when_no_longer_read() {
    let (mut source, clock, mut text) = setup(true, false);
    let calls = source.view.calls;
    clock.advance(Duration::from_millis(80)).unwrap();
    sample(&mut source, &mut text);
    assert_eq!(source.view.calls, calls + 1);
    assert_eq!(offset(&source), 420.);
    source.update(|view| view.observe = false);
    sample(&mut source, &mut text);
    let calls = source.view.calls;
    clock.advance(Duration::from_millis(80)).unwrap();
    sample(&mut source, &mut text);
    assert_eq!(source.view.calls, calls);
    assert_eq!(offset(&source), 480.);
}
#[test]
fn virtual_rows_are_declared_at_each_presented_position() {
    let (mut source, clock, mut text) = setup(false, true);
    let calls = source.view.calls;
    clock.advance(Duration::from_millis(80)).unwrap();
    sample(&mut source, &mut text);
    assert_eq!(source.view.calls, calls + 1);
    assert_eq!(source.view.first, 15); // floor(420 / 24) - overscan(2)
    assert!(source.adapter.placed.len() < 50);
    clock.advance(Duration::from_millis(80)).unwrap();
    sample(&mut source, &mut text);
    assert_eq!(source.view.first, 18);
    assert_eq!(offset(&source), 480.);
}
#[test]
fn event_preparation_commits_current_scroll_geometry_without_declaration() {
    let (mut source, clock, mut text) = setup(false, false);
    let calls = source.view.calls;
    clock.advance(Duration::from_millis(80)).unwrap();
    source.prepare_for_event(&mut text);
    assert_eq!(source.view.calls, calls);
    assert_eq!(offset(&source), 420.);
    assert_eq!(source.engine.time().now, clock.now());
}

#[test]
fn initial_missing_feedback_is_still_observed_and_observation_is_container_local() {
    let mut engine = plico_core::Engine::default();
    engine
        .frame_declarations(|cx| {
            column(cx).key("watched").scroll_y(true).children(|cx| {
                assert!(cx.scroll_viewport().is_none());
            });
            column(cx).key("plain").scroll_y(true).children(|_| {});
        })
        .unwrap();
    let watched = engine.find(engine.root(), "watched").unwrap();
    let plain = engine.find(engine.root(), "plain").unwrap();
    assert!(engine.observes_scroll(watched));
    assert!(!engine.observes_scroll(plain));
    engine
        .frame_declarations(|cx| {
            column(cx).key("watched").scroll_y(true).children(|_| {});
            column(cx).key("plain").scroll_y(true).children(|_| {});
        })
        .unwrap();
    assert!(!engine.observes_scroll(watched));
}

#[test]
fn final_scroll_position_is_presented_even_when_an_event_hits_nothing() {
    let (mut source, clock, mut text) = setup(false, false);
    clock.advance(Duration::from_millis(160)).unwrap();
    source.prepare_for_event(&mut text);
    assert_eq!(offset(&source), 480.);
    assert!(!source.has_anim());
    let response = source.handle_event(UiEvent::Pointer(plico_renderer::PointerEvent {
        phase: plico_renderer::PointerPhase::Move,
        position: Point { x: 550., y: 350. },
        button: None,
        buttons: 0,
        modifiers: Modifiers::empty(),
        properties: Default::default(),
    }));
    assert!(response.request_redraw);
    sample(&mut source, &mut text);
    assert_eq!(source.adapter.stats.scene_builds, 1);
}
