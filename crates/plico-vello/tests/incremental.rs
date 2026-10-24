use plico_core::{Changes, UiContext, VirtualClock};
use plico_renderer::{FrameSource, Scene, TextContext};
use plico_vello::{Source, View};
use std::time::Duration;
struct Sparse {
    labels: Vec<String>,
    fade: bool,
    wider: bool,
    calls: usize,
}
impl Sparse {
    fn new() -> Self {
        Self {
            labels: (0..1000).map(|i| format!("Row {i}")).collect(),
            fade: false,
            wider: false,
            calls: 0,
        }
    }
}
impl View for Sparse {
    fn show(&mut self, ui: &mut UiContext<'_>) {
        self.calls += 1;
        for (i, label) in self.labels.iter().enumerate() {
            ui.text(label.as_str())
                .key(i)
                .width(if i == 0 && self.wider { 300. } else { 100. })
                .height(24.)
                .opacity(if i == 0 && self.fade { 0.2 } else { 1. })
                .transition(200);
        }
    }
}
fn paint(
    s: &mut Source<Sparse>,
    text: &mut TextContext,
    nodes: &[plico_renderer::layout::PlacedNode],
) {
    s.paint_scene(&mut Scene::new(), nodes, text, (800, 30000), 1.);
}
#[test]
fn sparse_paint_animation_touches_only_active_node_and_skips_layout() {
    let clock = VirtualClock::default();
    let mut s = Source::with_clock(Sparse::new(), clock.clone());
    let mut text = TextContext::new();
    let nodes = s.build_frame(&mut text, 800, 30000);
    paint(&mut s, &mut text, &nodes);
    let geometry: Vec<_> = nodes.iter().map(|n| n.rect).collect();
    let nodes = s.build_frame(&mut text, 800, 30000);
    paint(&mut s, &mut text, &nodes);
    assert_eq!(s.adapter.stats.sync_visits, 0);
    assert_eq!(s.adapter.stats.layout_runs, 0);
    assert_eq!(s.adapter.stats.scene_reuses, 1);
    s.view.fade = true;
    s.request_rebuild();
    s.build_frame(&mut text, 800, 30000);
    let calls = s.view.calls;
    for ms in [100, 100] {
        clock.advance(Duration::from_millis(ms)).unwrap();
        let nodes = s.build_frame(&mut text, 800, 30000);
        paint(&mut s, &mut text, &nodes);
        let stats = s.adapter.stats;
        assert_eq!(s.view.calls, calls);
        assert_eq!(stats.sync_visits, 1);
        assert_eq!(stats.layout_style_builds, 0);
        assert_eq!(stats.layout_runs, 0);
        assert_eq!(stats.flatten_visits, 0);
        assert_eq!(stats.paint_patches, 1);
        assert!(stats.changes.contains(Changes::PAINT));
        assert!(
            !stats
                .changes
                .intersects(Changes::LAYOUT | Changes::STRUCTURE)
        );
        assert_eq!(geometry, nodes.iter().map(|n| n.rect).collect::<Vec<_>>());
    }
    assert!(!s.has_anim());
    let nodes = s.build_frame(&mut text, 800, 30000);
    paint(&mut s, &mut text, &nodes);
    assert_eq!(s.adapter.stats.sync_visits, 0);
    assert_eq!(s.adapter.stats.scene_reuses, 1);
}
#[test]
fn sparse_layout_animation_syncs_one_node_and_commits_current_geometry() {
    let clock = VirtualClock::default();
    let mut s = Source::with_clock(Sparse::new(), clock.clone());
    let mut text = TextContext::new();
    s.build_frame(&mut text, 800, 30000);
    s.view.wider = true;
    s.request_rebuild();
    s.build_frame(&mut text, 800, 30000);
    let calls = s.view.calls;
    clock.advance(Duration::from_millis(100)).unwrap();
    let nodes = s.build_frame(&mut text, 800, 30000);
    assert_eq!(s.view.calls, calls);
    assert_eq!(s.adapter.stats.sync_visits, 1);
    assert_eq!(s.adapter.stats.layout_style_builds, 1);
    assert_eq!(s.adapter.stats.layout_runs, 1);
    assert_eq!(s.adapter.stats.flatten_visits, nodes.len());
    let first = nodes
        .iter()
        .find(|n| n.paint.text.as_deref() == Some("Row 0"))
        .unwrap();
    assert_eq!(first.rect[2] - first.rect[0], 200.);
    clock.advance(Duration::from_millis(100)).unwrap();
    s.build_frame(&mut text, 800, 30000);
    assert!(!s.has_anim());
}
#[test]
fn declaration_journal_survives_settling_and_patches_only_the_changed_label() {
    struct Counter(u32);
    impl View for Counter {
        fn show(&mut self, ui: &mut UiContext<'_>) {
            ui.text(format!("Count: {}", self.0)).key("count");
            if ui.button("Increment").key("increment").clicked() {
                self.0 += 1;
            }
        }
    }
    let mut s = Source::new(Counter(0));
    let mut text = TextContext::new();
    s.build_frame(&mut text, 400, 300);
    let id = s
        .adapter
        .semantics(&s.engine, 0)
        .node_by_role(plico_renderer::SemanticRole::Button, "Increment", None)
        .unwrap()
        .id;
    s.handle_semantic_action(plico_renderer::SemanticAction::Click { target: id });
    let nodes = s.build_frame(&mut text, 400, 300);
    assert_eq!(s.adapter.stats.sync_visits, 1);
    assert_eq!(s.adapter.stats.layout_style_builds, 0);
    assert_eq!(s.adapter.stats.layout_runs, 1);
    assert!(
        nodes
            .iter()
            .any(|n| n.paint.text.as_deref() == Some("Count: 1"))
    );
}
#[test]
fn disabling_transition_midflight_snaps_geometry_and_paint_without_losing_dirty_nodes() {
    let clock = VirtualClock::default();
    let mut s = Source::with_clock(Sparse::new(), clock.clone());
    let mut text = TextContext::new();
    s.build_frame(&mut text, 800, 30000);
    s.view.fade = true;
    s.view.wider = true;
    s.request_rebuild();
    s.build_frame(&mut text, 800, 30000);
    clock.advance(Duration::from_millis(50)).unwrap();
    s.build_frame(&mut text, 800, 30000);
    s.set_reduced_motion(true);
    let nodes = s.build_frame(&mut text, 800, 30000);
    let first = nodes
        .iter()
        .find(|n| n.paint.text.as_deref() == Some("Row 0"))
        .unwrap();
    assert_eq!(first.rect[2] - first.rect[0], 300.);
    assert_eq!(first.paint.opacity, 0.2);
    assert!(!s.has_anim());
}

#[test]
fn failed_declaration_does_not_hide_property_changes_from_the_next_commit() {
    struct Repair {
        width: f32,
        invalid: bool,
    }
    impl View for Repair {
        fn show(&mut self, ui: &mut UiContext<'_>) {
            ui.text("Box").key("box").width(self.width);
            if self.invalid {
                ui.text("Duplicate").key("box");
            }
        }
    }
    let mut s = Source::new(Repair {
        width: 100.,
        invalid: false,
    });
    let mut text = TextContext::new();
    s.build_frame(&mut text, 400, 300);
    s.view.width = 300.;
    s.view.invalid = true;
    s.request_rebuild();
    s.build_frame(&mut text, 400, 300);
    assert!(s.last_error.is_some());
    s.view.invalid = false;
    s.request_rebuild();
    let nodes = s.build_frame(&mut text, 400, 300);
    assert!(s.last_error.is_none());
    let p = nodes
        .iter()
        .find(|n| n.paint.text.as_deref() == Some("Box"))
        .unwrap();
    assert_eq!(p.rect[2] - p.rect[0], 300.);
    assert!(s.adapter.stats.changes.contains(Changes::STRUCTURE));
}

#[test]
fn manual_declaration_preserves_pending_backend_work_until_commit() {
    struct Width(f32);
    impl View for Width {
        fn show(&mut self, ui: &mut UiContext<'_>) {
            ui.text("Box").width(self.0);
        }
    }
    let mut s = Source::new(Width(100.));
    let mut text = TextContext::new();
    s.build_frame(&mut text, 400, 300);
    s.view.0 = 300.;
    s.try_declare().unwrap();
    let nodes = s.build_frame(&mut text, 400, 300);
    let p = nodes
        .iter()
        .find(|n| n.paint.text.as_deref() == Some("Box"))
        .unwrap();
    assert_eq!(p.rect[2] - p.rect[0], 300.);
}

#[test]
fn interaction_and_disabled_paint_update_without_declared_geometry_changes() {
    struct Controls(bool);
    impl View for Controls {
        fn show(&mut self, ui: &mut UiContext<'_>) {
            ui.button("A").key("a").disabled(self.0);
            ui.button("B").key("b");
        }
    }
    use plico_renderer::{SemanticAction, SemanticRole};
    let clock = VirtualClock::default();
    let mut s = Source::with_clock(Controls(false), clock.clone());
    let mut text = TextContext::new();
    s.build_frame(&mut text, 400, 300);
    let snapshot = s.adapter.semantics(&s.engine, 0);
    let a = snapshot
        .node_by_role(SemanticRole::Button, "A", None)
        .unwrap()
        .id;
    let b = snapshot
        .node_by_role(SemanticRole::Button, "B", None)
        .unwrap()
        .id;
    assert!(s.handle_semantic_action(SemanticAction::Focus { target: a }));
    s.build_frame(&mut text, 400, 300);
    assert_eq!(s.adapter.stats.sync_visits, 1);
    assert_eq!(s.adapter.stats.layout_runs, 0);
    clock.advance(Duration::from_millis(200)).unwrap();
    let nodes = s.build_frame(&mut text, 400, 300);
    assert_eq!(
        nodes
            .iter()
            .find(|n| n.paint.text.as_deref() == Some("A"))
            .unwrap()
            .paint
            .outline_width,
        2.
    );
    s.view.0 = true;
    s.request_rebuild();
    s.build_frame(&mut text, 400, 300);
    clock.advance(Duration::from_millis(200)).unwrap();
    let nodes = s.build_frame(&mut text, 400, 300);
    assert!(s.adapter.semantics(&s.engine, 0).focus.is_none());
    assert_eq!(s.adapter.stats.layout_runs, 0);
    assert_eq!(
        nodes
            .iter()
            .find(|n| n.paint.text.as_deref() == Some("A"))
            .unwrap()
            .paint
            .outline_width,
        0.
    );
    assert!(!s.handle_semantic_action(SemanticAction::Click { target: a }));
    assert!(s.handle_semantic_action(SemanticAction::Focus { target: b }));
    s.build_frame(&mut text, 400, 300);
    assert_eq!(s.adapter.stats.sync_visits, 1);
    assert_eq!(s.adapter.stats.layout_runs, 0);
}

#[test]
fn retained_presentation_exports_no_nodes_and_shared_scene_cache_is_immutable() {
    let clock = VirtualClock::default();
    let mut s = Source::with_clock(Sparse::new(), clock.clone());
    let mut text = TextContext::new();
    let mut scene = Scene::new();
    let first = s
        .render_retained_frame(&mut scene, &mut text, (800, 30000), 1.)
        .unwrap();
    assert_eq!(first.0, 1001);
    assert_eq!(s.adapter.stats.placed_node_clones, 0);
    let original = scene.clone();
    s.render_retained_frame(&mut scene, &mut text, (800, 30000), 1.);
    assert_eq!(s.adapter.stats.placed_node_clones, 0);
    assert_eq!(s.adapter.stats.scene_reuses, 1);
    assert_eq!(original.commands.as_ptr(), scene.commands.as_ptr());
    s.view.fade = true;
    s.request_rebuild();
    s.render_retained_frame(&mut scene, &mut text, (800, 30000), 1.);
    let calls = s.view.calls;
    clock.advance(Duration::from_millis(100)).unwrap();
    s.render_retained_frame(&mut scene, &mut text, (800, 30000), 1.);
    assert_eq!(s.view.calls, calls);
    assert_eq!(s.adapter.stats.sync_visits, 1);
    assert_eq!(s.adapter.stats.placed_node_clones, 0);
    assert_eq!(s.adapter.stats.layout_runs, 0);
    assert_ne!(original.commands, scene.commands);
    // The former immutable commands remain usable after subsequent paints.
    assert_ne!(original.commands.as_ptr(), scene.commands.as_ptr());
}
#[test]
fn event_preparation_commits_geometry_without_exporting_a_snapshot() {
    let mut s = Source::new(Sparse::new());
    let mut text = TextContext::new();
    let mut scene = Scene::new();
    s.render_retained_frame(&mut scene, &mut text, (800, 30000), 1.);
    s.view.wider = true;
    s.request_rebuild();
    s.prepare_for_event(&mut text);
    assert_eq!(s.adapter.stats.placed_node_clones, 0);
}
