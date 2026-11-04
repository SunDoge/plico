//! Own SDF quads and Vello vector paths, generated from identical quad descriptions.
use image::{Rgba, RgbaImage, imageops};
use kurbo::{Affine, Rect, Shape};
use peniko::{Color, Fill, Gradient};
use plico_renderer::{
    PaintContext, PaintScene, Scene, ShaderEffectId, ShaderEffectSource, TextContext,
};
use plico_wgpu::{MATERIAL_INTERFACE, Quad, Renderer};
use snafu::{ResultExt, Snafu};
use std::{path::Path, sync::Arc};

#[derive(Debug, Snafu)]
enum Error {
    #[snafu(display("quad renderer failed: {source}"))]
    Render { source: plico_wgpu::Error },
    #[snafu(display("file operation failed: {source}"))]
    Io { source: std::io::Error },
    #[snafu(display("image operation failed: {source}"))]
    Image { source: image::ImageError },
    #[snafu(display("reference shader failed: {message}"))]
    Shader { message: String },
    #[snafu(display("bad image dimensions"))]
    Dimensions,
}
fn color(c: [f32; 4]) -> Color {
    Color::new(c)
}
fn fixture(scale: f32) -> Vec<Quad> {
    let mut quads = Vec::new();
    let mut panel = Quad::new([18., 18., 604., 404.], [0.98, 0.985, 0.99, 1.]);
    panel.radius = 18.;
    panel.border_width = 1.;
    panel.border_color = [0.78, 0.81, 0.86, 1.];
    quads.push(panel);
    for (i, radius) in [0., 3., 12., 28.].into_iter().enumerate() {
        let mut q = Quad::new(
            [40. + i as f32 * 145., 42., 125., 72.],
            [0.32, 0.54, 0.92, 1.],
        );
        q.radius = radius;
        q.colors[1] = [0.10, 0.27, 0.63, 1.];
        q.border_width = 1.;
        q.border_color = [0.06, 0.18, 0.44, 1.];
        quads.push(q);
    }
    for (i, width) in [0.5, 1., 2., 4.].into_iter().enumerate() {
        let mut q = Quad::new(
            [40.25 + i as f32 * 145., 140.25, 125., 64.],
            [0.97, 0.97, 0.97, 1.],
        );
        q.radius = 14.;
        q.border_width = width;
        q.border_color = [0.20, 0.24, 0.30, 1.];
        quads.push(q);
    }
    let mut clip = Quad::new([40., 230., 240., 72.], [0.95, 0.44, 0.34, 0.65]);
    clip.radius = 24.;
    clip.clip = Some([72., 246., 170., 38.]);
    quads.push(clip);
    let mut overlap = Quad::new([180., 258., 155., 48.], [0.24, 0.69, 0.49, 0.65]);
    overlap.radius = 16.;
    quads.push(overlap);
    let mut progress = Quad::new([360., 242., 235., 40.], [0.22, 0.52, 0.86, 1.]);
    progress.colors[1] = [0.12, 0.34, 0.71, 1.];
    progress.radius = 20.;
    progress.border_width = 1.;
    progress.border_color = [0.65, 0.73, 0.84, 1.];
    progress.material = true;
    progress.parameter = 0.72;
    quads.push(progress);
    for (i, size) in [0.5, 1., 1.5, 2., 3., 4., 6.].into_iter().enumerate() {
        let mut q = Quad::new(
            [40. + i as f32 * 28., 340.25, size, 40.],
            [0.12, 0.16, 0.23, 1.],
        );
        q.radius = size * 0.5;
        quads.push(q);
    }
    let mut pill = Quad::new([300.25, 334.5, 295., 48.], [0.96, 0.96, 0.96, 1.]);
    pill.radius = 100.;
    pill.border_width = 1.;
    pill.border_color = [0.70, 0.72, 0.76, 1.];
    quads.push(pill);
    for q in &mut quads {
        for v in &mut q.rect {
            *v *= scale;
        }
        q.radius *= scale;
        q.border_width *= scale;
        if let Some(clip) = &mut q.clip {
            for v in clip {
                *v *= scale;
            }
        }
    }
    quads
}
fn reference(quads: &[Quad], material: &str, time: f32) -> Result<Scene, Error> {
    let mut scene = Scene::new();
    let mut text = TextContext::new();
    let shader=ShaderEffectSource::new(&format!("{MATERIAL_INTERFACE}\n{material}\n{}",r#"
fn wabou_effect(uv:vec2f) -> vec4f {
 return plico_material(Material(uv,wabou.resolution_time_scale.xy,wabou.values[0],wabou.values[1],wabou.resolution_time_scale.z,wabou.values[2].x));
}"#)).map_err(|message|Error::Shader {message})?;
    for q in quads {
        let [x, y, w, h] = q.rect.map(f64::from);
        let rect = Rect::new(x, y, x + w, y + h);
        let radius = f64::from(q.radius).min(w.min(h) * 0.5);
        let rounded = rect.to_rounded_rect(radius);
        if let Some([x, y, w, h]) = q.clip {
            scene.push_clip_layer(
                Affine::IDENTITY,
                &Rect::new(x as f64, y as f64, (x + w) as f64, (y + h) as f64),
            );
        }
        if q.material {
            scene.push_clip_layer(Affine::IDENTITY, &rounded);
            let mut cx = PaintContext::new(w as f32, h as f32, 1., &mut text);
            let values: Arc<[f32]> = q
                .colors
                .into_iter()
                .flatten()
                .chain([q.parameter])
                .collect();
            cx.draw_shader_effect(ShaderEffectId::new(), shader.clone(), time, values);
            scene.append_scene(cx.finish(), Affine::translate((x, y)));
            scene.pop_layer();
        } else {
            let gradient = Gradient::new_linear((x, y), (x, y + h))
                .with_interpolation_cs(peniko::color::ColorSpaceTag::Srgb)
                .with_stops([color(q.colors[0]), color(q.colors[1])]);
            scene.fill(Fill::NonZero, Affine::IDENTITY, &gradient, None, &rounded);
        }
        if q.border_width > 0. {
            let border = f64::from(q.border_width).min(w.min(h) * 0.5);
            let mut ring = rounded.to_path(0.01);
            if w > 2. * border && h > 2. * border {
                ring.extend(
                    rect.inset(-border)
                        .to_rounded_rect((radius - border).max(0.))
                        .path_elements(0.01),
                );
            }
            scene.fill(
                Fill::EvenOdd,
                Affine::IDENTITY,
                color(q.border_color),
                None,
                &ring,
            );
        }
        if q.clip.is_some() {
            scene.pop_layer();
        }
    }
    Ok(scene)
}
fn image(width: u32, height: u32, pixels: Vec<u8>) -> Result<RgbaImage, Error> {
    RgbaImage::from_raw(width, height, pixels).ok_or(Error::Dimensions)
}
fn main() -> Result<(), Error> {
    let output = Path::new("reports/wgpu-quads");
    std::fs::create_dir_all(output).context(IoSnafu)?;
    // Read at runtime. The binary may be rerun after editing WGSL without cargo.
    let material_path = std::env::var_os("PLICO_WGPU_MATERIAL")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/materials/progress.wgsl")
        });
    let mut material = std::fs::read_to_string(&material_path).context(IoSnafu)?;
    let mut renderer = pollster::block_on(Renderer::new()).context(RenderSnafu)?;
    println!("wgpu adapter: {:?}", renderer.adapter_info());
    pollster::block_on(renderer.set_material(&material)).context(RenderSnafu)?;
    let mut vello = plico_vello::ImageRenderer::new(640, 440);
    capture(&mut renderer, &mut vello, &material, output)?;
    println!("WGSL file: {}", material_path.display());
    if std::env::var_os("PLICO_WGPU_WATCH").is_some() {
        println!(
            "watching WGSL; save the file to rebuild GPU pipelines and regenerate images (Ctrl+C to stop)"
        );
        loop {
            std::thread::sleep(std::time::Duration::from_millis(200));
            let Ok(next) = std::fs::read_to_string(&material_path) else {
                continue;
            };
            if next == material {
                continue;
            }
            material = next;
            match pollster::block_on(renderer.set_material(&material)) {
                Ok(_) => {
                    if let Err(error) = capture(&mut renderer, &mut vello, &material, output) {
                        eprintln!("capture failed: {error}");
                    }
                }
                Err(error) => {
                    eprintln!("material rejected; keeping previous pipeline and images: {error}")
                }
            }
        }
    }
    Ok(())
}
fn capture(
    renderer: &mut Renderer,
    vello: &mut plico_vello::ImageRenderer,
    material: &str,
    output: &Path,
) -> Result<(), Error> {
    let background = [0.93, 0.94, 0.96, 1.];
    for scale in [1., 1.25, 1.5, 2.] {
        let size = [(640. * scale) as u32, (440. * scale) as u32];
        let [width, height] = size;
        let quads = fixture(scale);
        let time = 1.25;
        let own = image(
            width,
            height,
            renderer
                .render(&quads, size, background, time)
                .context(RenderSnafu)?,
        )?;
        let scene = reference(&quads, material, time)?;
        let reference = image(
            width,
            height,
            vello.render(&scene, width, height, color(background)),
        )?;
        let mut combined = RgbaImage::from_pixel(width * 3, height, Rgba([255; 4]));
        imageops::overlay(&mut combined, &own, 0, 0);
        imageops::overlay(&mut combined, &reference, i64::from(width), 0);
        let mut diff = RgbaImage::new(width, height);
        let mut total = 0u64;
        let mut changed = 0usize;
        for ((a, b), d) in own.pixels().zip(reference.pixels()).zip(diff.pixels_mut()) {
            let error = std::array::from_fn::<_, 3, _>(|c| a[c].abs_diff(b[c]));
            total += error.iter().map(|v| u64::from(*v)).sum::<u64>();
            if error.iter().any(|e| *e > 8) {
                changed += 1;
            }
            *d = Rgba([
                error[0].saturating_mul(8),
                error[1].saturating_mul(8),
                error[2].saturating_mul(8),
                255,
            ]);
        }
        imageops::overlay(&mut combined, &diff, i64::from(width * 2), 0);
        combined
            .save(output.join(format!("wgpu-vello-diff-{scale}x.png")))
            .context(ImageSnafu)?;
        println!(
            "scale={scale} quads={} mean RGB error={:.3}/255 pixels with error>8={:.2}% (right panel=8x difference)",
            quads.len(),
            total as f64 / f64::from(width * height * 3),
            changed as f64 / f64::from(width * height) * 100.
        );
    }
    // Fixed-time snapshots demonstrate WGSL animation without rebuilding CPU geometry.
    let size = [640, 440];
    let quads = fixture(1.);
    let mut sheet = RgbaImage::new(640 * 3, 440);
    for (i, time) in [0., 0.5, 1.].into_iter().enumerate() {
        let pixels = renderer
            .render(&quads, size, background, time)
            .context(RenderSnafu)?;
        imageops::overlay(&mut sheet, &image(640, 440, pixels)?, (i * 640) as i64, 0);
    }
    sheet
        .save(output.join("material-time-snapshots.png"))
        .context(ImageSnafu)?;
    println!(
        "capture complete; total pipeline compilations={}",
        renderer.pipeline_compilations()
    );
    Ok(())
}
