use plico_core::prelude::*;
use plico_renderer::{
    FrameSource, Modifiers, Point, PointerButton, PointerEvent, PointerPhase, SemanticRole,
    TextContext, UiEvent,
};
use plico_vello::{Source, View};
use std::time::Duration;
struct Demo {
    x: f32,
    scale: f32,
    opacity: f32,
    calls: usize,
    clicks: usize,
    clip: bool,
    noise: usize,
}
impl Default for Demo {
    fn default() -> Self {
        Self {
            x: 0.,
            scale: 1.,
            opacity: 1.,
            calls: 0,
            clicks: 0,
            clip: false,
            noise: 0,
        }
    }
}
impl View for Demo {
    #[component]
    fn show(&mut self, cx: &mut UiContext) {
        self.calls += 1;
        column(cx)
            .width(180.)
            .height(100.)
            .clip(self.clip)
            .children(|cx| {
                column(cx)
                    .absolute(40., 20.)
                    .width(80.)
                    .height(40.)
                    .translate(self.x, 0.)
                    .scale(self.scale, self.scale)
                    .opacity(self.opacity)
                    .motion(Motion::Smooth)
                    .children(|cx| {
                        if button(cx).text("Target").width(80.).height(40.).clicked() {
                            self.clicks += 1;
                        }
                    });
                for i in 0..self.noise {
                    text(cx, "Unchanged").key(i as u64);
                }
            })
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
#[test]
fn host_motion_updates_nested_paint_hits_and_semantics_without_layout_or_declaration() {
    let clock = plico_core::VirtualClock::default();
    let mut s = Source::with_clock(Demo::default(), clock.clone());
    let mut tcx = TextContext::new();
    s.build_frame(&mut tcx, 400, 200);
    s.view.x = 100.;
    s.view.scale = 2.;
    s.build_frame(&mut tcx, 400, 200);
    let calls = s.view.calls;
    clock.advance(Duration::from_millis(120)).unwrap();
    s.build_frame(&mut tcx, 400, 200);
    assert_eq!(s.view.calls, calls);
    assert_eq!(s.adapter.stats.layout_runs, 0);
    assert_eq!(s.adapter.stats.text_measure_invalidations, 0);
    assert_eq!(s.adapter.stats.style_writes, 0);
    assert_eq!(s.adapter.stats.flatten_visits, 0);
    let tree = s.adapter.semantics(&s.engine, 0);
    let bounds = tree
        .node_by_role(SemanticRole::Button, "Target", None)
        .unwrap()
        .bounds;
    assert!((bounds[0] - 70.).abs() < 0.01, "{bounds:?}");
    assert!((bounds[2] - 190.).abs() < 0.01);
    // Presented area, outside the button's unchanged layout rectangle.
    s.handle_event(pointer(PointerPhase::Down, 170., 40.));
    s.handle_event(pointer(PointerPhase::Up, 170., 40.));
    s.build_frame(&mut tcx, 400, 200);
    assert_eq!(s.view.clicks, 1);
    clock.advance(Duration::from_millis(120)).unwrap();
    s.build_frame(&mut tcx, 400, 200);
    assert!(!s.has_anim());
}
#[test]
fn transformed_children_obey_ancestor_clip_and_zero_scale_is_not_hittable() {
    let clock = plico_core::VirtualClock::default();
    let mut s = Source::with_clock(
        Demo {
            clip: true,
            ..Demo::default()
        },
        clock.clone(),
    );
    let mut tcx = TextContext::new();
    s.build_frame(&mut tcx, 400, 200);
    s.view.x = 150.;
    s.build_frame(&mut tcx, 400, 200);
    clock.advance(Duration::from_millis(240)).unwrap();
    s.build_frame(&mut tcx, 400, 200);
    s.handle_event(pointer(PointerPhase::Down, 210., 40.));
    s.handle_event(pointer(PointerPhase::Up, 210., 40.));
    s.build_frame(&mut tcx, 400, 200);
    assert_eq!(s.view.clicks, 0);
    s.view.clip = false;
    s.view.scale = 0.;
    s.build_frame(&mut tcx, 400, 200);
    clock.advance(Duration::from_millis(240)).unwrap();
    s.build_frame(&mut tcx, 400, 200);
    s.handle_event(pointer(PointerPhase::Down, 230., 40.));
    s.handle_event(pointer(PointerPhase::Up, 230., 40.));
    s.build_frame(&mut tcx, 400, 200);
    assert_eq!(s.view.clicks, 0);
}

#[derive(Default)]
struct SemanticOnly {
    selected: bool,
}
impl View for SemanticOnly {
    #[component]
    fn show(&mut self, cx: &mut UiContext) {
        button_base(cx, "Choice")
            .selected(self.selected)
            .children(|cx| {
                text(cx, "Choice");
            });
    }
}
#[test]
fn semantic_only_change_does_not_write_paint_or_layout() {
    let mut s = Source::new(SemanticOnly::default());
    let mut tcx = TextContext::new();
    s.build_frame(&mut tcx, 400, 200);
    s.view.selected = true;
    s.build_frame(&mut tcx, 400, 200);
    assert_eq!(s.engine.changes(), plico_core::Changes::SEMANTICS);
    assert_eq!(s.adapter.stats.paint_writes, 0);
    assert_eq!(s.adapter.stats.layout_runs, 0);
    assert_eq!(s.adapter.stats.flatten_visits, 0);
    assert!(
        s.adapter
            .semantics(&s.engine, 0)
            .node_by_role(SemanticRole::Button, "Choice", None)
            .unwrap()
            .states
            .selected
            .unwrap()
    );
}

#[test]
fn a_small_transform_only_updates_its_subtree_among_many_static_nodes() {
    let clock = plico_core::VirtualClock::default();
    let mut s = Source::with_clock(
        Demo {
            noise: 1000,
            ..Demo::default()
        },
        clock.clone(),
    );
    let mut tcx = TextContext::new();
    s.build_frame(&mut tcx, 400, 200);
    s.view.x = 100.;
    s.build_frame(&mut tcx, 400, 200);
    clock.advance(Duration::from_millis(120)).unwrap();
    s.build_frame(&mut tcx, 400, 200);
    // Animated container, button container, and its text content.
    assert_eq!(s.adapter.stats.presentation_visits, 3);
    assert_eq!(s.adapter.stats.layout_runs, 0);
    assert_eq!(s.adapter.stats.flatten_visits, 0);
}

#[derive(Default)]
struct CollapsedChild {
    x: f32,
    opacity: f32,
}
impl View for CollapsedChild {
    #[component]
    fn show(&mut self, cx: &mut UiContext) {
        column(cx)
            .width(200.)
            .height(100.)
            .translate(self.x, 0.)
            .opacity(self.opacity)
            .motion(Motion::Smooth)
            .children(|cx| {
                column(cx).height(0.).clip(true).shrink(0.).children(|cx| {
                    text(cx, "Clipped descendant");
                });
                button(cx).text("Visible sibling");
            })
    }
}
#[test]
fn presentation_motion_only_visits_placed_nodes_when_children_are_collapsed() {
    let clock = plico_core::VirtualClock::default();
    let mut s = Source::with_clock(
        CollapsedChild {
            opacity: 1.,
            ..Default::default()
        },
        clock.clone(),
    );
    let mut tcx = TextContext::new();
    s.build_frame(&mut tcx, 400, 200);
    assert!(
        !s.adapter
            .placed
            .iter()
            .any(|p| p.paint.text.as_deref() == Some("Clipped descendant"))
    );
    let sibling = s
        .adapter
        .placed
        .iter()
        .find(|p| p.paint.text.as_deref() == Some("Visible sibling"))
        .unwrap()
        .node_id;
    let old = s.adapter.visual_bounds(sibling).unwrap();
    s.view.x = 80.;
    s.view.opacity = 0.5;
    s.build_frame(&mut tcx, 400, 200);
    clock.advance(Duration::from_millis(120)).unwrap();
    s.build_frame(&mut tcx, 400, 200);
    let moved = s.adapter.visual_bounds(sibling).unwrap();
    assert!(moved[0] > old[0] && moved[0] < old[0] + 80.);
    assert_eq!(s.adapter.stats.layout_runs, 0);
    assert_eq!(s.adapter.stats.flatten_visits, 0);
    clock.advance(Duration::from_millis(120)).unwrap();
    s.build_frame(&mut tcx, 400, 200);
    assert!((s.adapter.visual_bounds(sibling).unwrap()[0] - old[0] - 80.).abs() < 0.01);
    assert!(!s.has_anim());
}
