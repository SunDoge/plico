use plico_backend::{Source, View};
use plico_core::{Ui, VirtualClock};
use plico_renderer::{FrameSource, PaintCommand, SemanticAction, SemanticRole, TextContext};
use std::time::Duration;
use vello_common::kurbo::Shape;
#[derive(Default)]
struct Feedback {
    checked: bool,
    calls: usize,
}
impl View for Feedback {
    fn show(&mut self, ui: &mut Ui<'_>) {
        self.calls += 1;
        ui.switch("Toggle", &mut self.checked);
    }
}
fn knob(s: &Source<Feedback>) -> f64 {
    let scene = s
        .adapter
        .placed
        .iter()
        .find(|n| n.paint.text.as_deref() == Some("Toggle"))
        .unwrap()
        .paint
        .widget
        .as_ref()
        .unwrap();
    match &scene.commands[1] {
        PaintCommand::Fill { shape, .. } => shape.bounding_box().center().x,
        _ => panic!("expected switch thumb"),
    }
}
fn click(s: &mut Source<Feedback>, tcx: &mut TextContext) {
    let id = s
        .adapter
        .semantics(&s.engine, 0)
        .node_by_role(SemanticRole::Switch, "Toggle", None)
        .unwrap()
        .id;
    assert!(s.handle_semantic_action(SemanticAction::Click { target: id }));
    s.build_frame(tcx, 300, 100);
}
#[test]
fn toggle_thumb_moves_on_host_clock_and_reverses_without_a_jump() {
    let clock = VirtualClock::default();
    let mut s = Source::with_clock(Feedback::default(), clock.clone());
    let mut tcx = TextContext::new();
    s.build_frame(&mut tcx, 300, 100);
    assert_eq!(knob(&s), -32.);
    click(&mut s, &mut tcx);
    assert_eq!(knob(&s), -32.);
    let calls = s.view.calls;
    clock.set(Duration::from_millis(70)).unwrap();
    s.build_frame(&mut tcx, 300, 100);
    assert!((knob(&s) + 24.).abs() < 0.01);
    assert_eq!(s.view.calls, calls);
    assert_eq!(s.adapter.stats.sync_visits, 1);
    assert_eq!(s.adapter.stats.layout_runs, 0);
    let middle = knob(&s);
    click(&mut s, &mut tcx);
    assert_eq!(knob(&s), middle);
    clock.set(Duration::from_millis(105)).unwrap();
    s.build_frame(&mut tcx, 300, 100);
    assert!(knob(&s) < middle && knob(&s) > -32.);
    clock.set(Duration::from_millis(210)).unwrap();
    s.build_frame(&mut tcx, 300, 100);
    assert_eq!(knob(&s), -32.);
    assert!(!s.has_anim());
}
#[test]
fn reduced_motion_snaps_interaction_feedback_and_stops_clock_requests() {
    let clock = VirtualClock::default();
    let mut s = Source::with_clock(Feedback::default(), clock.clone());
    let mut tcx = TextContext::new();
    s.build_frame(&mut tcx, 300, 100);
    click(&mut s, &mut tcx);
    clock.set(Duration::from_millis(70)).unwrap();
    s.build_frame(&mut tcx, 300, 100);
    s.set_reduced_motion(true);
    s.build_frame(&mut tcx, 300, 100);
    assert_eq!(knob(&s), -16.);
    assert!(!s.has_anim());
    click(&mut s, &mut tcx);
    assert_eq!(knob(&s), -32.);
    assert!(!s.has_anim());
}
