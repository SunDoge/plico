//! Deterministic real-render captures; no native window or sleeps.
use gallery::Gallery;
use plico::UiContext;
use plico_desktop::{Source, View};
use plico_renderer::{FrameSource, Scene, SemanticAction, SemanticRole, TextContext};
use std::time::{Duration, Instant};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let directory =
        std::env::var("PLICO_REPORT_DIR").unwrap_or_else(|_| "reports/layout-style".into());
    std::fs::create_dir_all(&directory)?;
    let mut source = Source::new(Gallery {
        router: plico::Router::new(gallery::Page::Layout),
        ..Default::default()
    });
    let mut tcx = TextContext::new();
    let now = Instant::now();
    source.build_frame_at(&mut tcx, 920, 780, now);
    let target = source
        .adapter
        .semantics(&source.engine, 0)
        .node_by_role(SemanticRole::Button, "Animate layout", None)
        .unwrap()
        .id;
    source.handle_semantic_action(SemanticAction::Click { target });
    source.build_frame_at(&mut tcx, 920, 780, now);
    let mut report = Vec::new();
    for ms in [0, 70, 140, 280] {
        let start = Instant::now();
        let nodes = source.build_frame_at(&mut tcx, 920, 780, now + Duration::from_millis(ms));
        let build_us = start.elapsed().as_micros();
        let start = Instant::now();
        let mut scene = Scene::new();
        source.paint_scene(&mut scene, &nodes, &mut tcx, (920, 780), 1.);
        source.paint_debug_overlay(&mut scene, &nodes, &mut tcx, 1.);
        let scene_us = start.elapsed().as_micros();
        plico_shell::renderer::render_to_png(
            &scene,
            920,
            780,
            source.base_color(),
            &format!("{directory}/{ms}.png"),
        )?;
        report.push(serde_json::json!({"time_ms":ms,"build_us":build_us,"scene_us":scene_us,"has_animation":source.has_anim(),"layout_style_writes":source.adapter.stats.style_writes,"paint_writes":source.adapter.stats.paint_writes}));
    }
    assert!(!source.has_anim());
    std::fs::write(
        format!("{directory}/report.json"),
        serde_json::to_vec_pretty(
            &serde_json::json!({"note":"Simulated clock; real Taffy/Parley/Vello captures; timings exclude GPU and PNG output; one local run.","frames":report}),
        )?,
    )?;
    // Batch layout sampling, intentionally without GPU capture or scene encoding.
    let mut batches = Vec::new();
    for count in [100, 1000] {
        let mut source = Source::new(Batch {
            count,
            expanded: false,
            calls: 0,
        });
        source.build_frame_at(&mut tcx, 920, 30000, now);
        source.view.expanded = true;
        source.request_rebuild();
        source.build_frame_at(&mut tcx, 920, 30000, now);
        let calls = source.view.calls;
        for ms in [100, 200] {
            let start = Instant::now();
            source.build_frame_at(&mut tcx, 920, 30000, now + Duration::from_millis(ms));
            assert_eq!(source.view.calls, calls);
            batches.push(serde_json::json!({"nodes":count,"time_ms":ms,"build_us":start.elapsed().as_micros(),"app_calls":source.view.calls,"has_animation":source.has_anim()}));
        }
        assert!(!source.has_anim());
    }
    std::fs::write(
        format!("{directory}/batch.json"),
        serde_json::to_vec_pretty(
            &serde_json::json!({"note":"One local sample; active layout transitions, no scene encoding/GPU/presentation. Not an FPS benchmark.","frames":batches}),
        )?,
    )?;
    println!("wrote {directory}");
    Ok(())
}

struct Batch {
    count: usize,
    expanded: bool,
    calls: usize,
}
impl View for Batch {
    fn show(&mut self, ui: &mut UiContext<'_>) {
        self.calls += 1;
        for id in 0..self.count {
            ui.text("Animated")
                .key(id)
                .width(if self.expanded { 200. } else { 100. })
                .height(24.)
                .transition(200);
        }
    }
}
