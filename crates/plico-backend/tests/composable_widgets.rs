use plico_backend::{Source, View};
use plico_core::prelude::*;
use plico_renderer::{
    FrameSource, KeyEvent, KeyLocation, KeyPhase, Modifiers, Point, PointerButton, PointerEvent,
    PointerPhase, SemanticAction, SemanticRole, TextContext, UiEvent,
};
static ICON: SvgAsset = SvgAsset::new(
    r##"<svg xmlns="http://www.w3.org/2000/svg" width="18" height="18"><path d="M1 1h16v16H1Z" fill="#ffffff"/></svg>"##,
);
#[derive(Default)]
struct Demo {
    count: usize,
    disabled: bool,
}
impl View for Demo {
    #[component]
    fn show(&mut self, ui: &mut Ui) {
        if ui
            .button("Save")
            .primary()
            .disabled(self.disabled)
            .children(|ui| {
                ui.svg(&ICON).size(18., 18.);
                ui.text("Save content");
            })
            .response
            .clicked()
        {
            self.count += 1;
        }
        ui.button_base("Bare action").children(|ui| {
            ui.text("Bare content");
        });
    }
}
fn pointer(phase: PointerPhase, x: f64, y: f64) -> UiEvent {
    UiEvent::Pointer(PointerEvent {
        phase,
        position: Point { x, y },
        button: Some(PointerButton::Primary),
        buttons: if phase == PointerPhase::Down { 1 } else { 0 },
        modifiers: Modifiers::empty(),
        properties: Default::default(),
    })
}
fn key(value: &str) -> UiEvent {
    UiEvent::Key(KeyEvent {
        key: value.into(),
        key_without_modifiers: value.into(),
        text: None,
        text_with_all_modifiers: None,
        synthetic: false,
        code: value.into(),
        location: KeyLocation::Standard,
        phase: KeyPhase::Down,
        modifiers: Modifiers::empty(),
        repeat: false,
    })
}
#[test]
fn clicking_an_svg_child_and_keyboard_activation_target_the_composite_parent() {
    let mut s = Source::new(Demo::default());
    let mut tcx = TextContext::new();
    s.build_frame(&mut tcx, 320, 200);
    let icon = s
        .adapter
        .placed
        .iter()
        .find(|n| n.paint.svg.is_some())
        .unwrap()
        .rect;
    let x = ((icon[0] + icon[2]) / 2.) as f64;
    let y = ((icon[1] + icon[3]) / 2.) as f64;
    s.handle_event(pointer(PointerPhase::Down, x, y));
    s.handle_event(pointer(PointerPhase::Up, x, y));
    s.build_frame(&mut tcx, 320, 200);
    assert_eq!(s.view.count, 1);
    let save = s
        .adapter
        .semantics(&s.engine, 0)
        .node_by_role(SemanticRole::Button, "Save", None)
        .unwrap()
        .id;
    assert_eq!(s.adapter.semantics(&s.engine, 0).focus, Some(save));
    s.handle_event(key("Enter"));
    s.build_frame(&mut tcx, 320, 200);
    assert_eq!(s.view.count, 2);
    s.handle_event(key("Tab"));
    let bare = s
        .adapter
        .semantics(&s.engine, 0)
        .node_by_role(SemanticRole::Button, "Bare action", None)
        .unwrap()
        .id;
    assert_eq!(s.adapter.semantics(&s.engine, 0).focus, Some(bare));
    s.view.disabled = true;
    s.build_frame(&mut tcx, 320, 200);
    assert!(!s.handle_semantic_action(SemanticAction::Click { target: save }));
    assert!(
        s.adapter
            .semantics(&s.engine, 0)
            .node_by_role(SemanticRole::Label, "Save content", None)
            .unwrap()
            .disabled
    );
    assert_eq!(s.view.count, 2);
}
#[test]
fn headless_button_has_no_implicit_background_or_control_padding() {
    let mut s = Source::new(Demo::default());
    let mut tcx = TextContext::new();
    s.build_frame(&mut tcx, 320, 200);
    let semantics = s.adapter.semantics(&s.engine, 0);
    let bare = semantics
        .node_by_role(SemanticRole::Button, "Bare action", None)
        .unwrap();
    let p = s
        .adapter
        .placed
        .iter()
        .find(|p| p.rect == bare.bounds && p.depth == 1)
        .unwrap();
    assert!(p.paint.background.is_none());
    assert!(p.rect[3] - p.rect[1] < 36.);
    let text = s
        .adapter
        .placed
        .iter()
        .find(|n| n.paint.text.as_deref() == Some("Save content"))
        .unwrap();
    assert_eq!(
        text.paint.text_color,
        vello_common::peniko::Color::from_rgb8(255, 255, 255)
    );
}
