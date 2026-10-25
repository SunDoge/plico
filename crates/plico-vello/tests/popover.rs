use plico_core::prelude::*;
use plico_renderer::{
    FrameSource, KeyEvent, KeyLocation, KeyPhase, Modifiers, SemanticAction, SemanticRole,
    TextContext, UiEvent,
};
use plico_vello::{Source, View};
use std::time::Duration;
#[derive(Default)]
struct Demo {
    popup: PopoverState,
    clicks: usize,
    disabled: bool,
    shows: usize,
}
impl View for Demo {
    #[component]
    fn show(&mut self, cx: &mut UiContext) {
        self.shows += 1;
        column(cx)
            .width_percent(100.)
            .height_percent(100.)
            .padding(12)
            .justify(Justify::End)
            .align(Align::End)
            .children(|cx| {
                popover(cx, &mut self.popup, "Options")
                    .width(240.)
                    .disabled(self.disabled)
                    .motion(Motion::Quick)
                    .children(|cx| {
                        text(cx, "Project options");
                        if button(cx).text("Apply").clicked() {
                            self.clicks += 1;
                            cx.close_popover();
                        }
                        button(cx).text("Unavailable").disabled(true);
                    });
                button(cx).text("After");
            })
    }
}
fn action(s: &mut Source<Demo>, label: &str, focus: bool) {
    let id = s
        .adapter
        .semantics(&s.engine, 0)
        .node_by_role(SemanticRole::Button, label, None)
        .unwrap()
        .id;
    assert!(s.handle_semantic_action(if focus {
        SemanticAction::Focus { target: id }
    } else {
        SemanticAction::Click { target: id }
    }));
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
#[test]
fn anchored_panel_flips_clamps_and_routes_action_in_placed_geometry() {
    let mut s = Source::new(Demo::default());
    let mut tcx = TextContext::new();
    s.build_frame(&mut tcx, 320, 240);
    action(&mut s, "Options", true);
    action(&mut s, "Options", false);
    s.build_frame(&mut tcx, 320, 240);
    let snapshot = s.adapter.semantics(&s.engine, 0);
    let trigger = snapshot
        .node_by_role(SemanticRole::Button, "Options", None)
        .unwrap();
    let panel = snapshot
        .node_by_role(SemanticRole::Dialog, "Options", None)
        .unwrap();
    assert_eq!(panel.states.modal, Some(false));
    assert_eq!(snapshot.modal_root, None);
    assert!(panel.bounds[3] <= trigger.bounds[1] - 5.);
    assert!(panel.bounds[0] >= 8. && panel.bounds[2] <= 312.);
    let apply = snapshot
        .node_by_role(SemanticRole::Button, "Apply", None)
        .unwrap();
    assert_eq!(snapshot.focus, Some(apply.id));
    assert!(apply.bounds[1] >= panel.bounds[1] && apply.bounds[3] <= panel.bounds[3]);
    action(&mut s, "Apply", false);
    s.build_frame(&mut tcx, 320, 240);
    assert_eq!(s.view.clicks, 1);
    assert!(!s.view.popup.is_open());
    assert_eq!(s.adapter.semantics(&s.engine, 0).focus, Some(trigger.id));
}
#[test]
fn tab_leaves_popup_and_exit_does_not_steal_focus() {
    let clock = plico_core::VirtualClock::default();
    let mut s = Source::with_clock(Demo::default(), clock.clone());
    let mut tcx = TextContext::new();
    s.build_frame(&mut tcx, 600, 400);
    action(&mut s, "Options", true);
    action(&mut s, "Options", false);
    s.build_frame(&mut tcx, 600, 400);
    assert!(s.handle_event(key("Tab")).handled);
    s.build_frame(&mut tcx, 600, 400);
    assert!(!s.view.popup.is_open());
    let snapshot = s.adapter.semantics(&s.engine, 0);
    let after = snapshot
        .node_by_role(SemanticRole::Button, "After", None)
        .unwrap()
        .id;
    assert_eq!(snapshot.focus, Some(after));
    clock.advance(Duration::from_secs(1)).unwrap();
    s.build_frame(&mut tcx, 600, 400);
    assert_eq!(s.adapter.semantics(&s.engine, 0).focus, Some(after));
}
#[test]
fn disabled_trigger_does_not_open_and_popup_state_is_portable() {
    let state: PopoverState = serde_json::from_str("{\"open\":true}").unwrap();
    let mut s = Source::new(Demo {
        popup: state,
        disabled: true,
        ..Default::default()
    });
    s.build_frame(&mut TextContext::new(), 180, 100);
    assert!(!s.view.popup.is_open());
    assert!(
        !s.engine
            .snapshot()
            .iter()
            .any(|n| n.style.popup_anchor.is_some())
    );
}

#[test]
fn pointer_uses_positioned_panel_and_resize_reanchors_without_ui_state_reset() {
    use plico_renderer::{Point, PointerButton, PointerEvent, PointerPhase};
    let mut s = Source::new(Demo::default());
    let mut tcx = TextContext::new();
    s.build_frame(&mut tcx, 600, 400);
    action(&mut s, "Options", false);
    s.build_frame(&mut tcx, 600, 400);
    s.build_frame(&mut tcx, 260, 220);
    let snapshot = s.adapter.semantics(&s.engine, 0);
    let panel = snapshot
        .node_by_role(SemanticRole::Dialog, "Options", None)
        .unwrap();
    assert!(panel.bounds[0] >= 8. && panel.bounds[2] <= 252.);
    let item = snapshot
        .node_by_role(SemanticRole::Button, "Apply", None)
        .unwrap();
    let x = (item.bounds[0] + item.bounds[2]) as f64 / 2.;
    let y = (item.bounds[1] + item.bounds[3]) as f64 / 2.;
    for phase in [PointerPhase::Down, PointerPhase::Up] {
        s.handle_event(UiEvent::Pointer(PointerEvent {
            phase,
            position: Point { x, y },
            button: Some(PointerButton::Primary),
            buttons: 1,
            modifiers: Modifiers::empty(),
            properties: Default::default(),
        }));
    }
    s.build_frame(&mut tcx, 260, 220);
    assert_eq!(s.view.clicks, 1);
    assert!(!s.view.popup.is_open());
}

#[derive(Default)]
struct Nested {
    dialog: ModalState,
    popup: PopoverState,
}
impl View for Nested {
    #[component]
    fn show(&mut self, cx: &mut UiContext) {
        button(cx).text("Background");
        modal(cx, &mut self.dialog)
            .title("Parent")
            .motion(Motion::None)
            .children(|cx| {
                popover(cx, &mut self.popup, "Child popup")
                    .motion(Motion::None)
                    .children(|cx| {
                        button(cx).text("Child action");
                    });
                button(cx).text("Parent action");
            });
    }
}
#[test]
fn popup_inside_modal_is_exposed_to_accessibility_and_escape_restores_parent_scope() {
    let mut view = Nested::default();
    view.dialog.open();
    let mut s = Source::new(view);
    let mut tcx = TextContext::new();
    s.build_frame(&mut tcx, 600, 400);
    let trigger = s
        .adapter
        .semantics(&s.engine, 0)
        .node_by_role(SemanticRole::Button, "Child popup", None)
        .unwrap()
        .id;
    assert!(s.handle_semantic_action(SemanticAction::Click { target: trigger }));
    s.build_frame(&mut tcx, 600, 400);
    let snapshot = s.adapter.semantics(&s.engine, 0);
    assert!(
        snapshot
            .node_by_role(SemanticRole::Button, "Child action", None)
            .is_some()
    );
    assert!(
        snapshot
            .node_by_role(SemanticRole::Button, "Background", None)
            .is_none()
    );
    let parent_action = snapshot
        .node_by_role(SemanticRole::Button, "Parent action", None)
        .unwrap()
        .id;
    assert!(!s.handle_semantic_action(SemanticAction::Click {
        target: parent_action
    }));
    assert_eq!(
        snapshot
            .node_by_role(SemanticRole::Button, "Child popup", None)
            .unwrap()
            .controls
            .len(),
        1
    );
    s.handle_event(key("Escape"));
    s.build_frame(&mut tcx, 600, 400);
    assert!(s.view.dialog.is_open());
    assert!(!s.view.popup.is_open());
    assert_eq!(s.adapter.semantics(&s.engine, 0).focus, Some(trigger));
    assert!(s.handle_semantic_action(SemanticAction::Click {
        target: parent_action
    }));
}

#[test]
fn popup_animation_frames_keep_application_and_layout_idle() {
    let clock = plico_core::VirtualClock::default();
    let mut s = Source::with_clock(Demo::default(), clock.clone());
    let mut tcx = TextContext::new();
    s.build_frame(&mut tcx, 600, 400);
    action(&mut s, "Options", false);
    s.build_frame(&mut tcx, 600, 400);
    let before = s.view.shows;
    clock.advance(Duration::from_millis(70)).unwrap();
    s.build_frame(&mut tcx, 600, 400);
    assert_eq!(s.view.shows, before);
    assert_eq!(s.adapter.stats.layout_runs, 0);
}

#[test]
fn automatic_popup_focus_preserves_pointer_or_keyboard_modality() {
    let clock = plico_core::VirtualClock::default();
    let mut source = Source::with_clock(Demo::default(), clock.clone());
    let mut text = TextContext::new();
    source.build_frame(&mut text, 400, 400);
    action(&mut source, "Options", false);
    source.build_frame(&mut text, 400, 400);
    clock.advance(Duration::from_secs(1)).unwrap();
    let nodes = source.build_frame(&mut text, 400, 400);
    assert!(source.view.popup.is_open());
    assert!(
        nodes.iter().all(|n| n.paint.outline_width == 0.),
        "automatic popup focus must not opt pointer users into focus rings"
    );
    action(&mut source, "Apply", false);
    source.build_frame(&mut text, 400, 400);
    clock.advance(Duration::from_secs(1)).unwrap();
    let nodes = source.build_frame(&mut text, 400, 400);
    assert!(
        nodes.iter().all(|n| n.paint.outline_width == 0.),
        "restoring focus must preserve pointer modality too"
    );
    action(&mut source, "Options", true);
    action(&mut source, "Options", false);
    source.build_frame(&mut text, 400, 400);
    clock.advance(Duration::from_secs(1)).unwrap();
    let nodes = source.build_frame(&mut text, 400, 400);
    assert!(
        nodes.iter().any(|n| n.paint.outline_width > 0.),
        "keyboard focus must remain visible inside the popup"
    );
}
