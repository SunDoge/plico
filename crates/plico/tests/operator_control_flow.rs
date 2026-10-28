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
fn parent(engine: &Engine, child: Entity) -> Option<Entity> {
    fn walk(engine: &Engine, id: Entity, child: Entity) -> Option<Entity> {
        let node = engine.node(id).unwrap();
        if node.children.contains(&child) {
            return Some(id);
        }
        node.children.iter().find_map(|id| walk(engine, *id, child))
    }
    walk(engine, engine.root(), child)
}
fn logic(value: &mut usize) {
    *value += 1;
}

#[component]
#[rustfmt::skip]
fn screen(ui: &mut UiContext, extra: bool, clicks: &mut usize, value: &mut usize) {
    // One semicolon plus a normal block must remain ordinary Rust.
    logic(value);
    {
        *value += 10;
    }
    column(ui).gap(12) + |ui| {
        if extra {
            button(ui).text("Optional");
        }
        if button(ui).text("Stable").clicked() {
            *clicks += 1;
        }
        row(ui).gap(8) + |ui| /* retained comment */ {
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
    };
}
#[test]
fn operators_preserve_parentage_identity_clicks_and_normal_rust() {
    let mut engine = Engine::default();
    let (mut clicks, mut value) = (0, 0);
    engine
        .frame_declarations(|ui| screen(ui, true, &mut clicks, &mut value))
        .unwrap();
    assert_eq!(value, 11);
    let stable = find(&engine, "Stable");
    let nested = find(&engine, "Nested");
    let row = parent(&engine, nested).unwrap();
    let column = parent(&engine, stable).unwrap();
    assert_ne!(row, column);
    assert_eq!(parent(&engine, row), Some(column));
    engine.queue_click(stable);
    engine
        .frame_declarations(|ui| screen(ui, false, &mut clicks, &mut value))
        .unwrap();
    assert_eq!(value, 22);
    assert_eq!(clicks, 1);
    assert_eq!(find(&engine, "Stable"), stable);
    assert_eq!(find(&engine, "Nested"), nested);
    assert_eq!(parent(&engine, nested), Some(row));
}

#[component]
#[rustfmt::skip]
fn mixed(cx: &mut UiContext, extra: bool) {
    column(cx).children(|inner| {
        if extra {
            text(inner, "Optional");
        }
        row(inner) + |inner| {
            text(inner, "Mixed");
        };
    });
}
#[test]
fn markers_follow_renamed_native_children_context() {
    let mut engine = Engine::default();
    engine.frame_declarations(|cx| mixed(cx, true)).unwrap();
    let node = find(&engine, "Mixed");
    let container = parent(&engine, node).unwrap();
    engine.frame_declarations(|cx| mixed(cx, false)).unwrap();
    assert_eq!(find(&engine, "Mixed"), node);
    assert_eq!(parent(&engine, node), Some(container));
}
