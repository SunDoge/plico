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

#[test]
#[ignore = "requires a working wgpu adapter"]
fn sibling_filter_workspaces_are_distinct_and_reused_across_frames() {
    let mut pooled = pollster::block_on(Renderer::new()).unwrap();
    let mut fresh = pollster::block_on(Renderer::new()).unwrap();
    for frame in 0..12 {
        let mut s = Scene::new();
        for sibling in 0..3 {
            s.append_scene(
                scene(
                    (frame + sibling) as f64 * 0.25,
                    if frame % 2 == 0 { 3. } else { 2.9 },
                ),
                Affine::translate((sibling as f64 * 64., 0.)),
            );
        }
        fresh.trim_layer_pool();
        let actual = pooled.render_scene(&s, [192, 64], [0.; 4], false).unwrap();
        assert_eq!(
            actual,
            fresh.render_scene(&s, [192, 64], [0.; 4], false).unwrap(),
            "frame {frame}"
        );
    }
    let stats = pooled.layer_pool_stats();
    assert_eq!(stats.filter_allocations, 3);
    assert_eq!(stats.filter_reuses, 33);
    assert_eq!(stats.entries, 3);
    assert_eq!(stats.retained_bytes, 3 * 72 * 72 * 12);
    assert_eq!(fresh.layer_pool_stats().filter_allocations, 36);
    pooled.trim_layer_pool();
    assert_eq!(pooled.layer_pool_stats().retained_bytes, 0);
    pooled
        .render_scene(&scene(0., 3.), [64, 64], [0.; 4], false)
        .unwrap();
    assert_eq!(pooled.layer_pool_stats().filter_allocations, 4);
    pooled
        .render_scene(&Scene::new(), [64, 64], [0.; 4], false)
        .unwrap();
    assert_eq!(pooled.layer_pool_stats().retained_bytes, 0);
}

#[test]
#[ignore = "requires a working wgpu adapter"]
fn viewport_cropping_happens_after_filter_sampling() {
    let mut renderer = pollster::block_on(Renderer::new()).unwrap();
    let sources = [
        Rect::new(-20., -20., 0., 52.),
        Rect::new(32., -20., 52., 52.),
        Rect::new(-20., -20., 52., 0.),
        Rect::new(-20., 32., 52., 52.),
    ];
    for (edge, source) in sources.iter().enumerate() {
        for nested in [false, true] {
            let mut scene = Scene::new();
            scene.push_layer(
                BlendMode::default(),
                1.,
                Affine::IDENTITY,
                &Rect::new(0., 0., 32., 32.),
                Some(Arc::new(
                    GaussianBlur {
                        sigma: 3.,
                        edge: BlurEdge::Transparent,
                    }
                    .into_filter(),
                )),
                None,
            );
            if nested {
                scene.push_layer(
                    BlendMode::default(),
                    0.75,
                    Affine::IDENTITY,
                    &Rect::new(-32., -32., 64., 64.),
                    None,
                    None,
                );
            }
            scene.fill(Fill::NonZero, Affine::IDENTITY, Color::WHITE, None, source);
            if nested {
                scene.pop_layer();
            }
            scene.pop_layer();
            let mut large = Scene::new();
            large.append_scene(scene.clone(), Affine::translate((32., 32.)));
            let expected = renderer
                .render_scene(&large, [96, 96], [0.; 4], false)
                .unwrap();
            let actual = renderer
                .render_scene(&scene, [32, 32], [0.; 4], false)
                .unwrap();
            assert!(actual.iter().any(|v| *v > 60), "empty filtered output");
            for y in 0..32 {
                assert_eq!(
                    &actual[y * 32 * 4..(y + 1) * 32 * 4],
                    &expected[((y + 32) * 96 + 32) * 4..((y + 32) * 96 + 64) * 4],
                    "edge={edge} nested={nested} row={y}: source pixels outside viewport were lost"
                );
            }
        }
    }
}
