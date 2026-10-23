use plico_backend::{Source, View};
use plico_core::prelude::*;
use plico_renderer::{
    FrameSource, KeyEvent, KeyLocation, KeyPhase, Modifiers, Point, PointerButton, PointerEvent,
    PointerPhase, SemanticAction, SemanticRole, TextContext, UiEvent,
};
use std::time::Duration;
#[derive(Default)]
struct Demo {
    dialog: ModalState,
    nested: ModalState,
    edits: TextBuffer,
    background: usize,
    confirmed: usize,
    shows: usize,
    locked: bool,
}
impl View for Demo {
    #[component]
    fn show(&mut self, cx: &mut UiContext) {
        self.shows += 1;
        column(cx)
            .width(160.)
            .height(90.)
            .clip(true)
            .padding(10)
            .children(|cx| {
                if button(cx).text("Open").clicked() {
                    self.dialog.open();
                }
                if button(cx).text("Background").clicked() {
                    self.background += 1;
                }
                modal(cx, &mut self.dialog)
                    .title("Edit project")
                    .dismiss_on_escape(!self.locked)
                    .dismiss_on_backdrop(!self.locked)
                    .children(|cx| {
                        text_input(cx, "Project name", &self.edits);
                        if button(cx).text("Nested").clicked() {
                            self.nested.open();
                        }
                        modal(cx, &mut self.nested)
                            .title("Confirm change")
                            .children(|cx| {
                                if button(cx).text("Confirm").clicked() {
                                    self.confirmed += 1;
                                    cx.close_modal();
                                }
                            });
                        if button(cx).text("Cancel").clicked() {
                            cx.close_modal();
                        }
                    });
            })
    }
}
fn key(value: &str, modifiers: Modifiers) -> UiEvent {
    UiEvent::Key(KeyEvent {
        phase: KeyPhase::Down,
        key: value.into(),
        code: value.into(),
        key_without_modifiers: value.into(),
        text: None,
        text_with_all_modifiers: None,
        location: KeyLocation::Standard,
        modifiers,
        repeat: false,
        synthetic: false,
    })
}
fn click(s: &mut Source<Demo>, name: &str) -> bool {
    let id = s
        .adapter
        .semantics(&s.engine, 0)
        .node_by_role(SemanticRole::Button, name, None)
        .unwrap()
        .id;
    s.handle_semantic_action(SemanticAction::Click { target: id })
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
#[test]
fn portal_escapes_clip_traps_focus_and_restores_after_animated_nested_close() {
    let clock = plico_core::VirtualClock::default();
    let mut s = Source::with_clock(Demo::default(), clock.clone());
    let mut text = TextContext::new();
    s.build_frame(&mut text, 600, 500);
    let background_id = s
        .adapter
        .semantics(&s.engine, 0)
        .node_by_role(SemanticRole::Button, "Background", None)
        .unwrap()
        .id;
    let opener = s
        .adapter
        .semantics(&s.engine, 0)
        .node_by_role(SemanticRole::Button, "Open", None)
        .unwrap()
        .id;
    assert!(s.handle_semantic_action(SemanticAction::Focus { target: opener }));
    assert!(click(&mut s, "Open"));
    s.build_frame(&mut text, 600, 500);
    let semantics = s.adapter.semantics(&s.engine, 0);
    let dialog = semantics
        .node_by_role(SemanticRole::Dialog, "Edit project", None)
        .unwrap();
    assert_eq!(semantics.modal_root, Some(dialog.id));
    assert!(dialog.bounds[0] > 70. && dialog.bounds[2] > 400.);
    assert!(!s.handle_semantic_action(SemanticAction::Click {
        target: background_id
    }));
    let input = semantics
        .node_by_role(SemanticRole::TextInput, "Project name", None)
        .unwrap()
        .id;
    assert_eq!(semantics.focus, Some(input));
    for _ in 0..9 {
        s.handle_event(key("Tab", Modifiers::empty()));
        s.build_frame(&mut text, 600, 500);
        assert_ne!(s.adapter.semantics(&s.engine, 0).focus, Some(opener));
    }
    assert!(click(&mut s, "Nested"));
    s.build_frame(&mut text, 600, 500);
    assert!(s.view.nested.is_open());
    s.handle_event(key("Escape", Modifiers::empty()));
    s.build_frame(&mut text, 600, 500);
    assert!(!s.view.nested.is_open());
    assert!(s.view.dialog.is_open());
    assert!(s.has_anim());
    let shows = s.view.shows;
    clock.advance(Duration::from_millis(70)).unwrap();
    s.build_frame(&mut text, 600, 500);
    assert_eq!(s.view.shows, shows);
    assert!(
        s.adapter
            .semantics(&s.engine, 0)
            .node_by_role(SemanticRole::Dialog, "Confirm change", None)
            .is_some()
    );
    clock.advance(Duration::from_millis(100)).unwrap();
    s.build_frame(&mut text, 600, 500);
    assert!(
        s.adapter
            .semantics(&s.engine, 0)
            .node_by_role(SemanticRole::Dialog, "Confirm change", None)
            .is_none()
    );
    s.handle_event(key("Escape", Modifiers::empty()));
    s.build_frame(&mut text, 600, 500);
    clock.advance(Duration::from_millis(150)).unwrap();
    s.build_frame(&mut text, 600, 500);
    assert_eq!(s.adapter.semantics(&s.engine, 0).focus, Some(opener));
    assert_eq!(s.view.background, 0);
}
#[test]
fn backdrop_dismisses_without_click_through_and_close_button_unmounts() {
    let clock = plico_core::VirtualClock::default();
    let mut s = Source::with_clock(Demo::default(), clock.clone());
    let mut text = TextContext::new();
    s.build_frame(&mut text, 600, 500);
    click(&mut s, "Open");
    s.build_frame(&mut text, 600, 500);
    s.handle_event(pointer(PointerPhase::Down, 5., 5.));
    s.handle_event(pointer(PointerPhase::Cancel, 5., 5.));
    s.handle_event(pointer(PointerPhase::Up, 5., 5.));
    s.build_frame(&mut text, 600, 500);
    assert!(s.view.dialog.is_open());
    s.handle_event(pointer(PointerPhase::Down, 5., 5.));
    s.handle_event(pointer(PointerPhase::Up, 5., 5.));
    s.build_frame(&mut text, 600, 500);
    assert!(!s.view.dialog.is_open());
    assert_eq!(s.view.background, 0);
    clock.advance(Duration::from_millis(150)).unwrap();
    s.build_frame(&mut text, 600, 500);
    assert_eq!(s.adapter.semantics(&s.engine, 0).modal_root, None);
    click(&mut s, "Open");
    s.build_frame(&mut text, 600, 500);
    click(&mut s, "Close dialog");
    s.build_frame(&mut text, 600, 500);
    assert!(!s.view.dialog.is_open());
}

#[test]
fn ime_escape_cancels_composition_first_and_dismissal_policy_is_respected() {
    let mut s = Source::new(Demo::default());
    let mut text = TextContext::new();
    s.build_frame(&mut text, 600, 500);
    click(&mut s, "Open");
    s.build_frame(&mut text, 600, 500);
    s.handle_event(UiEvent::Ime(plico_renderer::ImeEvent::Preedit {
        text: "合成".into(),
        cursor: None,
    }));
    s.handle_event(key("Escape", Modifiers::empty()));
    s.build_frame(&mut text, 600, 500);
    assert!(s.view.dialog.is_open());
    assert!(!s.view.edits.is_composing());
    s.view.locked = true;
    s.request_rebuild();
    s.build_frame(&mut text, 600, 500);
    s.handle_event(key("Escape", Modifiers::empty()));
    s.build_frame(&mut text, 600, 500);
    assert!(s.view.dialog.is_open());
    s.handle_event(pointer(PointerPhase::Down, 5., 5.));
    s.handle_event(pointer(PointerPhase::Up, 5., 5.));
    s.build_frame(&mut text, 600, 500);
    assert!(s.view.dialog.is_open());
    click(&mut s, "Cancel");
    s.build_frame(&mut text, 600, 500);
    assert!(!s.view.dialog.is_open());
}

#[test]
fn small_window_scrolls_dialog_content_without_scrolling_background() {
    struct Tall {
        state: ModalState,
    }
    impl View for Tall {
        #[component]
        fn show(&mut self, cx: &mut UiContext) {
            column(cx)
                .width(120.)
                .height(100.)
                .scroll_y(true)
                .children(|cx| {
                    for i in 0..20 {
                        cx.keyed(i as u64, |cx| {
                            text(cx, "Background row");
                        });
                    }
                    modal(cx, &mut self.state)
                        .title("Long settings")
                        .children(|cx| {
                            for i in 0..30 {
                                cx.keyed(i as u64, |cx| {
                                    button(cx).text("Setting").height(30.);
                                });
                            }
                        });
                })
        }
    }
    let mut state = ModalState::default();
    state.open();
    let mut s = Source::new(Tall { state });
    let mut text = TextContext::new();
    s.build_frame(&mut text, 320, 240);
    let semantics = s.adapter.semantics(&s.engine, 0);
    let dialog = semantics
        .node_by_role(SemanticRole::Dialog, "Long settings", None)
        .unwrap();
    assert!(
        dialog.bounds[0] >= 19.
            && dialog.bounds[1] >= 19.
            && dialog.bounds[2] <= 301.
            && dialog.bounds[3] <= 221.
    );
    let wheel = |x, y| {
        UiEvent::Wheel(plico_renderer::WheelEvent {
            position: Point { x, y },
            delta_x: 0.,
            delta_y: 70.,
            delta_mode: plico_renderer::WheelDeltaMode::Pixel,
            phase: plico_renderer::GesturePhase::Changed,
            modifiers: Modifiers::empty(),
        })
    };
    s.handle_event(wheel(5., 5.));
    s.build_frame(&mut text, 320, 240);
    assert!(s.adapter.placed.iter().all(|p| p.scroll.offset[1] == 0.));
    s.handle_event(wheel(160., 120.));
    s.build_frame(&mut text, 320, 240);
    assert_eq!(
        s.adapter
            .placed
            .iter()
            .filter(|p| p.scroll.offset[1] > 0.)
            .count(),
        1
    );
}

#[test]
fn fades_sample_in_host_without_redeclaring_and_reopen_reverses_the_exit() {
    let clock = plico_core::VirtualClock::default();
    let mut s = Source::with_clock(Demo::default(), clock.clone());
    let mut text = TextContext::new();
    s.build_frame(&mut text, 600, 500);
    click(&mut s, "Open");
    s.build_frame(&mut text, 600, 500);
    let overlay = s
        .engine
        .node(s.engine.root())
        .unwrap()
        .children
        .iter()
        .copied()
        .find(|e| s.engine.node(*e).unwrap().kind == plico_core::Kind::Overlay)
        .unwrap();
    let opacity = |s: &Source<Demo>| {
        let id = s.adapter.backend_id(overlay).unwrap();
        s.adapter
            .placed
            .iter()
            .find(|p| p.node_id == id)
            .unwrap()
            .paint
            .opacity
    };
    assert_eq!(opacity(&s), 0.);
    let shows = s.view.shows;
    clock.advance(Duration::from_millis(70)).unwrap();
    s.build_frame(&mut text, 600, 500);
    assert!(opacity(&s) > 0. && opacity(&s) < 1.);
    assert_eq!(s.view.shows, shows);
    clock.advance(Duration::from_millis(100)).unwrap();
    s.build_frame(&mut text, 600, 500);
    assert_eq!(opacity(&s), 1.);
    assert_eq!(
        s.adapter
            .placed
            .iter()
            .filter(|p| p.paint.text.as_deref() == Some("Edit project"))
            .count(),
        1
    );
    s.handle_event(key("Escape", Modifiers::empty()));
    s.build_frame(&mut text, 600, 500);
    let shows = s.view.shows;
    clock.advance(Duration::from_millis(70)).unwrap();
    s.build_frame(&mut text, 600, 500);
    let halfway = opacity(&s);
    assert!(halfway > 0. && halfway < 1.);
    assert_eq!(s.view.shows, shows);
    s.view.dialog.open();
    s.request_rebuild();
    s.build_frame(&mut text, 600, 500);
    assert!((opacity(&s) - halfway).abs() < 0.001);
    clock.advance(Duration::from_millis(150)).unwrap();
    s.build_frame(&mut text, 600, 500);
    assert_eq!(opacity(&s), 1.);
    assert!(s.view.dialog.is_open());
}
