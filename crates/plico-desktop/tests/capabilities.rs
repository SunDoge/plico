use plico::prelude::*;
use plico_desktop::{HostView, Source};
use plico_renderer::{
    FrameSource, KeyEvent, KeyLocation, KeyPhase, Modifiers, SemanticAction, SemanticRole,
    TextContext, UiEvent,
};
#[derive(Default)]
struct Demo {
    input: TextBuffer,
    value: f64,
    clicks: usize,
    checked: bool,
}
impl HostView for Demo {
    #[component]
    fn show(&mut self, cx: &mut UiContext) {
        column(cx).children(|cx| {
            text(cx, "Not a tab stop");
            text_input(cx, "Editor", &self.input);
            slider(cx, "Value", &mut self.value, 0.0..=10.0);
            checkbox(cx, "Check", &mut self.checked);
            if button(cx).text("Action").clicked() {
                self.clicks += 1;
            }
            button(cx).text("Disabled").disabled(true);
        });
    }
}
fn tab() -> UiEvent {
    UiEvent::Key(KeyEvent {
        phase: KeyPhase::Down,
        key: "Tab".into(),
        code: "Tab".into(),
        key_without_modifiers: "Tab".into(),
        text: None,
        text_with_all_modifiers: None,
        location: KeyLocation::Standard,
        modifiers: Modifiers::empty(),
        repeat: false,
        synthetic: false,
    })
}
#[test]
fn tab_navigation_and_assistive_activation_use_distinct_capabilities() {
    let mut source = Source::new(Demo::default());
    let mut text = TextContext::new();
    source.build_frame(&mut text, 400, 400);
    let semantics = source.adapter.semantics(&source.engine, 0);
    let ids = [
        (SemanticRole::TextInput, "Editor"),
        (SemanticRole::Slider, "Value"),
        (SemanticRole::CheckBox, "Check"),
        (SemanticRole::Button, "Action"),
    ]
    .map(|(role, label)| semantics.node_by_role(role, label, None).unwrap().id);
    for id in ids.into_iter().chain([ids[0]]) {
        assert!(source.handle_event(tab()).handled);
        source.build_frame(&mut text, 400, 400);
        assert_eq!(source.adapter.semantics(&source.engine, 0).focus, Some(id));
    }
    // A non-activatable editor/slider can be focused by an assistive click,
    // without emitting an application activation or changing its value.
    for id in [ids[0], ids[1]] {
        assert!(source.handle_semantic_action(SemanticAction::Click { target: id }));
        source.build_frame(&mut text, 400, 400);
        assert_eq!(source.adapter.semantics(&source.engine, 0).focus, Some(id));
        assert_eq!(source.view.clicks, 0);
        assert_eq!(source.view.value, 0.);
    }
    assert!(source.handle_semantic_action(SemanticAction::Click { target: ids[2] }));
    source.build_frame(&mut text, 400, 400);
    assert!(source.view.checked);
    assert!(source.handle_semantic_action(SemanticAction::Click { target: ids[3] }));
    source.build_frame(&mut text, 400, 400);
    assert_eq!(source.view.clicks, 1);
}

#[test]
fn programmatic_dialog_focus_does_not_require_a_tab_stop() {
    struct EmptyDialog {
        state: ModalState,
    }
    impl HostView for EmptyDialog {
        #[component]
        fn show(&mut self, cx: &mut UiContext) {
            modal(cx, &mut self.state).title("Empty").children(|cx| {
                text(cx, "Read-only content");
            });
        }
    }
    let mut state = ModalState::default();
    state.open();
    let mut source = Source::new(EmptyDialog { state });
    let mut text = TextContext::new();
    source.build_frame(&mut text, 400, 400);
    let snapshot = source.adapter.semantics(&source.engine, 0);
    let dialog = snapshot
        .node_by_role(SemanticRole::Dialog, "Empty", None)
        .unwrap()
        .id;
    let close = snapshot
        .node_by_role(SemanticRole::Button, "Close dialog", None)
        .unwrap()
        .id;
    assert!(source.handle_semantic_action(SemanticAction::Focus { target: dialog }));
    source.build_frame(&mut text, 400, 400);
    assert_eq!(
        source.adapter.semantics(&source.engine, 0).focus,
        Some(dialog)
    );
    // The dialog itself accepts programmatic focus, but Tab enters its actual
    // control (the built-in close button), not the dialog or read-only label.
    for _ in 0..2 {
        source.handle_event(tab());
        source.build_frame(&mut text, 400, 400);
        assert_eq!(
            source.adapter.semantics(&source.engine, 0).focus,
            Some(close)
        );
    }
}
