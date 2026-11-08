use kurbo::{Affine, Rect};
use peniko::{Color, Fill};
use plico_renderer::{PaintScene, Scene, corner_shape::corner_shape, style::CornerRadii};
use plico_wgpu::Renderer;

#[test]
#[ignore = "requires a working wgpu adapter"]
fn inset_shadows_blur_the_hole_complement_and_respect_parent_clips_at_all_scales() {
    let mut renderer = pollster::block_on(Renderer::new()).unwrap();
    for scale in [1., 1.5, 2.] {
        for sigma in [0., 4.] {
            for spread in [-4., 0., 8., 40.] {
                let transform = Affine::scale(scale);
                let caster = Rect::new(20., 20., 80., 80.);
                let hole = Rect::new(20. + spread, 26. + spread, 80. - spread, 86. - spread);
                let mut scene = Scene::new();
                // Model a scroll port cutting through the panel at x=70.
                scene.push_clip_layer(transform, &Rect::new(0., 0., 70., 100.));
                scene.fill(Fill::NonZero, transform, Color::WHITE, None, &caster);
                scene.push_clip_layer(
                    transform,
                    &corner_shape(caster, CornerRadii::uniform(10.), 1.),
                );
                scene.draw_inset_box_shadow(
                    transform,
                    caster,
                    hole,
                    Color::BLACK,
                    (10_f64 - spread).max(0.),
                    sigma,
                );
                scene.pop_layer();
                scene.pop_layer();
                let size = (100. * scale) as u32;
                let pixels = renderer
                    .render_scene(&scene, [size, size], [0.2, 0.4, 0.8, 1.], true)
                    .unwrap();
                let pixel = |x: f64, y: f64| {
                    let i = (((y * scale) as u32 * size + (x * scale) as u32) * 4) as usize;
                    <[u8; 4]>::try_from(&pixels[i..i + 4]).unwrap()
                };
                assert_eq!(
                    pixel(75., 50.),
                    [51, 102, 204, 255],
                    "scroll clip, scale {scale}"
                );
                assert_eq!(pixel(10., 50.), [51, 102, 204, 255], "outside caster");
                assert_eq!(pixel(20., 20.), [255; 4], "rounded inset clip");
                if spread < 20. {
                    assert_eq!(pixel(50., 50.), [255; 4], "hole center, spread {spread}");
                    assert!(
                        pixel(50., 21.)[0] < 240,
                        "top inner edge, sigma {sigma} spread {spread}"
                    );
                    if sigma > 0. && spread == 0. {
                        let a = pixel(50., 25.)[0];
                        let b = pixel(50., 30.)[0];
                        assert!(a > 50 && a < b && b < 255, "Gaussian falloff {a} {b}");
                    }
                } else {
                    assert_eq!(
                        pixel(50., 50.),
                        [0, 0, 0, 255],
                        "collapsed hole fills caster"
                    );
                }
            }
        }
    }
}
