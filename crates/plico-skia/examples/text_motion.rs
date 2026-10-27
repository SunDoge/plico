//! Isolate fractional text translation from scrolling, scheduling and damage.
//! cargo run -p plico-skia --features vulkan --example text_motion
use plico_renderer::PaintScene;
use plico_renderer::{
    Scene, TextContext, paragraph::ParagraphRequest, style::Paint, text::FontAsset,
};
use plico_skia::{SkiaGpuSurface, SkiaSurface};
use std::sync::Arc;
use vello_common::{
    kurbo::{Affine, Rect},
    peniko::{Color, Fill},
};

fn centroid(pixels: &[u8]) -> [f64; 2] {
    let (mut weight, mut x, mut y) = (0., 0., 0.);
    for (i, p) in pixels.as_chunks::<4>().0.iter().enumerate() {
        let w = (255 - p[0]) as f64;
        weight += w;
        x += (i % 256) as f64 * w;
        y += (i / 256) as f64 * w;
    }
    assert!(weight > 0.);
    [x / weight, y / weight]
}
fn probe(name: &str, request: Arc<ParagraphRequest>, mut render: impl FnMut(&Scene) -> Vec<u8>) {
    for text in [true, false] {
        for vertical in [true, false] {
            let mut previous = None;
            let mut origin = [0.; 2];
            for step in 0..9 {
                let offset = step as f64 / 8.;
                let transform = Affine::translate((
                    12. + if vertical { 0. } else { offset },
                    12. + if vertical { offset } else { 0. },
                ));
                let mut scene = Scene::new();
                if text {
                    scene
                        .commands
                        .push(plico_renderer::PaintCommand::Paragraph {
                            request: request.clone(),
                            transform,
                        });
                } else {
                    scene.fill(
                        Fill::NonZero,
                        transform,
                        Color::BLACK,
                        None,
                        &Rect::new(0., 0., 120., 24.),
                    );
                }
                let pixels = render(&scene);
                let position = centroid(&pixels);
                if step == 0 {
                    origin = position;
                }
                println!(
                    "{name},{text},{vertical},{offset:.3},{:.4},{:.4},{}",
                    position[0] - origin[0],
                    position[1] - origin[1],
                    previous.as_ref() == Some(&pixels)
                );
                previous = Some(pixels);
            }
        }
    }
}
fn main() -> Result<(), plico_skia::Error> {
    let mut text = TextContext::new();
    text.register_font(
        &FontAsset::from_static(include_bytes!(
            "../../../backend/examples/web-preview/public/DejaVuSans.ttf"
        ))
        .with_family("MotionFixture"),
    );
    std::fs::create_dir_all("reports").map_err(|source| plico_skia::Error::Write { source })?;
    println!("backend,text,vertical,offset,centroid_dx,centroid_dy,repeated_pixels");
    let mode = if std::env::var("PLICO_SKIA_TEXT_MODE").as_deref() == Ok("mask") {
        "gpu-mask"
    } else {
        "gpu-distance-field"
    };
    for size in [12., 16., 28.] {
        let paint = Paint {
            text: Some("Smooth S".into()),
            font_family: Some("MotionFixture".into()),
            font_size: size,
            ..Default::default()
        };
        let mut cpu = SkiaSurface::new(256, 96)?;
        cpu.configure_text(&mut text);
        let request = Arc::new(text.paragraph_request(&paint, None).unwrap());

        probe(&format!("raster-{size}"), request, |s| {
            cpu.render_scene(s, Color::WHITE).unwrap()
        });
        let mut gpu = SkiaGpuSurface::new(256, 96)?;
        gpu.configure_text(&mut text);
        let request = Arc::new(text.paragraph_request(&paint, None).unwrap());
        probe(&format!("vulkan-{size}"), request.clone(), |s| {
            gpu.render_scene(s, Color::WHITE).unwrap()
        });
        let mut snapshot = Scene::new();
        snapshot
            .commands
            .push(plico_renderer::PaintCommand::Paragraph {
                request,
                transform: Affine::translate((12., 12.)),
            });
        cpu.draw_scene(&snapshot, Color::WHITE)?;
        gpu.draw_scene(&snapshot, Color::WHITE)?;
        cpu.write_png(std::path::Path::new(&format!(
            "reports/text-motion-{size}-mask.png"
        )))?;
        gpu.write_png(std::path::Path::new(&format!(
            "reports/text-motion-{size}-{mode}.png"
        )))?;
    }
    Ok(())
}
