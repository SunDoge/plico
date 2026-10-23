use plico_core::prelude::*;
use plico_core::{Clock, VirtualClock};
use plico_renderer::{
    FrameSource, GesturePhase, Modifiers, Point, TextContext, UiEvent, WheelDeltaMode, WheelEvent,
};
use plico_vello::{Source, View};
use std::time::Duration;
struct Demo {
    count: usize,
    built: Vec<usize>,
    smooth: bool,
}
impl View for Demo {
    #[component]
    fn show(&mut self, cx: &mut UiContext) {
        self.built.clear();
        virtual_list(cx, self.count, 40.)
            .width(240.)
            .height(180.)
            .smooth_scroll(self.smooth)
            .rows(
                |i| i as u64,
                |cx, i| {
                    self.built.push(i);
                    text!(cx, "Item {i}");
                },
            );
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
fn offset(s: &Source<Demo>) -> f32 {
    s.adapter
        .placed
        .iter()
        .find(|p| p.scroll.port[3] == 180. && p.parent_node_id.is_some())
        .unwrap()
        .scroll
        .offset[1]
}
fn setup() -> (Source<Demo>, VirtualClock, TextContext) {
    let clock = VirtualClock::default();
    let mut s = Source::with_clock(
        Demo {
            count: 10_000,
            built: vec![],
            smooth: true,
        },
        clock.clone(),
    );
    let mut tcx = TextContext::new();
    s.build_frame(&mut tcx, 400, 300);
    (s, clock, tcx)
}
#[test]
fn clock_samples_visible_rows_pauses_and_finishes_exactly() {
    let (mut s, clock, mut tcx) = setup();
    assert!(s.handle_event(wheel(4000.)).handled);
    s.build_frame(&mut tcx, 400, 300);
    assert_eq!(offset(&s), 0.);
    assert!(s.has_anim());
    clock.advance(Duration::from_millis(80)).unwrap();
    s.build_frame(&mut tcx, 400, 300);
    assert_eq!(offset(&s), 3500.);
    assert_eq!(s.view.built.first(), Some(&85));
    assert!(s.adapter.placed.len() < 25);
    s.build_frame(&mut tcx, 400, 300);
    assert_eq!(offset(&s), 3500.);
    clock.advance(Duration::from_millis(80)).unwrap();
    s.build_frame(&mut tcx, 400, 300);
    assert_eq!(offset(&s), 4000.);
    assert_eq!(s.view.built.first(), Some(&98));
    assert!(!s.has_anim());
    assert_eq!(clock.now(), Duration::from_millis(160));
}
#[test]
fn successive_wheels_accumulate_and_can_reverse_without_jumping() {
    let (mut s, clock, mut tcx) = setup();
    s.handle_event(wheel(400.));
    clock.advance(Duration::from_millis(80)).unwrap();
    s.build_frame(&mut tcx, 400, 300);
    assert_eq!(offset(&s), 350.);
    s.handle_event(wheel(400.));
    s.build_frame(&mut tcx, 400, 300);
    assert_eq!(offset(&s), 350.);
    clock.advance(Duration::from_millis(160)).unwrap();
    s.build_frame(&mut tcx, 400, 300);
    assert_eq!(offset(&s), 800.);
    s.handle_event(wheel(-600.));
    clock.advance(Duration::from_millis(80)).unwrap();
    s.build_frame(&mut tcx, 400, 300);
    assert_eq!(offset(&s), 275.);
    s.handle_event(wheel(-1000.));
    clock.advance(Duration::from_millis(160)).unwrap();
    s.build_frame(&mut tcx, 400, 300);
    assert_eq!(offset(&s), 0.);
    assert!(!s.has_anim());
    assert!(!s.handle_event(wheel(-40.)).handled);
}
#[test]
fn reduced_motion_snaps_pending_target_and_future_wheels() {
    let (mut s, clock, mut tcx) = setup();
    s.handle_event(wheel(4000.));
    clock.advance(Duration::from_millis(40)).unwrap();
    s.build_frame(&mut tcx, 400, 300);
    s.set_reduced_motion(true);
    s.build_frame(&mut tcx, 400, 300);
    assert_eq!(offset(&s), 4000.);
    assert!(!s.has_anim());
    s.handle_event(wheel(200.));
    s.build_frame(&mut tcx, 400, 300);
    assert_eq!(offset(&s), 4200.);
    assert!(!s.has_anim());
}
#[test]
fn shrinking_content_cancels_out_of_range_motion() {
    let (mut s, clock, mut tcx) = setup();
    s.handle_event(wheel(4000.));
    clock.advance(Duration::from_millis(40)).unwrap();
    s.build_frame(&mut tcx, 400, 300);
    s.view.count = 1;
    s.request_rebuild();
    s.build_frame(&mut tcx, 400, 300);
    assert_eq!(offset(&s), 0.);
    assert!(!s.has_anim());
    clock.advance(Duration::from_millis(200)).unwrap();
    s.build_frame(&mut tcx, 400, 300);
    assert_eq!(offset(&s), 0.);
}

#[test]
fn grabbing_thumb_interrupts_animation_and_dragging_is_direct() {
    use plico_renderer::{PointerButton, PointerEvent, PointerPhase};
    let (mut s, clock, mut tcx) = setup();
    s.handle_event(wheel(4000.));
    clock.advance(Duration::from_millis(80)).unwrap();
    s.build_frame(&mut tcx, 400, 300);
    let port = s
        .adapter
        .placed
        .iter()
        .find(|p| p.scroll.scrollable[1])
        .unwrap();
    let thumb =
        plico_renderer::scrollbar::thumb(port, plico_renderer::scrollbar::ScrollAxis::Vertical)
            .unwrap();
    let position = Point {
        x: (thumb.x0 + thumb.x1) / 2.,
        y: (thumb.y0 + thumb.y1) / 2.,
    };
    let pointer = |phase, position| {
        UiEvent::Pointer(PointerEvent {
            phase,
            position,
            button: Some(PointerButton::Primary),
            buttons: 1,
            modifiers: Modifiers::empty(),
            properties: Default::default(),
        })
    };
    assert!(
        s.handle_event(pointer(PointerPhase::Down, position))
            .handled
    );
    s.build_frame(&mut tcx, 400, 300);
    assert_eq!(offset(&s), 3500.);
    assert!(!s.has_anim());
    assert!(
        s.handle_event(pointer(
            PointerPhase::Move,
            Point {
                y: position.y + 10.,
                ..position
            }
        ))
        .handled
    );
    s.build_frame(&mut tcx, 400, 300);
    assert!(offset(&s) > 3500.);
    assert!(!s.has_anim());
}

#[test]
fn nested_smooth_ports_chain_to_ancestors_and_settle() {
    struct Nested;
    impl View for Nested {
        #[component]
        fn show(&mut self, cx: &mut UiContext) {
            column(cx)
                .width(240.)
                .height(180.)
                .scroll_y(true)
                .smooth_scroll(true)
                .children(|cx| {
                    text(cx, "Before").height(100.).motion(Motion::None);
                    column(cx)
                        .height(120.)
                        .shrink(0.)
                        .scroll_y(true)
                        .smooth_scroll(true)
                        .children(|cx| {
                            for i in 0..10u64 {
                                cx.keyed(i, |cx| {
                                    text(cx, "Inside").height(40.).motion(Motion::None);
                                });
                            }
                        });
                    text(cx, "After").height(300.).motion(Motion::None);
                })
        }
    }
    let clock = VirtualClock::default();
    let mut s = Source::with_clock(Nested, clock.clone());
    let mut tcx = TextContext::new();
    s.build_frame(&mut tcx, 400, 300);
    let UiEvent::Wheel(mut event) = wheel(1000.) else {
        unreachable!()
    };
    event.position.y = 110.;
    assert!(s.handle_event(UiEvent::Wheel(event)).handled);
    clock.advance(Duration::from_millis(160)).unwrap();
    s.build_frame(&mut tcx, 400, 300);
    let ports: Vec<_> = s
        .adapter
        .placed
        .iter()
        .filter(|p| p.scroll.scrollable[1])
        .collect();
    assert_eq!(ports.len(), 2);
    for port in ports {
        assert_eq!(port.scroll.offset[1], port.scroll.range[1]);
    }
    assert!(!s.has_anim());
    assert_eq!(s.adapter.stats.layout_runs, 0);
}

#[test]
fn focusing_a_control_cancels_scroll_and_reveals_it_immediately() {
    use plico_renderer::{SemanticAction, SemanticRole};
    struct FocusDemo;
    impl View for FocusDemo {
        #[component]
        fn show(&mut self, cx: &mut UiContext) {
            column(cx)
                .height(180.)
                .width(240.)
                .scroll_y(true)
                .smooth_scroll(true)
                .children(|cx| {
                    for i in 0..20u64 {
                        cx.keyed(i, |cx| {
                            button(cx)
                                .text(format!("Item {i}"))
                                .height(40.)
                                .motion(Motion::None);
                        });
                    }
                })
        }
    }
    let clock = VirtualClock::default();
    let mut s = Source::with_clock(FocusDemo, clock);
    let mut tcx = TextContext::new();
    s.build_frame(&mut tcx, 400, 300);
    s.handle_event(wheel(400.));
    assert!(s.has_anim());
    let target = s
        .adapter
        .semantics(&s.engine, 0)
        .node_by_role(SemanticRole::Button, "Item 19", None)
        .unwrap()
        .id;
    assert!(s.handle_semantic_action(SemanticAction::Focus { target }));
    s.build_frame(&mut tcx, 400, 300);
    let port = s
        .adapter
        .placed
        .iter()
        .find(|p| p.scroll.scrollable[1])
        .unwrap();
    assert_eq!(port.scroll.offset[1], port.scroll.range[1]);
    assert!(!s.has_anim());
}
