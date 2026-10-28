use plico::{Engine, Entity, Layout, prelude::*};
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
fn parent(engine: &Engine, child: Entity) -> Option<Entity> {
    fn walk(engine: &Engine, node: Entity, child: Entity) -> Option<Entity> {
        let current = engine.node(node).unwrap();
        if current.children.contains(&child) {
            return Some(node);
        }
        current
            .children
            .iter()
            .find_map(|node| walk(engine, *node, child))
    }
    walk(engine, engine.root(), child)
}
fn surface<'u, 'e>(cx: &'u mut UiContext<'e>) -> Layout<'u, 'e> {
    column(cx).padding(12)
}
macro_rules! opaque {
    ($cx:ident; row($arg:ident) { untouched }) => {
        text($cx, "Opaque");
    };
}
#[component]
#[rustfmt::skip]
fn screen(ui: &mut UiContext, extra: bool, clicks: &mut usize, runs: &mut usize) {
    *runs += 1;
    column(ui).gap(12) + |ui| {
        if extra {
            button(ui).text("Optional");
        }
        if button(ui).text("Stable").clicked() {
            *clicks += 1;
        }
        surface(ui).gap(8) + |ui| {
            text(ui, "Nested");
        };
        match extra {
            true => {
                text(ui, "Expanded");
            }
            false => {
                text(ui, "Compact");
            }
        }
        opaque!(ui; row(ui) { untouched });
        macro_rules! inner_opaque {
            ($cx:ident; row($arg:ident) { untouched }) => {
                text($cx, "Inner opaque");
            };
        }
        inner_opaque! { ui; row(ui) { untouched } }
    };
}
#[component]
fn decorated(cx: &mut UiContext) -> Response {
    button_base(cx, "Decorated")
        .children(|cx| {
            text(cx, "Decoration");
        })
        .response
}
#[test]
fn component_children_preserve_identity_events_and_other_macro_tokens() {
    let mut engine = Engine::default();
    let (mut clicks, mut runs) = (0, 0);
    engine
        .frame_declarations(|cx| {
            screen(cx, true, &mut clicks, &mut runs);
            decorated(cx);
        })
        .unwrap();
    let stable = find(&engine, "Stable");
    let nested = find(&engine, "Nested");
    let container = parent(&engine, nested).unwrap();
    let outer = parent(&engine, stable).unwrap();
    assert_ne!(container, outer);
    assert_eq!(parent(&engine, container), Some(outer));
    find(&engine, "Opaque");
    engine.queue_click(stable);
    engine
        .frame_declarations(|cx| {
            screen(cx, false, &mut clicks, &mut runs);
            decorated(cx);
        })
        .unwrap();
    assert_eq!(find(&engine, "Stable"), stable);
    assert_eq!(find(&engine, "Nested"), nested);
    find(&engine, "Opaque"); // Opaque macro tokens survived; its expansion has no injected identity.
    assert_eq!(clicks, 1);
    assert_eq!(runs, 2);
}
#[test]
fn native_children_allow_explicit_context_bindings() {
    let mut engine = Engine::default();
    engine
        .frame_declarations(|cx| {
            row(cx).children(|cx| {
                text(cx, "Native");
            });
            column(cx).children(|inner| {
                text(inner, "Explicit");
            });
            cx.column().gap(8).children(|cx| {
                cx.row().children(|cx| {
                    cx.text("Methods");
                });
            });
        })
        .unwrap();
    assert_ne!(find(&engine, "Native"), find(&engine, "Explicit"));
    find(&engine, "Methods");
}
