#![cfg(feature = "vello")]
use backend_settings::{Page, Settings};
use plico::VirtualClock;
use plico_renderer::{FrameSource, SemanticAction, SemanticRole, TextContext};
use plico_runtime::Source;
use std::time::Duration;
fn click(source: &mut Source<Settings>, label: &str) {
    let id = source
        .adapter
        .semantics(&source.engine, 0)
        .node_by_role(SemanticRole::Button, label, None)
        .unwrap()
        .id;
    assert!(source.handle_semantic_action(SemanticAction::Click { target: id }));
}
#[test]
fn routed_workspace_uses_the_same_clock_and_grid_on_vello() {
    let clock = VirtualClock::default();
    let mut source = Source::with_clock(Settings::default(), clock.clone());
    let mut tcx = TextContext::new();
    source.build_frame(&mut tcx, 1100, 820);
    click(&mut source, "Details");
    source.build_frame(&mut tcx, 1100, 820);
    clock.set(Duration::from_millis(130)).unwrap();
    source.build_frame(&mut tcx, 1100, 820);
    assert!(source.view.inspector_width() > 0. && source.view.inspector_width() < 248.);
    clock.set(Duration::from_millis(300)).unwrap();
    source.build_frame(&mut tcx, 1100, 820);
    assert_eq!(source.view.inspector_width(), 248.);
    click(&mut source, "Run preview");
    source.build_frame(&mut tcx, 1100, 820);
    clock.set(Duration::from_millis(1500)).unwrap();
    source.build_frame(&mut tcx, 1100, 820);
    assert!((source.view.progress() - 0.5).abs() < 0.001);
    clock.set(Duration::from_millis(2700)).unwrap();
    source.build_frame(&mut tcx, 1100, 820);
    assert_eq!(source.view.progress(), 1.);
    assert!(!source.view.animation_active());
    click(&mut source, "Activity");
    source.build_frame(&mut tcx, 1100, 820);
    assert_eq!(source.view.current_page(), Page::Activity);
    let find_x = |label: &str| {
        source
            .adapter
            .placed
            .iter()
            .find(|n| n.paint.text.as_deref() == Some(label))
            .unwrap()
            .rect[0]
    };
    assert!((find_x("WORKSPACE") - find_x("Desktop client")).abs() < 1.);
    assert!((find_x("STATE") - find_x("Complete")).abs() < 1.);
    click(&mut source, "Back");
    source.build_frame(&mut tcx, 1100, 820);
    assert_eq!(source.view.current_page(), Page::Overview);
    click(&mut source, "Forward");
    source.build_frame(&mut tcx, 1100, 820);
    assert_eq!(source.view.current_page(), Page::Activity);
    source.view.go(Page::Preferences);
    source.request_rebuild();
    source.build_frame(&mut tcx, 520, 760);
    assert!(source.last_error.is_none());
}
