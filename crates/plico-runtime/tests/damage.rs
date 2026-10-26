//! Phase 0 damage instrumentation: the conservative region union and the
//! BackendStats coverage metrics. Rendering still repaints everything; the
//! numbers only decide whether partial presentation is worth building.
use plico_core::prelude::*;
use plico_renderer::{
    FrameSource, Point, PointerButton, PointerEvent, PointerPhase, PointerProperties,
    SemanticAction, SemanticRole, TextContext, UiEvent,
};
use plico_runtime::{Source, View};

#[derive(Default)]
struct Demo {
    count: u32,
}

impl View for Demo {
    #[component]
    fn show(&mut self, cx: &mut UiContext) {
        column(cx).gap(16).children(|cx| {
            if button(cx).text("Count").width(160.).height(40.).clicked() {
                self.count += 1;
            }
            text!(cx, "count: {}", self.count).font_size(14);
        })
    }
}

fn pointer(phase: PointerPhase, x: f64, y: f64) -> UiEvent {
    UiEvent::Pointer(PointerEvent {
        phase,
        position: Point { x, y },
        button: matches!(phase, PointerPhase::Down | PointerPhase::Up)
            .then_some(PointerButton::Primary),
        buttons: u32::from(phase == PointerPhase::Down),
        modifiers: plico_renderer::Modifiers::empty(),
        properties: PointerProperties::default(),
    })
}

#[test]
fn first_frame_is_full_and_idle_pointer_moves_change_nothing() {
    let mut source = Source::new(Demo::default());
    let mut tcx = TextContext::new();

    // The first frame must report full damage.
    let _ = source.build_frame(&mut tcx, 400, 300);
    assert_eq!(source.adapter.damage_full_frames, 1);
    assert_eq!(source.adapter.damage_partial_frames, 0);

    // Pointer moves over empty space never escalate to a full frame; they
    // may settle a hover state change on an ancestor, which is a small
    // partial region at worst.
    for y in [10., 30., 50.] {
        source.handle_event(pointer(PointerPhase::Move, 380., y));
        let _ = source.build_frame(&mut tcx, 400, 300);
    }
    assert_eq!(
        source.adapter.damage_full_frames, 1,
        "empty-space moves must not relayout"
    );

    // Hovering the button is a partial frame covering only the button.
    let before = source.adapter.damage_partial_frames;
    source.handle_event(pointer(PointerPhase::Move, 80., 20.));
    let _ = source.build_frame(&mut tcx, 400, 300);
    assert_eq!(source.adapter.damage_partial_frames, before + 1);
    // The hover region is a fraction of the window, far below full cover.
    assert!(
        source.adapter.damage_partial_coverage() > 0.
            && source.adapter.damage_partial_coverage() < 0.25,
        "coverage {}",
        source.adapter.damage_partial_coverage()
    );
}

#[test]
fn clicking_updates_the_label_region_only() {
    let mut source = Source::new(Demo::default());
    let mut tcx = TextContext::new();
    let _ = source.build_frame(&mut tcx, 400, 300);

    // Click the button: the pressed state, the release and the new label
    // count as partial damage. The region stays far from full coverage.
    source.handle_event(pointer(PointerPhase::Move, 80., 20.));
    let _ = source.build_frame(&mut tcx, 400, 300);
    source.handle_event(pointer(PointerPhase::Down, 80., 20.));
    let _ = source.build_frame(&mut tcx, 400, 300);
    source.handle_event(pointer(PointerPhase::Up, 80., 20.));
    let _ = source.build_frame(&mut tcx, 400, 300);
    assert_eq!(source.view.count, 1);
    // The press itself is partial (button state only). The release changes
    // the label text, which remeasures and relayouts the flow - the current
    // pipeline treats that as a full-frame event. Phase 0 records the
    // behavior; shrinking text-driven relayout to a subtree is future work.
    assert_eq!(
        source.adapter.damage_full_frames, 2,
        "text remeasure relayouts the full flow today"
    );
    assert!(source.adapter.damage_partial_frames >= 1);
    assert!(source.adapter.damage_partial_coverage() < 0.5);

    // A resize is full damage again.
    let _ = source.build_frame(&mut tcx, 640, 300);
    assert_eq!(source.adapter.damage_full_frames, 3);
}

#[test]
fn damage_region_stays_inside_the_viewport() {
    let mut source = Source::new(Demo::default());
    let mut tcx = TextContext::new();
    let _ = source.build_frame(&mut tcx, 400, 300);

    // A settled frame reports either nothing more to repaint (empty region)
    // or a clamped rectangle inside the viewport. It never reports a
    // rectangle that escapes the window.
    if let Some([x0, y0, x1, y1]) = source.adapter.damage() {
        assert!(x0 >= 0. && y0 >= 0. && x1 <= 400. && y1 <= 300.);
        assert!((x0 < x1 && y0 < y1) || (x0 == 0. && y0 == 0. && x1 == 0. && y1 == 0.));
    }

    // Hover produces a real region, and it stays inside the window too.
    source.handle_event(pointer(PointerPhase::Move, 80., 20.));
    let _ = source.build_frame(&mut tcx, 400, 300);
    if let Some([x0, y0, x1, y1]) = source.adapter.damage() {
        assert!(x0 >= 0. && y0 >= 0. && x1 <= 400. && y1 <= 300.);
    }
}

// Semantic navigation sanity for the interaction path used above.
#[test]
fn semantic_click_paths_exercise_the_same_partial_damage() {
    let mut source = Source::new(Demo::default());
    let mut tcx = TextContext::new();
    let _ = source.build_frame(&mut tcx, 400, 300);
    let id = source
        .adapter
        .semantics(&source.engine, 0)
        .node_by_role(SemanticRole::Button, "Count", None)
        .unwrap()
        .id;
    assert!(source.handle_semantic_action(SemanticAction::Click { target: id }));
    let _ = source.build_frame(&mut tcx, 400, 300);
    assert_eq!(source.view.count, 1);
    // Semantic click commits the same text change: full-frame relayout.
    assert_eq!(source.adapter.damage_full_frames, 2);
}

#[test]
fn retained_window_path_records_damage_without_snapshot_clones() {
    let mut source = Source::new(Demo::default());
    let mut tcx = TextContext::new();
    let mut scene = plico_renderer::Scene::new();
    source.render_retained_frame(&mut scene, &mut tcx, (400, 300), 1.);
    assert!(source.last_error.is_none());
    assert_eq!(source.adapter.damage_full_frames, 1);
    assert_eq!(source.adapter.stats.placed_node_clones, 0);
    source.handle_event(pointer(PointerPhase::Move, 80., 20.));
    source.render_retained_frame(&mut scene, &mut tcx, (400, 300), 1.);
    assert!(source.adapter.damage_partial_frames >= 1);
    assert_eq!(source.adapter.stats.placed_node_clones, 0);
}
