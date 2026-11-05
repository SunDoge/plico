//! MyGO's find bar: query field, match status, previous/next and Done.
use plico::prelude::*;
use plico::{Engine, Entity};

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
fn find_bar_moves_round_the_matches_and_reports_changes() {
    let mut open = true;
    let mut query = String::from("needle");
    let mut current = 0;
    let mut engine = Engine::default();
    let mut changed = true;
    let view = |cx: &mut UiContext,
                open: &mut bool,
                query: &mut String,
                current: &mut usize,
                changed: &mut bool| {
        *changed = find_bar(cx, open, query, 3, current).changed();
    };
    frame(&mut engine, |cx| {
        view(cx, &mut open, &mut query, &mut current, &mut changed)
    });
    assert!(!changed);
    assert!(find(&engine, "Find").is_some());
    assert!(find(&engine, "1 of 3").is_some());

    engine.queue_click(named(&engine, "Next match"));
    frame(&mut engine, |cx| {
        view(cx, &mut open, &mut query, &mut current, &mut changed)
    });
    assert_eq!(current, 1);
    assert!(changed);
    frame(&mut engine, |cx| {
        view(cx, &mut open, &mut query, &mut current, &mut changed)
    });
    assert!(find(&engine, "2 of 3").is_some());

    // The previous button wraps from the first match to the last.
    current = 0;
    engine.queue_click(named(&engine, "Previous match"));
    frame(&mut engine, |cx| {
        view(cx, &mut open, &mut query, &mut current, &mut changed)
    });
    assert_eq!(current, 2);
    assert!(changed);
}

#[test]
fn find_bar_shows_no_matches_and_clears_the_status_for_an_empty_query() {
    let mut open = true;
    let mut query = String::from("missing");
    let mut current = 4;
    let mut engine = Engine::default();
    frame(&mut engine, |cx| {
        find_bar(cx, &mut open, &mut query, 0, &mut current);
    });
    assert_eq!(current, 0, "no matches clamps the current index");
    assert!(find(&engine, "No matches").is_some());
    assert!(
        engine
            .node(named(&engine, "Next match"))
            .unwrap()
            .style
            .disabled
    );

    query.clear();
    frame(&mut engine, |cx| {
        find_bar(cx, &mut open, &mut query, 0, &mut current);
    });
    assert!(find(&engine, "No matches").is_none());
}

#[test]
fn find_bar_closes_with_done_and_renders_nothing_when_closed() {
    let mut open = true;
    let mut query = String::from("needle");
    let mut current = 0;
    let mut engine = Engine::default();
    frame(&mut engine, |cx| {
        find_bar(cx, &mut open, &mut query, 3, &mut current);
    });
    assert!(find(&engine, "Find").is_some());
    engine.queue_click(named(&engine, "Done"));
    frame(&mut engine, |cx| {
        find_bar(cx, &mut open, &mut query, 3, &mut current);
    });
    assert!(!open);
    // The closing frame already declared the field; the next declaration
    // removes it.
    frame(&mut engine, |cx| {
        find_bar(cx, &mut open, &mut query, 3, &mut current);
    });
    assert!(find(&engine, "Find").is_none());
}
