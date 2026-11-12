use kurbo::{Affine, Rect};
use peniko::Color;
use plico_renderer::{PaintScene, Scene, ShadowProfile};
use plico_wgpu::Renderer;

fn shadow(x: f64, radius: f64, sigma: f64) -> Scene {
    let mut scene = Scene::new();
    // These tests use an independent circular-arc convolution oracle. The
    // public convenience method defaults to continuous corners; request the
    // circular reference profile explicitly here.
    scene.draw_box_shadow_profile(
        Affine::translate((x, 0.)),
        Rect::new(32.5, 32.5, 96.5, 96.5),
        Color::from_rgba8(0, 0, 0, 255),
        ShadowProfile {
            radius,
            std_dev: sigma,
            corner_smoothing: 0.,
        },
    );
    scene
}
#[test]
#[ignore = "requires a working wgpu adapter"]
fn gaussian_edges_round_corners_and_zero_sigma_are_well_defined() {
    let mut renderer = pollster::block_on(Renderer::new()).unwrap();
    let pixels = renderer
        .render_scene(&shadow(0., 0., 4.), [128, 128], [0.; 4], false)
        .unwrap();
    let alpha = |x: usize, y: usize| pixels[(y * 128 + x) * 4 + 3];
    assert!(alpha(32, 64).abs_diff(128) <= 1, "straight edge");
    assert!(alpha(32, 32).abs_diff(64) <= 1, "square corner");
    assert_eq!(alpha(64, 64), 255);
    assert_eq!(alpha(8, 64), 0);
    let rounded = renderer
        .render_scene(&shadow(0., 16., 4.), [128, 128], [0.; 4], false)
        .unwrap();
    for y in 16..112 {
        for x in 16..112 {
            let a = rounded[(y * 128 + x) * 4 + 3];
            let b = rounded[(x * 128 + y) * 4 + 3];
            assert!(a.abs_diff(b) <= 2, "asymmetric corner at {x},{y}: {a}/{b}");
        }
    }
    assert!(rounded[(32 * 128 + 32) * 4 + 3] < alpha(32, 32));
    let sharp = renderer
        .render_scene(&shadow(0., 8., 0.), [128, 128], [0.; 4], false)
        .unwrap();
    assert_eq!(sharp[(64 * 128 + 64) * 4 + 3], 255);
    assert_eq!(sharp[(20 * 128 + 20) * 4 + 3], 0);
}
#[test]
#[ignore = "requires a working wgpu adapter"]
fn moving_clipped_shadow_damage_matches_full_redraw_and_removal() {
    let mut partial = pollster::block_on(Renderer::new()).unwrap();
    let mut full = pollster::block_on(Renderer::new()).unwrap();
    for frame in 0..8 {
        let mut scene = Scene::new();
        if frame < 7 {
            scene.push_clip_layer(Affine::IDENTITY, &Rect::new(20., 16., 116., 110.));
            scene.append_scene(shadow(frame as f64 * 0.25, 12., 3.5), Affine::IDENTITY);
            scene.pop_layer();
        }
        let actual = partial
            .render_scene(&scene, [160, 144], [1.; 4], true)
            .unwrap();
        let expected = full
            .render_scene(&scene, [160, 144], [1.; 4], false)
            .unwrap();
        assert_eq!(actual, expected, "frame {frame}");
    }
}

fn cdf(x: f64) -> f64 {
    let t = 1. / (1. + 0.2316419 * x.abs());
    let tail = 0.3989422804014327
        * (-0.5 * x * x).exp()
        * t
        * (0.319381530
            + t * (-0.356563782 + t * (1.781477937 + t * (-1.821255978 + t * 1.330274429))));
    if x >= 0. { 1. - tail } else { tail }
}
// Independent, high-resolution f64 convolution oracle: no four-sigma truncation
// and 8192 vertical samples instead of the shader's bounded 32 samples.
fn reference(x: f64, y: f64, r: f64, sigma: f64) -> u8 {
    let step = 64. / 8192.;
    let mut sum = 0.;
    for i in 0..8192 {
        let yy = (i as f64 + 0.5) * step;
        let dy = ((yy - 32.).abs() - (32. - r)).max(0.);
        let inset = r - (r * r - dy * dy).max(0.).sqrt();
        let horizontal = cdf((64. - inset - x) / sigma) - cdf((inset - x) / sigma);
        sum += horizontal * (-0.5 * ((yy - y) / sigma).powi(2)).exp();
    }
    (sum * step * 0.3989422804014327 / sigma * 255.).round() as u8
}
#[test]
#[ignore = "requires a working wgpu adapter"]
fn rounded_shadow_matches_high_resolution_gaussian_convolution() {
    let mut renderer = pollster::block_on(Renderer::new()).unwrap();
    for sigma in [0.3, 1., 4., 16.] {
        for r in [2., 16., 32.] {
            let pixels = renderer
                .render_scene(&shadow(0., r, sigma), [128, 128], [0.; 4], false)
                .unwrap();
            for (x, y) in [
                (31, 32),
                (32, 32),
                (35, 35),
                (40, 40),
                (48, 32),
                (64, 32),
                (30, 64),
                (64, 64),
            ] {
                let expected = reference(x as f64 - 32., y as f64 - 32., r, sigma);
                let actual = pixels[(y * 128 + x) * 4 + 3];
                assert!(
                    actual.abs_diff(expected) <= 2,
                    "sigma={sigma} radius={r} at {x},{y}: {actual} != {expected}"
                );
            }
        }
    }
}
