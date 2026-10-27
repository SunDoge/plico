#![cfg(all(target_os = "linux", feature = "linux-text"))]
use kurbo::Affine;
use peniko::{Color, ImageQuality};
use plico_native::text::{SystemText, TextRequest};
use plico_renderer::{PaintScene, Scene, image::RasterImage};

#[test]
fn native_bitmaps_survive_the_gpu_image_path_without_dark_alpha_fringes() {
    let mut system = SystemText::new(1024 * 1024).unwrap();
    for scale in [1., 1.5, 2.] {
        let mut request = TextRequest::new("SsS fi 中文");
        request.scale = scale;
        request.color = [255, 0, 0, 180];
        let bitmap = system.rasterize(&request).unwrap();
        let raster = RasterImage::from_rgba(
            image::RgbaImage::from_raw(bitmap.pixels[0], bitmap.pixels[1], bitmap.rgba.clone())
                .unwrap(),
        )
        .with_quality(ImageQuality::Low);
        let mut scene = Scene::new();
        scene.draw_image(raster.brush().into(), Affine::translate((3., 5.)));
        let output = plico_shell::renderer::render_to_image(
            &scene,
            bitmap.pixels[0] + 6,
            bitmap.pixels[1] + 10,
            Color::WHITE,
        )
        .unwrap();
        for y in 0..bitmap.pixels[1] {
            for x in 0..bitmap.pixels[0] {
                let i = (y as usize * bitmap.pixels[0] as usize + x as usize) * 4;
                let src = &bitmap.rgba[i..i + 4];
                let actual = output.get_pixel(x + 3, y + 5).0;
                for channel in 0..3 {
                    let expected = ((u32::from(src[channel]) * u32::from(src[3])
                        + 255 * (255 - u32::from(src[3]))
                        + 127)
                        / 255) as u8;
                    assert!(
                        actual[channel].abs_diff(expected) <= 1,
                        "{scale}x pixel {x},{y}: {actual:?}, expected {expected}"
                    );
                }
                assert_eq!(actual[3], 255);
            }
        }
    }
}
