use kurbo::{Rect, Shape};
use plico::prelude::*;
use plico::{Entity, VirtualClock};
use plico_desktop::{Source, View};
use plico_renderer::layout::PlacedNode;
use plico_renderer::{
    FrameSource, KeyEvent, KeyLocation, KeyPhase, Modifiers, PaintCommand, SemanticAction,
    SemanticRole, SemanticToggleState, TextContext, UiEvent,
};
use std::time::Duration;
#[derive(Default)]
struct Demo {
    checked: bool,
    locked: bool,
    compact: bool,
    gap: Option<u16>,
    actions: usize,
}
impl View for Demo {
    #[component]
    fn show(&mut self, cx: &mut UiContext) {
        column(cx).gap(8).children(|cx| {
            let mut control = switch(cx, "Editable", &mut self.checked)
                .disabled(self.locked)
                .width(240.);
            if let Some(gap) = self.gap {
                control = control.gap(gap);
            }
            if control.changed() {
                self.actions += 1;
            }
            checkbox(cx, "Locked", &mut self.checked)
                .disabled(true)
                .width(240.);
        })
    }
    fn theme(&self) -> Theme {
        let mut theme = Theme::light();
        if self.compact {
            theme.metrics = Metrics::compact();
        }
        theme
    }
}
fn placed<'a>(s: &'a Source<Demo>, label: &str) -> &'a PlacedNode {
    s.adapter
        .placed
        .iter()
        .find(|p| p.paint.text.as_deref() == Some(label))
        .unwrap()
}
fn indicator(p: &PlacedNode) -> Rect {
    match &p.paint.widget.as_ref().unwrap().commands[0] {
        PaintCommand::Fill { shape, .. } => shape.bounding_box(),
        _ => panic!("expected indicator"),
    }
}
fn key(key: &str) -> UiEvent {
    UiEvent::Key(KeyEvent {
        phase: KeyPhase::Down,
        key: key.into(),
        key_without_modifiers: key.into(),
        code: key.into(),
        text: None,
        text_with_all_modifiers: None,
        location: KeyLocation::Standard,
        modifiers: Modifiers::empty(),
        repeat: false,
        synthetic: false,
    })
}
fn entity(s: &Source<Demo>, label: &str) -> Entity {
    let root = s.engine.node(s.engine.root()).unwrap().children[0];
    s.engine
        .node(root)
        .unwrap()
        .children
        .iter()
        .copied()
        .find(|id| s.engine.node(*id).unwrap().text == label)
        .unwrap()
}
#[test]
fn keyboard_and_semantic_actions_apply_final_disabled_config_and_toggle_once() {
    let mut s = Source::new(Demo::default());
    let mut tcx = TextContext::new();
    s.build_frame(&mut tcx, 400, 240);
    s.handle_event(key("Tab"));
    s.prepare_for_event(&mut tcx);
    assert_eq!(s.adapter.focused(), Some(entity(&s, "Editable")));
    s.handle_event(key("Space"));
    s.prepare_for_event(&mut tcx);
    s.build_frame(&mut tcx, 400, 240);
    assert!(s.view.checked);
    assert_eq!(s.view.actions, 1);
    let semantics = s.adapter.semantics(&s.engine, 0);
    let locked = semantics
        .node_by_role(SemanticRole::CheckBox, "Locked", None)
        .unwrap();
    assert!(locked.disabled);
    assert_eq!(locked.states.checked, Some(SemanticToggleState::On));
    assert!(!s.handle_semantic_action(SemanticAction::Click { target: locked.id }));
    s.handle_event(key("Tab"));
    s.prepare_for_event(&mut tcx);
    assert_eq!(s.adapter.focused(), Some(entity(&s, "Editable"))); // skips the disabled checkbox
    s.view.locked = true;
    s.request_rebuild();
    s.build_frame(&mut tcx, 400, 240);
    assert!(s.adapter.focused().is_none());
    assert!(!s.handle_event(key("Space")).handled);
    s.build_frame(&mut tcx, 400, 240);
    assert_eq!(s.view.actions, 1);
}
#[test]
fn density_changes_indicator_and_label_geometry_together() {
    let mut s = Source::new(Demo::default());
    s.set_reduced_motion(true);
    let mut tcx = TextContext::new();
    s.build_frame(&mut tcx, 400, 240);
    let full_switch = placed(&s, "Editable");
    let full_indicator = indicator(full_switch);
    assert_eq!(
        (full_indicator.width(), full_indicator.height()),
        (36., 20.)
    );
    assert_eq!(full_switch.content_origin[0] - full_switch.rect[0], 50.);
    assert_eq!(indicator(placed(&s, "Locked")).width(), 18.);
    s.view.compact = true;
    s.request_rebuild();
    s.build_frame(&mut tcx, 400, 240);
    let compact = placed(&s, "Editable");
    let compact_indicator = indicator(compact);
    assert_eq!(
        (compact_indicator.width(), compact_indicator.height()),
        (27., 15.)
    );
    assert_eq!(compact.content_origin[0] - compact.rect[0], 38.);
    assert!(compact.rect[3] - compact.rect[1] >= Metrics::compact().control_height as f32);
    assert!(compact.rect[3] - compact.rect[1] < 36.);
    assert_eq!(indicator(placed(&s, "Locked")).width(), 13.5);
    assert!(!s.has_anim());
}
#[test]
fn animated_gap_keeps_the_indicator_anchored_to_padding() {
    let clock = VirtualClock::default();
    let mut s = Source::with_clock(Demo::default(), clock.clone());
    let mut tcx = TextContext::new();
    s.build_frame(&mut tcx, 400, 240);
    s.view.gap = Some(20);
    s.request_rebuild();
    s.build_frame(&mut tcx, 400, 240);
    let start = placed(&s, "Editable").content_origin[0];
    clock.set(Duration::from_millis(70)).unwrap();
    s.build_frame(&mut tcx, 400, 240);
    let mid = placed(&s, "Editable");
    assert!(mid.content_origin[0] > start && mid.content_origin[0] < start + 14.);
    assert!(
        (mid.content_origin[0] as f64 + indicator(mid).x0 - mid.rect[0] as f64 - 8.).abs() < 0.001
    );
    clock.set(Duration::from_millis(140)).unwrap();
    s.build_frame(&mut tcx, 400, 240);
    let end = placed(&s, "Editable");
    assert!((end.content_origin[0] - start - 14.).abs() < 0.001);
    assert!(!s.has_anim());
}
