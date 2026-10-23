use plico_core::widgets::{column, row, text};
use plico_core::{Engine, UiContext, component, view};
#[component]
fn panel(cx: &mut UiContext) {
    let ready = true;
    view! { row(cx).gap(4.) => |cx| {
        view! { column(cx).gap(2.) => |cx| {
            text(cx, "Hello");
        }}
        text(cx, if ready { "ready" } else { "idle" });
    }}
}
#[test]
fn nested_view_statements_mount() {
    let mut engine = Engine::default();
    engine.frame(panel).unwrap();
    let root = engine.root();
    assert!(!engine.node(root).unwrap().children.is_empty());
}
