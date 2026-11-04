#![cfg(feature = "paint-scene")]
use kurbo::{Affine, Circle, Rect, Shape};
use peniko::{
    Blob, Extend, Fill, ImageAlphaType, ImageBrush, ImageData, ImageFormat, ImageQuality,
};
use plico_renderer::{PaintScene, Scene};
use plico_wgpu::Renderer;
use std::sync::Arc;
fn source() -> (ImageData, Vec<u8>) {
    let pixels: Vec<u8> = (0..12)
        .flat_map(|i| {
            [
                ((i * 73) % 256) as u8,
                ((i * 117 + 31) % 256) as u8,
                ((i * 43 + 19) % 256) as u8,
                255,
            ]
        })
        .collect();
    (
        ImageData {
            data: Blob::new(Arc::new(pixels.clone())),
            format: ImageFormat::Rgba8,
            alpha_type: ImageAlphaType::AlphaPremultiplied,
            width: 4,
            height: 3,
        },
        pixels,
    )
}
#[test]
#[ignore = "requires a working wgpu adapter"]
fn rotated_sheared_and_reflected_images_sample_the_original_source() {
    let (image, pixels) = source();
    let mut brush = ImageBrush::new(image);
    brush.sampler.quality = ImageQuality::Low;
    let mut renderer = pollster::block_on(Renderer::new()).unwrap();
    for transform in [
        Affine::translate((40., 10.)) * Affine::rotate(0.35) * Affine::scale_non_uniform(8., 6.),
        Affine::translate((15., 20.)) * Affine::skew(0.3, -0.15) * Affine::scale(6.),
        Affine::translate((75., 15.)) * Affine::scale_non_uniform(-8., 7.),
    ] {
        let mut scene = Scene::new();
        scene.draw_image((&brush).into(), transform);
        let actual = renderer
            .render_scene(&scene, [96, 64], [0.; 4], false)
            .unwrap();
        let inverse = transform.inverse();
        let mut checked = 0;
        for y in 0..64 {
            for x in 0..96 {
                let p = inverse * kurbo::Point::new(x as f64 + 0.5, y as f64 + 0.5);
                let actual = &actual[(y * 96 + x) * 4..(y * 96 + x) * 4 + 4];
                if p.x > 0.3 && p.x < 3.7 && p.y > 0.3 && p.y < 2.7 {
                    let i = (p.y.floor() as usize * 4 + p.x.floor() as usize) * 4;
                    assert_eq!(actual, &pixels[i..i + 4], "at {x},{y} with {transform:?}");
                    checked += 1;
                } else if p.x < -0.3 || p.x > 4.3 || p.y < -0.3 || p.y > 3.3 {
                    assert_eq!(actual, [0; 4]);
                }
            }
        }
        assert!(checked > 100);
    }
}
#[test]
#[ignore = "requires a working wgpu adapter"]
fn arbitrary_image_fill_preserves_shape_and_affine_brush_coordinates() {
    let (image, pixels) = source();
    let mut brush = ImageBrush::new(image);
    brush.sampler.quality = ImageQuality::Low;
    brush.sampler.x_extend = Extend::Repeat;
    brush.sampler.y_extend = Extend::Reflect;
    let shape = Circle::new((2., 1.5), 1.2).to_path(0.001);
    let transform = Affine::translate((25., 12.)) * Affine::rotate(-0.25) * Affine::scale(10.);
    let brush_transform =
        Affine::translate((0.2, -0.3)) * Affine::skew(0.25, 0.1) * Affine::scale(0.7);
    let mut scene = Scene::new();
    scene.fill(
        Fill::NonZero,
        transform,
        &brush,
        Some(brush_transform),
        &shape,
    );
    let mut renderer = pollster::block_on(Renderer::new()).unwrap();
    let actual = renderer
        .render_scene(&scene, [80, 64], [0.; 4], false)
        .unwrap();
    let inverse = transform.inverse();
    let sample = (transform * brush_transform).inverse();
    let mut checked = 0;
    for y in 0..64 {
        for x in 0..80 {
            let point = kurbo::Point::new(x as f64 + 0.5, y as f64 + 0.5);
            let local = inverse * point;
            let distance = local.distance((2., 1.5).into());
            let a = &actual[(y * 80 + x) * 4..(y * 80 + x) * 4 + 4];
            if distance < 0.9 {
                let source = sample * point;
                let sx = (source.x.floor() as i32).rem_euclid(4) as usize;
                let sy = (source.y.floor() as i32).rem_euclid(6);
                let sy = sy.min(5 - sy) as usize;
                assert_eq!(a, &pixels[(sy * 4 + sx) * 4..(sy * 4 + sx) * 4 + 4]);
                checked += 1;
            } else if distance > 1.5 {
                assert_eq!(a, [0; 4]);
            }
        }
    }
    assert!(checked > 100);
}
#[test]
#[ignore = "requires a working wgpu adapter"]
fn affine_image_motion_clips_and_mask_restoration_match_full_redraw() {
    let (image, _) = source();
    let mut brush = ImageBrush::new(image.clone());
    brush.sampler.quality = ImageQuality::High;
    let mut partial = pollster::block_on(Renderer::new()).unwrap();
    let mut full = pollster::block_on(Renderer::new()).unwrap();
    for frame in 0..7 {
        let mut scene = Scene::new();
        scene.push_layer(
            peniko::BlendMode::default(),
            0.75,
            Affine::IDENTITY,
            &Circle::new((40., 32.), 25.),
            None,
            None,
        );
        if frame < 6 {
            scene.draw_image(
                (&brush).into(),
                Affine::translate((25. + frame as f64 * 0.25, 15.))
                    * Affine::rotate(0.15 + frame as f64 * 0.04)
                    * Affine::scale(8.),
            );
        }
        scene.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            peniko::Color::from_rgb8(0, 255, 0),
            None,
            &Rect::new(50., 22., 60., 32.),
        );
        scene.pop_layer();
        let actual = partial
            .render_scene(&scene, [80, 64], [0.; 4], true)
            .unwrap();
        assert_eq!(
            actual,
            full.render_scene(&scene, [80, 64], [0.; 4], false).unwrap()
        );
        assert_eq!(
            &actual[(27 * 80 + 55) * 4..(27 * 80 + 55) * 4 + 4],
            [0, 191, 0, 191],
            "image coverage mask leaked into following draw"
        );
    }
    let uploads = partial.image_cache_stats().2;
    let quad = plico_wgpu::image::ImageQuad {
        quality: ImageQuality::High,
        ..plico_wgpu::image::ImageQuad::new(plico_wgpu::Quad::new([0., 0., 8., 8.], [1.; 4]), image)
    };
    partial
        .render_items(&[plico_wgpu::Draw::Image(&quad)], [80, 64], [0.; 4], 0.)
        .unwrap();
    assert_eq!(uploads, partial.image_cache_stats().2);
}

#[test]
#[ignore = "requires a working wgpu adapter"]
fn image_path_fill_preserves_even_odd_holes() {
    let (image, _) = source();
    let mut brush = ImageBrush::new(image);
    brush.sampler.quality = ImageQuality::Low;
    let mut path = Rect::new(0., 0., 4., 3.).to_path(0.1);
    path.extend(Rect::new(1., 1., 3., 2.).to_path(0.1));
    let transform = Affine::translate((12., 5.)) * Affine::rotate(0.2) * Affine::scale(10.);
    let mut scene = Scene::new();
    scene.fill(Fill::EvenOdd, transform, &brush, None, &path);
    let mut renderer = pollster::block_on(Renderer::new()).unwrap();
    let pixels = renderer
        .render_scene(&scene, [80, 64], [0.; 4], false)
        .unwrap();
    let mut hole = 0;
    let mut filled = 0;
    for y in 0..64 {
        for x in 0..80 {
            let p = transform.inverse() * kurbo::Point::new(x as f64 + 0.5, y as f64 + 0.5);
            let alpha = pixels[(y * 80 + x) * 4 + 3];
            if p.x > 1.25 && p.x < 2.75 && p.y > 1.25 && p.y < 1.75 {
                assert_eq!(alpha, 0);
                hole += 1;
            }
            if p.x > 0.25 && p.x < 3.75 && p.y > 0.25 && p.y < 0.75 {
                assert_eq!(alpha, 255);
                filled += 1;
            }
        }
    }
    assert!(hole > 4 && filled > 4);
}

#[test]
#[ignore = "requires a working wgpu adapter"]
fn image_stroke_keeps_ring_coverage_and_independent_brush_coordinates() {
    let (image, pixels) = source();
    let mut brush = ImageBrush::new(image);
    brush.sampler.quality = ImageQuality::Low;
    brush.sampler.x_extend = Extend::Repeat;
    brush.sampler.y_extend = Extend::Reflect;
    let transform = Affine::translate((30., 16.)) * Affine::rotate(0.3) * Affine::scale(12.);
    let brush_transform =
        Affine::translate((0.3, -0.2)) * Affine::skew(0.2, 0.1) * Affine::scale(0.6);
    let circle = Circle::new((2., 1.5), 1.2);
    let mut scene = Scene::new();
    scene.stroke(
        &kurbo::Stroke::new(0.5),
        transform,
        &brush,
        Some(brush_transform),
        &circle,
    );
    let mut renderer = pollster::block_on(Renderer::new()).unwrap();
    let actual = renderer
        .render_scene(&scene, [96, 80], [0.; 4], false)
        .unwrap();
    let local = transform.inverse();
    let sample = (transform * brush_transform).inverse();
    let mut checked = 0;
    for y in 0..80 {
        for x in 0..96 {
            let p = kurbo::Point::new(x as f64 + 0.5, y as f64 + 0.5);
            let distance = (local * p).distance((2., 1.5).into());
            let ring = (distance - 1.2).abs();
            let actual = &actual[(y * 96 + x) * 4..(y * 96 + x) * 4 + 4];
            if ring < 0.12 {
                let uv = sample * p;
                let sx = (uv.x.floor() as i32).rem_euclid(4) as usize;
                let sy = (uv.y.floor() as i32).rem_euclid(6);
                let sy = sy.min(5 - sy) as usize;
                assert_eq!(
                    actual,
                    &pixels[(sy * 4 + sx) * 4..(sy * 4 + sx) * 4 + 4],
                    "stroke sample {x},{y}"
                );
                checked += 1;
            } else if ring > 0.4 {
                assert_eq!(actual, [0; 4], "stroke filled its hole/outside");
            }
        }
    }
    assert!(checked > 100);
    let uploads = renderer.image_cache_stats().2;
    assert_eq!(
        renderer
            .render_scene(&scene, [96, 80], [0.; 4], false)
            .unwrap(),
        actual
    );
    assert_eq!(
        renderer.image_cache_stats().2,
        uploads,
        "unchanged stroke reuploaded resources"
    );
}

#[test]
#[ignore = "requires a working wgpu adapter"]
fn moving_dashed_image_strokes_restore_parent_masks_and_match_full_redraw() {
    let (image, _) = source();
    let mut brush = ImageBrush::new(image);
    brush.sampler.quality = ImageQuality::High;
    brush.sampler.x_extend = Extend::Repeat;
    brush.sampler.y_extend = Extend::Reflect;
    brush.sampler.alpha = 0.8;
    let mut partial = pollster::block_on(Renderer::new()).unwrap();
    let mut full = pollster::block_on(Renderer::new()).unwrap();
    for frame in 0..8 {
        let mut scene = Scene::new();
        scene.push_layer(
            peniko::BlendMode::default(),
            0.75,
            Affine::IDENTITY,
            &Circle::new((40., 32.), 27.),
            None,
            None,
        );
        if frame < 7 {
            let style = kurbo::Stroke::new(4.)
                .with_caps(kurbo::Cap::Round)
                .with_dashes(frame as f64 * 0.3, [5., 3.]);
            scene.stroke(
                &style,
                Affine::translate((frame as f64 * 0.25, 0.)) * Affine::skew(0.15, 0.),
                &brush,
                Some(Affine::scale(7.)),
                &Circle::new((32., 28.), 19.),
            );
        }
        scene.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            peniko::Color::from_rgb8(255, 0, 255),
            None,
            &Rect::new(35., 25., 45., 35.),
        );
        scene.pop_layer();
        let actual = partial
            .render_scene(&scene, [80, 64], [0.; 4], true)
            .unwrap();
        let expected = full.render_scene(&scene, [80, 64], [0.; 4], false).unwrap();
        assert_eq!(actual, expected, "image stroke frame {frame}");
        assert_eq!(
            &actual[(30 * 80 + 40) * 4..(30 * 80 + 40) * 4 + 4],
            &[191, 0, 191, 191],
            "stroke mask leaked to next draw"
        );
    }
}

#[test]
#[ignore = "requires a working wgpu adapter"]
fn mipmapped_affine_brushes_cross_extension_seams_and_match_partial_redraw() {
    let image = ImageData {
        data: Blob::new(Arc::new(
            (0..193)
                .flat_map(|y| {
                    (0..257).flat_map(move |x| {
                        let value = if (x + y) % 2 == 0 { 0u8 } else { 255 };
                        [value, value, value, 255]
                    })
                })
                .collect::<Vec<_>>(),
        )),
        width: 257,
        height: 193,
        format: ImageFormat::Rgba8,
        alpha_type: ImageAlphaType::AlphaPremultiplied,
    };
    let mut partial = pollster::block_on(Renderer::new()).unwrap();
    let mut full = pollster::block_on(Renderer::new()).unwrap();
    let mut partial_frames = 0;
    for quality in [ImageQuality::Medium, ImageQuality::High] {
        for (frame, extend) in [Extend::Repeat, Extend::Reflect, Extend::Pad, Extend::Repeat]
            .into_iter()
            .enumerate()
        {
            let mut brush = ImageBrush::new(image.clone());
            brush.sampler.quality = quality;
            brush.sampler.x_extend = extend;
            brush.sampler.y_extend = Extend::Reflect;
            let mut scene = Scene::new();
            scene.push_clip_layer(Affine::IDENTITY, &Circle::new((64., 48.), 40.));
            scene.fill(
                Fill::NonZero,
                Affine::IDENTITY,
                &brush,
                Some(
                    Affine::translate((-9. + frame as f64 * 0.37, -11.))
                        * Affine::rotate(0.15 + frame as f64 * 0.13)
                        * Affine::scale(1. / (12. + frame as f64)),
                ),
                &Rect::new(0., 0., 128., 96.),
            );
            scene.pop_layer();
            let actual = partial
                .render_scene(&scene, [128, 96], [0.; 4], true)
                .unwrap();
            if partial.scene_damage_coverage([128, 96]) < 1. {
                partial_frames += 1;
            }
            let expected = full
                .render_scene(&scene, [128, 96], [0.; 4], false)
                .unwrap();
            assert_eq!(actual, expected, "damage: {quality:?} {extend:?} {frame}");
            for y in 0..96 {
                for x in 0..128 {
                    if (x as f64 + 0.5 - 64.).hypot(y as f64 + 0.5 - 48.) < 36. {
                        let pixel = &actual[(y * 128 + x) * 4..(y * 128 + x) * 4 + 4];
                        for &channel in &pixel[..3] {
                            assert!(
                                channel.abs_diff(128) <= 1,
                                "mip/seam artifact at {x},{y}: {pixel:?}, {quality:?} {extend:?}"
                            );
                        }
                        assert_eq!(pixel[3], 255);
                    }
                }
            }
        }
    }
    assert!(partial_frames > 0, "fixture only exercised full redraw");
}
