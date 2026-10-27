//! Native and Parley text side by side, submitted through the same GPU renderer.
#[cfg(target_os = "linux")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use kurbo::Affine;
    use peniko::{Color, ImageQuality};
    use plico_native::text::{SystemText, TextRequest};
    use plico_renderer::{
        PaintScene, Scene, TextContext, image::RasterImage, style::TextAlign,
        text::layout_text_styled,
    };
    use std::sync::Arc;
    let output = std::path::Path::new("reports/system-text");
    std::fs::create_dir_all(output)?;
    let mut native = SystemText::new(16 * 1024 * 1024)?;
    let mut parley = TextContext::new();
    let family: Arc<str> = Arc::from("Noto Sans, Noto Sans CJK SC, sans-serif");
    for scale in [1., 1.5, 2.] {
        let mut scene = Scene::new();
        let mut y = 12.;
        for (label, size, weight) in [
            (
                "Native Pango / Cairo        |        Parley / Vello",
                14.,
                400,
            ),
            ("Sss 0123456789 Settings", 12., 400),
            ("Sss 0123456789 Settings", 14., 400),
            ("Sss 0123456789 Settings", 14., 500),
            ("Sss 0123456789 Settings", 24., 500),
            ("SsS Rendering", 48., 400),
            ("中文文字与图标 👩‍💻", 20., 400),
            ("مرحبا بالعالم — ffi office", 20., 400),
        ] {
            let mut request = TextRequest::new(label);
            request.family = &family;
            request.size = size;
            request.weight = weight;
            request.scale = scale;
            request.color = [32, 32, 36, 255];
            let bitmap = native.rasterize(&request)?;
            let raster = RasterImage::from_rgba(
                image::RgbaImage::from_raw(bitmap.pixels[0], bitmap.pixels[1], bitmap.rgba.clone())
                    .unwrap(),
            )
            .with_quality(ImageQuality::Low);
            scene.draw_image(
                raster.brush().into(),
                Affine::translate((
                    16. * scale + f64::from(bitmap.pixel_origin[0]),
                    y * scale + f64::from(bitmap.pixel_origin[1]),
                )),
            );
            let layout = layout_text_styled(
                &mut parley,
                Arc::from(label),
                size as f32,
                f32::from(weight),
                false,
                None,
                TextAlign::Start,
                request.color,
                Arc::from([]),
                Some(&family),
                None,
            );
            let baseline = f64::from(layout.lines().next().unwrap().metrics().baseline);
            plico_renderer::scene::draw_text_layout_into(
                &mut scene,
                &mut parley,
                &layout,
                Affine::IDENTITY,
                Affine::scale(scale) * Affine::translate((560., y + bitmap.baseline - baseline)),
                scale,
            );
            y += bitmap.logical_size[1].max(f64::from(layout.height())) + 18.;
        }
        let path = output.join(format!("native-vello-{scale}x.png"));
        plico_shell::renderer::render_to_png(
            &scene,
            (1120. * scale) as u32,
            ((y + 12.) * scale) as u32,
            Color::WHITE,
            path.to_str().unwrap(),
        )?;
        println!("{}", path.display());
    }
    Ok(())
}
#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!("System text experiment currently supports Linux only.");
}
