//! Backend work counters, not a cross-language benchmark.
//!
//! Two modes:
//!   probe            # all pages, cold/idle/scroll/idle phases
//!   probe --legacy   # the original Controls-page interaction script
use plico_gallery::{Gallery, Page};
use plico_renderer::{FrameSource, SemanticAction, SemanticRole, TextContext};
use plico_vello::Source;

fn record(s: &Source<Gallery>, phase: &str, nodes: usize, scene_us: u128) -> serde_json::Value {
    let b = &s.adapter.stats;
    serde_json::json!({
        "phase": phase,
        "nodes": nodes,
        "scene_us": scene_us,
        "declaration_us": s.declaration_micros,
        "sync_us": b.sync_micros,
        "layout_and_flatten_us": b.layout_micros,
        "sync_visits": b.sync_visits,
        "layout_runs": b.layout_runs,
        "flatten_visits": b.flatten_visits,
        "paint_patches": b.paint_patches,
        "presentation_visits": b.presentation_visits,
        "text_measure_invalidations": b.text_measure_invalidations,
        "placed_node_clones": b.placed_node_clones,
        "backend_created": b.created,
        "backend_removed": b.removed,
        "style_writes": b.style_writes,
        "paint_writes": b.paint_writes,
        "child_writes": b.child_writes,
    })
}
fn frame(s: &mut Source<Gallery>, tcx: &mut TextContext, w: u32, h: u32) -> usize {
    let nodes = s.build_frame(tcx, w, h);
    assert!(s.last_error.is_none(), "{:?}", s.last_error);
    nodes.len()
}
fn wheel(s: &mut Source<Gallery>, x: f64, y: f64, delta: f64) {
    use plico_renderer::{GesturePhase, Modifiers, Point, UiEvent, WheelDeltaMode, WheelEvent};
    s.handle_event(UiEvent::Wheel(WheelEvent {
        position: Point { x, y },
        delta_x: 0.,
        delta_y: delta,
        delta_mode: WheelDeltaMode::Pixel,
        phase: GesturePhase::Changed,
        modifiers: Modifiers::empty(),
    }));
}
fn page_from(name: &str) -> Page {
    match name {
        "workspace" => Page::Workspace,
        "layout" => Page::Layout,
        "controls" => Page::Controls,
        "motion" => Page::Motion,
        "lists" => Page::Lists,
        "2048" => Page::Game2048,
        "markdown" => Page::Markdown,
        "glass" => Page::Glass,
        _ => panic!("unknown page {name}"),
    }
}
fn probe_page(page: Page) -> Vec<serde_json::Value> {
    let mut s = Source::new(Gallery {
        router: plico_core::Router::new(page),
        ..Default::default()
    });
    let mut tcx = TextContext::new();
    let (w, h) = (1120, 820);
    let mut out = Vec::new();
    // Cold: first declaration, full backend creation, first layout.
    let nodes = frame(&mut s, &mut tcx, w, h);
    out.push(record(&s, "cold", nodes, 0));
    // Idle: nothing changed; the retained frame should skip nearly everything.
    let nodes = frame(&mut s, &mut tcx, w, h);
    out.push(record(&s, "idle", nodes, 0));
    // Scroll: wheel over the page center, then a settling frame.
    wheel(&mut s, (w / 2) as f64, (h / 2) as f64, 480.);
    let nodes = frame(&mut s, &mut tcx, w, h);
    out.push(record(&s, "scroll", nodes, 0));
    // Idle again after motion settles.
    let nodes = frame(&mut s, &mut tcx, w, h);
    out.push(record(&s, "idle2", nodes, 0));
    // The host's combined build+paint path: the placed list is painted from
    // retained state instead of being exported. placed_node_clones stays 0.
    let mut scene = plico_renderer::Scene::new();
    let (nodes, _, _) = s.build_and_paint(&mut scene, &mut tcx, w, h, 1.);
    out.push(record(&s, "desktop", nodes, 0));
    let mut scene = plico_renderer::Scene::new();
    let (nodes, _, _) = s.build_and_paint(&mut scene, &mut tcx, w, h, 1.);
    out.push(record(&s, "desktop-idle", nodes, 0));
    out
}
fn legacy_frames() -> Vec<serde_json::Value> {
    let mut s = Source::new(Gallery {
        router: plico_core::Router::new(plico_gallery::Page::Controls),
        ..Default::default()
    });
    let mut tcx = TextContext::new();
    let mut results = Vec::new();
    for phase in [
        "cold",
        "idle",
        "counter",
        "notifications",
        "large-text",
        "theme",
        "idle-dark",
    ] {
        let action = match phase {
            "counter" => Some((SemanticRole::Button, "Increment")),
            "notifications" => Some((SemanticRole::Switch, "Notifications")),
            "large-text" => Some((SemanticRole::CheckBox, "Large text")),
            "theme" => Some((SemanticRole::Switch, "Dark theme")),
            _ => None,
        };
        if let Some((role, label)) = action {
            let snap = s.adapter.semantics(&s.engine, 0);
            let target = snap.node_by_role(role, label, None).unwrap().id;
            assert!(s.handle_semantic_action(SemanticAction::Click { target }));
        }
        let nodes = s.build_frame(&mut tcx, 920, 780);
        let start = std::time::Instant::now();
        let mut scene = plico_renderer::Scene::new();
        plico_renderer::scene::build_scene(&mut scene, &nodes, &mut tcx, 920, 780, s.base_color());
        let scene_micros = start.elapsed().as_micros();
        let b = &s.adapter.stats;
        results.push(serde_json::json!({"phase":phase,"nodes":nodes.len(),"scene_us":scene_micros,"declaration_us":s.declaration_micros,"sync_us":b.sync_micros,"layout_and_flatten_us":b.layout_micros,"sync_visits":b.sync_visits,"layout_runs":b.layout_runs,"flatten_visits":b.flatten_visits,"paint_patches":b.paint_patches,"backend_created":b.created,"backend_removed":b.removed,"style_writes":b.style_writes,"paint_writes":b.paint_writes,"child_writes":b.child_writes,"state":{"count":s.view.count,"notifications":s.view.notifications,"large":s.view.large,"dark":s.view.dark}}));
    }
    results
}
fn main() {
    let legacy = std::env::args().any(|a| a == "--legacy");
    let pages = [
        "workspace",
        "layout",
        "controls",
        "motion",
        "lists",
        "2048",
        "markdown",
        "glass",
    ];
    let mut report = serde_json::Map::new();
    if legacy {
        report.insert("frames".into(), legacy_frames().into());
    } else {
        for name in pages {
            let page = page_from(name);
            let frames = probe_page(page);
            report.insert(name.into(), frames.into());
        }
    }
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "note": "One run; build-only wall times, no GPU or vsync. Settling passes included. Not comparable to Solid CLI build/scene metrics.",
            "report": report,
        }))
        .unwrap()
    );
}
