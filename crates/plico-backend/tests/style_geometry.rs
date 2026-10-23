use plico_backend::{Source, View};
use plico_core::{VirtualClock, prelude::*};
use plico_renderer::{
    FrameSource, Modifiers, Point, PointerButton, PointerEvent, PointerPhase, SemanticAction,
    SemanticRole, TextContext, UiEvent,
};
use std::{sync::Arc, time::Duration};
use vello_common::peniko::Color as PaintColor;

#[derive(Default)]
struct Demo {
    roomy: bool,
    locked: bool,
    calls: usize,
}
impl View for Demo {
    #[component]
    fn show(&mut self, cx: &mut UiContext) {
        self.calls += 1;
        column(cx)
            .padding_x(if self.roomy { 30 } else { 10 })
            .padding_y(6)
            .border(2., Color::Border)
            .motion(Motion::Smooth)
            .children(|cx| {
                button(cx)
                    .text("Action")
                    .motion(Motion::None)
                    .disabled(self.locked);
            });
        column(cx)
            .width(80.)
            .height(24.)
            .clip(true)
            .radius(6.)
            .children(|cx| {
                text(cx, "Clipped content").height(100.);
            })
    }
}
fn pointer(phase: PointerPhase) -> UiEvent {
    UiEvent::Pointer(PointerEvent {
        phase,
        position: Point { x: 24., y: 18. },
        button: Some(PointerButton::Primary),
        buttons: u32::from(phase == PointerPhase::Down),
        modifiers: Modifiers::empty(),
        properties: Default::default(),
    })
}
fn rgb(value: u32) -> PaintColor {
    PaintColor::from_rgb8((value >> 16) as u8, (value >> 8) as u8, value as u8)
}
#[test]
fn borders_padding_clips_and_keyboard_only_focus_are_real_backend_properties() {
    let mut s = Source::new(Demo::default());
    let mut tcx = TextContext::new();
    let nodes = s.build_frame(&mut tcx, 400, 300);
    let action = nodes
        .iter()
        .find(|n| n.paint.border.is_some_and(|(width, _)| width == 1.))
        .unwrap();
    assert_eq!(action.rect[0], 12.);
    assert_eq!(action.rect[1], 8.);
    assert_eq!(action.paint.border, Some((1., rgb(s.view.theme().border))));
    let clipped = nodes
        .iter()
        .find(|n| n.paint.text.as_deref() == Some("Clipped content"))
        .unwrap();
    assert!(clipped.clip.is_some());
    s.handle_event(pointer(PointerPhase::Down));
    let nodes = s.build_frame(&mut tcx, 400, 300);
    let action = nodes
        .iter()
        .find(|n| n.paint.border.is_some_and(|(width, _)| width == 1.))
        .unwrap();
    assert_eq!(
        action.paint.background,
        Some(rgb(s.view.theme().control_pressed))
    );
    assert_eq!(action.paint.outline_width, 0.);
    s.handle_event(pointer(PointerPhase::Up));
    s.build_frame(&mut tcx, 400, 300);
    let id = s
        .adapter
        .semantics(&s.engine, 0)
        .node_by_role(SemanticRole::Button, "Action", None)
        .unwrap()
        .id;
    assert!(s.handle_semantic_action(SemanticAction::Focus { target: id }));
    let nodes = s.build_frame(&mut tcx, 400, 300);
    let action = nodes
        .iter()
        .find(|n| n.paint.border.is_some_and(|(width, _)| width == 1.))
        .unwrap();
    assert_eq!(action.paint.outline_width, 2.);
    assert_eq!(action.paint.outline_color, Some(rgb(s.view.theme().focus)));
    s.view.locked = true;
    s.build_frame(&mut tcx, 400, 300);
    assert!(s.adapter.focused().is_none());
    assert!(!s.handle_semantic_action(SemanticAction::Click { target: id }));
}

#[test]
fn axis_padding_animates_with_virtual_clock_without_redeclaring_view() {
    let clock = Arc::new(VirtualClock::default());
    let mut s = Source::with_clock(Demo::default(), clock.clone());
    let mut tcx = TextContext::new();
    s.build_frame(&mut tcx, 400, 300);
    s.view.roomy = true;
    s.request_rebuild();
    s.build_frame(&mut tcx, 400, 300);
    let calls = s.view.calls;
    clock.advance(Duration::from_millis(120)).unwrap();
    let nodes = s.build_frame(&mut tcx, 400, 300);
    let action = nodes
        .iter()
        .find(|n| n.paint.border.is_some_and(|(width, _)| width == 1.))
        .unwrap();
    assert!(action.rect[0] > 12. && action.rect[0] < 32.);
    assert_eq!(s.view.calls, calls);
    clock.advance(Duration::from_millis(120)).unwrap();
    let nodes = s.build_frame(&mut tcx, 400, 300);
    let action = nodes
        .iter()
        .find(|n| n.paint.border.is_some_and(|(width, _)| width == 1.))
        .unwrap();
    assert_eq!(action.rect[0], 32.);
    assert_eq!(s.view.calls, calls);
}
