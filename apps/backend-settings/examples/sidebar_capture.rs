//! Offline captures of the animated sidebar: expanded, mid-flight and compact.
//! Virtual time drives the tween deterministically - no window is required.
use backend_settings::Settings;
use plico_core::VirtualClock;
use plico_renderer::{FrameSource, Scene, SemanticAction, SemanticRole, TextContext};
use plico_runtime::Source;
use std::time::Duration;

fn capture(source: &mut Source<Settings>, tcx: &mut TextContext, path: &str) {
    let nodes = source.build_frame(tcx, 1100, 820);
    let mut scene = Scene::new();
    source.paint_scene(&mut scene, &nodes, tcx, (1100, 820), 1.);
    plico_shell::renderer::render_to_png(&scene, 1100, 820, source.base_color(), path)
        .expect("render to png");
    println!("wrote {path}");
}

fn click(source: &mut Source<Settings>, label: &str) {
    let id = source
        .adapter
        .semantics(&source.engine, 0)
        .node_by_role(SemanticRole::Button, label, None)
        .expect("button")
        .id;
    assert!(source.handle_semantic_action(SemanticAction::Click { target: id }));
}

fn main() {
    let clock = VirtualClock::default();
    let mut source = Source::with_clock(Settings::default(), clock.clone());
    let mut tcx = TextContext::new();
    std::fs::create_dir_all("reports/sidebar").unwrap();

    // Expanded at rest.
    capture(&mut source, &mut tcx, "reports/sidebar/expanded.png");

    // Collapse and catch the mid-flight frame (labels half-faded, sidebar 88).
    click(&mut source, "Toggle sidebar");
    source.build_frame(&mut tcx, 1100, 820); // settle the click at t=0
    clock.set(Duration::from_millis(130)).unwrap();
    capture(&mut source, &mut tcx, "reports/sidebar/intermediate.png");

    // Compact at rest: icon-only navigation, footer single-line.
    clock.set(Duration::from_millis(400)).unwrap();
    capture(&mut source, &mut tcx, "reports/sidebar/compact.png");
}
