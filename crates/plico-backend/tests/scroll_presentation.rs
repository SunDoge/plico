use plico_backend::{Source, View};
use plico_core::prelude::*;
use plico_core::{Clock, VirtualClock};
use plico_renderer::{
    FrameSource, GesturePhase, Modifiers, Point, PointerButton, PointerEvent, PointerPhase, Scene,
    TextContext, UiEvent, WheelDeltaMode, WheelEvent,
};
use std::time::Duration;

#[derive(Default)]
struct Demo {
    calls: usize,
    observe: bool,
    virtualized: bool,
    direct: bool,
    first: usize,
    clicks: usize,
    last_clicked: Option<usize>,
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
                .smooth_scroll(!self.direct)
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
                .smooth_scroll(!self.direct)
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
                            self.last_clicked = Some(i);
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
fn click(source: &mut Source<Demo>, text: &mut TextContext) {
    let rect = source
        .adapter
        .placed
        .iter()
        .find(|node| node.paint.text.as_deref() == Some("Row 20"))
        .unwrap()
        .rect;
    let position = Point {
        x: f64::from((rect[0] + rect[2]) * 0.5),
        y: f64::from((rect[1] + rect[3]) * 0.5),
    };
    for phase in [PointerPhase::Down, PointerPhase::Up] {
        source.prepare_for_event(text);
        assert!(
            source
                .handle_event(UiEvent::Pointer(PointerEvent {
                    phase,
                    position,
                    button: Some(PointerButton::Primary),
                    buttons: u32::from(phase == PointerPhase::Down),
                    modifiers: Modifiers::empty(),
                    properties: Default::default(),
                }))
                .handled
        );
    }
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
    click(&mut source, &mut text);
    sample(&mut source, &mut text);
    assert_eq!(source.view.clicks, 1);
    assert_eq!(source.view.last_clicked, Some(20));
}
#[test]
fn repeated_wheel_events_only_redeclare_when_scroll_is_observed() {
    for direct in [false, true] {
        for observe in [false, true] {
            let clock = VirtualClock::default();
            let mut source = Source::with_clock(
                Demo {
                    direct,
                    observe,
                    ..Default::default()
                },
                clock.clone(),
            );
            let mut text = TextContext::new();
            sample(&mut source, &mut text);
            let initial_calls = source.view.calls;
            for _ in 0..8 {
                clock.advance(Duration::from_millis(16)).unwrap();
                source.prepare_for_event(&mut text);
                let calls = source.view.calls;
                assert!(source.handle_event(wheel(48.)).request_redraw);
                sample(&mut source, &mut text);
                assert_eq!(source.view.calls - calls, usize::from(observe && direct));
                assert_eq!(source.adapter.stats.layout_runs, 0);
            }
            if !observe {
                assert_eq!(source.view.calls, initial_calls);
            }
            clock.advance(Duration::from_millis(200)).unwrap();
            sample(&mut source, &mut text);
            assert_eq!(offset(&source), 384.);
            assert!(!source.has_anim());
            sample(&mut source, &mut text);
            assert_eq!(source.adapter.stats.scene_reuses, 1);
        }
    }
}
#[test]
fn smooth_virtual_wheel_retarget_does_not_declare_twice_at_the_same_position() {
    let (mut source, clock, mut text) = setup(false, true);
    clock.advance(Duration::from_millis(80)).unwrap();
    source.prepare_for_event(&mut text);
    assert_eq!(source.view.first, 15);
    let calls = source.view.calls;
    assert!(source.handle_event(wheel(48.)).request_redraw);
    sample(&mut source, &mut text);
    assert_eq!(source.view.calls, calls);
    assert_eq!(source.view.first, 15);
    clock.advance(Duration::from_millis(160)).unwrap();
    sample(&mut source, &mut text);
    assert_eq!(source.view.calls, calls + 1);
    assert_eq!(offset(&source), 528.);
    assert_eq!(source.view.first, 20);
}
#[test]
fn direct_scroll_prepares_click_geometry_and_preserves_external_updates() {
    let clock = VirtualClock::default();
    let mut source = Source::with_clock(
        Demo {
            direct: true,
            ..Default::default()
        },
        clock,
    );
    let mut text = TextContext::new();
    sample(&mut source, &mut text);
    let calls = source.view.calls;
    source.handle_event(wheel(480.));
    source.prepare_for_event(&mut text);
    assert_eq!(source.view.calls, calls);
    assert_eq!(offset(&source), 480.);
    click(&mut source, &mut text);
    sample(&mut source, &mut text);
    assert_eq!(source.view.clicks, 1);
    assert_eq!(source.view.last_clicked, Some(20));
    source.update(|view| view.clicks += 10);
    source.handle_event(wheel(48.));
    let calls = source.view.calls;
    sample(&mut source, &mut text);
    assert!(source.view.calls > calls);
    assert_eq!(source.view.clicks, 11);
}
#[test]
fn direct_virtual_scroll_mounts_rows_before_the_next_event() {
    let mut source = Source::with_clock(
        Demo {
            direct: true,
            virtualized: true,
            ..Default::default()
        },
        VirtualClock::default(),
    );
    let mut text = TextContext::new();
    sample(&mut source, &mut text);
    let calls = source.view.calls;
    source.handle_event(wheel(480.));
    source.prepare_for_event(&mut text);
    assert_eq!(source.view.calls, calls + 1);
    assert_eq!(source.view.first, 18);
    assert_eq!(offset(&source), 480.);
    assert!(source.adapter.placed.len() < 50);
    sample(&mut source, &mut text);
    assert_eq!(source.view.calls, calls + 1);
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
    let calls = source.view.calls;
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
    assert_eq!(source.view.calls, calls);
}
