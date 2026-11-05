//! Container arrow/Home/End traversal and Response focus reporting.
use plico::prelude::*;
use plico_renderer::{
    FrameSource, KeyEvent, KeyLocation, KeyPhase, Modifiers, SemanticAction, SemanticRole,
    TextContext, UiEvent,
};
use plico_runtime::{HostView, Source};

#[derive(Default)]
struct Demo {
    tab: usize,
    radio: usize,
    plain_focused: bool,
}

impl HostView for Demo {
    fn show(&mut self, cx: &mut UiContext) {
        tabs(cx, &mut self.tab, &["One", "Two", "Three"]);
        radio_group(cx, |cx| {
            for (index, label) in ["Alpha", "Beta", "Gamma"].iter().enumerate() {
                if radio(cx, label, self.radio == index).clicked() {
                    self.radio = index;
                }
            }
        });
        self.plain_focused = button(cx).text("Plain").response().focused();
    }
}

fn key(value: &str) -> UiEvent {
    UiEvent::Key(KeyEvent {
        phase: KeyPhase::Down,
        key: value.into(),
        code: value.into(),
        key_without_modifiers: value.into(),
        text: None,
        text_with_all_modifiers: None,
        location: KeyLocation::Standard,
        modifiers: Modifiers::empty(),
        repeat: false,
        synthetic: false,
    })
}

fn focus(source: &mut Source<Demo>, label: &str, role: SemanticRole, text: &mut TextContext) {
    let id = source
        .adapter
        .semantics(&source.engine, 0)
        .node_by_role(role, label, None)
        .unwrap_or_else(|| panic!("{label:?} has role {role:?}"))
        .id;
    source.handle_semantic_action(SemanticAction::Focus { target: id });
    source.request_rebuild();
    source.build_frame(text, 720, 400);
}

fn focused_label(source: &Source<Demo>, role: SemanticRole, target: &str) -> bool {
    let semantics = source.adapter.semantics(&source.engine, 0);
    semantics
        .node_by_role(role, target, None)
        .is_some_and(|node| semantics.focus == Some(node.id))
}

#[test]
fn tab_strip_and_radio_group_move_focus_with_arrows() {
    let mut source = Source::new(Demo::default());
    let mut text = TextContext::new();
    source.build_frame(&mut text, 720, 400);
    focus(&mut source, "One", SemanticRole::Tab, &mut text);
    source.handle_event(key("ArrowRight"));
    source.build_frame(&mut text, 720, 400);
    assert!(focused_label(&source, SemanticRole::Tab, "Two"));
    source.handle_event(key("End"));
    source.build_frame(&mut text, 720, 400);
    assert!(focused_label(&source, SemanticRole::Tab, "Three"));
    source.handle_event(key("Home"));
    source.build_frame(&mut text, 720, 400);
    assert!(focused_label(&source, SemanticRole::Tab, "One"));

    focus(&mut source, "Alpha", SemanticRole::RadioButton, &mut text);
    source.handle_event(key("ArrowDown"));
    source.build_frame(&mut text, 720, 400);
    assert!(focused_label(&source, SemanticRole::RadioButton, "Beta"));
}

#[test]
fn response_reports_backend_focus() {
    let mut source = Source::new(Demo::default());
    let mut text = TextContext::new();
    source.build_frame(&mut text, 720, 400);
    assert!(!source.view.plain_focused);
    focus(&mut source, "Plain", SemanticRole::Button, &mut text);
    assert!(source.view.plain_focused);
}
