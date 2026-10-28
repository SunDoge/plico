//! Native children with component-generated identity.
use plico::{Engine, Entity, prelude::*};

fn find(engine: &Engine, label: &str) -> Entity {
    fn walk(engine: &Engine, id: Entity, label: &str) -> Option<Entity> {
        let node = engine.node(id).unwrap();
        if node.text == label {
            return Some(id);
        }
        node.children.iter().find_map(|id| walk(engine, *id, label))
    }
    walk(engine, engine.root(), label).unwrap()
}
#[component]
#[rustfmt::skip]
fn native(cx: &mut UiContext, extra: bool, clicks: &mut usize) {
    column(cx).gap(16).padding(20).children(|cx| {
        if extra {
            button(cx).text("Optional");
        }
        if button(cx).text("Stable").clicked() {
            *clicks += 1;
        }
        row(cx).gap(8).children(|cx| {
            text(cx, "Nested");
        });
    });
}
#[test]
fn native_closures_preserve_identity_and_queued_clicks_across_branches() {
    let mut engine = Engine::default();
    let mut clicks = 0;
    engine
        .frame_declarations(|cx| native(cx, true, &mut clicks))
        .unwrap();
    let stable = find(&engine, "Stable");
    let nested = find(&engine, "Nested");
    engine.queue_click(stable);
    engine
        .frame_declarations(|cx| native(cx, false, &mut clicks))
        .unwrap();
    assert_eq!(find(&engine, "Stable"), stable);
    assert_eq!(find(&engine, "Nested"), nested);
    assert_eq!(clicks, 1);
}
#[component]
#[rustfmt::skip]
fn renamed(cx: &mut UiContext, extra: bool) {
    column(cx).children(|inner| {
        if extra {
            button(inner).text("Optional");
        }
        button(inner).text("Stable");
    });
}
#[test]
fn renamed_context_preserves_identity() {
    let mut engine = Engine::default();
    engine.frame_declarations(|cx| renamed(cx, true)).unwrap();
    let stable = find(&engine, "Stable");
    engine.frame_declarations(|cx| renamed(cx, false)).unwrap();
    assert_eq!(find(&engine, "Stable"), stable);
}
