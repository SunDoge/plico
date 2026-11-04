#![cfg(feature = "paint-scene")]
use kurbo::{Affine, Rect, Shape};
use peniko::{Color, Fill, Gradient, InterpolationAlphaSpace};
use plico_renderer::{PaintScene, Scene};
use plico_wgpu::Renderer;
#[test]
#[ignore = "requires a working wgpu adapter"]
fn path_gradient_pixels_and_fractional_damage_match_full_render() {
    let mut partial = pollster::block_on(Renderer::new()).unwrap();
    let mut full = pollster::block_on(Renderer::new()).unwrap();
    for frame in 0..6 {
        let gradient = Gradient::new_linear((0., 0.), (32., 0.))
            .with_stops([
                Color::from_rgba8(255, 0, 0, 0),
                Color::from_rgba8(0, 0, 255, 255),
            ])
            .with_interpolation_alpha_space(if frame % 2 == 0 {
                InterpolationAlphaSpace::Premultiplied
            } else {
                InterpolationAlphaSpace::Unpremultiplied
            });
        let mut scene = Scene::new();
        if frame < 5 {
            scene.fill(
                Fill::NonZero,
                Affine::translate((frame as f64 * 0.25, 0.)),
                &gradient,
                None,
                &Rect::new(0., 0., 32., 24.).to_path(0.1),
            );
        }
        let actual = partial
            .render_scene(&scene, [64, 32], [0.; 4], true)
            .unwrap();
        let expected = full.render_scene(&scene, [64, 32], [0.; 4], false).unwrap();
        assert_eq!(actual, expected, "frame {frame}");
        if frame == 0 {
            let pixel = &actual[(12 * 64 + 16) * 4..(12 * 64 + 16) * 4 + 4];
            assert_eq!(pixel[0], 0);
            assert!(pixel[2].abs_diff(131) <= 1);
            assert_eq!(pixel[2], pixel[3]);
        }
    }
}
