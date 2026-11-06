//! The dual-thumb range slider over the shared slider core.
use plico::prelude::*;
use plico::{Engine, Entity, Kind};

fn frame(engine: &mut Engine, view: impl FnOnce(&mut UiContext<'_>)) {
    engine.frame_declarations(view).unwrap();
}

fn slider(engine: &Engine) -> Entity {
    fn walk(engine: &Engine, id: Entity, found: &mut Option<Entity>) {
        let node = engine.node(id).unwrap();
        if node.kind == Kind::Slider {
            *found = Some(id);
        }
        let children = node.children.to_vec();
        for child in children {
            walk(engine, child, found);
        }
    }
    let mut found = None;
    walk(engine, engine.root(), &mut found);
    found.expect("a slider node")
}

#[test]
fn range_slider_reports_both_ends_and_clamps_them() {
    let mut low = 20.0;
    let mut high = 80.0;
    let mut engine = Engine::default();
    let mut changed = false;
    let view = |cx: &mut UiContext, low: &mut f64, high: &mut f64, changed: &mut bool| {
        *changed = range_slider(cx, "Price", low, high, 0.0..=100.0);
    };
    frame(&mut engine, |cx| {
        view(cx, &mut low, &mut high, &mut changed)
    });
    let node = slider(&engine);
    let numeric = engine.node(node).unwrap().numeric.unwrap();
    assert_eq!(numeric.value, 20.);
    assert_eq!(numeric.high, Some(80.));
    assert!(!changed);

    engine.queue_high_value(node, 65.);
    frame(&mut engine, |cx| {
        view(cx, &mut low, &mut high, &mut changed)
    });
    assert_eq!(high, 65.);
    assert!(changed);

    engine.queue_high_value(node, 10.);
    frame(&mut engine, |cx| {
        view(cx, &mut low, &mut high, &mut changed)
    });
    assert_eq!(high, 20., "the high thumb cannot cross the low one");

    engine.queue_value(node, 90.);
    frame(&mut engine, |cx| {
        view(cx, &mut low, &mut high, &mut changed)
    });
    assert_eq!(low, 20., "the high bound still caps the low thumb");
    engine.queue_value(node, 10.);
    frame(&mut engine, |cx| {
        view(cx, &mut low, &mut high, &mut changed)
    });
    assert_eq!(low, 10.);
}
