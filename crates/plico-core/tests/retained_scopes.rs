#![cfg(feature = "experimental-scopes")]
use plico_core::{Engine, FrameError, prelude::*};

#[test]
fn shared_subtrees_preserve_identity_layout_and_other_queued_events() {
    let mut engine = Engine::default();
    let (mut a, mut b) = (None, None);
    engine
        .frame(|cx| {
            text(cx, "synchronous parent").key("parent");
            a = Some(column(cx).key("a").retain());
            b = Some(column(cx).key("b").retain());
        })
        .unwrap();
    let (a, b) = (a.unwrap(), b.unwrap());
    engine
        .frame_scope(a, |cx| {
            button(cx).text("A").key("button");
        })
        .unwrap();
    engine
        .frame_scope(b, |cx| {
            button(cx).text("B").key("button");
        })
        .unwrap();
    let button_a = engine.find(a.entity(), "button").unwrap();
    let button_b = engine.find(b.entity(), "button").unwrap();
    assert_ne!(button_a, button_b);
    engine.queue_click(button_a);
    engine.queue_click(button_b);
    engine
        .frame(|cx| {
            text(cx, "synchronous parent").key("parent");
            assert_eq!(column(cx).key("a").retain(), a);
            assert_eq!(column(cx).key("b").retain(), b);
        })
        .unwrap();
    let mut clicked = false;
    let stats = engine
        .frame_scope(a, |cx| {
            clicked = button(cx).text("A").key("button").clicked();
        })
        .unwrap();
    assert!(clicked);
    assert!(stats.visited < 5);
    engine
        .frame_scope(b, |cx| {
            clicked = button(cx).text("B").key("button").clicked();
        })
        .unwrap();
    assert!(clicked);
    assert_eq!(engine.find(a.entity(), "button"), Some(button_a));
    engine
        .frame_scope(a, |cx| {
            let _ = column(cx).height(100.) + |_cx| {};
        })
        .unwrap();
    assert!(engine.node(engine.root()).unwrap().size.height >= 100.);
    assert!(engine.node(button_b).is_some());
    engine
        .frame(|cx| {
            column(cx).key("b").retain();
        })
        .unwrap();
    assert!(!engine.scope_mounted(a));
    assert_eq!(
        engine.frame_scope(a, |_| {}),
        Err(FrameError::MissingScope { entity: a.entity() })
    );
}

#[test]
fn parent_partial_declaration_preserves_queued_nested_scope_input() {
    let mut engine = Engine::default();
    let mut parent = None;
    engine
        .frame(|cx| {
            parent = Some(column(cx).key("parent").retain());
        })
        .unwrap();
    let parent = parent.unwrap();
    let mut child = None;
    engine
        .frame_scope(parent, |cx| {
            child = Some(column(cx).key("child").retain());
        })
        .unwrap();
    let child = child.unwrap();
    engine
        .frame_scope(child, |cx| {
            button(cx).text("Child").key("click");
        })
        .unwrap();
    let button = engine.find(child.entity(), "click").unwrap();
    engine.queue_click(button);
    engine
        .frame_scope(parent, |cx| {
            column(cx).key("child").retain();
        })
        .unwrap();
    let mut clicked = false;
    engine
        .frame_scope(child, |cx| {
            clicked = plico_core::widgets::button(cx)
                .text("Child")
                .key("click")
                .clicked();
        })
        .unwrap();
    assert!(clicked);
}

#[test]
fn nested_control_validation_and_failed_order_preservation_still_apply() {
    let mut engine = Engine::default();
    let mut scope = None;
    engine
        .frame(|cx| {
            let _ = button_base(cx, "Composite").children(|cx| {
                scope = Some(column(cx).retain());
            });
        })
        .unwrap();
    assert_eq!(
        engine.frame_scope(scope.unwrap(), |cx| {
            button(cx).text("Nested");
        }),
        Err(FrameError::NestedControl)
    );
    engine
        .frame(|cx| {
            scope = Some(column(cx).key("normal").retain());
        })
        .unwrap();
    let scope = scope.unwrap();
    engine
        .frame_scope(scope, |cx| {
            text(cx, "A").key("a");
            text(cx, "B").key("b");
        })
        .unwrap();
    let old = engine.node(scope.entity()).unwrap().children.to_vec();
    assert!(
        engine
            .frame_scope(scope, |cx| {
                text(cx, "B").key("b");
                text(cx, "B").key("b");
            })
            .is_err()
    );
    assert_eq!(engine.node(scope.entity()).unwrap().children, old);
}

#[test]
fn failed_partial_mounts_do_not_leave_orphans_and_foreign_anchors_are_rejected() {
    let mut engine = Engine::default();
    let mut scope = None;
    engine
        .frame(|cx| {
            scope = Some(column(cx).retain());
        })
        .unwrap();
    let scope = scope.unwrap();
    assert!(
        engine
            .frame_scope(scope, |cx| {
                text(cx, "invalid").key("orphan").width(f32::NAN);
            })
            .is_err()
    );
    assert!(engine.find(scope.entity(), "orphan").is_none());
    engine
        .frame_scope(scope, |cx| {
            text(cx, "valid").key("valid");
        })
        .unwrap();
    assert_eq!(engine.node(scope.entity()).unwrap().children.len(), 1);
    let mut other = Engine::default();
    let mut foreign = None;
    other
        .frame(|cx| {
            foreign = Some(column(cx).retain());
        })
        .unwrap();
    assert!(!engine.scope_mounted(foreign.unwrap()));
    assert!(engine.frame_scope(foreign.unwrap(), |_| {}).is_err());
}

#[test]
fn portals_cannot_escape_partial_scope_and_panics_release_the_bound() {
    let mut engine = Engine::default();
    let mut scope = None;
    engine
        .frame(|cx| {
            scope = Some(column(cx).key("scope").retain());
        })
        .unwrap();
    let scope = scope.unwrap();
    let mut modal_state = plico_core::ModalState::default();
    modal_state.open();
    assert_eq!(
        engine.frame_scope(scope, |cx| {
            modal(cx, &mut modal_state).children(|cx| {
                text(cx, "Dialog");
            });
        }),
        Err(FrameError::ScopeEscape)
    );
    let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        engine
            .frame_scope(scope, |cx| {
                text(cx, "not committed").key("panic");
                panic!("test");
            })
            .unwrap();
    }));
    assert!(panic.is_err());
    assert!(engine.find(scope.entity(), "panic").is_none());
    engine
        .frame(|cx| {
            text(cx, "back to normal");
        })
        .unwrap();
    assert!(!engine.scope_mounted(scope));
}
