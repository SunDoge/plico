use plico::prelude::*;
use plico_renderer::{
    FrameSource, Modifiers, Point, PointerEvent, PointerPhase, SemanticAction, TextContext, UiEvent,
};
use plico_runtime::{HostView, Source};
#[derive(Default)]
struct Activity {
    count: usize,
}
impl HostView for Activity {
    fn show(&mut self, cx: &mut UiContext) {
        button(cx).text("Action");
    }
    fn on_user_activity(&mut self) {
        self.count += 1;
    }
}
#[test]
fn user_input_and_accessibility_actions_count_but_frames_and_hover_do_not() {
    let mut source = Source::new(Activity::default());
    source.set_semantics_enabled(true);
    let mut text = TextContext::new();
    source.build_frame(&mut text, 400, 300);
    source.build_frame(&mut text, 400, 300);
    let pointer = |phase, buttons| {
        UiEvent::Pointer(PointerEvent {
            phase,
            position: Point { x: 2., y: 2. },
            button: None,
            buttons,
            modifiers: Modifiers::empty(),
            properties: Default::default(),
        })
    };
    source.handle_event(pointer(PointerPhase::Move, 0));
    source.handle_event(UiEvent::Focus(true));
    assert_eq!(source.view.count, 0);
    source.handle_event(pointer(PointerPhase::Down, 1));
    source.handle_event(pointer(PointerPhase::Move, 1));
    source.handle_event(UiEvent::TextInput("synthetic".into()));
    assert_eq!(source.view.count, 3);
    let tree = source.semantic_snapshot().unwrap();
    let target = tree
        .nodes
        .iter()
        .find(|node| node.label.as_deref() == Some("Action"))
        .unwrap()
        .id;
    assert!(source.handle_semantic_action(SemanticAction::Click { target }));
    assert_eq!(source.view.count, 4);
    source.build_frame(&mut text, 400, 300);
    assert_eq!(source.view.count, 4);
}
