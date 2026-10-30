//! Isolate full Skia CPU replay; construction, readback, and warmup are untimed.
use kurbo::Affine;
use peniko::Color;
use plico_renderer::{PaintCommand, Scene, paragraph::ParagraphRequest, style::Paint};
use plico_skia::SkiaSurface;
use std::{hint::black_box, sync::Arc, time::Instant};

fn main() {
    for rotated in [false, true] {
        let mut scene = Scene::new();
        let request = Arc::new(
            ParagraphRequest::from_paint(
                &Paint {
                    text: Some("Settings".into()),
                    font_size: 13.,
                    ..Default::default()
                },
                None,
            )
            .unwrap(),
        );
        for index in 0..500 {
            let position = Affine::translate((
                (index % 20) as f64 * 70. + 4.25,
                (index / 20) as f64 * 36. + 4.5,
            ));
            scene.commands.push(PaintCommand::Paragraph {
                request: request.clone(),
                transform: position
                    * if rotated {
                        Affine::rotate(0.02)
                    } else {
                        Affine::IDENTITY
                    },
            });
        }
        let mut surface = SkiaSurface::new(1440, 1000).unwrap();
        let mut samples = Vec::with_capacity(500);
        for frame in 0..600 {
            let start = Instant::now();
            surface.draw_scene(black_box(&scene), Color::WHITE).unwrap();
            if frame >= 100 {
                samples.push(start.elapsed().as_nanos());
            }
        }
        samples.sort_unstable();
        println!(
            "rotated={rotated} paragraphs=500 replay_p50_us={:.2} replay_p95_us={:.2}",
            samples[250] as f64 / 1000.,
            samples[475] as f64 / 1000.
        );
        black_box(surface.read_pixels().unwrap());
    }
}
