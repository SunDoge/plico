//! Offline captures of the animated sidebar and responsive Preferences page.
//! Virtual time drives the tween deterministically - no window is required.
use backend_settings::Settings;
use plico::VirtualClock;
use plico_renderer::{FrameSource, Scene, SemanticAction, SemanticRole, TextContext};
use plico_runtime::Source;
use std::time::Duration;

fn capture(source: &mut Source<Settings>, tcx: &mut TextContext, path: &str) {
    capture_at(source, tcx, path, 1100);
}

fn capture_at(source: &mut Source<Settings>, tcx: &mut TextContext, path: &str, width: u32) {
    let nodes = source.build_frame(tcx, width, 820);
    let mut scene = Scene::new();
    source.paint_scene(&mut scene, &nodes, tcx, (width, 820), 1.);
    plico_shell::renderer::render_to_png(&scene, width, 820, source.base_color(), path)
        .expect("render to png");
    println!("wrote {path}");
}

fn click(source: &mut Source<Settings>, label: &str) {
    let id = source
        .semantics()
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

    click(&mut source, "Toggle sidebar");
    source.build_frame(&mut tcx, 1100, 820);
    clock.set(Duration::from_millis(800)).unwrap();
    source.build_frame(&mut tcx, 1100, 820);
    click(&mut source, "Preferences");
    capture(&mut source, &mut tcx, "reports/sidebar/preferences.png");
    capture_at(
        &mut source,
        &mut tcx,
        "reports/sidebar/preferences-narrow.png",
        640,
    );
}
