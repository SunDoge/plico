#![cfg(feature = "paint-scene")]
use kurbo::{Affine, Rect};
use peniko::{BlendMode, Color, Fill};
use plico_renderer::{
    PaintScene, Scene,
    filter::{BlurEdge, GaussianBlur},
};
use plico_wgpu::Renderer;
use std::sync::Arc;
fn scene(offset: f64, sigma: f32) -> Scene {
    let mut s = Scene::new();
    s.push_layer(
        BlendMode::default(),
        0.5,
        Affine::IDENTITY,
        &Rect::new(8., 8., 56., 56.),
        Some(Arc::new(
            GaussianBlur {
                sigma,
                edge: BlurEdge::Transparent,
            }
            .into_filter(),
        )),
        None,
    );
    s.fill(
        Fill::NonZero,
        Affine::translate((offset, 0.)),
        Color::from_rgb8(255, 0, 0),
        None,
        &Rect::new(16., 16., 48., 48.),
    );
    s.pop_layer();
    s
}
#[test]
#[ignore = "requires a working wgpu adapter"]
fn gaussian_layer_preserves_premultiplication_alpha_and_damage() {
    let mut partial = pollster::block_on(Renderer::new()).unwrap();
    let mut full = pollster::block_on(Renderer::new()).unwrap();
    for i in 0..5 {
        let s = if i == 4 {
            Scene::new()
        } else {
            scene(i as f64 * 0.5, 3.)
        };
        let actual = partial.render_scene(&s, [64, 64], [0.; 4], true).unwrap();
        assert_eq!(
            actual,
            full.render_scene(&s, [64, 64], [0.; 4], false).unwrap()
        );
        if i == 0 {
            for pixel in actual.as_chunks::<4>().0 {
                assert_eq!(pixel[0], pixel[3]);
                assert_eq!(pixel[1], 0);
                assert_eq!(pixel[2], 0);
            }
            let alpha = |x: usize, y: usize| actual[(y * 64 + x) * 4 + 3];
            assert!(alpha(32, 32).abs_diff(128) <= 1);
            assert!((65..=79).contains(&alpha(16, 32)));
            assert_eq!(alpha(4, 32), 0, "filter escaped final layer clip");
            assert!(alpha(12, 32) > 0, "Gaussian tail missing");
        }
    }
}
#[test]
#[ignore = "requires a working wgpu adapter"]
fn zero_sigma_is_identity_and_large_sigma_is_rejected_before_submission() {
    let mut r = pollster::block_on(Renderer::new()).unwrap();
    let pixels = r
        .render_scene(&scene(0., 0.), [64, 64], [0.; 4], false)
        .unwrap();
    assert_eq!(pixels[(32 * 64 + 32) * 4 + 3], 128);
    assert!(matches!(
        r.render_scene(&scene(0., 65.), [64, 64], [0.; 4], false),
        Err(plico_wgpu::Error::Unsupported { .. })
    ));
}

#[test]
#[ignore = "requires a working wgpu adapter"]
fn filter_reads_outside_final_clip_but_composites_only_inside() {
    let mut s = Scene::new();
    s.push_layer(
        BlendMode::default(),
        1.,
        Affine::IDENTITY,
        &Rect::new(16., 8., 48., 56.),
        Some(Arc::new(
            GaussianBlur {
                sigma: 3.,
                edge: BlurEdge::Transparent,
            }
            .into_filter(),
        )),
        None,
    );
    s.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        Color::WHITE,
        None,
        &Rect::new(8., 8., 16., 56.),
    );
    s.pop_layer();
    let mut r = pollster::block_on(Renderer::new()).unwrap();
    let pixels = r.render_scene(&s, [64, 64], [0.; 4], false).unwrap();
    let alpha = |x: usize, y: usize| pixels[(y * 64 + x) * 4 + 3];
    assert!(
        alpha(16, 32) > 80,
        "source was prematurely clipped before filtering"
    );
    assert_eq!(alpha(15, 32), 0, "filtered output escaped final clip");
    assert_eq!(alpha(32, 32), 0);
}
