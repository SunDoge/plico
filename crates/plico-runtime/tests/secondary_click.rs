use plico::prelude::*;
use plico_renderer::{
    FrameSource, Point, PointerButton, PointerEvent, PointerPhase, PointerProperties, TextContext,
    UiEvent,
};
use plico_runtime::{HostView, Source};

#[derive(Default)]
struct Demo {
    primary: u32,
    secondary: u32,
    disabled: bool,
}
impl HostView for Demo {
    #[component]
    fn show(&mut self, cx: &mut UiContext) {
        let r = button_base(cx, "Cell")
            .width(100.)
            .height(60.)
            .disabled(self.disabled)
            .children(|cx| cx.secondary_clicked());
        self.primary += u32::from(r.clicked());
        self.secondary += u32::from(r.inner);
    }
}
fn pointer(phase: PointerPhase, x: f64) -> UiEvent {
    UiEvent::Pointer(PointerEvent {
        phase,
        position: Point { x, y: 20. },
        button: Some(PointerButton::Secondary),
        buttons: if phase == PointerPhase::Down { 2 } else { 0 },
        modifiers: Modifiers::empty(),
        properties: PointerProperties::default(),
    })
}
fn frame(s: &mut Source<Demo>, t: &mut TextContext) {
    s.build_frame(t, 300, 200);
    assert!(s.last_error.is_none());
}
#[test]
fn secondary_click_is_consumed_once_and_never_primary() {
    let mut s = Source::new(Demo::default());
    let mut t = TextContext::new();
    frame(&mut s, &mut t);
    s.handle_event(pointer(PointerPhase::Down, 20.));
    frame(&mut s, &mut t);
    assert_eq!((s.view.primary, s.view.secondary), (0, 0));
    s.handle_event(pointer(PointerPhase::Up, 20.));
    frame(&mut s, &mut t);
    frame(&mut s, &mut t);
    assert_eq!((s.view.primary, s.view.secondary), (0, 1));
}
#[test]
fn drag_away_cancel_leave_blur_and_disabled_do_not_activate() {
    for end in [
        pointer(PointerPhase::Up, 200.),
        pointer(PointerPhase::Cancel, 20.),
        pointer(PointerPhase::Leave, 20.),
        UiEvent::Focus(false),
    ] {
        let mut s = Source::new(Demo::default());
        let mut t = TextContext::new();
        frame(&mut s, &mut t);
        s.handle_event(pointer(PointerPhase::Down, 20.));
        s.handle_event(end);
        s.handle_event(pointer(PointerPhase::Up, 20.));
        frame(&mut s, &mut t);
        assert_eq!(s.view.secondary, 0);
    }
    let mut s = Source::new(Demo {
        disabled: true,
        ..Default::default()
    });
    let mut t = TextContext::new();
    frame(&mut s, &mut t);
    s.handle_event(pointer(PointerPhase::Down, 20.));
    s.handle_event(pointer(PointerPhase::Up, 20.));
    frame(&mut s, &mut t);
    assert_eq!((s.view.primary, s.view.secondary), (0, 0));
}

#[test]
fn disabling_between_input_and_declaration_discards_queued_activation() {
    let mut s = Source::new(Demo::default());
    let mut t = TextContext::new();
    frame(&mut s, &mut t);
    s.handle_event(pointer(PointerPhase::Down, 20.));
    s.handle_event(pointer(PointerPhase::Up, 20.));
    s.view.disabled = true;
    frame(&mut s, &mut t);
    s.view.disabled = false;
    frame(&mut s, &mut t);
    assert_eq!((s.view.primary, s.view.secondary), (0, 0));
}
