//! Pixel comparisons for the production GPU path, not screenshots by eye.
#![cfg(not(target_arch = "wasm32"))]
use kurbo::Affine;
use peniko::{Color, ImageQuality};
use plico_renderer::{PaintScene, Scene, image::RasterImage};
fn render(
    image: &RasterImage,
    transform: Affine,
    width: u32,
    height: u32,
    background: Color,
) -> image::RgbaImage {
    let mut scene = Scene::new();
    scene.draw_image(image.brush().into(), transform);
    plico_shell::renderer::render_to_image(&scene, width, height, background).unwrap()
}
#[test]
fn native_size_pixels_and_integer_nearest_zoom_are_exact() {
    let source = image::RgbaImage::from_fn(128, 96, |x, y| {
        let stripe = if (x + y) % 2 == 0 { 0 } else { 255 };
        image::Rgba([stripe, (x * 2) as u8, (y * 2) as u8, 255])
    });
    let raster = RasterImage::from_rgba(source.clone());
    let output = render(&raster, Affine::translate((3., 5.)), 134, 106, Color::WHITE);
    for (x, y, p) in source.enumerate_pixels() {
        assert_eq!(output.get_pixel(x + 3, y + 5), p, "native pixel {x},{y}");
    }
    let source = image::RgbaImage::from_fn(8, 8, |x, y| {
        image::Rgba([
            if (x + y) % 2 == 0 { 0 } else { 255 },
            (x * 30) as u8,
            (y * 30) as u8,
            255,
        ])
    });
    let raster = RasterImage::from_rgba(source.clone()).with_quality(ImageQuality::Low);
    let output = render(&raster, Affine::scale(4.), 32, 32, Color::WHITE);
    for (x, y, p) in output.enumerate_pixels() {
        assert_eq!(p, source.get_pixel(x / 4, y / 4), "nearest zoom {x},{y}");
    }
}
#[test]
fn bilinear_interior_matches_reference_and_alpha_has_no_dark_fringe() {
    let mut source = image::RgbaImage::new(2, 2);
    for y in 0..2 {
        source.put_pixel(0, y, image::Rgba([0, 0, 0, 255]));
        source.put_pixel(1, y, image::Rgba([255, 255, 255, 255]));
    }
    let raster = RasterImage::from_rgba(source).with_quality(ImageQuality::Medium);
    let output = render(&raster, Affine::scale(4.), 8, 8, Color::BLACK);
    // Interior sampling is correct; Pad at the far edge has an upstream bug.
    for x in 0..4 {
        let expected = (((x as f32 + 0.5) / 4. - 0.5).clamp(0., 1.) * 255.).round() as u8;
        let pixel = output.get_pixel(x, 4).0;
        assert!(
            pixel[0].abs_diff(expected) <= 1,
            "bilinear {x}: {pixel:?}, expected {expected}"
        );
    }
    let mut source = image::RgbaImage::new(2, 2);
    for y in 0..2 {
        source.put_pixel(0, y, image::Rgba([255, 0, 0, 255]));
        source.put_pixel(1, y, image::Rgba([0, 0, 0, 0]));
    }
    let raster = RasterImage::from_rgba(source).with_quality(ImageQuality::Medium);
    let output = render(&raster, Affine::scale(4.), 8, 8, Color::WHITE);
    for x in 0..8 {
        let pixel = output.get_pixel(x, 4).0;
        assert!(
            pixel[0] >= 254 && pixel[3] == 255,
            "dark transparent fringe at {x}: {pixel:?}"
        );
    }
}
#[path = "../../plico-renderer/tests/support/webp.rs"]
mod webp;
#[test]
fn animated_webp_composited_frames_reach_gpu_pixels() {
    use plico_renderer::animated_image::{AnimatedImage, AnimationLimits};
    let frames = AnimatedImage::decode_webp(&webp::fixture(1), AnimationLimits::default()).unwrap();
    for ms in [0, 40, 120, 240] {
        let sample = frames.sample(std::time::Duration::from_millis(ms));
        let output = render(&sample.image, Affine::IDENTITY, 4, 2, Color::WHITE);
        for x in 0..4 {
            let p = &sample.image.rgba()[x * 4..x * 4 + 4];
            let actual = output.get_pixel(x as u32, 0).0;
            for channel in 0..3 {
                let expected = ((u32::from(p[channel]) * u32::from(p[3])
                    + 255 * (255 - u32::from(p[3]))
                    + 127)
                    / 255) as u8;
                assert!(
                    actual[channel].abs_diff(expected) <= 1,
                    "{ms}ms pixel {x}: {actual:?} expected {expected}"
                );
            }
        }
    }
}

// Keep the exact failing assertion for an upstream sampler fix. The shader's
// extend_mode(Pad) clamps to size - 1 before subtracting the half-texel offset.
#[test]
#[ignore = "vello_gpu 0.3 Pad clamps before half-texel correction at the far edge"]
fn bilinear_pad_far_edge_matches_reference() {
    let source =
        image::RgbaImage::from_fn(2, 2, |x, _| image::Rgba([if x == 0 { 0 } else { 255 }; 4]));
    let mut source = source;
    for p in source.pixels_mut() {
        p[3] = 255;
    }
    let raster = RasterImage::from_rgba(source).with_quality(ImageQuality::Medium);
    let output = render(&raster, Affine::scale(4.), 8, 8, Color::BLACK);
    for x in 4..8 {
        let expected = (((x as f32 + 0.5) / 4. - 0.5).clamp(0., 1.) * 255.).round() as u8;
        assert!(output.get_pixel(x, 4)[0].abs_diff(expected) <= 1);
    }
}
