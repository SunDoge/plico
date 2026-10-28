#[allow(dead_code)]
#[path = "support/icons.rs"]
mod icons;
use plico::{Engine, Entity, FrameError, Kind, prelude::*};

fn root_child(engine: &Engine, index: usize) -> Entity {
    engine.node(engine.root()).unwrap().children[index]
}

#[component]
fn shortcuts(cx: &mut UiContext, with_icon: bool, disabled: bool, clicks: &mut usize) {
    let mut save = button(cx).text("Save").primary().disabled(disabled);
    if with_icon {
        save = save.icon(&icons::SAVE);
    }
    if save.clicked() {
        *clicks += 1;
    }
    button(cx).icon(&icons::SETTINGS).aria_label("Settings");
}

#[test]
fn shortcuts_are_children_and_keep_identity_when_icon_changes() {
    let mut engine = Engine::default();
    let mut clicks = 0;
    engine
        .frame_declarations(|cx| shortcuts(cx, false, false, &mut clicks))
        .unwrap();
    let save = root_child(&engine, 0);
    let label = engine.node(save).unwrap().children[0];
    assert_eq!(engine.node(save).unwrap().kind, Kind::ButtonBase);
    engine.queue_click(save);
    engine
        .frame_declarations(|cx| shortcuts(cx, true, false, &mut clicks))
        .unwrap();
    assert_eq!(clicks, 1);
    assert_eq!(root_child(&engine, 0), save);
    let children = engine.node(save).unwrap().children;
    assert_eq!(children[1], label);
    assert_eq!(engine.node(children[0]).unwrap().kind, Kind::Svg);
    for child in children {
        assert_eq!(
            engine.node(*child).unwrap().style.foreground,
            Color::OnAccent
        );
    }
    assert_eq!(engine.accessible_name(save).as_deref(), Some("Save"));
    let settings = root_child(&engine, 1);
    assert_eq!(
        engine.accessible_name(settings).as_deref(),
        Some("Settings")
    );
    assert_eq!(engine.node(settings).unwrap().children.len(), 1);
    engine.queue_click(save);
    engine
        .frame_declarations(|cx| shortcuts(cx, true, true, &mut clicks))
        .unwrap();
    assert_eq!(clicks, 1);
    for child in engine.node(save).unwrap().children {
        assert!(engine.node(*child).unwrap().style.disabled);
    }
    engine
        .frame_declarations(|cx| shortcuts(cx, true, true, &mut clicks))
        .unwrap();
    assert!(engine.changes().is_empty());
}

#[component]
#[rustfmt::skip]
fn custom(cx: &mut UiContext) {
    button(cx).primary() + |cx| {
        icon(cx, &icons::SAVE);
        column(cx) + |cx| {
            text(cx, "Save");
            text(cx, "document");
        };
    };
    button(cx).aria_label("Explicit name").children(|cx| {
        text(cx, "Visible name");
    });
}

#[test]
fn custom_children_derive_accessible_name_and_support_override() {
    let mut engine = Engine::default();
    engine.frame_declarations(custom).unwrap();
    let first = root_child(&engine, 0);
    assert_eq!(
        engine.accessible_name(first).as_deref(),
        Some("Save document")
    );
    assert_eq!(engine.node(first).unwrap().children.len(), 2);
    assert_eq!(
        engine.accessible_name(root_child(&engine, 1)).as_deref(),
        Some("Explicit name")
    );
}

#[test]
fn custom_content_retains_return_value_and_rejects_nested_controls() {
    let mut engine = Engine::default();
    engine
        .frame_declarations(|cx| {
            let result = button(cx).children(|cx| {
                text(cx, "Answer");
                42
            });
            assert_eq!(result.inner, 42);
            assert!(result.response.enabled());
        })
        .unwrap();
    let error = engine
        .frame_declarations(|cx| {
            button(cx).children(|cx| {
                button(cx).text("Nested control");
            });
        })
        .unwrap_err();
    assert!(matches!(error, FrameError::NestedControl));
}
