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
        .semantics()
        .node_by_role(role, label, None)
        .unwrap_or_else(|| panic!("{label:?} has role {role:?}"))
        .id;
    source.handle_semantic_action(SemanticAction::Focus { target: id });
    source.request_rebuild();
    source.build_frame(text, 720, 400);
}

fn focused_label(source: &Source<Demo>, role: SemanticRole, target: &str) -> bool {
    let semantics = source.semantics();
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
    assert!(!source.view().plain_focused);
    focus(&mut source, "Plain", SemanticRole::Button, &mut text);
    assert!(source.view().plain_focused);
}

#[test]
fn semantic_click_activates_an_unselected_roving_tab() {
    let mut source = Source::new(Demo::default());
    let mut text = TextContext::new();
    source.build_frame(&mut text, 720, 400);
    let target = source
        .semantics()
        .node_by_role(SemanticRole::Tab, "Two", None)
        .unwrap()
        .id;
    assert!(source.handle_semantic_action(SemanticAction::Click { target }));
    source.build_frame(&mut text, 720, 400);
    assert_eq!(source.view().tab, 1);
}

#[test]
fn accessibility_reveals_an_offscreen_label_through_nested_ports_without_focusing_it() {
    struct Scroller;
    impl HostView for Scroller {
        fn show(&mut self, cx: &mut UiContext) {
            column(cx).height(120.).scroll_y(true).children(|cx| {
                button(cx).text("First").height(180.).shrink(0.);
                column(cx)
                    .height(100.)
                    .shrink(0.)
                    .scroll_y(true)
                    .children(|cx| {
                        text(cx, "Spacer").height(180.).shrink(0.);
                        text(cx, "Last").height(40.).shrink(0.);
                    });
            });
        }
    }
    let mut source = Source::new(Scroller);
    let mut text = TextContext::new();
    let mut scene = plico_renderer::Scene::new();
    source
        .render_frame(&mut scene, &mut text, (400, 300), 1.)
        .unwrap();
    let before = source.semantics();
    let first = before
        .node_by_role(SemanticRole::Button, "First", None)
        .unwrap()
        .id;
    let last = before
        .node_by_role(SemanticRole::Label, "Last", None)
        .unwrap();
    assert!(last.bounds[1] >= 120.);
    let target = last.id;
    assert!(source.handle_semantic_action(SemanticAction::Focus { target: first }));
    source
        .render_frame(&mut scene, &mut text, (400, 300), 1.)
        .unwrap();
    for _ in 0..2 {
        assert!(source.handle_semantic_action(SemanticAction::ScrollIntoView { target }));
        source
            .render_frame(&mut scene, &mut text, (400, 300), 1.)
            .unwrap();
        let snapshot = source.semantics();
        let last = snapshot
            .node_by_role(SemanticRole::Label, "Last", None)
            .unwrap();
        assert_eq!([last.bounds[1], last.bounds[3]], [80., 120.]);
        assert_eq!(snapshot.focus, Some(first));
    }
    for _ in 0..2 {
        assert!(source.handle_semantic_action(SemanticAction::ScrollIntoView { target: first }));
        source
            .render_frame(&mut scene, &mut text, (400, 300), 1.)
            .unwrap();
        assert_eq!(
            source
                .semantics()
                .node_by_role(SemanticRole::Button, "First", None)
                .unwrap()
                .bounds[1],
            0.
        );
    }
}
