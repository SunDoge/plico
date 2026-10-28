#[allow(dead_code)]
#[path = "support/icons.rs"]
mod icons;
use plico::prelude::*;
use plico_desktop::{Source, View};
use plico_renderer::{FrameSource, SemanticAction, SemanticRole, TextContext};

#[derive(Default)]
struct Demo {
    clicks: usize,
}
impl View for Demo {
    #[component]
    #[rustfmt::skip]
    fn show(&mut self, cx: &mut UiContext) {
        column(cx).gap(8) + |cx| {
            if button(cx).icon(&icons::SAVE).text("Save").primary().clicked() {
                self.clicks += 1;
            }
            button(cx).icon(&icons::SETTINGS).aria_label("Settings");
            button(cx).disabled(true) + |cx| {
                icon(cx, &icons::LOCK);
                text(cx, "Locked");
            };
        };
    }
}

#[test]
fn composite_buttons_expose_names_and_route_activation_to_parent() {
    let mut source = Source::new(Demo::default());
    let mut text = TextContext::new();
    source.build_frame(&mut text, 400, 300);
    let snapshot = source.adapter.semantics(&source.engine, 0);
    let save = snapshot
        .node_by_role(SemanticRole::Button, "Save", None)
        .unwrap();
    assert_eq!(save.children.len(), 2);
    assert!(!save.disabled);
    assert!(
        snapshot
            .node_by_role(SemanticRole::Button, "Settings", None)
            .is_some()
    );
    let locked = snapshot
        .node_by_role(SemanticRole::Button, "Locked", None)
        .unwrap();
    assert!(locked.disabled);
    assert!(source.handle_semantic_action(SemanticAction::Click { target: save.id }));
    source.build_frame(&mut text, 400, 300);
    assert_eq!(source.view.clicks, 1);
}
