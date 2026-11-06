//! MyGO's alert dialog: title, message and buttons; the last button is the
//! default, and the call returns the chosen index.
use plico::prelude::*;
use plico::{Engine, Entity, ModalState, WidgetRole};

fn frame(engine: &mut Engine, view: impl FnOnce(&mut UiContext<'_>)) {
    engine.frame_declarations(view).unwrap();
}

fn find(engine: &Engine, label: &str) -> Option<Entity> {
    fn walk(engine: &Engine, id: Entity, label: &str, found: &mut Option<Entity>) {
        let node = engine.node(id).unwrap();
        if node.text == label && found.is_none() {
            *found = Some(id);
        }
        let children = node.children.to_vec();
        for child in children {
            walk(engine, child, label, found);
        }
    }
    let mut found = None;
    walk(engine, engine.root(), label, &mut found);
    found
}

fn named(engine: &Engine, label: &str) -> Entity {
    find(engine, label).unwrap_or_else(|| panic!("no node named {label:?}"))
}

#[test]
fn alert_dialog_returns_the_chosen_button_and_carries_the_alert_role() {
    let mut state = ModalState::default();
    let mut engine = Engine::default();
    let mut chosen = -1;
    let view = |cx: &mut UiContext, state: &mut ModalState, chosen: &mut i32| {
        let result = alert_dialog(
            cx,
            state,
            "Delete \u{201c}Notes\u{201d}?",
            "You can't undo this.",
            &["Cancel", "Delete"],
        );
        if result >= 0 {
            *chosen = result;
        }
    };
    frame(&mut engine, |cx| view(cx, &mut state, &mut chosen));
    assert_eq!(chosen, -1, "a closed alert reports nothing");
    assert!(find(&engine, "Delete \u{201c}Notes\u{201d}?").is_none());

    state.open();
    frame(&mut engine, |cx| view(cx, &mut state, &mut chosen));
    assert_eq!(chosen, -1);
    assert!(find(&engine, "You can't undo this.").is_some());
    let panel = engine
        .node(named(&engine, "Delete \u{201c}Notes\u{201d}?"))
        .unwrap();
    assert_eq!(panel.style.role, Some(WidgetRole::AlertDialog));
    assert_eq!(
        engine
            .node(named(&engine, "Delete"))
            .unwrap()
            .style
            .background,
        Some(Color::Accent),
        "the last button is the default"
    );

    engine.queue_click(named(&engine, "Delete"));
    frame(&mut engine, |cx| view(cx, &mut state, &mut chosen));
    assert_eq!(chosen, 1);
    assert!(!state.is_open(), "choosing a button closes the alert");
}
