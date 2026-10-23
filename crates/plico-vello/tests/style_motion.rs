use plico_core::{Color, Ui};
use plico_renderer::{
    FrameSource, Modifiers, Point, PointerButton, PointerEvent, PointerPhase, SemanticRole,
    TextContext, UiEvent,
};
use plico_vello::{Source, View};
use std::time::{Duration, Instant};
#[derive(Default)]
struct Demo {
    expanded: bool,
    clicks: u32,
    declarations: usize,
    mounted: bool,
}
impl View for Demo {
    fn show(&mut self, ui: &mut Ui<'_>) {
        self.declarations += 1;
        if !self.mounted {
            return;
        }
        if ui
            .button("Animated")
            .key("box")
            .width(if self.expanded { 300. } else { 100. })
            .height(if self.expanded { 80. } else { 40. })
            .background(Color::Literal(if self.expanded {
                0x0000ff
            } else {
                0xff0000
            }))
            .radius(if self.expanded { 20. } else { 0. })
            .opacity(if self.expanded { 0.5 } else { 1. })
            .transition(200)
            .clicked()
        {
            self.clicks += 1;
        }
    }
}
fn pointer(phase: PointerPhase, x: f64) -> UiEvent {
    UiEvent::Pointer(PointerEvent {
        phase,
        position: Point { x, y: 10. },
        button: Some(PointerButton::Primary),
        buttons: if phase == PointerPhase::Down { 1 } else { 0 },
        modifiers: Modifiers::empty(),
        properties: Default::default(),
    })
}
#[test]
fn layout_style_and_hits_share_current_animated_geometry() {
    let now = Instant::now();
    let mut tcx = TextContext::new();
    let mut source = Source::new(Demo {
        mounted: true,
        ..Default::default()
    });
    source.build_frame_at(&mut tcx, 600, 300, now);
    source.view.expanded = true;
    source.request_rebuild();
    source.build_frame_at(&mut tcx, 600, 300, now);
    assert!(source.has_anim());
    let declarations = source.view.declarations;
    let nodes = source.build_frame_at(&mut tcx, 600, 300, now + Duration::from_millis(100));
    assert_eq!(source.view.declarations, declarations);
    let button = source
        .adapter
        .semantics(&source.engine, 0)
        .node_by_role(SemanticRole::Button, "Animated", None)
        .unwrap()
        .clone();
    assert!((button.bounds[2] - button.bounds[0] - 200.).abs() < 0.01);
    let p = nodes
        .iter()
        .find(|n| n.paint.text.as_deref() == Some("Animated"))
        .unwrap();
    assert!((p.paint.opacity - 0.75).abs() < 0.001);
    assert_eq!(p.rect[3] - p.rect[1], 60.);
    // x=250 is inside the final target, but outside the current animated box.
    source.handle_event(pointer(PointerPhase::Down, 250.));
    source.handle_event(pointer(PointerPhase::Up, 250.));
    source.build_frame_at(&mut tcx, 600, 300, now + Duration::from_millis(100));
    assert_eq!(source.view.clicks, 0);
    source.handle_event(pointer(PointerPhase::Down, 150.));
    source.handle_event(pointer(PointerPhase::Up, 150.));
    source.build_frame_at(&mut tcx, 600, 300, now + Duration::from_millis(100));
    assert_eq!(source.view.clicks, 1);
    source.build_frame_at(&mut tcx, 600, 300, now + Duration::from_millis(200));
    assert!(!source.has_anim());
    source.view.expanded = false;
    source.request_rebuild();
    source.build_frame_at(&mut tcx, 600, 300, now + Duration::from_millis(200));
    assert!(source.has_anim());
    source.view.mounted = false;
    source.request_rebuild();
    source.build_frame_at(&mut tcx, 600, 300, now + Duration::from_millis(250));
    assert!(!source.has_anim());
}
#[test]
fn reduced_motion_snaps_layout_and_paint() {
    let now = Instant::now();
    let mut tcx = TextContext::new();
    let mut source = Source::new(Demo {
        mounted: true,
        ..Default::default()
    });
    source.build_frame_at(&mut tcx, 600, 300, now);
    source.view.expanded = true;
    source.request_rebuild();
    source.build_frame_at(&mut tcx, 600, 300, now);
    assert!(source.has_anim());
    source.set_reduced_motion(true);
    let nodes = source.build_frame_at(&mut tcx, 600, 300, now);
    let p = nodes
        .iter()
        .find(|n| n.paint.text.as_deref() == Some("Animated"))
        .unwrap();
    assert_eq!(p.rect[2] - p.rect[0], 300.);
    assert_eq!(p.paint.opacity, 0.5);
    assert!(!source.has_anim());
}

#[test]
fn animated_spacing_reflows_children_and_updates_semantic_bounds() {
    struct Spacing {
        expanded: bool,
        calls: usize,
    }
    impl View for Spacing {
        fn show(&mut self, ui: &mut Ui<'_>) {
            self.calls += 1;
            ui.column()
                .key("panel")
                .width(300.)
                .padding(if self.expanded { 20 } else { 0 })
                .gap(if self.expanded { 40 } else { 0 })
                .transition(200)
                .children(|ui| {
                    ui.button("First").key("first");
                    ui.button("Second").key("second");
                });
        }
    }
    let now = Instant::now();
    let mut tcx = TextContext::new();
    let mut s = Source::new(Spacing {
        expanded: false,
        calls: 0,
    });
    s.build_frame_at(&mut tcx, 600, 300, now);
    s.view.expanded = true;
    s.request_rebuild();
    s.build_frame_at(&mut tcx, 600, 300, now);
    let calls = s.view.calls;
    s.build_frame_at(&mut tcx, 600, 300, now + Duration::from_millis(100));
    assert_eq!(s.view.calls, calls);
    let semantics = s.adapter.semantics(&s.engine, 0);
    let first = semantics
        .node_by_role(SemanticRole::Button, "First", None)
        .unwrap()
        .bounds;
    let second = semantics
        .node_by_role(SemanticRole::Button, "Second", None)
        .unwrap()
        .bounds;
    assert_eq!(first[0], 10.);
    assert_eq!(first[1], 10.);
    assert_eq!(second[1] - first[3], 20.);
}

#[test]
fn input_after_long_idle_starts_a_new_transition_instead_of_finishing_it() {
    let now = Instant::now();
    let mut tcx = TextContext::new();
    let mut s = Source::new(Demo {
        mounted: true,
        ..Default::default()
    });
    s.build_frame_at(&mut tcx, 600, 300, now);
    s.view.expanded = true;
    s.request_rebuild();
    s.prepare_for_event_at(&mut tcx, now + Duration::from_secs(60));
    let nodes = s.build_frame_at(&mut tcx, 600, 300, now + Duration::from_secs(60));
    let p = nodes
        .iter()
        .find(|n| n.paint.text.as_deref() == Some("Animated"))
        .unwrap();
    assert_eq!(p.rect[2] - p.rect[0], 100.);
    assert!(s.has_anim());
    let nodes = s.build_frame_at(&mut tcx, 600, 300, now + Duration::from_millis(60_100));
    let p = nodes
        .iter()
        .find(|n| n.paint.text.as_deref() == Some("Animated"))
        .unwrap();
    assert_eq!(p.rect[2] - p.rect[0], 200.);
}

#[test]
fn events_automatically_settle_text_declared_before_the_handler() {
    struct Counter(u32);
    impl View for Counter {
        fn show(&mut self, ui: &mut Ui<'_>) {
            ui.text(format!("Count: {}", self.0));
            if ui.button("Increment").clicked() {
                self.0 += 1;
            }
        }
    }
    let mut s = Source::new(Counter(0));
    let mut tcx = TextContext::new();
    s.build_frame(&mut tcx, 600, 300);
    let id = s
        .adapter
        .semantics(&s.engine, 0)
        .node_by_role(SemanticRole::Button, "Increment", None)
        .unwrap()
        .id;
    s.handle_semantic_action(plico_renderer::SemanticAction::Click { target: id });
    let nodes = s.build_frame(&mut tcx, 600, 300);
    assert_eq!(s.view.0, 1);
    assert!(
        nodes
            .iter()
            .any(|n| n.paint.text.as_deref() == Some("Count: 1"))
    );
    s.build_frame(&mut tcx, 600, 300);
    assert_eq!(s.view.0, 1);
}
