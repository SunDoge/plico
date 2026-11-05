//! The MyGO search field: a magnifier, the editable text and a clear button
//! that reports a change.
use plico::prelude::*;
use plico::{Engine, Entity, Kind};

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

#[test]
fn search_field_clears_the_text_and_reports_the_change() {
    let mut value = String::from("invoice");
    let mut engine = Engine::default();
    let mut response = SearchResponse::default();
    frame(&mut engine, |cx| {
        response = search_field(cx, "Search mail", "Search", &mut value);
    });
    assert!(!response.changed());
    assert!(!response.submitted());
    assert_eq!(value, "invoice");
    let clear = find(&engine, "Clear search").expect("a filled field shows the clear button");
    engine.queue_click(clear);
    frame(&mut engine, |cx| {
        response = search_field(cx, "Search mail", "Search", &mut value);
    });
    assert!(value.is_empty(), "the clear button empties the field");
    assert!(response.changed(), "clearing is reported like typing");
}

#[test]
fn search_field_hides_the_clear_button_when_empty() {
    let mut value = String::new();
    let mut engine = Engine::default();
    frame(&mut engine, |cx| {
        search_field(cx, "Search pages", "Search pages", &mut value);
    });
    assert!(find(&engine, "Clear search").is_none());
    let input = find(&engine, "Search pages").expect("the label names the input");
    assert_eq!(engine.node(input).unwrap().kind, Kind::TextInput);
}
