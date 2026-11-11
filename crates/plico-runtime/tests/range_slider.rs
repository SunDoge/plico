//! Pointer and keyboard editing of the dual-thumb range slider.
use plico::prelude::*;
use plico_renderer::{
    FrameSource, KeyEvent, KeyLocation, KeyPhase, Modifiers, Point, PointerButton, PointerEvent,
    PointerPhase, PointerProperties, SemanticAction, SemanticRole, TextContext, UiEvent,
};
use plico_runtime::{HostView, Source};

#[derive(Default)]
struct Demo {
    low: f64,
    high: f64,
}

impl HostView for Demo {
    fn show(&mut self, cx: &mut UiContext) {
        range_slider(cx, "Price", &mut self.low, &mut self.high, 0.0..=100.0);
    }
}

fn pointer(phase: PointerPhase, x: f64, y: f64) -> UiEvent {
    UiEvent::Pointer(PointerEvent {
        phase,
        position: Point { x, y },
        button: matches!(phase, PointerPhase::Down | PointerPhase::Up)
            .then_some(PointerButton::Primary),
        buttons: u32::from(phase == PointerPhase::Down),
        modifiers: Modifiers::empty(),
        properties: PointerProperties::default(),
    })
}

fn key(value: &str) -> UiEvent {
    UiEvent::Key(KeyEvent {
        phase: KeyPhase::Down,
        key: value.into(),
        code: value.into(),
        key_without_modifiers: value.into(),
        text: None,
        text_with_all_modifiers: None,
        location: KeyLocation::Standard,
        modifiers: Modifiers::empty(),
        repeat: false,
        synthetic: false,
    })
}

fn bounds(source: &Source<Demo>) -> [f32; 4] {
    source
        .adapter
        .semantics(&source.engine, 0)
        .node_by_role(SemanticRole::Group, "Price", None)
        .expect("the range slider exposes a named group")
        .bounds
}

fn click(source: &mut Source<Demo>, text: &mut TextContext, fraction: f64) {
    let [x0, y0, x1, y1] = bounds(source);
    let x = f64::from(x0) + f64::from(x1 - x0) * fraction;
    let y = f64::from(y0 + y1) / 2.;
    source.handle_event(pointer(PointerPhase::Down, x, y));
    source.handle_event(pointer(PointerPhase::Up, x, y));
    source.build_frame(text, 400, 100);
}

#[test]
fn pressing_picks_the_nearer_thumb_and_keys_move_it() {
    let mut source = Source::new(Demo {
        low: 20.,
        high: 80.,
    });
    let mut text = TextContext::new();
    source.build_frame(&mut text, 400, 100);
    assert_eq!((source.view.low, source.view.high), (20., 80.));
    let semantics = source.adapter.semantics(&source.engine, 0);
    let group = semantics
        .node_by_role(SemanticRole::Group, "Price", None)
        .unwrap();
    let group_id = group.id;
    assert_eq!(group.numeric_value, None);
    let minimum = semantics
        .node_by_role(SemanticRole::Slider, "Price minimum", None)
        .unwrap();
    let maximum = semantics
        .node_by_role(SemanticRole::Slider, "Price maximum", None)
        .unwrap();
    assert_eq!(minimum.numeric_value, Some(20.));
    assert_eq!(maximum.numeric_value, Some(80.));
    assert!(minimum.bounds[2] <= maximum.bounds[0]);
    assert!(minimum.bounds[2] - minimum.bounds[0] < group.bounds[2] - group.bounds[0]);
    assert!(maximum.bounds[2] - maximum.bounds[0] < group.bounds[2] - group.bounds[0]);
    assert_eq!(
        (minimum.min_numeric_value, minimum.max_numeric_value),
        (Some(0.), Some(100.))
    );
    assert_eq!(
        (maximum.min_numeric_value, maximum.max_numeric_value),
        (Some(0.), Some(100.))
    );
    assert!(!source.handle_semantic_action(SemanticAction::Focus { target: group_id }));

    click(&mut source, &mut text, 0.9);
    assert!(
        (source.view.high - 90.).abs() <= 5.0,
        "high={}",
        source.view.high
    );
    assert!((source.view.low - 20.).abs() < 1e-9);

    let before = source.view.high;
    source.handle_event(key("ArrowLeft"));
    source.build_frame(&mut text, 400, 100);
    assert!((source.view.high - (before - 1.)).abs() < 1e-9);

    let minimum = source
        .adapter
        .semantics(&source.engine, 0)
        .node_by_role(SemanticRole::Slider, "Price minimum", None)
        .unwrap()
        .id;
    source.handle_semantic_action(SemanticAction::Focus { target: minimum });
    source.handle_event(key("ArrowRight"));
    source.build_frame(&mut text, 400, 100);
    assert_eq!(source.view.low, 21.);

    source.handle_event(key("Tab"));
    source.handle_event(key("ArrowLeft"));
    source.build_frame(&mut text, 400, 100);
    assert_eq!(
        source.view.high,
        before - 2.,
        "Tab reaches the maximum thumb"
    );

    click(&mut source, &mut text, 0.1);
    assert!(
        (source.view.low - 10.).abs() <= 5.0,
        "low={}",
        source.view.low
    );
    assert_eq!(source.view.high, before - 2.);

    let maximum = source
        .adapter
        .semantics(&source.engine, 0)
        .node_by_role(SemanticRole::Slider, "Price maximum", None)
        .unwrap()
        .id;
    assert!(
        source.handle_semantic_action(SemanticAction::SetNumericValue {
            target: maximum,
            value: 95.,
        })
    );
    source.build_frame(&mut text, 400, 100);
    assert_eq!(source.view.high, 95.);
}

#[test]
fn keyboard_thumb_selection_is_local_to_each_slider() {
    struct Pair {
        a: Demo,
        b: Demo,
    }
    impl HostView for Pair {
        fn show(&mut self, cx: &mut UiContext) {
            column(cx).children(|cx| {
                range_slider(cx, "Price", &mut self.a.low, &mut self.a.high, 0.0..=100.0);
                range_slider(cx, "Other", &mut self.b.low, &mut self.b.high, 0.0..=100.0);
            });
        }
    }
    let mut source = Source::new(Pair {
        a: Demo {
            low: 20.,
            high: 80.,
        },
        b: Demo {
            low: 30.,
            high: 70.,
        },
    });
    let mut text = TextContext::new();
    source.build_frame(&mut text, 400, 160);
    let snapshot = source.adapter.semantics(&source.engine, 0);
    let first = snapshot
        .node_by_role(SemanticRole::Group, "Price", None)
        .unwrap();
    let [x0, y0, x1, y1] = first.bounds;
    let other = snapshot
        .node_by_role(SemanticRole::Slider, "Other minimum", None)
        .unwrap()
        .id;
    let x = f64::from(x0 + (x1 - x0) * 0.9);
    let y = f64::from(y0 + y1) / 2.;
    source.handle_event(pointer(PointerPhase::Down, x, y));
    source.handle_event(pointer(PointerPhase::Up, x, y));
    source.build_frame(&mut text, 400, 160);
    source.handle_semantic_action(SemanticAction::Focus { target: other });
    source.handle_event(key("ArrowRight"));
    source.build_frame(&mut text, 400, 160);
    assert_eq!((source.view.b.low, source.view.b.high), (31., 70.));
}

#[test]
fn unstyled_range_keeps_two_focusable_numeric_thumbs() {
    struct Bare(Demo);
    impl HostView for Bare {
        fn show(&mut self, cx: &mut UiContext) {
            let _ = cx
                .range_slider("Bare", &mut self.0.low, &mut self.0.high, 0.0..=100.0)
                .unstyled();
        }
    }

    let mut source = Source::new(Bare(Demo {
        low: 25.,
        high: 75.,
    }));
    let mut text = TextContext::new();
    source.build_frame(&mut text, 400, 100);
    let minimum = source
        .adapter
        .semantics(&source.engine, 0)
        .node_by_role(SemanticRole::Slider, "Bare minimum", None)
        .unwrap()
        .id;
    source.handle_semantic_action(SemanticAction::Focus { target: minimum });
    source.handle_event(key("ArrowRight"));
    source.build_frame(&mut text, 400, 100);
    assert_eq!((source.view.0.low, source.view.0.high), (26., 75.));
    assert!(
        source
            .adapter
            .semantics(&source.engine, 0)
            .node_by_role(SemanticRole::Slider, "Bare maximum", None)
            .is_some()
    );
}
