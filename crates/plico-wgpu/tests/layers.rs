#![cfg(all(feature = "paint-scene", feature = "system-text", target_os = "linux"))]
use kurbo::{Affine, Circle, Rect};
use peniko::{Color as PaintColor, Fill};
use plico_renderer::{
    PaintCommand, PaintScene, Scene, TextContext, paragraph::ParagraphRequest, style::Paint,
};
use plico_wgpu::{Error, Renderer};
use std::sync::Arc;

#[test]
#[ignore = "requires a working wgpu adapter"]
fn isolated_opacity_is_applied_once_after_child_overlap_and_nested_groups() {
    use peniko::BlendMode;
    let mut renderer = pollster::block_on(Renderer::new()).unwrap();
    let mut scene = Scene::new();
    scene.push_layer(
        BlendMode::default(),
        0.5,
        Affine::IDENTITY,
        &Rect::new(0., 0., 64., 32.),
        None,
        None,
    );
    scene.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        PaintColor::from_rgba8(255, 0, 0, 255),
        None,
        &Rect::new(0., 0., 24., 32.),
    );
    scene.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        PaintColor::from_rgba8(0, 0, 255, 255),
        None,
        &Rect::new(16., 0., 40., 32.),
    );
    scene.push_layer(
        BlendMode::default(),
        0.5,
        Affine::IDENTITY,
        &Rect::new(40., 0., 64., 32.),
        None,
        None,
    );
    scene.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        PaintColor::from_rgba8(0, 255, 0, 255),
        None,
        &Rect::new(40., 0., 64., 32.),
    );
    scene.pop_layer();
    scene.pop_layer();
    let pixels = renderer
        .render_scene(&scene, [64, 32], [0.; 4], true)
        .unwrap();
    let pixel = |x: usize| &pixels[(16 * 64 + x) * 4..(16 * 64 + x) * 4 + 4];
    assert_eq!(pixel(4), [128, 0, 0, 128]);
    assert_eq!(
        pixel(20),
        [0, 0, 128, 128],
        "overlap was attenuated per child instead of per group"
    );
    assert_eq!(renderer.layer_frame_bytes(), (64 * 32 + 24 * 32) * 4);
    for (a, b) in pixel(50).iter().zip([0_u8, 64, 0, 64]) {
        assert!(a.abs_diff(b) <= 1);
    }
}

#[test]
#[ignore = "requires a working wgpu adapter and system fonts"]
fn isolated_groups_preserve_native_text_masks_images_and_damage() {
    use peniko::{BlendMode, Blob, ImageAlphaType, ImageBrush, ImageData, ImageFormat};
    let mut renderer = pollster::block_on(Renderer::new()).unwrap();
    let mut full = pollster::block_on(Renderer::new()).unwrap();
    renderer
        .configure_text(&mut TextContext::new(), 1.)
        .unwrap();
    full.configure_text(&mut TextContext::new(), 1.).unwrap();
    let image = ImageBrush::new(ImageData {
        data: Blob::new(Arc::new(vec![200, 100, 0, 255])),
        format: ImageFormat::Rgba8,
        alpha_type: ImageAlphaType::Alpha,
        width: 1,
        height: 1,
    });
    for frame in 0..5 {
        let size = if frame < 4 { [128, 96] } else { [144, 104] };
        let mut scene = Scene::new();
        scene.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            PaintColor::from_rgba8(220, 230, 240, 255),
            None,
            &Rect::new(0., 0., 144., 104.),
        );
        scene.push_clip_layer(Affine::IDENTITY, &Circle::new((60., 48.), 44.));
        scene.push_layer(
            BlendMode::default(),
            0.2 + frame as f32 * 0.15,
            Affine::IDENTITY,
            &Circle::new((52. + frame as f64, 44.), 38.),
            None,
            None,
        );
        scene.draw_image(
            (&image).into(),
            Affine::translate((20., 20.)) * Affine::scale(30.),
        );
        let request = ParagraphRequest::from_paint(
            &Paint {
                text: Some(format!("文字 {frame}").into()),
                font_size: 20.,
                ..Default::default()
            },
            None,
        )
        .unwrap();
        scene.commands.push(PaintCommand::Paragraph {
            request: Arc::new(request),
            transform: Affine::translate((24., 30.)),
        });
        scene.pop_layer();
        scene.draw_image(
            (&image).into(),
            Affine::translate((80., 32.)) * Affine::scale(12.),
        );
        scene.pop_layer();
        scene.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            PaintColor::from_rgba8(0, 200, 100, 255),
            None,
            &Rect::new(0., 0., 8., 8.),
        );
        let actual = renderer.render_scene(&scene, size, [0.; 4], true).unwrap();
        assert_eq!(
            actual,
            full.render_scene(&scene, size, [0.; 4], false).unwrap(),
            "layer damage mismatch frame={frame}"
        );
        assert_eq!(
            renderer.render_scene(&scene, size, [0.; 4], true).unwrap(),
            actual
        );
        assert_eq!(
            &actual[(4 * size[0] as usize + 4) * 4..(4 * size[0] as usize + 4) * 4 + 4],
            [0, 200, 100, 255]
        );
    }
}

#[test]
#[ignore = "requires a working wgpu adapter"]
fn layer_depth_limit_rejects_unbounded_recursion_and_recovers() {
    use peniko::BlendMode;
    let make = |depth| {
        let mut scene = Scene::new();
        for _ in 0..depth {
            scene.push_layer(
                BlendMode::default(),
                1.,
                Affine::IDENTITY,
                &Rect::new(0., 0., 1., 1.),
                None,
                None,
            );
        }
        scene.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            PaintColor::from_rgba8(20, 40, 60, 255),
            None,
            &Rect::new(0., 0., 1., 1.),
        );
        for _ in 0..depth {
            scene.pop_layer();
        }
        scene
    };
    let mut renderer = pollster::block_on(Renderer::new()).unwrap();
    assert!(matches!(
        renderer.prepare_scene(&make(17), [1, 1]),
        Err(Error::Unsupported { .. })
    ));
    let actual = renderer
        .render_scene(&make(16), [1, 1], [0.; 4], true)
        .unwrap();
    assert_eq!(actual, [20, 40, 60, 255]);
    let mut unclosed = make(1);
    unclosed.commands.pop();
    assert!(renderer.prepare_scene(&unclosed, [1, 1]).is_err());
    assert_eq!(
        renderer
            .render_scene(&make(0), [1, 1], [0.; 4], true)
            .unwrap(),
        actual
    );
}

#[test]
#[ignore = "requires a working wgpu adapter and system fonts"]
fn cropped_layer_preserves_screen_space_text_phase_and_allocates_only_its_bounds() {
    use peniko::BlendMode;
    let mut renderer = pollster::block_on(Renderer::new()).unwrap();
    renderer
        .configure_text(&mut TextContext::new(), 1.25)
        .unwrap();
    let clip = Rect::new(20.25, 10.25, 100.25, 65.25);
    let request = Arc::new(
        ParagraphRequest::from_paint(
            &Paint {
                text: Some("W W W W W".into()),
                font_size: 20.,
                ..Default::default()
            },
            None,
        )
        .unwrap(),
    );
    let mut direct = Scene::new();
    direct.push_clip_layer(Affine::IDENTITY, &clip);
    direct.commands.push(PaintCommand::Paragraph {
        request: request.clone(),
        transform: Affine::translate((24.25, 22.5)) * Affine::scale(1.25),
    });
    direct.pop_layer();
    let expected = renderer
        .render_scene(&direct, [128, 96], [1.; 4], false)
        .unwrap();
    let mut group = Scene::new();
    group.push_layer(
        BlendMode::default(),
        1.,
        Affine::IDENTITY,
        &clip,
        None,
        None,
    );
    group.commands.push(PaintCommand::Paragraph {
        request,
        transform: Affine::translate((24.25, 22.5)) * Affine::scale(1.25),
    });
    group.pop_layer();
    let actual = renderer
        .render_scene(&group, [128, 96], [1.; 4], false)
        .unwrap();
    assert!(
        actual.iter().zip(expected).all(|(a, b)| a.abs_diff(b) <= 1),
        "cropped layer changed native text phase"
    );
    assert_eq!(renderer.layer_frame_bytes(), 81 * 56 * 4);
}

#[test]
#[ignore = "requires a working wgpu adapter"]
fn layer_pool_reuses_distinct_live_targets_and_releases_removed_groups() {
    use peniko::BlendMode;
    let scene = |frame: usize| {
        let mut scene = Scene::new();
        scene.push_layer(
            BlendMode::default(),
            0.4 + frame as f32 * 0.01,
            Affine::IDENTITY,
            &Rect::new(8., 8., 40., 40.),
            None,
            None,
        );
        scene.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            PaintColor::from_rgba8(200, 30, 50, 255),
            None,
            &Rect::new(8., 8., 16. + (frame % 8) as f64, 40.),
        );
        scene.push_layer(
            BlendMode::default(),
            0.6,
            Affine::IDENTITY,
            &Rect::new(12., 12., 24., 24.),
            None,
            None,
        );
        scene.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            PaintColor::from_rgba8(0, 100, 240, 255),
            None,
            &Rect::new(12., 12., 24., 24.),
        );
        scene.pop_layer();
        scene.pop_layer();
        let right = if frame < 12 { 80. } else { 88. };
        scene.push_layer(
            BlendMode::default(),
            0.7 - frame as f32 * 0.01,
            Affine::IDENTITY,
            &Rect::new(48., 8., right, 40.),
            None,
            None,
        );
        scene.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            PaintColor::from_rgba8(30, 200, 80, 255),
            None,
            &Rect::new(48., 8., right, 20. + (frame % 8) as f64),
        );
        scene.pop_layer();
        scene
    };
    let mut pooled = pollster::block_on(Renderer::new()).unwrap();
    let mut fresh = pollster::block_on(Renderer::new()).unwrap();
    let mut last = Vec::new();
    for frame in 0..24 {
        let scene = scene(frame);
        let size = if frame < 8 { [96, 64] } else { [112, 72] };
        fresh.trim_layer_pool();
        let expected = fresh.render_scene(&scene, size, [1.; 4], false).unwrap();
        last = pooled.render_scene(&scene, size, [1.; 4], true).unwrap();
        assert_eq!(
            last, expected,
            "pool reused live sibling or uncleared pixels at frame {frame}"
        );
        let stats = pooled.layer_pool_stats();
        assert!(stats.retained_bytes <= 128 * 1024 * 1024);
        assert!(stats.entries <= 128);
        assert_eq!(stats.allocations, if frame < 12 { 3 } else { 4 });
        // An unchanged readback must not throw away the warmed targets.
        assert_eq!(
            pooled.render_scene(&scene, size, [1.; 4], true).unwrap(),
            last
        );
        assert_eq!(pooled.layer_pool_stats().allocations, stats.allocations);
    }
    assert_eq!(fresh.layer_pool_stats().allocations, 72);
    assert_eq!(pooled.layer_pool_stats().allocations, 4);
    assert_eq!(pooled.layer_pool_stats().reuses, 68);
    pooled.trim_layer_pool();
    assert_eq!(pooled.layer_pool_stats().entries, 0);
    // Final retained pixels remain valid after trimming only intermediates.
    assert_eq!(
        pooled
            .render_scene(&scene(23), [112, 72], [1.; 4], true)
            .unwrap(),
        last
    );
    assert_eq!(pooled.layer_pool_stats().allocations, 4);
    pooled
        .render_scene(&scene(22), [112, 72], [1.; 4], true)
        .unwrap();
    assert_eq!(pooled.layer_pool_stats().allocations, 7);
    pooled
        .render_scene(&Scene::new(), [112, 72], [1.; 4], true)
        .unwrap();
    assert_eq!(pooled.layer_pool_stats().entries, 0);
}

#[test]
#[ignore = "requires a working wgpu adapter"]
fn damage_skips_unaffected_layer_rendering_before_texture_work() {
    use peniko::BlendMode;
    let make = |alpha| {
        let mut scene = Scene::new();
        for (x, a, color) in [
            (4., 0.5, PaintColor::from_rgba8(200, 0, 0, 255)),
            (64., alpha, PaintColor::from_rgba8(0, 0, 200, 255)),
        ] {
            scene.push_layer(
                BlendMode::default(),
                a,
                Affine::IDENTITY,
                &Rect::new(x, 4., x + 24., 28.),
                None,
                None,
            );
            scene.fill(
                Fill::NonZero,
                Affine::IDENTITY,
                color,
                None,
                &Rect::new(x, 4., x + 24., 28.),
            );
            scene.pop_layer();
        }
        scene
    };
    let mut renderer = pollster::block_on(Renderer::new()).unwrap();
    renderer
        .render_scene(&make(0.4), [96, 40], [1.; 4], true)
        .unwrap();
    let before = renderer.layer_pool_stats();
    let actual = renderer
        .render_scene(&make(0.7), [96, 40], [1.; 4], true)
        .unwrap();
    let after = renderer.layer_pool_stats();
    assert_eq!(after.allocations, before.allocations);
    assert_eq!(
        after.reuses - before.reuses,
        1,
        "unchanged sibling layer was rendered"
    );
    assert_eq!(renderer.layer_frame_bytes(), 24 * 24 * 4);
    assert_eq!(
        actual,
        pollster::block_on(Renderer::new())
            .unwrap()
            .render_scene(&make(0.7), [96, 40], [1.; 4], false)
            .unwrap()
    );
}

#[test]
#[ignore = "requires a working wgpu adapter"]
fn pool_eviction_keeps_current_frame_layers_distinct_and_alive() {
    use peniko::BlendMode;
    let mut scene = Scene::new();
    for x in 0..140_u32 {
        let rect = Rect::new(x as f64, 0., x as f64 + 1., 1.);
        scene.push_layer(
            BlendMode::default(),
            0.5,
            Affine::IDENTITY,
            &rect,
            None,
            None,
        );
        scene.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            PaintColor::from_rgba8(((x % 120) * 2) as u8, 80, 200, 255),
            None,
            &rect,
        );
        scene.pop_layer();
    }
    let mut renderer = pollster::block_on(Renderer::new()).unwrap();
    let pixels = renderer
        .render_scene(&scene, [160, 2], [0.; 4], true)
        .unwrap();
    for x in 0..140_usize {
        assert_eq!(&pixels[x * 4..x * 4 + 4], [(x % 120) as u8, 40, 100, 128]);
    }
    let stats = renderer.layer_pool_stats();
    assert_eq!(stats.allocations, 140);
    assert_eq!(stats.entries, 128);
    assert_eq!(stats.retained_bytes, 128 * 4);
}

#[test]
#[ignore = "requires a working wgpu adapter"]
fn additive_groups_isolate_children_restore_source_over_and_match_damage() {
    let mut partial = pollster::block_on(Renderer::new()).unwrap();
    let mut full = pollster::block_on(Renderer::new()).unwrap();
    for compose in [
        peniko::Compose::SrcOver,
        peniko::Compose::Plus,
        peniko::Compose::PlusLighter,
    ] {
        for frame in 0..4 {
            let mut scene = Scene::new();
            scene.fill(
                Fill::NonZero,
                Affine::IDENTITY,
                PaintColor::from_rgba8(255, 0, 0, 128),
                None,
                &Rect::new(0., 0., 64., 32.),
            );
            if frame < 3 {
                scene.push_layer(
                    compose,
                    0.5,
                    Affine::translate((frame as f64 * 0.25, 0.)),
                    &Rect::new(0., 0., 48., 32.),
                    None,
                    None,
                );
                scene.fill(
                    Fill::NonZero,
                    Affine::IDENTITY,
                    PaintColor::from_rgb8(0, 0, 255),
                    None,
                    &Rect::new(0., 0., 20., 32.),
                );
                scene.push_layer(
                    peniko::BlendMode::default(),
                    1.,
                    Affine::IDENTITY,
                    &Rect::new(12., 0., 32., 32.),
                    None,
                    None,
                );
                scene.fill(
                    Fill::NonZero,
                    Affine::IDENTITY,
                    PaintColor::from_rgb8(0, 0, 255),
                    None,
                    &Rect::new(12., 0., 32., 32.),
                );
                scene.pop_layer();
                scene.pop_layer();
            }
            scene.fill(
                Fill::NonZero,
                Affine::IDENTITY,
                PaintColor::from_rgba8(0, 255, 0, 128),
                None,
                &Rect::new(24., 0., 40., 32.),
            );
            if frame == 1 {
                // Replacing material must also invalidate the lazy blend variant.
                pollster::block_on(partial.set_material(&format!(
                    "fn plico_material(m: Material)->vec4f {{return m.color;}} // {compose:?}"
                )))
                .unwrap();
            }
            let compilations = partial.pipeline_compilations();
            let actual = partial
                .render_scene(&scene, [64, 32], [0.; 4], true)
                .unwrap();
            let expected = full.render_scene(&scene, [64, 32], [0.; 4], false).unwrap();
            assert_eq!(actual, expected, "{compose:?} frame {frame}");
            if frame == 1 && compose != peniko::Compose::SrcOver {
                assert_eq!(
                    partial.pipeline_compilations(),
                    compilations + 1,
                    "material replacement did not recreate additive variant"
                );
            }
            if frame == 0 {
                let additive = compose != peniko::Compose::SrcOver;
                for (x, expected) in [
                    (
                        8,
                        if additive {
                            [128, 0, 128, 255]
                        } else {
                            [64, 0, 128, 192]
                        },
                    ),
                    (
                        16,
                        if additive {
                            [128, 0, 128, 255]
                        } else {
                            [64, 0, 128, 192]
                        },
                    ),
                    (
                        28,
                        if additive {
                            [64, 128, 64, 255]
                        } else {
                            [32, 128, 64, 224]
                        },
                    ),
                    (36, [64, 128, 0, 192]),
                    (54, [128, 0, 0, 128]),
                ] {
                    let pixel = &actual[(16 * 64 + x) * 4..(16 * 64 + x) * 4 + 4];
                    assert!(
                        pixel.iter().zip(expected).all(|(a, b)| a.abs_diff(b) <= 1),
                        "{compose:?} at {x}: {pixel:?} != {expected:?}"
                    );
                }
            }
        }
    }
    assert!(partial.pipeline_compilations() >= 3);
    for compose in [peniko::Compose::Plus, peniko::Compose::PlusLighter] {
        let mut scene = Scene::new();
        scene.push_layer(
            compose,
            0.75,
            Affine::IDENTITY,
            &Rect::new(0., 0., 4., 4.),
            None,
            None,
        );
        scene.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            PaintColor::WHITE,
            None,
            &Rect::new(0., 0., 4., 4.),
        );
        scene.pop_layer();
        let pixels = partial
            .render_scene(&scene, [4, 4], [0.75; 4], true)
            .unwrap();
        assert!(
            pixels.iter().all(|c| *c == 255),
            "additive saturation: {compose:?}"
        );
    }
}
