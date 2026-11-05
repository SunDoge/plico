use plico::{Engine, Entity, Kind, prelude::*};
fn find(engine: &Engine, root: Entity, name: &str) -> Option<Entity> {
    let node = engine.node(root)?;
    if node.kind == Kind::ButtonBase && engine.accessible_name(root).as_deref() == Some(name) {
        return Some(root);
    }
    node.children.iter().find_map(|&id| find(engine, id, name))
}
#[component]
fn form(cx: &mut UiContext, description: bool, clicks: &mut usize) -> i32 {
    let section = settings_section(cx, "Account");
    let section = if description {
        section.description("Your account preferences.")
    } else {
        section
    };
    (section
        + |cx| {
            (settings_row(cx, "Connection").description("Choose how to connect.")
                + |cx| {
                    if button(cx).text("Connect").clicked() {
                        *clicks += 1;
                    }
                    42
                })
            .inner
        })
    .inner
}
#[test]
fn descriptions_and_resizing_preserve_control_identity_and_return_values() {
    let mut engine = Engine::default();
    let mut clicks = 0;
    engine.set_viewport(1000., 800.).unwrap();
    engine
        .frame(|cx| assert_eq!(form(cx, true, &mut clicks), 42))
        .unwrap();
    let button = find(&engine, engine.root(), "Connect").unwrap();
    engine.queue_click(button);
    engine.set_viewport(480., 800.).unwrap();
    engine
        .frame(|cx| assert_eq!(form(cx, false, &mut clicks), 42))
        .unwrap();
    assert_eq!(find(&engine, engine.root(), "Connect"), Some(button));
    assert_eq!(clicks, 1);
    engine
        .frame(|cx| {
            form(cx, false, &mut clicks);
        })
        .unwrap();
    assert!(engine.changes().is_empty());
}
