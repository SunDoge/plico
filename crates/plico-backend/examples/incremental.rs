//! Sparse animation work counters with an injected clock; no window or sleeps.
use plico_backend::{Source, View};
use plico_core::{Ui, VirtualClock, component};
use plico_renderer::{FrameSource, Scene, TextContext};
use std::time::{Duration, Instant};
struct Demo {
    labels: Vec<String>,
    changed: bool,
    layout: bool,
    calls: usize,
}
impl View for Demo {
    #[component]
    fn show(&mut self, ui: &mut Ui<'_>) {
        self.calls += 1;
        ui.column().children(|ui| {
            for (id, label) in self.labels.iter().enumerate() {
                ui.text(label.as_str())
                    .key(id)
                    .height(24.)
                    .width(if id == 0 && self.changed && self.layout {
                        300.
                    } else {
                        100.
                    })
                    .opacity(if id == 0 && self.changed && !self.layout {
                        0.2
                    } else {
                        1.
                    })
                    .transition(200);
            }
        })
    }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut frames = Vec::new();
    for layout in [false, true] {
        let clock = VirtualClock::default();
        let mut text = TextContext::new();
        let mut source = Source::with_clock(
            Demo {
                labels: (0..1000).map(|id| format!("Row {id}")).collect(),
                changed: false,
                layout,
                calls: 0,
            },
            clock.clone(),
        );
        for phase in ["cold", "idle", "retarget", "half", "end", "settled"] {
            if phase == "retarget" {
                source.view.changed = true;
                source.request_rebuild();
            }
            if matches!(phase, "half" | "end") {
                clock.advance(Duration::from_millis(100))?;
            }
            let start = Instant::now();
            let nodes = source.build_frame(&mut text, 800, 30000);
            let build_us = start.elapsed().as_micros();
            let start = Instant::now();
            let mut scene = Scene::new();
            source.paint_scene(&mut scene, &nodes, &mut text, (800, 30000), 1.);
            let scene_us = start.elapsed().as_micros();
            let stats = source.adapter.stats;
            if phase == "half" {
                assert_eq!(stats.sync_visits, 1);
                assert_eq!(stats.layout_runs, usize::from(layout));
            }
            if matches!(phase, "idle" | "settled") {
                assert_eq!(stats.sync_visits, 0);
                assert_eq!(stats.layout_runs, 0);
                assert_eq!(stats.scene_reuses, 1);
            }
            frames.push(serde_json::json!({"mode":if layout{"layout"}else{"paint"},"phase":phase,"nodes":nodes.len(),"build_us":build_us,"scene_us":scene_us,"app_calls":source.view.calls,"sync_visits":stats.sync_visits,"layout_runs":stats.layout_runs,"flatten_visits":stats.flatten_visits,"paint_patches":stats.paint_patches,"scene_builds":stats.scene_builds,"scene_reuses":stats.scene_reuses,"has_animation":source.has_anim()}));
        }
        assert!(!source.has_anim());
    }
    let directory =
        std::env::var("PLICO_REPORT_DIR").unwrap_or_else(|_| "reports/incremental".into());
    std::fs::create_dir_all(&directory)?;
    std::fs::write(
        format!("{directory}/report.json"),
        serde_json::to_vec_pretty(
            &serde_json::json!({"note":"1000 declared transitions, one active. One local CPU sample; no GPU/present. Returning PlacedNode Vec still clones the full output; paint updates still encode a scene.","frames":frames}),
        )?,
    )?;
    println!("wrote {directory}/report.json");
    Ok(())
}
