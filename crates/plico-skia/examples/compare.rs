//! Same Parley-shaped glyphs and scene, replayed by Skia CPU and Vello GPU.
use kurbo::{Affine, Rect, RoundedRect};
use peniko::{Color, Fill};
use plico_renderer::style::TextAlign;
use plico_renderer::{
    PaintScene, Scene, TextContext,
    text::{FontAsset, layout_text_styled},
};
use plico_skia::SkiaSurface;
use snafu::{ResultExt, Snafu};
use std::{path::Path, sync::Arc};

#[derive(Debug, Snafu)]
enum Error {
    #[snafu(display("file operation failed: {source}"))]
    Io { source: std::io::Error },
    #[snafu(display("Skia replay failed: {source}"))]
    Skia { source: plico_skia::Error },
    #[snafu(display("Vello render failed: {source}"))]
    Vello { source: plico_shell::Error },
    #[snafu(display("image output failed: {source}"))]
    Image { source: image::ImageError },
    #[snafu(display("comparison font could not be registered or has missing glyphs"))]
    Font,
}
fn main() -> Result<(), Error> {
    let output = Path::new("reports/skia");
    std::fs::create_dir_all(output).context(IoSnafu)?;
    let font_path = std::env::var("PLICO_PROBE_FONT")
        .unwrap_or_else(|_| "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf".into());
    let mut text = TextContext::new();
    let family: Arc<str> = Arc::from("PlicoComparison");
    if !text.register_font(
        &FontAsset::new(std::fs::read(&font_path).context(IoSnafu)?).with_family(family.clone()),
    ) {
        return Err(Error::Font);
    }
    let cjk_path = std::env::var("PLICO_PROBE_CJK_FONT").ok().or_else(|| {
        let path = "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc";
        Path::new(path).exists().then(|| path.to_owned())
    });
    let cjk_family: Arc<str> = Arc::from("PlicoCjkComparison");
    if let Some(path) = &cjk_path {
        if !text.register_font(
            &FontAsset::new(std::fs::read(path).context(IoSnafu)?).with_family(cjk_family.clone()),
        ) {
            return Err(Error::Font);
        }
    } else {
        eprintln!("No CJK font: set PLICO_PROBE_CJK_FONT to include Chinese samples");
    }
    let background = Color::from_rgb8(247, 248, 250);
    #[cfg(feature = "vulkan")]
    let mut gpu = {
        let gpu = match std::env::var("PLICO_SKIA_DEVICE") {
            Ok(name) => plico_skia::SkiaGpuSurface::with_device(900, 560, &name),
            Err(_) => plico_skia::SkiaGpuSurface::new(900, 560),
        }
        .context(SkiaSnafu)?;
        println!(
            "Skia Vulkan device: {} (software: {})",
            gpu.device_name(),
            gpu.is_software_device()
        );
        gpu
    };
    for scale in [1., 1.25, 1.5, 2.] {
        let (width, height) = ((900. * scale) as u32, (560. * scale) as u32);
        let mut scene = Scene::new();
        scene.fill(
            Fill::NonZero,
            Affine::scale(scale),
            Color::WHITE,
            None,
            &RoundedRect::new(20., 20., 880., 540., 12.),
        );
        let mut y = 36.;
        for size in [12., 14., 18., 32., 64.] {
            let label: Arc<str> = Arc::from(if size == 64. {
                "64px   S s Ss 0123".to_owned()
            } else {
                format!("{size:.0}px   S s Ss 0123456789  Smooth curves")
            });
            let layout = layout_text_styled(
                &mut text,
                label,
                size,
                400.,
                false,
                None,
                TextAlign::Start,
                [28, 32, 40, 255],
                Arc::from([]),
                Some(&family),
                None,
            );
            let glyphs = text.glyph_scene_scaled(&layout, scale);
            scene.append_scene(
                (*glyphs).clone(),
                Affine::translate((40. * scale, y * scale)),
            );
            y += size as f64 * 1.6 + 16.;
        }
        if cjk_path.is_some() {
            for (label, size, y) in [
                ("14px 中文笔画：文字渲染，图标对齐", 14., 355.),
                ("24px 中文笔画：文字渲染，图标对齐", 24., 390.),
            ] {
                let layout = layout_text_styled(
                    &mut text,
                    Arc::from(label),
                    size,
                    400.,
                    false,
                    None,
                    TextAlign::Start,
                    [28, 32, 40, 255],
                    Arc::from([]),
                    Some(&cjk_family),
                    None,
                );
                let glyphs = text.glyph_scene_scaled(&layout, scale);
                scene.append_scene(
                    (*glyphs).clone(),
                    Affine::translate((40. * scale, y * scale)),
                );
            }
        }
        for command in &scene.commands {
            if let plico_renderer::PaintCommand::GlyphRun { glyphs, .. } = command
                && glyphs.iter().any(|g| g.id == 0)
            {
                return Err(Error::Font);
            }
        }
        scene.push_clip_layer(Affine::scale(scale), &Rect::new(40., 465., 850., 510.));
        for i in 0..14 {
            let x = 40. + i as f64 * 55. + 0.25;
            scene.fill(
                Fill::NonZero,
                Affine::scale(scale),
                Color::from_rgb8(48, 112, 210),
                None,
                &Rect::new(x, 450., x + 1. + i as f64 * 0.2, 530.),
            );
        }
        scene.pop_layer();
        let mut surface = SkiaSurface::new(width, height).context(SkiaSnafu)?;
        surface
            .render_scene(&scene, background)
            .context(SkiaSnafu)?;
        let skia_path = output.join(format!("skia-{scale}.png"));
        surface.write_png(&skia_path).context(SkiaSnafu)?;
        let vello_path = output.join(format!("vello-{scale}.png"));
        plico_shell::renderer::render_to_png(
            &scene,
            width,
            height,
            background,
            vello_path.to_str().expect("static ASCII output path"),
        )
        .context(VelloSnafu)?;
        let skia = image::open(&skia_path).context(ImageSnafu)?.to_rgba8();
        let vello = image::open(&vello_path).context(ImageSnafu)?.to_rgba8();
        let mut pair = image::RgbaImage::new(width * 2, height);
        image::imageops::overlay(&mut pair, &skia, 0, 0);
        image::imageops::overlay(&mut pair, &vello, width as i64, 0);
        pair.save(output.join(format!("compare-{scale}.png")))
            .context(ImageSnafu)?;
        #[cfg(feature = "vulkan")]
        {
            gpu.resize(width, height).context(SkiaSnafu)?;
            // Normal drawing only submits GPU work. Screenshot export is the
            // explicit synchronization/readback boundary.
            gpu.draw_scene(&scene, background).context(SkiaSnafu)?;
            let gpu_path = output.join(format!("skia-gpu-{scale}.png"));
            gpu.write_png(&gpu_path).context(SkiaSnafu)?;
            let pixels = image::open(gpu_path).context(ImageSnafu)?.to_rgba8();
            let cpu_gpu_changed = skia
                .pixels()
                .zip(pixels.pixels())
                .filter(|(a, b)| a != b)
                .count();
            let gpu_vello_changed = pixels
                .pixels()
                .zip(vello.pixels())
                .filter(|(a, b)| a != b)
                .count();
            println!(
                "{scale}x pixel differences (not quality scores): CPU/GPU Skia {cpu_gpu_changed}; GPU Skia/Vello {gpu_vello_changed}; total {}",
                width as u64 * height as u64
            );
            image::imageops::overlay(&mut pair, &pixels, 0, 0);
            pair.save(output.join(format!("compare-gpu-{scale}.png")))
                .context(ImageSnafu)?;
        }
        println!(
            "{scale}x: {} commands, {}x{}; left Skia CPU, right Vello GPU; font {font_path}",
            scene.commands.len(),
            width,
            height
        );
    }
    Ok(())
}
