//! Native operator children with component-generated identity.
use plico_core::{Engine, Entity, prelude::*};

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
    column(cx).gap(16).padding(20) + |cx| {
        if extra {
            button(cx).text("Optional");
        }
        if button(cx).text("Stable").clicked() {
            *clicks += 1;
        }
        row(cx).gap(8) + |cx| {
            text(cx, "Nested");
        };
    };
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
    column(cx) + |inner| {
        if extra {
            button(inner).text("Optional");
        }
        button(inner).text("Stable");
    };
}
#[test]
fn renamed_context_preserves_identity() {
    let mut engine = Engine::default();
    engine.frame_declarations(|cx| renamed(cx, true)).unwrap();
    let stable = find(&engine, "Stable");
    engine.frame_declarations(|cx| renamed(cx, false)).unwrap();
    assert_eq!(find(&engine, "Stable"), stable);
}

#[test]
fn operator_preserves_fn_once_captures_and_the_closure_return_value() {
    let mut engine = Engine::default();
    engine
        .frame_declarations(|cx| {
            let owned = String::from("owned");
            let result = column(cx)
                + |cx| {
                    text(cx, owned.as_str());
                    owned
                };
            assert_eq!(result.inner, "owned");
            text(cx, "Sibling after consumed builder");
        })
        .unwrap();
    assert_eq!(engine.node(engine.root()).unwrap().children.len(), 2);
}

fn panel<'u, 'e>(cx: &'u mut UiContext<'e>) -> plico_core::Layout<'u, 'e> {
    column(cx).padding(12).gap(8)
}
#[component]
#[rustfmt::skip]
fn custom_panel(cx: &mut UiContext, extra: bool) {
    panel(cx) + |child| {
        if extra {
            button(child).text("Optional");
        }
        text(child, "Custom panel stable");
    };
}
#[test]
fn custom_builder_factory_keeps_its_borrow_and_child_identity() {
    let mut engine = Engine::default();
    engine
        .frame_declarations(|cx| custom_panel(cx, true))
        .unwrap();
    let stable = find(&engine, "Custom panel stable");
    engine
        .frame_declarations(|cx| custom_panel(cx, false))
        .unwrap();
    assert_eq!(find(&engine, "Custom panel stable"), stable);
}

#[component]
fn all_container_responses(
    cx: &mut UiContext,
    state: &mut plico_core::ModalState,
    popup: &mut plico_core::PopoverState,
) {
    let button = button(cx).primary()
        + |cx| {
            text(cx, "Save");
            7
        };
    assert_eq!(button.inner, 7);
    let base = button_base(cx, "Base")
        + |cx| {
            text(cx, "Content");
            8
        };
    assert_eq!(base.inner, 8);
    let tip = tooltip(cx, "Tip")
        + |cx| {
            text(cx, "Help");
            9
        };
    assert_eq!(tip.inner, 9);
    let dialog = modal(cx, state).title("Dialog")
        + |cx| {
            text(cx, "Dialog body");
            10
        };
    assert_eq!(dialog.inner, Some(10));
    let popup = popover(cx, popup, "Options")
        + |cx| {
            text(cx, "Popup body");
            11
        };
    assert_eq!(popup.inner, None); // Closed popup doesn't invoke its children.
}
#[test]
fn all_container_operators_preserve_their_real_response_types() {
    let mut engine = Engine::default();
    let mut state = plico_core::ModalState::default();
    state.open();
    let mut popup = plico_core::PopoverState::default();
    engine
        .frame_declarations(|cx| all_container_responses(cx, &mut state, &mut popup))
        .unwrap();
}
