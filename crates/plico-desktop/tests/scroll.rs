use plico::prelude::*;
use plico_desktop::{HostView, Source};
use plico_renderer::{
    FrameSource, GesturePhase, Modifiers, Point, SemanticAction, SemanticRole, TextContext,
    UiEvent, WheelDeltaMode, WheelEvent,
};
struct Demo {
    count: usize,
}
impl HostView for Demo {
    #[component]
    fn show(&mut self, cx: &mut UiContext) {
        column(cx)
            .height(180.)
            .width(240.)
            .scroll_y(true)
            .children(|cx| {
                for i in 0..self.count {
                    cx.keyed(i as u64, |cx| {
                        button(cx)
                            .text(format!("Item {i}"))
                            .height(40.)
                            .motion(Motion::None);
                    });
                }
            })
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
#[test]
fn wheel_reflattens_without_taffy_and_keyboard_focus_reveals_clipped_items() {
    let mut s = Source::new(Demo { count: 20 });
    let mut tcx = TextContext::new();
    s.build_frame(&mut tcx, 400, 300);
    let scroll = s
        .adapter
        .placed
        .iter()
        .find(|p| p.scroll.scrollable[1])
        .unwrap()
        .node_id;
    assert!(s.handle_event(wheel(64.)).handled);
    s.build_frame(&mut tcx, 400, 300);
    assert_eq!(s.adapter.stats.layout_runs, 0);
    assert!(s.adapter.stats.flatten_visits > 0);
    assert_eq!(
        s.adapter
            .placed
            .iter()
            .find(|p| p.node_id == scroll)
            .unwrap()
            .scroll
            .offset[1],
        64.
    );
    let scroll_node = s
        .adapter
        .placed
        .iter()
        .find(|p| p.node_id == scroll)
        .unwrap();
    let thumb = plico_renderer::scrollbar::thumb(
        scroll_node,
        plico_renderer::scrollbar::ScrollAxis::Vertical,
    )
    .unwrap();
    let position = Point {
        x: (thumb.x0 + thumb.x1) / 2.,
        y: (thumb.y0 + thumb.y1) / 2.,
    };
    let pointer = |phase, position| {
        UiEvent::Pointer(plico_renderer::PointerEvent {
            phase,
            position,
            button: Some(plico_renderer::PointerButton::Primary),
            buttons: 1,
            modifiers: Modifiers::empty(),
            properties: Default::default(),
        })
    };
    assert!(
        s.handle_event(pointer(plico_renderer::PointerPhase::Down, position))
            .handled
    );
    s.build_frame(&mut tcx, 400, 300);
    assert!(
        s.handle_event(pointer(
            plico_renderer::PointerPhase::Move,
            Point {
                y: position.y + 20.,
                ..position
            }
        ))
        .handled
    );
    s.build_frame(&mut tcx, 400, 300);
    assert!(
        s.adapter
            .placed
            .iter()
            .find(|p| p.node_id == scroll)
            .unwrap()
            .scroll
            .offset[1]
            > 64.
    );
    s.handle_event(pointer(plico_renderer::PointerPhase::Up, position));
    s.build_frame(&mut tcx, 400, 300);
    let target = s
        .adapter
        .semantics(&s.engine, 0)
        .node_by_role(SemanticRole::Button, "Item 19", None)
        .unwrap()
        .id;
    assert!(s.handle_semantic_action(SemanticAction::Focus { target }));
    s.build_frame(&mut tcx, 400, 300);
    let last = s
        .adapter
        .placed
        .iter()
        .find(|p| p.paint.text.as_deref() == Some("Item 19"))
        .unwrap();
    assert!(last.rect[1] >= 0. && last.rect[3] <= 180.);
    s.view.count = 1;
    s.request_rebuild();
    s.build_frame(&mut tcx, 400, 300);
    let p = s
        .adapter
        .placed
        .iter()
        .find(|p| p.node_id == scroll)
        .unwrap();
    assert_eq!(p.scroll.offset, [0., 0.]);
    assert!(s.adapter.focused().is_none());
    assert!(!s.handle_event(wheel(f64::NAN)).handled);
    assert!(!s.handle_event(wheel(500.)).handled);
}

#[test]
fn nested_ports_chain_remaining_wheel_delta_to_ancestors() {
    struct Nested;
    impl HostView for Nested {
        #[component]
        fn show(&mut self, cx: &mut UiContext) {
            column(cx)
                .width(240.)
                .height(180.)
                .scroll_y(true)
                .children(|cx| {
                    button(cx).text("Before").height(100.).motion(Motion::None);
                    column(cx)
                        .height(120.)
                        .shrink(0.)
                        .scroll_y(true)
                        .children(|cx| {
                            for i in 0..10u64 {
                                cx.keyed(i, |cx| {
                                    button(cx).text("Inside").height(40.).motion(Motion::None);
                                });
                            }
                        });
                    button(cx).text("After").height(300.).motion(Motion::None);
                })
        }
    }
    let mut s = Source::new(Nested);
    let mut tcx = TextContext::new();
    s.build_frame(&mut tcx, 400, 300);
    let mut event = match wheel(1000.) {
        UiEvent::Wheel(event) => event,
        _ => unreachable!(),
    };
    event.position.y = 110.;
    assert!(s.handle_event(UiEvent::Wheel(event)).handled);
    s.build_frame(&mut tcx, 400, 300);
    let ports: Vec<_> = s
        .adapter
        .placed
        .iter()
        .filter(|p| p.scroll.scrollable[1])
        .collect();
    assert_eq!(ports.len(), 2);
    for p in ports {
        assert_eq!(p.scroll.offset[1], p.scroll.range[1]);
        assert!(p.scroll.offset[1] > 0.);
    }
    assert_eq!(s.adapter.stats.layout_runs, 0);
}
