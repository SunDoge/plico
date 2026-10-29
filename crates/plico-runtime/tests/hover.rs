use plico::prelude::*;
use plico_renderer::{
    FrameSource, Point, PointerButton, PointerEvent, PointerPhase, PointerProperties,
    SemanticAction, SemanticRole, TextContext, UiEvent,
};
use plico_runtime::{Source, View};
#[derive(Default)]
struct Demo {
    hovered: bool,
    clicks: u32,
    clicked_hovered: bool,
    disabled: bool,
}
impl View for Demo {
    #[component]
    fn show(&mut self, cx: &mut UiContext) {
        let response = button(cx)
            .text("Action")
            .width(160.)
            .height(40.)
            .disabled(self.disabled)
            .response();
        self.hovered = response.hovered();
        assert_eq!(response.hovered(), response.hovered());
        if response.clicked() {
            self.clicks += 1;
            self.clicked_hovered = response.hovered();
        }
        text!(cx, "Hovered: {}", response.hovered());
    }
}
fn pointer(phase: PointerPhase) -> UiEvent {
    UiEvent::Pointer(PointerEvent {
        phase,
        position: Point { x: 20., y: 20. },
        button: matches!(phase, PointerPhase::Down | PointerPhase::Up)
            .then_some(PointerButton::Primary),
        buttons: u32::from(phase == PointerPhase::Down),
        modifiers: Modifiers::empty(),
        properties: PointerProperties::default(),
    })
}
#[test]
fn pointer_hover_and_click_are_independent_borrow_free_response_values() {
    let mut source = Source::new(Demo::default());
    let mut text = TextContext::new();
    source.build_frame(&mut text, 300, 200);
    assert!(!source.view.hovered);
    source.handle_event(pointer(PointerPhase::Enter));
    source.build_frame(&mut text, 300, 200);
    assert!(source.view.hovered);
    assert_eq!(source.view.clicks, 0);
    source.handle_event(pointer(PointerPhase::Down));
    let (builds, checks, pruned) = source.adapter.experiment_hit_cache_stats();
    assert!(builds > 0);
    assert!(checks < source.adapter.placed.len(), "checks={checks}");
    assert!(pruned > 0);
    source.handle_event(pointer(PointerPhase::Up));
    assert_eq!(source.adapter.experiment_hit_cache_stats().0, builds);
    source.build_frame(&mut text, 300, 200);
    assert_eq!(source.view.clicks, 1);
    assert!(source.view.clicked_hovered);
    source.handle_event(pointer(PointerPhase::Leave));
    source.build_frame(&mut text, 300, 200);
    assert!(!source.view.hovered);
    let target = source
        .adapter
        .semantics(&source.engine, 0)
        .node_by_role(SemanticRole::Button, "Action", None)
        .unwrap()
        .id;
    source.handle_semantic_action(SemanticAction::Click { target });
    source.build_frame(&mut text, 300, 200);
    assert_eq!(source.view.clicks, 2);
    assert!(!source.view.clicked_hovered);
    source.view.disabled = true;
    source.request_rebuild();
    source.build_frame(&mut text, 300, 200);
    source.handle_event(pointer(PointerPhase::Move));
    source.build_frame(&mut text, 300, 200);
    assert!(!source.view.hovered);
    assert_eq!(source.view.clicks, 2);
}
