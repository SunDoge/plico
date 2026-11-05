use peniko::{Blob, ImageAlphaType, ImageBrush, ImageData, ImageFormat};
use plico_renderer::{PaintScene, Scene};
use plico_wgpu::{Draw, Quad, Renderer, image::ImageQuad};
use std::sync::Arc;
fn image(rgba: [u8; 4]) -> ImageData {
    ImageData {
        data: Blob::new(Arc::new(rgba.to_vec())),
        format: ImageFormat::Rgba8,
        alpha_type: ImageAlphaType::AlphaPremultiplied,
        width: 1,
        height: 1,
    }
}
#[test]
#[ignore = "requires a working wgpu adapter"]
fn adjacent_identical_image_bindings_use_one_ordered_draw_call() {
    let brush = ImageBrush::new(image([255, 0, 0, 255]));
    let mut scene = Scene::new();
    for x in 0..100 {
        scene.draw_image((&brush).into(), kurbo::Affine::translate((x as f64, 0.)));
    }
    let mut renderer = pollster::block_on(Renderer::new()).unwrap();
    let pixels = renderer
        .render_scene(&scene, [100, 1], [0.; 4], false)
        .unwrap();
    assert!(
        pixels
            .as_chunks::<4>()
            .0
            .iter()
            .all(|p| *p == [255, 0, 0, 255])
    );
    let stats = renderer.batch_stats();
    assert_eq!(stats.instances, 100);
    assert_eq!(stats.draw_calls, 1);
    assert_eq!(stats.passes, 1);
    assert_eq!(renderer.image_cache_stats().2, 1);
    // Registry reuse must not leave stale slots on another frame.
    assert_eq!(
        pixels,
        renderer
            .render_scene(&scene, [100, 1], [0.; 4], false)
            .unwrap()
    );
    assert_eq!(renderer.batch_stats().draw_calls, 1);
}
#[test]
#[ignore = "requires a working wgpu adapter"]
fn batching_never_sorts_transparent_images_or_collapses_instance_blending() {
    let red = ImageQuad::new(
        Quad::new([0., 0., 8., 8.], [1.; 4]),
        image([128, 0, 0, 128]),
    );
    let blue = ImageQuad::new(
        Quad::new([0., 0., 8., 8.], [1.; 4]),
        image([0, 0, 128, 128]),
    );
    let mut renderer = pollster::block_on(Renderer::new()).unwrap();
    let mixed = renderer
        .render_items(
            &[Draw::Image(&red), Draw::Image(&blue), Draw::Image(&red)],
            [8, 8],
            [0.; 4],
            0.,
        )
        .unwrap();
    assert_eq!(
        renderer.batch_stats().draw_calls,
        3,
        "nonadjacent images were reordered"
    );
    for pixel in mixed.as_chunks::<4>().0 {
        for (a, b) in pixel.iter().zip([160, 0, 64, 224]) {
            assert!(a.abs_diff(b) <= 1);
        }
    }
    let repeated = renderer
        .render_items(&[Draw::Image(&red); 3], [8, 8], [0.; 4], 0.)
        .unwrap();
    assert_eq!(renderer.batch_stats().draw_calls, 1);
    for pixel in repeated.as_chunks::<4>().0 {
        for (a, b) in pixel.iter().zip([224, 0, 0, 224]) {
            assert!(a.abs_diff(b) <= 1);
        }
    }
}
#[test]
#[ignore = "requires a working wgpu adapter"]
fn layer_local_slot_maps_cannot_alias_parent_bindings() {
    use kurbo::{Affine, Rect};
    let red = ImageBrush::new(image([255, 0, 0, 255]));
    let blue = ImageBrush::new(image([0, 0, 255, 255]));
    let mut scene = Scene::new();
    scene.draw_image((&red).into(), Affine::scale(8.));
    scene.push_layer(
        peniko::BlendMode::default(),
        0.5,
        Affine::IDENTITY,
        &Rect::new(8., 0., 16., 8.),
        None,
        None,
    );
    for _ in 0..2 {
        scene.draw_image(
            (&blue).into(),
            Affine::translate((8., 0.)) * Affine::scale(8.),
        );
    }
    scene.pop_layer();
    scene.draw_image(
        (&blue).into(),
        Affine::translate((16., 0.)) * Affine::scale(8.),
    );
    scene.draw_image(
        (&red).into(),
        Affine::translate((24., 0.)) * Affine::scale(8.),
    );
    let mut renderer = pollster::block_on(Renderer::new()).unwrap();
    for _ in 0..3 {
        let pixels = renderer
            .render_scene(&scene, [32, 8], [0.; 4], false)
            .unwrap();
        for y in 0..8 {
            for x in 0..32 {
                let expected = if (8..16).contains(&x) {
                    [0, 0, 128, 128]
                } else if (16..24).contains(&x) {
                    [0, 0, 255, 255]
                } else {
                    [255, 0, 0, 255]
                };
                let p = &pixels[(y * 32 + x) * 4..(y * 32 + x) * 4 + 4];
                for (a, b) in p.iter().zip(expected) {
                    assert!(a.abs_diff(b) <= 1);
                }
            }
        }
        let stats = renderer.batch_stats();
        assert_eq!(stats.passes, 2);
        assert_eq!(stats.instances, 6);
        assert_eq!(stats.draw_calls, 5);
    }
    renderer.trim_layer_pool();
    renderer
        .render_scene(&scene, [32, 8], [0.; 4], false)
        .unwrap();
    assert_eq!(renderer.batch_stats().draw_calls, 5);
}

#[test]
#[ignore = "requires a working wgpu adapter"]
fn shared_mask_texture_can_vary_rect_but_mask_binding_changes_split_batches() {
    use plico_wgpu::mask::CoverageMask;
    let half = image([128, 128, 128, 128]);
    let left = CoverageMask {
        image: half.clone(),
        rect: [0., 0., 1., 1.],
    };
    let middle = CoverageMask {
        image: half,
        rect: [1., 0., 1., 1.],
    };
    let opaque = CoverageMask {
        image: image([255; 4]),
        rect: [2., 0., 1., 1.],
    };
    let source = image([255, 0, 0, 255]);
    let red = ImageQuad::new(Quad::new([0., 0., 4., 1.], [1.; 4]), source.clone());
    let last = ImageQuad::new(Quad::new([3., 0., 1., 1.], [1.; 4]), source);
    let mut renderer = pollster::block_on(Renderer::new()).unwrap();
    let pixels = renderer
        .render_items(
            &[
                Draw::SetMask(&left),
                Draw::Image(&red),
                Draw::SetMask(&middle),
                Draw::Image(&red),
                Draw::SetMask(&opaque),
                Draw::Image(&red),
                Draw::ResetMask,
                Draw::Image(&last),
            ],
            [4, 1],
            [0.; 4],
            0.,
        )
        .unwrap();
    assert_eq!(
        pixels,
        [
            128, 0, 0, 128, 128, 0, 0, 128, 255, 0, 0, 255, 255, 0, 0, 255
        ]
    );
    assert_eq!(renderer.batch_stats().draw_calls, 3);
    assert_eq!(renderer.batch_stats().instances, 4);
}

#[test]
#[ignore = "requires a working wgpu adapter"]
fn separate_damage_regions_do_not_replay_unrelated_image_batches() {
    let mut partial = pollster::block_on(Renderer::new()).unwrap();
    let mut full = pollster::block_on(Renderer::new()).unwrap();
    for scale in [1., 1.25, 2.] {
        let size = [(160. * scale) as u32, (48. * scale) as u32];
        partial.invalidate_scene_damage();
        for frame in 0..5u8 {
            let mut scene = Scene::new();
            for (i, x) in [4., 64., 124.].into_iter().enumerate() {
                let brush = ImageBrush::new(image([40 + frame * 20, 60 + i as u8 * 30, 180, 200]));
                scene.draw_image(
                    (&brush).into(),
                    kurbo::Affine::scale(scale)
                        * kurbo::Affine::translate((x, 8.))
                        * kurbo::Affine::scale(16.),
                );
            }
            let actual = partial.render_scene(&scene, size, [1.; 4], true).unwrap();
            let expected = full.render_scene(&scene, size, [1.; 4], false).unwrap();
            assert!(
                actual == expected,
                "batch replay mismatch: frame={frame}, scale={scale}"
            );
            if frame > 0 {
                assert!(partial.scene_damage_coverage(size) < 0.5);
                assert_eq!(partial.batch_stats().clears, 3);
                assert_eq!(
                    partial.batch_stats().draw_calls,
                    3,
                    "each image only meets its own damage rectangle"
                );
            }
        }
    }
}
