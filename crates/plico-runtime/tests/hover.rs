use plico::prelude::*;
use plico_renderer::{
    FrameSource, Point, PointerButton, PointerEvent, PointerPhase, PointerProperties,
    SemanticAction, SemanticRole, TextContext, UiEvent,
};
use plico_runtime::{HostView, Source};
#[derive(Default)]
struct Demo {
    hovered: bool,
    clicks: u32,
    clicked_hovered: bool,
    disabled: bool,
}
impl HostView for Demo {
    #[component]
    fn show(&mut self, cx: &mut UiContext) {
        let response = button(cx)
            .text("Action")
            .width(160.)
            .height(40.)
            .disabled(self.disabled)
            .response();
        self.hovered = response.hovered();
        assert_eq!(response.hovered(), response.hovered());
        if response.clicked() {
            self.clicks += 1;
            self.clicked_hovered = response.hovered();
        }
        text!(cx, "Hovered: {}", response.hovered());
    }
}
fn pointer(phase: PointerPhase) -> UiEvent {
    pointer_at(phase, 20., 20.)
}
fn pointer_at(phase: PointerPhase, x: f64, y: f64) -> UiEvent {
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

#[derive(Default)]
struct PointerField {
    fraction: Option<[f32; 2]>,
}
impl HostView for PointerField {
    fn show(&mut self, cx: &mut UiContext) {
        button_base(cx, "Field")
            .width(200.)
            .height(100.)
            .padding(0.)
            .children(|cx| {
                let fraction = cx.pointer_fraction();
                if cx.pressed() {
                    self.fraction = fraction;
                }
            });
    }
}

#[test]
fn composed_controls_receive_transformed_normalized_pointer_geometry() {
    let mut source = Source::new(PointerField::default());
    let mut text = TextContext::new();
    source.build_frame(&mut text, 320, 180);
    let [x0, y0, x1, y1] = source
        .semantics()
        .node_by_role(SemanticRole::Button, "Field", None)
        .unwrap()
        .bounds;
    let x = f64::from(x0 + (x1 - x0) * 0.25);
    let y = f64::from(y0 + (y1 - y0) * 0.75);
    source.handle_event(pointer_at(PointerPhase::Down, x, y));
    source.build_frame(&mut text, 320, 180);
    let [fx, fy] = source.view().fraction.expect("pressed pointer fraction");
    assert!((fx - 0.25).abs() < 0.01, "fx={fx}");
    assert!((fy - 0.75).abs() < 0.01, "fy={fy}");
}

struct ColorField {
    color: Color,
    changed: bool,
}
impl HostView for ColorField {
    fn show(&mut self, cx: &mut UiContext) {
        self.changed = color_picker(cx, &mut self.color);
    }
}

#[test]
fn color_picker_uses_the_shared_pointer_geometry_for_its_two_axis_field() {
    let mut source = Source::new(ColorField {
        color: Color::Literal(0xff0000),
        changed: false,
    });
    let mut text = TextContext::new();
    source.build_frame(&mut text, 320, 360);
    let [x0, y0, x1, y1] = source
        .semantics()
        .node_by_role(SemanticRole::Slider, "Saturation and brightness", None)
        .expect("two-axis color field exposes slider semantics")
        .bounds;
    source.handle_event(pointer_at(
        PointerPhase::Down,
        f64::from(x0 + x1) / 2.,
        f64::from(y0 + y1) / 2.,
    ));
    source.build_frame(&mut text, 320, 360);
    assert_eq!(source.view().color, Color::Literal(0x804040));
}
#[test]
fn pointer_hover_and_click_are_independent_borrow_free_response_values() {
    let mut source = Source::new(Demo::default());
    let mut text = TextContext::new();
    source.build_frame(&mut text, 300, 200);
    assert!(!source.view().hovered);
    source.handle_event(pointer(PointerPhase::Enter));
    source.build_frame(&mut text, 300, 200);
    assert!(source.view().hovered);
    assert_eq!(source.view().clicks, 0);
    source.handle_event(pointer(PointerPhase::Down));
    let hit = source.frame_diagnostics().hit_test;
    assert!(hit.rebuilds > 0);
    assert!(
        hit.checks < source.placed_nodes().len(),
        "checks={}",
        hit.checks
    );
    assert!(hit.pruned > 0);
    source.handle_event(pointer(PointerPhase::Up));
    assert_eq!(source.frame_diagnostics().hit_test.rebuilds, hit.rebuilds);
    source.build_frame(&mut text, 300, 200);
    assert_eq!(source.view().clicks, 1);
    assert!(source.view().clicked_hovered);
    source.handle_event(pointer(PointerPhase::Leave));
    source.build_frame(&mut text, 300, 200);
    assert!(!source.view().hovered);
    let target = source
        .semantics()
        .node_by_role(SemanticRole::Button, "Action", None)
        .unwrap()
        .id;
    source.handle_semantic_action(SemanticAction::Click { target });
    source.build_frame(&mut text, 300, 200);
    assert_eq!(source.view().clicks, 2);
    assert!(!source.view().clicked_hovered);
    source.update(|view| view.disabled = true);
    source.build_frame(&mut text, 300, 200);
    source.handle_event(pointer(PointerPhase::Move));
    source.build_frame(&mut text, 300, 200);
    assert!(!source.view().hovered);
    assert_eq!(source.view().clicks, 2);
}
