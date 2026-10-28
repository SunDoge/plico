#![deny(unused_variables)]
use plico::{Engine, prelude::*};

#[component]
fn empty(cx: &mut UiContext) {}

#[component]
#[rustfmt::skip]
fn empty_children(cx: &mut UiContext) {
    column(cx) + |_cx| {};
    column(cx).children(|inner| {
        row(inner) + |_inner| {};
    });
}

#[test]
fn generated_context_bindings_do_not_warn_when_content_is_empty() {
    let mut engine = Engine::default();
    engine.frame_declarations(empty).unwrap();
    engine.frame_declarations(empty_children).unwrap();
    assert_eq!(engine.node(engine.root()).unwrap().children.len(), 2);
}
