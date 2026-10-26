//! Actual settings UI, semantic input and virtual time; tracked pixels must
//! match full raster redraws. Readback is outside both draw timing regions.
use backend_settings::Settings;
use plico_core::VirtualClock;
use plico_renderer::{DamageRegion, FrameSource, Scene, SemanticAction, SemanticRole, TextContext};
use plico_runtime::Source;
use plico_skia::SkiaSurface;
use std::{
    collections::BTreeMap,
    time::{Duration, Instant},
};

#[derive(Default)]
struct Stats {
    frames: usize,
    full: usize,
    partial: usize,
    empty: usize,
    area: f64,
    full_us: Vec<f64>,
    tracked_us: Vec<f64>,
}
fn main() {
    let clock = VirtualClock::default();
    let mut source = Source::with_clock(Settings::default(), clock.clone());
    let mut text = TextContext::new();
    let mut scene = Scene::new();
    let (mut tracked, mut full) = (
        SkiaSurface::new(1100, 820).unwrap(),
        SkiaSurface::new(1100, 820).unwrap(),
    );
    let mut phases: BTreeMap<&str, Stats> = BTreeMap::new();
    let mut previous = None;
    let mut previous_background = source.base_color();
    for frame in 0..150 {
        let phase = match frame {
            0..=29 => "idle",
            30..=59 => "sidebar",
            60..=89 => "navigation",
            90..=119 => "theme",
            _ => "text",
        };
        match frame {
            30 | 60 => {
                let label = if frame == 30 {
                    "Toggle sidebar"
                } else {
                    "Preferences"
                };
                let target = source
                    .adapter
                    .semantics(&source.engine, 0)
                    .node_by_role(SemanticRole::Button, label, None)
                    .unwrap()
                    .id;
                assert!(source.handle_semantic_action(SemanticAction::Click { target }));
            }
            90 => {
                source.view.dark = true;
                source.request_rebuild();
            }
            120 => {
                source
                    .view
                    .name
                    .set("A different workspace name".to_owned().into());
                source.request_rebuild();
            }
            _ => {}
        }
        clock.advance(Duration::from_millis(16)).unwrap();
        source.render_retained_frame(&mut scene, &mut text, (1100, 820), 1.);
        assert!(
            source.last_error.is_none(),
            "frame {frame}: {:?}",
            source.last_error
        );
        let t = Instant::now();
        let damage = tracked
            .draw_scene_tracked(&scene, source.base_color())
            .unwrap();
        let tracked_us = t.elapsed().as_secs_f64() * 1e6;
        let t = Instant::now();
        full.draw_scene(&scene, source.base_color()).unwrap();
        let full_us = t.elapsed().as_secs_f64() * 1e6;
        let actual = tracked.read_pixels().unwrap();
        let expected = full.read_pixels().unwrap();
        if actual != expected {
            if let Some(previous) = &previous {
                for padding in [0., 4., 16.] {
                    let mut no_culling = SkiaSurface::new(1100, 820).unwrap();
                    no_culling
                        .draw_scene(previous, previous_background)
                        .unwrap();
                    let padded = match damage {
                        DamageRegion::Rect(r) => DamageRegion::Rect(r.inflate(padding, padding)),
                        other => other,
                    };
                    no_culling
                        .draw_scene_damaged(&scene, source.base_color(), padded)
                        .unwrap();
                    let pixels = no_culling.read_pixels().unwrap();
                    let count = pixels
                        .chunks_exact(4)
                        .zip(expected.chunks_exact(4))
                        .filter(|(a, b)| a != b)
                        .count();
                    eprintln!("outer clip only, padding={padding}, differing_pixels={count}");
                }
            }
            let mut count = 0;
            let mut bounds = [1100usize, 820usize, 0, 0];
            let mut max_delta = 0;
            for (i, (a, b)) in actual
                .chunks_exact(4)
                .zip(expected.chunks_exact(4))
                .enumerate()
            {
                if a != b {
                    count += 1;
                    let (x, y) = (i % 1100, i / 1100);
                    bounds[0] = bounds[0].min(x);
                    bounds[1] = bounds[1].min(y);
                    bounds[2] = bounds[2].max(x);
                    bounds[3] = bounds[3].max(y);
                    max_delta =
                        max_delta.max(a.iter().zip(b).map(|(a, b)| a.abs_diff(*b)).max().unwrap());
                    if count <= 8 {
                        eprintln!("pixel ({x},{y}) actual={a:?} expected={b:?}");
                    }
                }
            }
            std::fs::create_dir_all("reports/damage-settings").unwrap();
            tracked
                .write_png(std::path::Path::new("reports/damage-settings/partial.png"))
                .unwrap();
            full.write_png(std::path::Path::new("reports/damage-settings/full.png"))
                .unwrap();
            std::fs::write(
                "reports/damage-settings/scene.txt",
                format!("{:#?}", scene.commands),
            )
            .unwrap();
            panic!(
                "frame {frame}, phase {phase}, damage {damage:?}, differing_pixels={count}, bounds={bounds:?}, max_delta={max_delta}"
            );
        }
        previous = Some(scene.clone());
        previous_background = source.base_color();
        let stats = phases.entry(phase).or_default();
        stats.frames += 1;
        match damage {
            DamageRegion::Full => stats.full += 1,
            DamageRegion::Empty => stats.empty += 1,
            _ => stats.partial += 1,
        }
        stats.area += damage.coverage(1100, 820);
        stats.full_us.push(full_us);
        stats.tracked_us.push(tracked_us);
    }
    for (phase, mut s) in phases {
        s.full_us.sort_by(f64::total_cmp);
        s.tracked_us.sort_by(f64::total_cmp);
        println!(
            "{phase}: frames={} full={} partial={} empty={} mean_coverage={:.3} full_p50_us={:.1} tracked_p50_us={:.1}",
            s.frames,
            s.full,
            s.partial,
            s.empty,
            s.area / s.frames as f64,
            s.full_us[s.frames / 2],
            s.tracked_us[s.frames / 2]
        );
    }
    println!("PASS: all 150 settings frames matched full redraw");
}
