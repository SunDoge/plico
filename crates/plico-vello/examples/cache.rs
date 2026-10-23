//! Compare immutable scene copies and owned/retained frame paths without a window.
use plico_core::{VirtualClock, prelude::*};
use plico_renderer::{FrameSource, Scene, TextContext};
use plico_vello::{Source, View};
use std::{
    hint::black_box,
    time::{Duration, Instant},
};
struct Demo {
    labels: Vec<String>,
    active: bool,
    layout: bool,
}
impl View for Demo {
    #[component]
    fn show(&mut self, cx: &mut UiContext) {
        column(cx).children(|cx| {
            for (i, label) in self.labels.iter().enumerate() {
                text(cx, label.as_str())
                    .key(i)
                    .width(if i == 0 && self.active && self.layout {
                        200.
                    } else {
                        100.
                    })
                    .height(24.)
                    .opacity(if i == 0 && self.active && !self.layout {
                        0.2
                    } else {
                        1.
                    })
                    .motion(Motion::Smooth);
            }
        })
    }
}
fn median(samples: &mut [u128]) -> u128 {
    samples.sort_unstable();
    samples[samples.len() / 2]
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut results = Vec::new();
    let mut scene_copies = None;
    for mode in ["idle", "paint", "layout"] {
        for retained in [false, true] {
            let clock = VirtualClock::default();
            let mut tcx = TextContext::new();
            let mut source = Source::with_clock(
                Demo {
                    labels: (0..1000).map(|i| format!("Row {i}")).collect(),
                    active: false,
                    layout: mode == "layout",
                },
                clock.clone(),
            );
            let mut scene = Scene::new();
            source.render_retained_frame(&mut scene, &mut tcx, (800, 30000), 1.);
            if scene_copies.is_none() {
                let mut deep = Vec::new();
                let mut shared = Vec::new();
                for _ in 0..200 {
                    let start = Instant::now();
                    black_box(scene.commands.to_vec());
                    deep.push(start.elapsed().as_nanos());
                    let start = Instant::now();
                    black_box(scene.clone());
                    shared.push(start.elapsed().as_nanos());
                }
                scene_copies = Some(
                    serde_json::json!({"commands":scene.commands.len(),"deep_copy_median_ns":median(&mut deep),"shared_snapshot_median_ns":median(&mut shared)}),
                );
            }
            if mode != "idle" {
                source.view.active = true;
                source.request_rebuild();
                source.render_retained_frame(&mut scene, &mut tcx, (800, 30000), 1.);
            }
            let mut times = Vec::new();
            let mut clones = 0;
            let mut layouts = 0;
            let mut builds = 0;
            for _ in 0..80 {
                clock.advance(Duration::from_millis(1))?;
                let start = Instant::now();
                if retained {
                    black_box(source.render_retained_frame(&mut scene, &mut tcx, (800, 30000), 1.));
                } else {
                    let nodes = source.build_frame(&mut tcx, 800, 30000);
                    source.paint_scene(&mut scene, &nodes, &mut tcx, (800, 30000), 1.);
                    source.paint_debug_overlay(&mut scene, &nodes, &mut tcx, 1.);
                    black_box(nodes);
                }
                times.push(start.elapsed().as_micros());
                clones += source.adapter.stats.placed_node_clones;
                layouts += source.adapter.stats.layout_runs;
                builds += source.adapter.stats.scene_builds;
            }
            if retained {
                assert_eq!(clones, 0);
            }
            results.push(serde_json::json!({"scenario":mode,"path":if retained {"retained"} else {"snapshot"},"frames":80,"median_cpu_us":median(&mut times),"exported_node_clones":clones,"layout_runs":layouts,"scene_builds":builds}));
        }
    }
    let report = serde_json::json!({"note":"1000 text nodes; 80 virtual-clock frames per path, 200 scene copy samples. CPU only: no GPU/present/FPS claim. Deep copy reproduces the previous Vec<PaintCommand> snapshot operation. Paint changes still encode the full scene.","scene_copy":scene_copies,"frames":results});
    std::fs::create_dir_all("reports/cache")?;
    std::fs::write(
        "reports/cache/report.json",
        serde_json::to_vec_pretty(&report)?,
    )?;
    println!("{report}");
    Ok(())
}
