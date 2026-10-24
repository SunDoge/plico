//! Composite a pre-rasterized physical-pixel coverage mask with production Vello.
use plico_shell::{PaintScene, Scene, renderer};
use std::{path::PathBuf, sync::Arc};
use vello_common::{
    kurbo::{Affine, Rect},
    peniko::{Blob, Color, Fill, ImageAlphaType, ImageBrush, ImageData, ImageFormat, ImageQuality},
};

#[derive(usage_rs::Cli)]
#[usage(about = "Composite a physical-pixel glyph alpha mask through Vello")]
struct Options {
    #[usage(long)]
    mask: PathBuf,
    #[usage(long)]
    output: PathBuf,
    #[usage(long)]
    foreground: String,
    #[usage(long)]
    background: String,
}
fn color(value: &str) -> Color {
    let value = u32::from_str_radix(value.trim_start_matches('#'), 16).expect("RGB hex");
    Color::from_rgb8((value >> 16) as u8, (value >> 8) as u8, value as u8)
}
fn main() {
    let options = Options::parse();
    let mask = image::open(&options.mask).expect("read mask").into_rgba8();
    let (width, height) = mask.dimensions();
    assert!(
        mask.pixels().any(|p| p.0[3] == 0),
        "mask must preserve transparent background"
    );
    assert!(
        mask.pixels().any(|p| p.0[3] > 0 && p.0[3] < 255),
        "mask must contain grayscale coverage"
    );
    let foreground = color(&options.foreground).to_rgba8();
    let mut pixels = mask.into_raw();
    for pixel in pixels.as_chunks_mut::<4>().0 {
        // Straight RGBA: coverage is alpha, RGB is never preblended with a background.
        pixel[..3].copy_from_slice(&[foreground.r, foreground.g, foreground.b]);
    }
    let mut brush = ImageBrush::new(ImageData {
        data: Blob::new(Arc::new(pixels)),
        format: ImageFormat::Rgba8,
        alpha_type: ImageAlphaType::Alpha,
        width,
        height,
    });
    brush.sampler.quality = ImageQuality::Low;
    let mut scene = Scene::new();
    scene.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        &brush,
        None,
        &Rect::new(0., 0., width as f64, height as f64),
    );
    renderer::render_to_png(
        &scene,
        width,
        height,
        color(&options.background),
        &options.output.to_string_lossy(),
    )
    .expect("render mask at 1:1 physical pixels");
}
