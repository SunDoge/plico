//! MyGO's color picker: named swatches and the hue/saturation/brightness
//! sliders write an RGB literal into the caller's color.
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
fn swatches_set_an_rgb_literal_and_the_hex_readout_follows() {
    let mut color = Color::Literal(0xef4444);
    let mut engine = Engine::default();
    let mut changed = true;
    frame(&mut engine, |cx| {
        changed = color_picker(cx, &mut color);
    });
    assert!(!changed, "declaring a picker does not change the color");
    assert_eq!(color, Color::Literal(0xef4444));
    assert!(find(&engine, "#ef4444").is_some());

    engine.queue_click(named(&engine, "Blue"));
    frame(&mut engine, |cx| {
        changed = color_picker(cx, &mut color);
    });
    assert!(changed);
    assert_eq!(color, Color::Literal(0x3b82f6));
    frame(&mut engine, |cx| {
        color_picker(cx, &mut color);
    });
    assert!(find(&engine, "#3b82f6").is_some());

    // Clicking the selected swatch keeps the color and reports no change.
    engine.queue_click(named(&engine, "Blue"));
    frame(&mut engine, |cx| {
        changed = color_picker(cx, &mut color);
    });
    assert!(!changed);
    assert_eq!(color, Color::Literal(0x3b82f6));
}

#[test]
fn the_hue_slider_rebuilds_the_literal_and_external_changes_reset_the_hue() {
    let mut color = Color::Literal(0xff0000);
    let mut engine = Engine::default();
    frame(&mut engine, |cx| {
        color_picker(cx, &mut color);
    });
    let hue = named(&engine, "Hue");
    let mut changed = false;
    engine.queue_value(hue, 120.);
    frame(&mut engine, |cx| {
        changed = color_picker(cx, &mut color);
    });
    assert!(changed);
    assert_eq!(
        color,
        Color::Literal(0x00ff00),
        "hue 120 of full value is green"
    );

    // The application replaced the color: the picker derives its hue again.
    color = Color::Literal(0x0000ff);
    frame(&mut engine, |cx| {
        color_picker(cx, &mut color);
    });
    engine.queue_value(hue, 0.);
    frame(&mut engine, |cx| {
        color_picker(cx, &mut color);
    });
    assert_eq!(
        color,
        Color::Literal(0xff0000),
        "the external blue moved the picker to hue 240"
    );
}

#[test]
fn color_well_discloses_the_picker_and_reports_color_edits() {
    let mut color = Color::Literal(0xef4444);
    let mut open = false;
    let mut engine = Engine::default();
    let mut changed = false;
    let view = |cx: &mut UiContext, color: &mut Color, open: &mut bool, changed: &mut bool| {
        *changed = color_well(cx, "Tint", color, open);
    };
    frame(&mut engine, |cx| {
        view(cx, &mut color, &mut open, &mut changed)
    });
    assert!(!open);
    assert!(!changed);
    assert!(find(&engine, "Tint").is_some());
    assert!(find(&engine, "#ef4444").is_some());
    assert!(find(&engine, "Hue").is_none(), "the picker starts folded");

    engine.queue_click(named(&engine, "Tint"));
    frame(&mut engine, |cx| {
        view(cx, &mut color, &mut open, &mut changed)
    });
    assert!(open);
    assert!(find(&engine, "Hue").is_some());

    engine.queue_click(named(&engine, "Blue"));
    frame(&mut engine, |cx| {
        view(cx, &mut color, &mut open, &mut changed)
    });
    assert_eq!(color, Color::Literal(0x3b82f6));
    assert!(changed);

    engine.queue_click(named(&engine, "Tint"));
    frame(&mut engine, |cx| {
        view(cx, &mut color, &mut open, &mut changed)
    });
    assert!(!open);
    assert!(
        find(&engine, "Hue").is_none(),
        "the picker folds with the well"
    );
}
