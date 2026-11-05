//! GPU minification diagnostic. Run with --example image-minification.
//! Area-average error measures lost high-frequency detail, not overall image quality.
use kurbo::Affine;
use peniko::{Blob, ImageAlphaType, ImageBrush, ImageData, ImageFormat, ImageQuality};
use plico_renderer::{PaintScene, Scene};
use plico_wgpu::Renderer;
use std::sync::Arc;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (width, height) = (257usize, 193usize);
    let pixels: Vec<u8> = (0..height)
        .flat_map(|y| {
            (0..width).flat_map(move |x| {
                let value = if (x + y) % 2 == 0 { 0 } else { 255 };
                [value, value, value, 255]
            })
        })
        .collect();
    let source = ImageData {
        data: Blob::new(Arc::new(pixels.clone())),
        width: width as u32,
        height: height as u32,
        format: ImageFormat::Rgba8,
        alpha_type: ImageAlphaType::AlphaPremultiplied,
    };
    let mut renderer = pollster::block_on(Renderer::new())?;
    println!("adapter: {:?}", renderer.adapter_info());
    println!("quality,size,rmse,max_error,source_textures,texture_bytes,uploads");
    for quality in [ImageQuality::Low, ImageQuality::Medium, ImageQuality::High] {
        let mut brush = ImageBrush::new(source.clone());
        brush.sampler.quality = quality;
        for [w, h] in [[128, 96], [64, 48], [17, 13], [8, 6]] {
            let mut scene = Scene::new();
            scene.draw_image(
                (&brush).into(),
                Affine::scale_non_uniform(w as f64 / width as f64, h as f64 / height as f64),
            );
            let actual = renderer.render_scene(&scene, [w, h], [0.; 4], false)?;
            let mut squared = 0.;
            let mut maximum = 0_f64;
            for y in 0..h {
                for x in 0..w {
                    let x0 = x as f64 * width as f64 / w as f64;
                    let x1 = (x + 1) as f64 * width as f64 / w as f64;
                    let y0 = y as f64 * height as f64 / h as f64;
                    let y1 = (y + 1) as f64 * height as f64 / h as f64;
                    let mut sum = 0.;
                    for sy in y0.floor() as usize..(y1.ceil() as usize).min(height) {
                        for sx in x0.floor() as usize..(x1.ceil() as usize).min(width) {
                            let area = (x1.min((sx + 1) as f64) - x0.max(sx as f64))
                                * (y1.min((sy + 1) as f64) - y0.max(sy as f64));
                            sum += area * f64::from(pixels[(sy * width + sx) * 4]);
                        }
                    }
                    let reference = sum / ((x1 - x0) * (y1 - y0));
                    let index = (y * w + x) as usize * 4;
                    assert_eq!(actual[index + 3], 255);
                    let error = (f64::from(actual[index]) - reference).abs();
                    squared += error * error;
                    maximum = maximum.max(error);
                }
            }
            let stats = renderer.image_cache_stats();
            println!(
                "{quality:?},{w}x{h},{:.3},{maximum:.3},{},{},{}",
                (squared / f64::from(w * h)).sqrt(),
                stats.0,
                stats.1,
                stats.2
            );
        }
    }
    Ok(())
}
