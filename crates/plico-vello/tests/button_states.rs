use plico_core::{Theme, VirtualClock, prelude::*};
use plico_renderer::{
    FrameSource, Modifiers, Point, PointerButton, PointerEvent, PointerPhase, PointerProperties,
    TextContext, UiEvent,
};
use plico_vello::{Source, View};
use std::time::Duration;
struct Buttons {
    calls: usize,
    disabled: bool,
    visible: bool,
    theme: Theme,
}
impl View for Buttons {
    fn theme(&self) -> Theme {
        self.theme
    }
    #[component]
    fn show(&mut self, cx: &mut UiContext) {
        self.calls += 1;
        if self.visible {
            button(cx)
                .text("Save")
                .width(120)
                .height(40)
                .disabled(self.disabled);
        }
    }
}
fn pointer(phase: PointerPhase) -> UiEvent {
    UiEvent::Pointer(PointerEvent {
        phase,
        position: Point { x: 20., y: 20. },
        button: matches!(phase, PointerPhase::Down | PointerPhase::Up)
            .then_some(PointerButton::Primary),
        buttons: u32::from(phase == PointerPhase::Down),
        modifiers: Modifiers::empty(),
        properties: PointerProperties::default(),
    })
}
fn setup() -> (Source<Buttons>, VirtualClock, TextContext) {
    let clock = VirtualClock::default();
    let source = Source::with_clock(
        Buttons {
            calls: 0,
            disabled: false,
            visible: true,
            theme: Theme::default(),
        },
        clock.clone(),
    );
    (source, clock, TextContext::new())
}
#[test]
fn hover_and_press_animate_without_declarations_or_layout_and_then_sleep() {
    let (mut source, clock, mut text) = setup();
    let base = source.build_frame(&mut text, 300, 200);
    let idle = base
        .iter()
        .find(|n| n.paint.background.is_some() && n.depth == 1)
        .unwrap()
        .paint
        .background;
    source.handle_event(pointer(PointerPhase::Move));
    source.build_frame(&mut text, 300, 200);
    assert!(source.has_anim());
    let calls = source.view.calls;
    clock.advance(Duration::from_millis(40)).unwrap();
    let hovered = source.build_frame(&mut text, 300, 200);
    assert_ne!(
        hovered
            .iter()
            .find(|n| n.depth == 1)
            .unwrap()
            .paint
            .background,
        idle,
        "default token hover must survive RGB conversion"
    );
    assert_eq!(source.adapter.stats.layout_runs, 0);
    assert_eq!(source.adapter.stats.created, 0);
    assert_eq!(source.view.calls, calls);
    source.handle_event(pointer(PointerPhase::Down));
    source.build_frame(&mut text, 300, 200);
    clock.advance(Duration::from_secs(2)).unwrap();
    source.build_frame(&mut text, 300, 200);
    assert!(!source.has_anim());
    source.build_frame(&mut text, 300, 200);
    assert_eq!(source.adapter.stats.sync_visits, 0);
}
#[test]
fn reduced_motion_theme_change_and_unmount_do_not_leave_animation_alive() {
    let (mut source, clock, mut text) = setup();
    source.build_frame(&mut text, 300, 200);
    source.handle_event(pointer(PointerPhase::Move));
    source.build_frame(&mut text, 300, 200);
    source.set_reduced_motion(true);
    let nodes = source.build_frame(&mut text, 300, 200);
    let background = nodes
        .iter()
        .find(|n| n.depth == 1)
        .unwrap()
        .paint
        .background;
    assert_eq!(
        background,
        Some(peniko::Color::from_rgb8(
            (source.view.theme.control_hover >> 16) as u8,
            (source.view.theme.control_hover >> 8) as u8,
            source.view.theme.control_hover as u8
        ))
    );
    assert!(!source.has_anim());
    source.set_reduced_motion(false);
    source.view.theme.control_hover ^= 0x111111;
    source.request_rebuild();
    source.build_frame(&mut text, 300, 200);
    assert!(!source.has_anim());
    source.view.disabled = true;
    source.request_rebuild();
    source.build_frame(&mut text, 300, 200);
    clock.advance(Duration::from_secs(2)).unwrap();
    let nodes = source.build_frame(&mut text, 300, 200);
    assert!((nodes.iter().find(|n| n.depth == 1).unwrap().paint.opacity - 0.45).abs() < 0.00001);
    source.view.visible = false;
    source.request_rebuild();
    source.build_frame(&mut text, 300, 200);
    assert!(!source.has_anim());
}

#[test]
fn focus_ring_fades_without_layout_and_cleans_up_on_blur() {
    use plico_renderer::{SemanticAction, SemanticRole};
    let (mut source, clock, mut text) = setup();
    source.build_frame(&mut text, 300, 200);
    let target = source
        .adapter
        .semantics(&source.engine, 0)
        .node_by_role(SemanticRole::Button, "Save", None)
        .unwrap()
        .id;
    assert!(source.handle_semantic_action(SemanticAction::Focus { target }));
    source.build_frame(&mut text, 300, 200);
    clock.advance(Duration::from_millis(32)).unwrap();
    let nodes = source.build_frame(&mut text, 300, 200);
    let paint = &nodes.iter().find(|n| n.depth == 1).unwrap().paint;
    assert_eq!(paint.outline_width, 1.5);
    let alpha = paint.outline_color.unwrap().to_rgba8().a;
    assert!(
        alpha > 0 && alpha < 255,
        "alpha={alpha}, stats={:?}",
        source.adapter.stats
    );
    assert_eq!(source.adapter.stats.layout_runs, 0);
    assert!(source.handle_semantic_action(SemanticAction::Blur { target }));
    source.build_frame(&mut text, 300, 200);
    clock.advance(Duration::from_secs(2)).unwrap();
    let nodes = source.build_frame(&mut text, 300, 200);
    let paint = &nodes.iter().find(|n| n.depth == 1).unwrap().paint;
    assert_eq!(paint.outline_width, 0.);
    assert_eq!(paint.outline_color, None);
    assert!(!source.has_anim());
}
