//! Real host animation work; virtual time excludes vsync and scheduling noise.
use gallery::{Gallery, Page, mines::Mines};
use plico::{Router, VirtualClock};
use plico_desktop::Source;
use plico_renderer::{FrameSource, Scene, TextContext};
use std::time::{Duration, Instant};
fn main() {
    let mut renderer = pollster::block_on(plico_wgpu::Renderer::new()).unwrap();
    eprintln!("adapter: {:?}", renderer.adapter_info());
    let mut reports = vec![];
    for (columns, rows, mines) in [(16, 16, 40), (30, 16, 99), (64, 64, 800)] {
        let clock = VirtualClock::default();
        let mut game = Mines::default();
        game.configure(columns, rows, mines);
        let mut source = Source::with_clock(
            Gallery {
                router: Router::new(Page::Mines),
                mines: game,
                ..Default::default()
            },
            clock.clone(),
        );
        let mut text = TextContext::new();
        renderer.configure_text(&mut text, 1.).unwrap();
        let mut scene = Scene::new();
        source
            .render_retained_frame(&mut scene, &mut text, (1120, 820), 1.)
            .unwrap();
        let label = format!("Row {}, column {}: hidden", rows / 2 + 1, columns / 2 + 1);
        let target = source
            .adapter
            .semantics(&source.engine, 0)
            .node_by_role(plico_renderer::SemanticRole::Button, &label, None)
            .unwrap()
            .id;
        assert!(source.handle_semantic_action(plico_renderer::SemanticAction::Click { target }));
        let mut frames = vec![];
        for frame in 0..45 {
            clock.set(Duration::from_millis(frame * 17)).unwrap();
            let start = Instant::now();
            source
                .render_retained_frame(&mut scene, &mut text, (1120, 820), 1.)
                .unwrap();
            assert!(source.last_error.is_none(), "{:?}", source.last_error);
            let cpu_us = start.elapsed().as_micros();
            let gpu = Instant::now();
            renderer
                .render_scene(&scene, [1120, 820], [1.; 4], true)
                .unwrap();
            let gpu_us = gpu.elapsed().as_micros();
            let batch = format!("{:?}", renderer.batch_stats());
            let s = source.adapter.stats;
            frames.push(serde_json::json!({"frame":frame,"us":cpu_us,"gpu_us":gpu_us,"batch":batch,"declare_us":source.declaration_micros,"sync_us":s.sync_micros,"layout_us":s.layout_micros,"layout":s.layout_runs,"flatten":s.flatten_visits,"patch":s.paint_patches,"presentation":s.presentation_visits,"created":s.created,"removed":s.removed}));
        }
        let mut timer = vec![];
        for ms in [2000, 2017, 2034, 2500, 3000] {
            clock.set(Duration::from_millis(ms)).unwrap();
            source
                .render_retained_frame(&mut scene, &mut text, (1120, 820), 1.)
                .unwrap();
            timer.push(
                serde_json::json!({"ms":ms,"declare_us":source.declaration_micros,
                "sync_visits":source.adapter.stats.sync_visits}),
            );
        }
        reports
            .push(serde_json::json!({"columns":columns,"rows":rows,"frames":frames,"timer":timer}));
    }
    println!("{}", serde_json::to_string_pretty(&reports).unwrap());
}
