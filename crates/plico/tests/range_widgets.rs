//! The dual-thumb range slider over the shared slider core.
use plico::prelude::*;
use plico::{Engine, Entity, Kind, SliderThumb, WidgetRole};

fn frame(engine: &mut Engine, view: impl FnOnce(&mut UiContext<'_>)) {
    engine.frame_declarations(view).unwrap();
}

fn slider(engine: &Engine) -> Entity {
    fn walk(engine: &Engine, id: Entity, found: &mut Option<Entity>) {
        let node = engine.node(id).unwrap();
        if node.kind == Kind::ValueControl
            && node.style.role == Some(WidgetRole::Group)
            && node.numeric.is_some_and(|range| range.high.is_some())
        {
            *found = Some(id);
        }
        let children = node.children.to_vec();
        for child in children {
            walk(engine, child, found);
        }
    }
    let mut found = None;
    walk(engine, engine.root(), &mut found);
    found.expect("a range slider group")
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
    let thumbs = engine.node(node).unwrap().children;
    assert_eq!(thumbs.len(), 2);
    assert_eq!(
        engine.node(thumbs[0]).unwrap().style.slider_thumb,
        Some(SliderThumb::Minimum)
    );
    assert_eq!(engine.node(thumbs[0]).unwrap().text, "Price minimum");
    assert_eq!(
        engine.node(thumbs[1]).unwrap().style.slider_thumb,
        Some(SliderThumb::Maximum)
    );
    assert_eq!(engine.node(thumbs[1]).unwrap().text, "Price maximum");
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
