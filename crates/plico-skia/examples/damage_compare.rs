//! CPU raster A/B probe: scene construction is outside both timed regions.
//! No readback, network, window or frame pacing is included in timings.
use plico_renderer::{DamageRegion, PaintScene, Scene};
use plico_skia::SkiaSurface;
use std::time::Instant;
use vello_common::{
    kurbo::{Affine, Rect},
    peniko::{Color, Fill},
};

fn scene(frame: usize, all_change: bool) -> Scene {
    let mut scene = Scene::new();
    for i in 0..2000 {
        let x = (i % 50) as f64 * 24.;
        let y = (i / 50) as f64 * 17.;
        let active = if all_change {
            frame.is_multiple_of(2)
        } else {
            i == frame % 2000
        };
        scene.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            if active {
                Color::from_rgb8(230, 70, 40)
            } else {
                Color::from_rgb8(30, 140, 240)
            },
            None,
            &Rect::new(x, y, x + 20., y + 13.),
        );
    }
    scene
}
fn median(samples: &mut [f64]) -> f64 {
    samples.sort_by(f64::total_cmp);
    samples[samples.len() / 2]
}
fn main() {
    for all_change in [false, true] {
        let mut full = SkiaSurface::new(1280, 720).unwrap();
        let mut tracked = SkiaSurface::new(1280, 720).unwrap();
        let (mut full_us, mut tracked_us) = (Vec::new(), Vec::new());
        let mut coverage = 0.;
        for frame in 0..160 {
            let scene = scene(frame, all_change);
            // Alternate order to reduce systematic cache/temperature bias.
            let mut time_full = || {
                let t = Instant::now();
                full.draw_scene(&scene, Color::WHITE).unwrap();
                t.elapsed().as_secs_f64() * 1e6
            };
            let mut time_tracked = || {
                let t = Instant::now();
                let damage = tracked.draw_scene_tracked(&scene, Color::WHITE).unwrap();
                (t.elapsed().as_secs_f64() * 1e6, damage)
            };
            let (a, (b, damage)) = if frame.is_multiple_of(2) {
                (time_full(), time_tracked())
            } else {
                let b = time_tracked();
                (time_full(), b)
            };
            if frame >= 40 {
                full_us.push(a);
                tracked_us.push(b);
                coverage += damage.coverage(1280, 720);
                if !all_change {
                    assert!(matches!(damage, DamageRegion::Rect(_)));
                }
            }
        }
        assert_eq!(tracked.read_pixels().unwrap(), full.read_pixels().unwrap());
        println!(
            "{} nodes=2000 full_p50_us={:.1} tracked_p50_us={:.1} mean_damage={:.4}",
            if all_change { "dense" } else { "sparse" },
            median(&mut full_us),
            median(&mut tracked_us),
            coverage / tracked_us.len() as f64
        );
    }
}
