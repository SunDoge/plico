//! Actual settings UI, semantic input and virtual time; tracked pixels must
//! match full raster redraws, apart from bounded one-step AA rounding.
//! Set PLICO_DAMAGE_STRICT=1 for exact equality. Readback is outside timings.
use backend_settings::Settings;
use plico::VirtualClock;
use plico_renderer::{DamageRegion, FrameSource, Scene, SemanticAction, SemanticRole, TextContext};
use plico_runtime::{
    Source,
    testing::{Command, dispatch},
};
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
    expanded_full: usize,
    expanded_partial: usize,
    expanded_empty: usize,
    expanded_area: f64,
    full_us: Vec<f64>,
    expanded_us: Vec<f64>,
    tracked_us: Vec<f64>,
}
fn main() {
    let clock = VirtualClock::default();
    let mut source = Source::with_clock(Settings::default(), clock.clone());
    let mut text = TextContext::new();
    let mut scene = Scene::new();
    let (mut tracked, mut expanded_tracked, mut full) = (
        SkiaSurface::new(1100, 820).unwrap(),
        SkiaSurface::new(1100, 820).unwrap(),
        SkiaSurface::new(1100, 820).unwrap(),
    );
    let mut phases: BTreeMap<&str, Stats> = BTreeMap::new();
    let strict = std::env::var_os("PLICO_DAMAGE_STRICT").is_some();
    let mut previous_pixels: Option<Vec<u8>> = None;
    let mut rounded_frames = 0;
    let mut max_rounding_pixels = 0;
    for frame in 0..240 {
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
                source.view.set_dark(true);
                source.request_rebuild();
            }
            120 => {
                for command in [
                    Command::Focus {
                        role: "textbox".into(),
                        label: "Workspace name".into(),
                        occurrence: None,
                    },
                    Command::KeyEvent {
                        event: plico::KeyEvent {
                            phase: plico::KeyPhase::Down,
                            key: "a".into(),
                            code: "KeyA".into(),
                            key_without_modifiers: "a".into(),
                            text: None,
                            text_with_all_modifiers: None,
                            location: plico::KeyLocation::Standard,
                            modifiers: plico::Modifiers::CONTROL,
                            repeat: false,
                            synthetic: false,
                        },
                    },
                    Command::Text {
                        text: "A different workspace name".into(),
                    },
                ] {
                    dispatch(&mut source, &mut text, &command).unwrap();
                }
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
        // Baseline for the previous backend: expand every Fragment before
        // damage comparison and replay. Include expansion in the measurement.
        let expanded = scene.flattened();
        let expanded_damage = expanded_tracked
            .draw_scene_tracked(&expanded, source.base_color())
            .unwrap();
        let expanded_us = t.elapsed().as_secs_f64() * 1e6;
        let t = Instant::now();
        full.draw_scene(&scene, source.base_color()).unwrap();
        let full_us = t.elapsed().as_secs_f64() * 1e6;
        let actual = tracked.read_pixels().unwrap();
        let expected = full.read_pixels().unwrap();
        if let Some(previous) = &previous_pixels {
            for (index, (before, after)) in previous
                .as_chunks::<4>()
                .0
                .iter()
                .zip(actual.as_chunks::<4>().0.iter())
                .enumerate()
            {
                let (x, y) = ((index % 1100) as f64, (index / 1100) as f64);
                let untouched = match damage {
                    DamageRegion::Full => false,
                    DamageRegion::Empty => true,
                    DamageRegion::Rect(r) => x < r.x0 || x >= r.x1 || y < r.y0 || y >= r.y1,
                };
                assert!(
                    !untouched || before == after,
                    "frame {frame}: pixel ({x},{y}) changed outside {damage:?}"
                );
            }
        }
        if actual != expected {
            let mut count = 0;
            let mut bounds = [1100usize, 820usize, 0, 0];
            let mut max_delta = 0;
            for (i, (a, b)) in actual
                .as_chunks::<4>()
                .0
                .iter()
                .zip(expected.as_chunks::<4>().0.iter())
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
                }
            }
            assert!(
                !strict && max_delta <= 1 && count <= 16,
                "frame {frame}, phase {phase}, damage {damage:?}, differing_pixels={count}, bounds={bounds:?}, max_delta={max_delta}"
            );
            rounded_frames += 1;
            max_rounding_pixels = max_rounding_pixels.max(count);
        }
        previous_pixels = Some(actual);
        let stats = phases.entry(phase).or_default();
        stats.frames += 1;
        match damage {
            DamageRegion::Full => stats.full += 1,
            DamageRegion::Empty => stats.empty += 1,
            _ => stats.partial += 1,
        }
        stats.area += damage.coverage(1100, 820);
        match expanded_damage {
            DamageRegion::Full => stats.expanded_full += 1,
            DamageRegion::Empty => stats.expanded_empty += 1,
            _ => stats.expanded_partial += 1,
        }
        stats.expanded_area += expanded_damage.coverage(1100, 820);
        stats.full_us.push(full_us);
        stats.expanded_us.push(expanded_us);
        stats.tracked_us.push(tracked_us);
    }
    for (phase, mut s) in phases {
        s.full_us.sort_by(f64::total_cmp);
        s.expanded_us.sort_by(f64::total_cmp);
        s.tracked_us.sort_by(f64::total_cmp);
        println!(
            "{phase}: frames={} full={} partial={} empty={} mean_coverage={:.3} expanded_full={} expanded_partial={} expanded_empty={} expanded_coverage={:.3} full_p50_us={:.1} expanded_p50_us={:.1} tracked_p50_us={:.1}",
            s.frames,
            s.full,
            s.partial,
            s.empty,
            s.area / s.frames as f64,
            s.expanded_full,
            s.expanded_partial,
            s.expanded_empty,
            s.expanded_area / s.frames as f64,
            s.full_us[s.frames / 2],
            s.expanded_us[s.frames / 2],
            s.tracked_us[s.frames / 2]
        );
    }
    println!(
        "PASS: 240 frames; AA rounding frames={rounded_frames}, maximum differing pixels={max_rounding_pixels} (limit 16 pixels, 1/255 per channel)"
    );
}
