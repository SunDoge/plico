use plico_core::{Engine, Entity, prelude::*};

fn named(engine: &Engine, label: &str) -> Entity {
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
fn screen(cx: &mut UiContext, selected: &mut usize, open: &mut bool, disabled: bool) {
    column(cx).disabled(disabled).children(|cx| {
        segmented(cx, selected, &["Stable", "Beta"]);
        collapsible(cx, "Details", open, |cx| {
            text(cx, "Panel content");
        });
        badge(cx, "Ready");
        separator(cx);
    })
}
#[test]
fn selection_disclosure_identity_and_disabled_state() {
    let mut e = Engine::default();
    let (mut selected, mut open) = (0, false);
    e.frame_declarations(|cx| screen(cx, &mut selected, &mut open, false))
        .unwrap();
    let beta = named(&e, "Beta");
    let details = named(&e, "Details");
    assert_eq!(e.node(details).unwrap().style.expanded, Some(false));
    e.queue_click(beta);
    e.queue_click(details);
    e.frame_declarations(|cx| screen(cx, &mut selected, &mut open, false))
        .unwrap();
    assert_eq!(selected, 1);
    assert!(open);
    let content = named(&e, "Panel content");
    // A settling declaration updates selected/expanded semantic states.
    e.frame_declarations(|cx| screen(cx, &mut selected, &mut open, false))
        .unwrap();
    assert_eq!(named(&e, "Beta"), beta);
    assert_eq!(e.node(beta).unwrap().style.selected, Some(true));
    assert_eq!(e.node(details).unwrap().style.expanded, Some(true));
    e.queue_click(named(&e, "Stable"));
    e.queue_click(details);
    e.frame_declarations(|cx| screen(cx, &mut selected, &mut open, true))
        .unwrap();
    assert_eq!(selected, 1);
    assert!(open);
    e.queue_click(details);
    e.frame_declarations(|cx| screen(cx, &mut selected, &mut open, false))
        .unwrap();
    assert!(!open);
    assert!(e.node(content).is_none());
}
#[test]
fn empty_selection_and_invalid_progress_values_are_safe() {
    let mut e = Engine::default();
    let mut selected = 99;
    e.frame_declarations(|cx| {
        assert!(!segmented(cx, &mut selected, &[]));
        for (index, value) in [f32::NAN, f32::INFINITY, -2., 2.].into_iter().enumerate() {
            cx.keyed(index as u64, |cx| progress(cx, "Download", value));
        }
    })
    .unwrap();
    assert_eq!(selected, 99);
    named(&e, "0%");
    named(&e, "100%");
    fn fills(e: &Engine, id: Entity, widths: &mut Vec<f32>) {
        let n = e.node(id).unwrap();
        if n.style.background == Some(Color::Accent) {
            widths.push(n.style.width_percent.unwrap());
        }
        for child in n.children {
            fills(e, *child, widths);
        }
    }
    let mut widths = Vec::new();
    fills(&e, e.root(), &mut widths);
    assert_eq!(widths, [0., 0., 0., 100.]);
}
