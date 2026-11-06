//! Escape dismisses the alert and a click outside it does nothing.
use plico::prelude::*;
use plico::widgets::{ALERT_DISMISSED, alert_dialog};
use plico_renderer::{
    FrameSource, KeyEvent, KeyLocation, KeyPhase, Modifiers, Point, PointerButton, PointerEvent,
    PointerPhase, PointerProperties, SemanticRole, TextContext, UiEvent,
};
use plico_runtime::{HostView, Source};

#[derive(Default)]
struct Demo {
    alert: ModalState,
    deleted: usize,
    dismissed: usize,
}

impl HostView for Demo {
    fn show(&mut self, cx: &mut UiContext) {
        match alert_dialog(
            cx,
            &mut self.alert,
            "Delete notes?",
            "You can't undo this.",
            &["Cancel", "Delete"],
        ) {
            1 => self.deleted += 1,
            ALERT_DISMISSED => self.dismissed += 1,
            _ => {}
        }
    }
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

#[test]
fn alert_dialog_is_an_alert_and_escape_dismisses_it() {
    let mut source = Source::new(Demo::default());
    let mut text = TextContext::new();
    source.view.alert.open();
    source.build_frame(&mut text, 720, 520);
    let semantics = source.adapter.semantics(&source.engine, 0);
    assert!(
        semantics
            .node_by_role(SemanticRole::AlertDialog, "Delete notes?", None)
            .is_some(),
        "the panel is an alert dialog named by its title"
    );
    assert_eq!(source.view.dismissed, 0);

    source.handle_event(key("Escape"));
    source.build_frame(&mut text, 720, 520);
    assert_eq!(source.view.dismissed, 1);
    assert!(!source.view.alert.is_open());
    assert_eq!(source.view.deleted, 0);
}

#[test]
fn clicking_outside_the_alert_does_nothing() {
    let mut source = Source::new(Demo::default());
    let mut text = TextContext::new();
    source.view.alert.open();
    source.build_frame(&mut text, 720, 520);
    source.handle_event(pointer(PointerPhase::Down, 4., 4.));
    source.handle_event(pointer(PointerPhase::Up, 4., 4.));
    source.build_frame(&mut text, 720, 520);
    assert_eq!(source.view.dismissed, 0);
    assert_eq!(source.view.deleted, 0);
    assert!(source.view.alert.is_open(), "the alert still asks");
}
