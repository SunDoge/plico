use plico_core::prelude::*;
use plico_renderer::{
    FrameSource, KeyEvent, KeyLocation, KeyPhase, Modifiers, Point, PointerButton, PointerEvent,
    PointerPhase, SemanticAction, SemanticRole, TextContext, UiEvent,
};
use plico_vello::{Source, View};
#[derive(Default)]
struct Demo {
    value: f64,
    vertical: bool,
    disabled: bool,
    changes: usize,
    shows: usize,
}
impl View for Demo {
    #[component]
    fn show(&mut self, cx: &mut UiContext) {
        self.shows += 1;
        column(cx).padding(20).gap(10).children(|cx| {
            text!(cx, "Value {:.0}", self.value);
            if slider(cx, "Volume", &mut self.value, 0.0..=10.0)
                .step(0.5)
                .vertical(self.vertical)
                .disabled(self.disabled)
                .response()
                .changed
            {
                self.changes += 1;
            }
        })
    }
}
fn key(name: &str) -> UiEvent {
    UiEvent::Key(KeyEvent {
        phase: KeyPhase::Down,
        key: name.into(),
        code: name.into(),
        key_without_modifiers: name.into(),
        text: None,
        text_with_all_modifiers: None,
        location: KeyLocation::Standard,
        modifiers: Modifiers::empty(),
        repeat: false,
        synthetic: false,
    })
}
fn pointer(phase: PointerPhase, x: f64, y: f64) -> UiEvent {
    UiEvent::Pointer(PointerEvent {
        phase,
        position: Point { x, y },
        button: Some(PointerButton::Primary),
        buttons: 1,
        modifiers: Modifiers::empty(),
        properties: Default::default(),
    })
}
fn node(s: &Source<Demo>) -> plico_renderer::SemanticNode {
    s.adapter
        .semantics(&s.engine, 0)
        .node_by_role(SemanticRole::Slider, "Volume", None)
        .unwrap()
        .clone()
}
#[test]
fn drag_capture_keyboard_and_assistive_actions_update_controlled_value_once() {
    let mut s = Source::new(Demo::default());
    let mut text = TextContext::new();
    s.build_frame(&mut text, 400, 300);
    let n = node(&s);
    let [x0, y0, x1, y1] = n.bounds;
    let y = (y0 + y1) as f64 / 2.;
    s.handle_event(pointer(PointerPhase::Down, (x0 + x1) as f64 / 2., y));
    s.build_frame(&mut text, 400, 300);
    assert_eq!(s.view.value, 5.);
    assert_eq!(s.view.changes, 1);
    assert_eq!(node(&s).numeric_value, Some(5.));
    assert!(
        !s.handle_event(pointer(PointerPhase::Move, f64::NAN, y))
            .handled
    );
    assert_eq!(s.view.value, 5.);
    s.handle_event(pointer(PointerPhase::Move, 700., y));
    s.build_frame(&mut text, 400, 300);
    assert_eq!(s.view.value, 10.);
    s.handle_event(pointer(PointerPhase::Up, 700., y));
    s.build_frame(&mut text, 400, 300);
    s.handle_event(key("Home"));
    s.build_frame(&mut text, 400, 300);
    assert_eq!(s.view.value, 0.);
    for _ in 0..3 {
        s.handle_event(key("ArrowRight"));
    }
    s.build_frame(&mut text, 400, 300);
    assert_eq!(s.view.value, 1.5);
    assert!(s.handle_semantic_action(SemanticAction::SetNumericValue {
        target: n.id,
        value: 3.3
    }));
    s.build_frame(&mut text, 400, 300);
    assert_eq!(s.view.value, 3.5);
    assert!(s.handle_semantic_action(SemanticAction::Increment { target: n.id }));
    s.build_frame(&mut text, 400, 300);
    assert_eq!(s.view.value, 4.);
    assert!(!s.handle_semantic_action(SemanticAction::SetNumericValue {
        target: n.id,
        value: f64::NAN
    }));
    s.view.disabled = true;
    s.request_rebuild();
    s.build_frame(&mut text, 400, 300);
    assert!(!s.handle_semantic_action(SemanticAction::Increment { target: n.id }));
    assert_eq!(s.view.value, 4.);
}
#[test]
fn vertical_values_and_paint_only_updates_do_not_relayout() {
    let mut s = Source::new(Demo {
        vertical: true,
        ..Default::default()
    });
    let mut text = TextContext::new();
    s.build_frame(&mut text, 400, 300);
    let n = node(&s);
    let [x0, y0, x1, _] = n.bounds;
    let x = (x0 + x1) as f64 / 2.;
    s.handle_event(pointer(PointerPhase::Down, x, y0 as f64 + 8.));
    s.build_frame(&mut text, 400, 300);
    assert_eq!(s.view.value, 10.);
    s.handle_event(pointer(PointerPhase::Cancel, x, y0 as f64 + 8.));
    s.build_frame(&mut text, 400, 300);
    assert!(s.handle_semantic_action(SemanticAction::Decrement { target: n.id }));
    s.build_frame(&mut text, 400, 300);
    assert_eq!(s.view.value, 9.5);
    assert_eq!(s.adapter.stats.layout_runs, 0);
}

#[test]
fn pressed_feedback_runs_in_the_host_without_redeclaring_or_relayout() {
    let clock = plico_core::VirtualClock::default();
    let mut s = Source::with_clock(Demo::default(), clock.clone());
    let mut text = TextContext::new();
    s.build_frame(&mut text, 400, 300);
    let n = node(&s);
    let [x0, y0, _, y1] = n.bounds;
    s.handle_event(pointer(
        PointerPhase::Down,
        x0 as f64 + 8.,
        (y0 + y1) as f64 / 2.,
    ));
    s.build_frame(&mut text, 400, 300);
    assert!(s.has_anim());
    let shows = s.view.shows;
    clock.advance(std::time::Duration::from_millis(70)).unwrap();
    s.build_frame(&mut text, 400, 300);
    assert_eq!(s.view.shows, shows);
    assert_eq!(s.adapter.stats.layout_runs, 0);
    s.handle_event(pointer(
        PointerPhase::Up,
        x0 as f64 + 8.,
        (y0 + y1) as f64 / 2.,
    ));
    s.build_frame(&mut text, 400, 300);
    clock
        .advance(std::time::Duration::from_millis(150))
        .unwrap();
    s.build_frame(&mut text, 400, 300);
    assert!(!s.has_anim());
    assert_eq!(s.view.value, 0.);
}
