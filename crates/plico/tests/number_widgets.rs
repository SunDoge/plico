//! MyGO's number input: bounded text field with step buttons and canonical Enter.
use plico::prelude::*;
use plico::{Engine, Entity, InputEdit, InputNavigation};

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

fn subtree_has_text(engine: &Engine, id: Entity, label: &str) -> bool {
    let node = engine.node(id).unwrap();
    node.text == label
        || node
            .children
            .iter()
            .any(|child| subtree_has_text(engine, *child, label))
}

fn button(engine: &Engine, label: &str) -> Entity {
    fn walk(engine: &Engine, id: Entity, label: &str, found: &mut Option<Entity>) {
        let node = engine.node(id).unwrap();
        if matches!(node.kind, plico::Kind::Button | plico::Kind::ButtonBase)
            && subtree_has_text(engine, id, label)
        {
            *found = Some(id);
            return;
        }
        let children = node.children.to_vec();
        for child in children {
            walk(engine, child, label, found);
        }
    }
    let mut found = None;
    walk(engine, engine.root(), label, &mut found);
    found.unwrap_or_else(|| panic!("no button containing {label:?}"))
}

fn text(engine: &Engine, label: &str) -> String {
    engine
        .input(named(engine, label))
        .expect("the labelled input exists")
        .0
        .text()
        .unwrap_or_default()
}

#[test]
fn number_input_steps_parses_and_clamps() {
    let mut value = 5.0;
    let mut engine = Engine::default();
    let mut changed = false;
    let view = |cx: &mut UiContext, value: &mut f64, changed: &mut bool| {
        *changed = number_input(cx, "Copies", value, 1., 99., 1.);
    };
    frame(&mut engine, |cx| view(cx, &mut value, &mut changed));
    assert_eq!(text(&engine, "Copies"), "5");
    assert!(!changed);

    engine.queue_click(button(&engine, "+"));
    frame(&mut engine, |cx| view(cx, &mut value, &mut changed));
    assert_eq!(value, 6.);
    assert!(changed);
    assert_eq!(text(&engine, "Copies"), "6");

    engine.queue_click(button(&engine, "−"));
    frame(&mut engine, |cx| view(cx, &mut value, &mut changed));
    assert_eq!(value, 5.);

    let buffer = engine.input(named(&engine, "Copies")).unwrap().0.clone();
    buffer.edit(InputEdit::SelectAll);
    buffer.edit(InputEdit::Insert("12".to_owned().into()));
    frame(&mut engine, |cx| view(cx, &mut value, &mut changed));
    assert_eq!(value, 12.);
    assert!(changed);

    // Out-of-range text stays visible and leaves the value alone.
    buffer.edit(InputEdit::SelectAll);
    buffer.edit(InputEdit::Insert("200".to_owned().into()));
    frame(&mut engine, |cx| view(cx, &mut value, &mut changed));
    assert_eq!(value, 12.);
    assert!(!changed);
    assert_eq!(text(&engine, "Copies"), "200");

    buffer.edit(InputEdit::Submit);
    frame(&mut engine, |cx| view(cx, &mut value, &mut changed));
    assert_eq!(value, 12.);
    assert_eq!(text(&engine, "Copies"), "12");
}

#[test]
fn number_input_formats_fractional_steps() {
    let mut value = 0.5;
    let mut engine = Engine::default();
    let mut changed = false;
    let view = |cx: &mut UiContext, value: &mut f64, changed: &mut bool| {
        *changed = number_input(cx, "Opacity", value, 0., 1., 0.05);
    };
    frame(&mut engine, |cx| view(cx, &mut value, &mut changed));
    assert_eq!(text(&engine, "Opacity"), "0.5");
    engine.queue_click(button(&engine, "+"));
    frame(&mut engine, |cx| view(cx, &mut value, &mut changed));
    assert_eq!(text(&engine, "Opacity"), "0.55");
    assert!((value - 0.55).abs() < 1e-9);

    let buffer = engine.input(named(&engine, "Opacity")).unwrap().0.clone();
    buffer.edit(InputEdit::SelectAll);
    buffer.edit(InputEdit::Insert("0.03".to_owned().into()));
    frame(&mut engine, |cx| view(cx, &mut value, &mut changed));
    assert!((value - 0.03).abs() < 1e-9);
    buffer.edit(InputEdit::Submit);
    frame(&mut engine, |cx| view(cx, &mut value, &mut changed));
    assert_eq!(text(&engine, "Opacity"), "0.03");
}

#[test]
fn number_input_steps_with_arrows_formats_on_blur_and_disables_bound_buttons() {
    let mut value = 5.0;
    let mut engine = Engine::default();
    let mut changed = false;
    let view = |cx: &mut UiContext, value: &mut f64, changed: &mut bool| {
        *changed = number_input(cx, "Count", value, 0., 10., 0.5);
    };
    frame(&mut engine, |cx| view(cx, &mut value, &mut changed));
    let input = named(&engine, "Count");
    let buffer = engine.input(input).unwrap().0.clone();

    engine.set_focused(Some(input));
    frame(&mut engine, |cx| view(cx, &mut value, &mut changed));
    buffer.edit(InputEdit::Navigate(InputNavigation::Previous));
    frame(&mut engine, |cx| view(cx, &mut value, &mut changed));
    assert_eq!(value, 5.5);
    assert!(changed);
    assert_eq!(text(&engine, "Count"), "5.5");

    buffer.edit(InputEdit::SelectAll);
    buffer.edit(InputEdit::Insert("not a number".to_owned().into()));
    frame(&mut engine, |cx| view(cx, &mut value, &mut changed));
    assert_eq!(text(&engine, "Count"), "not a number");
    assert_eq!(value, 5.5);
    engine.set_focused(None);
    frame(&mut engine, |cx| view(cx, &mut value, &mut changed));
    assert_eq!(text(&engine, "Count"), "5.5");

    value = 10.;
    frame(&mut engine, |cx| view(cx, &mut value, &mut changed));
    assert!(engine.node(button(&engine, "+")).unwrap().style.disabled);
    assert!(!engine.node(button(&engine, "−")).unwrap().style.disabled);
    value = 0.;
    frame(&mut engine, |cx| view(cx, &mut value, &mut changed));
    assert!(engine.node(button(&engine, "−")).unwrap().style.disabled);
    assert!(!engine.node(button(&engine, "+")).unwrap().style.disabled);
}

#[test]
fn number_input_edit_state_survives_sibling_insertion_and_resets_on_unmount() {
    fn view(cx: &mut UiContext, prefix: bool, visible: bool, value: &mut f64) {
        if prefix {
            plico::widgets::text(cx, "Extra sibling");
        }
        if visible {
            number_input(cx, "Copies", value, 1., 99., 1.);
        }
    }
    let mut engine = Engine::default();
    let mut value = 5.;
    frame(&mut engine, |cx| view(cx, false, true, &mut value));
    let buffer = engine.input(named(&engine, "Copies")).unwrap().0.clone();
    buffer.edit(InputEdit::SelectAll);
    buffer.edit(InputEdit::Insert("invalid".to_owned().into()));
    frame(&mut engine, |cx| view(cx, true, true, &mut value));
    assert_eq!(text(&engine, "Copies"), "invalid");
    frame(&mut engine, |cx| view(cx, true, false, &mut value));
    frame(&mut engine, |cx| view(cx, true, true, &mut value));
    assert_eq!(text(&engine, "Copies"), "5");
}

#[test]
fn number_input_rejects_nonfinite_bounds_and_recovers_nonfinite_values() {
    let mut engine = Engine::default();
    let mut value = f64::NAN;
    frame(&mut engine, |cx| {
        number_input(cx, "Copies", &mut value, 1., 99., 1.);
    });
    assert_eq!(value, 1.);
    frame(&mut engine, |cx| {
        assert!(!number_input(cx, "Copies", &mut value, f64::NAN, 99., 1.));
    });
    assert_eq!(value, 1.);
}
