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

// A configurable application container needs no Widget implementation.
fn surface<'u, 'e>(cx: &'u mut UiContext<'e>) -> Layout<'u, 'e> {
    column(cx).background(Color::Panel).padding(16)
}

#[component]
fn action(cx: &mut UiContext, label: &str) -> Response {
    button_base(cx, label)
        .children(|cx| {
            text(cx, label);
        })
        .response
}

#[component]
fn example(cx: &mut UiContext, extra: bool, enabled: &mut bool, clicks: &mut usize) {
    if extra {
        text(cx, "optional root");
    }
    surface(cx).gap(8).children(|cx| {
        if extra {
            button(cx).text("optional");
        }
        if button(cx).text("stable").primary().clicked() {
            *clicks += 1;
        }
        switch(cx, "Enabled", enabled);
        if extra {
            action(cx, "optional action");
        }
        action(cx, "stable action");
        row(cx).gap(4).children(|cx| {
            text(cx, "nested").weight(600);
        })
    })
}

#[test]
fn context_first_constructors_and_custom_functions_keep_identity_and_events() {
    let mut engine = Engine::default();
    let mut enabled = false;
    let mut clicks = 0;
    engine
        .frame_declarations(|cx| example(cx, true, &mut enabled, &mut clicks))
        .unwrap();
    let stable = find(&engine, "stable");
    let action = find(&engine, "stable action");
    let nested = find(&engine, "nested");
    let toggle = find(&engine, "Enabled");
    engine.queue_click(stable);
    engine.queue_click(toggle);
    engine
        .frame_declarations(|cx| example(cx, false, &mut enabled, &mut clicks))
        .unwrap();
    assert_eq!(find(&engine, "stable"), stable);
    assert_eq!(find(&engine, "stable action"), action);
    assert_eq!(find(&engine, "nested"), nested);
    assert_eq!(find(&engine, "Enabled"), toggle);
    assert!(enabled);
    assert_eq!(clicks, 1);
}

#[test]
fn response_uses_configuration_applied_after_construction() {
    let mut engine = Engine::default();
    #[component]
    fn screen(cx: &mut UiContext, disabled: bool, clicked: &mut bool) {
        *clicked = button(cx).text("action").disabled(disabled).clicked();
    }
    let mut clicked = false;
    engine
        .frame_declarations(|cx| screen(cx, false, &mut clicked))
        .unwrap();
    engine.queue_click(find(&engine, "action"));
    engine
        .frame_declarations(|cx| screen(cx, true, &mut clicked))
        .unwrap();
    assert!(!clicked);
    assert!(engine.node(find(&engine, "action")).unwrap().style.disabled);
}

#[test]
fn standalone_view_reborrows_context_for_function_constructors() {
    let mut engine = Engine::default();
    engine
        .frame_declarations(|cx| {
            let _ = surface(cx).gap(4).children(|inner| {
                text(inner, "borrowed label");
                checkbox(inner, "check", &mut false);
            });
        })
        .unwrap();
    find(&engine, "borrowed label");
}
