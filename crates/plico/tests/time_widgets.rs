//! MyGO's time input, ported as a validated `HH:MM` text field.
use plico::prelude::*;
use plico::{Engine, Entity, InputEdit};

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

fn text(engine: &Engine, label: &str) -> String {
    engine
        .input(named(engine, label))
        .expect("the labelled input exists")
        .0
        .text()
        .unwrap_or_default()
}

#[test]
fn time_of_day_validates_formats_and_deserializes() {
    let time = TimeOfDay::new(9, 5).unwrap();
    assert_eq!(time.to_string(), "09:05");
    assert_eq!(time.minutes(), 9 * 60 + 5);
    assert_eq!(TimeOfDay::from_minutes(545), Some(time));
    assert_eq!(TimeOfDay::new(24, 0), None);
    assert_eq!(TimeOfDay::new(23, 60), None);
    assert_eq!(TimeOfDay::from_minutes(1440), None);
    let json = serde_json::to_string(&time).unwrap();
    assert_eq!(json, r#"{"hour":9,"minute":5}"#);
    assert_eq!(serde_json::from_str::<TimeOfDay>(&json).unwrap(), time);
    assert!(serde_json::from_str::<TimeOfDay>(r#"{"hour":24,"minute":0}"#).is_err());
}

#[test]
fn time_input_syncs_external_values_and_parses_typing() {
    let mut time = TimeOfDay::new(9, 5).unwrap();
    let mut engine = Engine::default();
    let mut changed = false;
    let view = |cx: &mut UiContext, time: &mut TimeOfDay, changed: &mut bool| {
        *changed = time_input(cx, "Clock", time);
    };
    frame(&mut engine, |cx| view(cx, &mut time, &mut changed));
    assert!(!changed);
    assert_eq!(text(&engine, "Clock"), "09:05");

    // An application change rewrites the field without reporting a user edit.
    time = TimeOfDay::new(10, 30).unwrap();
    frame(&mut engine, |cx| view(cx, &mut time, &mut changed));
    assert!(!changed);
    assert_eq!(text(&engine, "Clock"), "10:30");

    // Typing a valid time applies as soon as it parses.
    let buffer = engine.input(named(&engine, "Clock")).unwrap().0.clone();
    buffer.edit(InputEdit::SelectAll);
    buffer.edit(InputEdit::Insert("15:45".to_owned().into()));
    frame(&mut engine, |cx| view(cx, &mut time, &mut changed));
    assert_eq!(time, TimeOfDay::new(15, 45).unwrap());
    assert!(changed);

    // Invalid text stays visible and leaves the value alone.
    buffer.edit(InputEdit::SelectAll);
    buffer.edit(InputEdit::Insert("25:00".to_owned().into()));
    frame(&mut engine, |cx| view(cx, &mut time, &mut changed));
    assert_eq!(time, TimeOfDay::new(15, 45).unwrap());
    assert!(!changed);
    assert_eq!(text(&engine, "Clock"), "25:00");

    // Enter with invalid text restores the current value in canonical form.
    buffer.edit(InputEdit::Submit);
    frame(&mut engine, |cx| view(cx, &mut time, &mut changed));
    assert_eq!(time, TimeOfDay::new(15, 45).unwrap());
    assert_eq!(text(&engine, "Clock"), "15:45");
}
