//! Independent Pango/Cairo glyph atlas + wgpu compositor; no Vello rendering.
#[cfg(target_os = "linux")]
use plico_native::text::atlas;
#[cfg(target_os = "linux")]
mod gpu;

#[cfg(target_os = "linux")]
fn quads(
    atlas: &mut atlas::Atlas,
    glyphs: &[atlas::Glyph],
    offset: [f64; 2],
    viewport: [u32; 2],
    phased: bool,
) -> Result<Vec<gpu::Quad>, Box<dyn std::error::Error>> {
    glyphs
        .iter()
        .map(|glyph| {
            let x = glyph.x + offset[0];
            let y = glyph.y + offset[1];
            let quantized = (x * 4.).round() as i64;
            let (x, y, phase) = if phased {
                (
                    quantized.div_euclid(4) as f64,
                    y.round(),
                    quantized.rem_euclid(4) as u8,
                )
            } else {
                (x, y, 0)
            };
            let entry = atlas.get(glyph, phase)?;
            let [ax, ay, w, h] = entry.rect;
            let [vw, vh] = viewport.map(f64::from);
            Ok(gpu::Quad {
                rect: [
                    ((x + f64::from(entry.origin[0])) / vw * 2. - 1.) as f32,
                    (1. - (y + f64::from(entry.origin[1])) / vh * 2.) as f32,
                    (f64::from(w) / vw * 2.) as f32,
                    (-f64::from(h) / vh * 2.) as f32,
                ],
                uv: [
                    ax as f32 / atlas::ATLAS_SIZE as f32,
                    ay as f32 / atlas::ATLAS_SIZE as f32,
                    w as f32 / atlas::ATLAS_SIZE as f32,
                    h as f32 / atlas::ATLAS_SIZE as f32,
                ],
                color: if entry.colored {
                    [1.; 4]
                } else {
                    [26. / 255., 31. / 255., 38. / 255., 1.]
                },
            })
        })
        .collect()
}

#[cfg(target_os = "linux")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use atlas::Atlas;
    use image::{RgbaImage, imageops};
    use plico_native::text::{SystemText, TextRequest};
    use plico_renderer::{
        PaintCommand, Scene, TextContext, paragraph::ParagraphRequest, style::Paint,
    };
    use std::{sync::Arc, time::Instant};
    let output = std::path::Path::new("reports/system-text-wgpu");
    std::fs::create_dir_all(output)?;
    let mut gpu = pollster::block_on(gpu::Gpu::new())?;
    let mut atlas = Atlas::new()?;
    let mut native = SystemText::new(16 * 1024 * 1024)?;
    let samples = [
        ("12px Sss Settings 0123456789", 12., 400),
        ("14px Sss Settings 0123456789", 14., 400),
        ("14px Sss Settings 0123456789", 14., 500),
        ("18px 中文笔画：清晰、均匀", 18., 400),
        ("24px 日本語の小さな文字", 24., 400),
        ("S s Ss — Smooth curves", 48., 400),
        ("مرحبا بالعالم — ffi office", 20., 400),
        ("中文 / 日本語 / 👩‍💻 / 🇯🇵", 20., 400),
    ];
    for scale in [1., 1.25, 1.5, 2.] {
        let [width, height] = [(640. * scale) as u32, (430. * scale) as u32];
        let mut surface = plico_skia::SkiaSurface::new(width, height)?;
        let mut text = TextContext::new();
        surface.configure_text(&mut text);
        assert_eq!(text.raster_backend_name(), "skparagraph");
        let mut scene = Scene::new();
        let mut draws = Vec::new();
        let mut pango = RgbaImage::from_pixel(width, height, image::Rgba([255; 4]));
        let mut y = 16.;
        for (label, size, weight) in samples {
            let mut req = TextRequest::new(label);
            req.size = size * scale;
            req.weight = weight;
            req.family = "Noto Sans, Noto Sans CJK SC, sans-serif";
            req.color = [26, 31, 38, 255];
            let bitmap = native.rasterize(&req)?;
            let glyphs = atlas.shape(label, size * scale, weight)?;
            draws.extend(quads(
                &mut atlas,
                &glyphs,
                [16. * scale, y * scale],
                [width, height],
                true,
            )?);
            let image =
                RgbaImage::from_raw(bitmap.pixels[0], bitmap.pixels[1], bitmap.rgba.clone())
                    .ok_or("bad native image")?;
            imageops::overlay(
                &mut pango,
                &image,
                (16. * scale) as i64 + i64::from(bitmap.pixel_origin[0]),
                (y * scale) as i64 + i64::from(bitmap.pixel_origin[1]),
            );
            let request = Arc::new(
                ParagraphRequest::from_paint(
                    &Paint {
                        text: Some(label.into()),
                        font_family: Some(req.family.into()),
                        font_size: req.size as f32,
                        font_weight: f32::from(weight),
                        text_color: peniko::Color::from_rgb8(26, 31, 38),
                        ..Default::default()
                    },
                    None,
                )
                .ok_or("missing paragraph")?,
            );
            let paragraph = text
                .native_paragraph(request.clone())
                .ok_or("native paragraph unavailable")?;
            let baseline = paragraph.metrics().baselines[0].unwrap_or(0.) as f64;
            scene.commands.push(PaintCommand::Paragraph {
                request,
                transform: kurbo::Affine::translate((
                    16. * scale,
                    y * scale + bitmap.baseline - baseline,
                )),
            });
            y += if size == 48. { 76. } else { 44. };
        }
        gpu.upload(&atlas.pixels, atlas.revision);
        let rgba = gpu.render(&draws, width, height)?;
        let own = RgbaImage::from_raw(width, height, rgba).ok_or("bad gpu image")?;
        let skia = RgbaImage::from_raw(
            width,
            height,
            surface.render_scene(&scene, peniko::Color::WHITE)?,
        )
        .ok_or("bad skia image")?;
        let mut combined = RgbaImage::from_pixel(width * 3, height, image::Rgba([255; 4]));
        imageops::overlay(&mut combined, &pango, 0, 0);
        imageops::overlay(&mut combined, &own, i64::from(width), 0);
        imageops::overlay(&mut combined, &skia, i64::from(width * 2), 0);
        let path = output.join(format!("pango-wgpu-skparagraph-{scale}x.png"));
        combined.save(&path)?;
        println!(
            "{} | left=Pango whole-layout, middle=own wgpu glyph atlas, right=SkParagraph",
            path.display()
        );
    }
    // Fixed shaped content, finite horizontal raster phases, both positive and negative motion.
    let glyphs = atlas.shape("Sss 中文 日本語 👩‍💻", 18., 400)?;
    for frame in 0..16 {
        let _ = quads(
            &mut atlas,
            &glyphs,
            [frame as f64 / 4. - 2., 0.],
            [640, 96],
            true,
        )?;
    }
    gpu.upload(&atlas.pixels, atlas.revision);
    let warm_revision = atlas.revision;
    let warm_uploads = gpu.uploads;
    let start = Instant::now();
    let iterations = 100_000;
    for frame in 0..iterations {
        let phase = (frame % 16) as f64 / 4. - 2.;
        std::hint::black_box(quads(&mut atlas, &glyphs, [phase, phase], [640, 96], true)?);
        gpu.upload(&atlas.pixels, atlas.revision);
    }
    let elapsed = start.elapsed();
    assert_eq!(
        atlas.revision, warm_revision,
        "translation caused glyph cache growth"
    );
    assert_eq!(
        gpu.uploads, warm_uploads,
        "translation uploaded the atlas again"
    );
    println!(
        "motion preparation: {:.3} us/frame ({} glyphs; CPU placement + cache lookup + allocating Vec only, no GPU submit)",
        elapsed.as_secs_f64() * 1e6 / iterations as f64,
        glyphs.len()
    );
    println!(
        "after warmup: {iterations} moves, new glyph entries=0, atlas uploads=0; total entries={}, atlas={} MiB fixed, rasterized glyph payload={} KiB",
        atlas.revision,
        atlas.pixels.len() / 1024 / 1024,
        atlas.rasterized_bytes / 1024
    );
    // Contact sheet: fractional texture translation versus re-rasterized X / pixel-snapped Y.
    let mut sheet = RgbaImage::from_pixel(1280, 8 * 64, image::Rgba([255; 4]));
    for frame in 0..8 {
        let offset = [16. + frame as f64 / 8., 8. + frame as f64 / 8.];
        for (column, phased) in [false, true].into_iter().enumerate() {
            let draw = quads(&mut atlas, &glyphs, offset, [640, 64], phased)?;
            gpu.upload(&atlas.pixels, atlas.revision);
            let image = RgbaImage::from_raw(640, 64, gpu.render(&draw, 640, 64)?)
                .ok_or("bad motion image")?;
            imageops::overlay(&mut sheet, &image, (column * 640) as i64, frame * 64);
        }
    }
    sheet.save(output.join("motion-sampled-vs-snapped.png"))?;
    let mut ink = [0u64; 2];
    let mut gamma = RgbaImage::from_pixel(1280, 128, image::Rgba([255; 4]));
    let mut draws = Vec::new();
    for (label, size, y) in [
        ("14px Sss Settings 中文笔画 日本語", 14., 12.),
        ("24px Sss Settings 中文笔画 日本語", 24., 60.),
    ] {
        let glyphs = atlas.shape(label, size, 400)?;
        draws.extend(quads(&mut atlas, &glyphs, [16., y], [640, 128], true)?);
    }
    gpu.upload(&atlas.pixels, atlas.revision);
    for (column, linear) in [false, true].into_iter().enumerate() {
        let image = RgbaImage::from_raw(640, 128, gpu.render_mode(&draws, 640, 128, linear)?)
            .ok_or("bad gamma image")?;
        ink[column] = image.pixels().map(|p| u64::from(255 - p[0])).sum();
        assert!(image.pixels().any(|p| p[0] < 100), "blank text output");
        imageops::overlay(&mut gamma, &image, (column * 640) as i64, 0);
    }
    gamma.save(output.join("blend-encoded-vs-linear.png"))?;
    assert!(
        ink[0] > ink[1],
        "blend-space control did not change edge intensity"
    );
    println!(
        "same glyph masks: linear-light blend has {:.1}% less red-channel ink against white than encoded-space blend (not a quality score)",
        100. * (1. - ink[1] as f64 / ink[0] as f64)
    );
    Ok(())
}
#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!("This system-text experiment currently supports Linux only.");
}
