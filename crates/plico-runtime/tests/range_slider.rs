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
        .node_by_role(SemanticRole::Slider, "Price", None)
        .expect("the range slider exposes a slider node")
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

    click(&mut source, &mut text, 0.9);
    assert!(
        (source.view.high - 90.).abs() <= 5.0,
        "high={}",
        source.view.high
    );
    assert!((source.view.low - 20.).abs() < 1e-9);

    let target = source
        .adapter
        .semantics(&source.engine, 0)
        .node_by_role(SemanticRole::Slider, "Price", None)
        .unwrap()
        .id;
    source.handle_semantic_action(SemanticAction::Focus { target });
    source.build_frame(&mut text, 400, 100);
    let before = source.view.high;
    source.handle_event(key("ArrowLeft"));
    source.build_frame(&mut text, 400, 100);
    assert!((source.view.high - (before - 1.)).abs() < 1e-9);

    click(&mut source, &mut text, 0.1);
    assert!(
        (source.view.low - 10.).abs() <= 5.0,
        "low={}",
        source.view.low
    );
    assert_eq!(source.view.high, before - 1.);
}
