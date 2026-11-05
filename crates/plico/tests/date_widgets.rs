//! MyGO's date input: an ISO date field whose popover calendar chooses a day.
use plico::prelude::*;
use plico::{CalendarDate, Engine, Entity, PopoverState};

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
fn date_input_shows_iso_and_chooses_a_day_from_its_calendar() {
    let mut date = CalendarDate::new(2026, 11, 6).unwrap();
    let mut popover = PopoverState::default();
    let mut engine = Engine::default();
    let mut changed = true;
    let view = |cx: &mut UiContext,
                date: &mut CalendarDate,
                popover: &mut PopoverState,
                changed: &mut bool| {
        *changed = date_input(cx, "Birthday", date, popover);
    };
    frame(&mut engine, |cx| {
        view(cx, &mut date, &mut popover, &mut changed)
    });
    assert!(!changed);
    assert!(find(&engine, "Birthday").is_some());
    assert!(find(&engine, "2026-11-06").is_some());
    assert!(!find(&engine, "November 2026").is_some());

    popover.open();
    frame(&mut engine, |cx| {
        view(cx, &mut date, &mut popover, &mut changed)
    });
    assert!(find(&engine, "November 2026").is_some());

    engine.queue_click(named(&engine, "7"));
    frame(&mut engine, |cx| {
        view(cx, &mut date, &mut popover, &mut changed)
    });
    assert_eq!(date.day(), 7);
    assert_eq!(date.month(), 11);
    assert!(changed, "choosing a day reports the new date");
    assert!(!popover.is_open(), "choosing a day closes the calendar");
    frame(&mut engine, |cx| {
        view(cx, &mut date, &mut popover, &mut changed)
    });
    assert!(find(&engine, "2026-11-07").is_some());

    // An application change resets the shown value and the calendar month.
    date = CalendarDate::new(2027, 1, 15).unwrap();
    frame(&mut engine, |cx| {
        view(cx, &mut date, &mut popover, &mut changed)
    });
    assert!(find(&engine, "2027-01-15").is_some());
}
