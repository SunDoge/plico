#![cfg(feature = "paint-scene")]
use kurbo::Affine;
use peniko::{Blob, ImageAlphaType, ImageBrush, ImageData, ImageFormat, ImageQuality};
use plico_renderer::{PaintScene, Scene};
use plico_wgpu::Renderer;
use std::sync::Arc;
fn mitchell(x: f64) -> f64 {
    let x = x.abs();
    let b = 1. / 3.;
    let c = 1. / 3.;
    if x < 1. {
        ((12. - 9. * b - 6. * c) * x.powi(3) + (-18. + 12. * b + 6. * c) * x * x + 6. - 2. * b) / 6.
    } else if x < 2. {
        ((-b - 6. * c) * x.powi(3)
            + (6. * b + 30. * c) * x * x
            + (-12. * b - 48. * c) * x
            + 8. * b
            + 24. * c)
            / 6.
    } else {
        0.
    }
}
#[test]
#[ignore = "requires a working wgpu adapter"]
fn high_quality_matches_f64_mitchell_reference_and_is_not_bilinear() {
    let source: Vec<u8> = (0..12)
        .flat_map(|i| {
            [
                ((i * 73) % 256) as u8,
                ((i * 117 + 31) % 256) as u8,
                ((i * 43 + 19) % 256) as u8,
                255,
            ]
        })
        .collect();
    let mut brush = ImageBrush::new(ImageData {
        data: Blob::new(Arc::new(source.clone())),
        format: ImageFormat::Rgba8,
        alpha_type: ImageAlphaType::AlphaPremultiplied,
        width: 4,
        height: 3,
    });
    let mut renderer = pollster::block_on(Renderer::new()).unwrap();
    for [width, height] in [[28, 20], [3, 2]] {
        brush.sampler.quality = ImageQuality::High;
        let mut scene = Scene::new();
        scene.draw_image(
            (&brush).into(),
            Affine::scale_non_uniform(width as f64 / 4., height as f64 / 3.),
        );
        let pixels = renderer
            .render_scene(&scene, [width, height], [0.; 4], false)
            .unwrap();
        for y in 0..height {
            for x in 0..width {
                let px = (x as f64 + 0.5) * 4. / width as f64 - 0.5;
                let py = (y as f64 + 0.5) * 3. / height as f64 - 0.5;
                let mut expected = [0.; 4];
                for dy in -1..=2 {
                    for dx in -1..=2 {
                        let sx = (px.floor() as i32 + dx).clamp(0, 3) as usize;
                        let sy = (py.floor() as i32 + dy).clamp(0, 2) as usize;
                        let w = mitchell(px - (px.floor() + dx as f64))
                            * mitchell(py - (py.floor() + dy as f64));
                        for c in 0..4 {
                            expected[c] += source[(sy * 4 + sx) * 4 + c] as f64 * w;
                        }
                    }
                }
                for c in 0..4 {
                    let expected = expected[c].round().clamp(0., 255.) as u8;
                    let actual = pixels[((y * width + x) * 4) as usize + c];
                    assert!(
                        actual.abs_diff(expected) <= 1,
                        "{width}x{height} at {x},{y} channel {c}: {actual}/{expected}"
                    );
                }
            }
        }
        brush.sampler.quality = ImageQuality::Medium;
        let mut linear = Scene::new();
        linear.draw_image(
            (&brush).into(),
            Affine::scale_non_uniform(width as f64 / 4., height as f64 / 3.),
        );
        assert_ne!(
            pixels,
            renderer
                .render_scene(&linear, [width, height], [0.; 4], false)
                .unwrap()
        );
    }
}
#[test]
#[ignore = "requires a working wgpu adapter"]
fn cubic_transparent_edges_remain_premultiplied_and_partial_frames_match() {
    let mut brush = ImageBrush::new(ImageData {
        data: Blob::new(Arc::new(vec![200, 0, 0, 128, 0, 255, 0, 0])),
        format: ImageFormat::Rgba8,
        alpha_type: ImageAlphaType::Alpha,
        width: 2,
        height: 1,
    });
    brush.sampler.quality = ImageQuality::High;
    let mut partial = pollster::block_on(Renderer::new()).unwrap();
    let mut full = pollster::block_on(Renderer::new()).unwrap();
    for frame in 0..8 {
        let mut scene = Scene::new();
        if frame != 7 {
            scene.draw_image(
                (&brush).into(),
                Affine::translate((4. + frame as f64 * 0.25, 4.)) * Affine::scale(12.),
            );
        }
        let actual = partial
            .render_scene(&scene, [48, 32], [0.; 4], true)
            .unwrap();
        assert_eq!(
            actual,
            full.render_scene(&scene, [48, 32], [0.; 4], false).unwrap()
        );
        for p in actual.as_chunks::<4>().0 {
            assert_eq!(p[1], 0);
            assert_eq!(p[2], 0);
            assert!(p[0] <= p[3]);
        }
    }
    assert_eq!(
        partial.image_cache_stats().2,
        1,
        "motion reuploaded immutable image data"
    );
}

#[test]
#[ignore = "requires a working wgpu adapter"]
fn cubic_pad_extension_handles_large_finite_uvs() {
    use plico_wgpu::{Draw, Quad, image::ImageQuad};
    let image = ImageData {
        data: Blob::new(Arc::new(vec![255, 0, 0, 255, 0, 0, 255, 255])),
        format: ImageFormat::Rgba8,
        alpha_type: ImageAlphaType::AlphaPremultiplied,
        width: 2,
        height: 1,
    };
    let mut renderer = pollster::block_on(Renderer::new()).unwrap();
    for (u, expected) in [(-1e30, [255, 0, 0, 255]), (1e30, [0, 0, 255, 255])] {
        let quad = ImageQuad {
            quad: Quad::new([0., 0., 8., 8.], [1.; 4]),
            image: image.clone(),
            uv: [u, 0., 0., 1.],
            uv_transform: None,
            quality: ImageQuality::High,
            extend: [peniko::Extend::Pad; 2],
            alpha: 1.,
        };
        let actual = renderer
            .render_items(&[Draw::Image(&quad)], [8, 8], [0.; 4], 0.)
            .unwrap();
        assert!(actual.as_chunks::<4>().0.iter().all(|p| *p == expected));
    }
}

#[test]
#[ignore = "requires a working wgpu adapter"]
fn image_extension_modes_match_cpu_sampling_across_negative_uvs_and_seams() {
    use peniko::Extend;
    use plico_wgpu::{Draw, Quad, image::ImageQuad};
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
    let image = ImageData {
        data: Blob::new(Arc::new(pixels.clone())),
        format: ImageFormat::Rgba8,
        alpha_type: ImageAlphaType::AlphaPremultiplied,
        width: 4,
        height: 3,
    };
    let coordinate = |v: i32, n: i32, mode: Extend| -> usize {
        (match mode {
            Extend::Pad => v.clamp(0, n - 1),
            Extend::Repeat => v.rem_euclid(n),
            Extend::Reflect => {
                let p = v.rem_euclid(2 * n);
                p.min(2 * n - 1 - p)
            }
        }) as usize
    };
    let mut renderer = pollster::block_on(Renderer::new()).unwrap();
    for xmode in [Extend::Pad, Extend::Repeat, Extend::Reflect] {
        for ymode in [Extend::Pad, Extend::Repeat, Extend::Reflect] {
            for quality in [ImageQuality::Low, ImageQuality::Medium, ImageQuality::High] {
                let quad = ImageQuad {
                    quad: Quad::new([0., 0., 19., 13.], [1.; 4]),
                    image: image.clone(),
                    uv: [-0.7, -0.35, 3.4, 2.2],
                    uv_transform: None,
                    quality,
                    extend: [xmode, ymode],
                    alpha: 1.,
                };
                let actual = renderer
                    .render_items(&[Draw::Image(&quad)], [19, 13], [0.; 4], 0.)
                    .unwrap();
                for y in 0..13 {
                    for x in 0..19 {
                        let px = (-0.7 + (x as f64 + 0.5) / 19. * 3.4) * 4. - 0.5;
                        let py = (-0.35 + (y as f64 + 0.5) / 13. * 2.2) * 3. - 0.5;
                        let mut result = [0.; 4];
                        if quality == ImageQuality::Low {
                            let sx = coordinate((px + 0.5).floor() as i32, 4, xmode);
                            let sy = coordinate((py + 0.5).floor() as i32, 3, ymode);
                            for c in 0..4 {
                                result[c] = pixels[(sy * 4 + sx) * 4 + c] as f64;
                            }
                        } else {
                            let range = if quality == ImageQuality::High {
                                -1..=2
                            } else {
                                0..=1
                            };
                            for dy in range.clone() {
                                for dx in range.clone() {
                                    let sx = coordinate(px.floor() as i32 + dx, 4, xmode);
                                    let sy = coordinate(py.floor() as i32 + dy, 3, ymode);
                                    let weight = |delta: f64| {
                                        if quality == ImageQuality::High {
                                            mitchell(delta)
                                        } else {
                                            (1. - delta.abs()).max(0.)
                                        }
                                    };
                                    let w = weight(px - (px.floor() + dx as f64))
                                        * weight(py - (py.floor() + dy as f64));
                                    for c in 0..4 {
                                        result[c] += pixels[(sy * 4 + sx) * 4 + c] as f64 * w;
                                    }
                                }
                            }
                        }
                        for c in 0..4 {
                            let expected = result[c].round().clamp(0., 255.) as u8;
                            let a = actual[(y * 19 + x) * 4 + c];
                            assert!(
                                a.abs_diff(expected) <= 1,
                                "{xmode:?}/{ymode:?} {quality:?} at {x},{y} channel {c}: {a}/{expected}"
                            );
                        }
                    }
                }
            }
        }
    }
    assert_eq!(
        renderer.image_cache_stats().2,
        1,
        "changing sampler modes reuploaded immutable pixels"
    );
    assert_eq!(renderer.image_binding_stats(), (27, 27));
    assert_eq!(renderer.image_cache_stats().1, 4 * 3 * 4);
}

#[test]
#[ignore = "requires a working wgpu adapter"]
fn shared_image_brush_preserves_independent_extend_axes() {
    use kurbo::Rect;
    use peniko::{Extend, Fill};
    use plico_wgpu::{Draw, Quad, image::ImageQuad};
    let image = ImageData {
        data: Blob::new(Arc::new(vec![
            255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 255,
        ])),
        format: ImageFormat::Rgba8,
        alpha_type: ImageAlphaType::AlphaPremultiplied,
        width: 2,
        height: 2,
    };
    let mut brush = ImageBrush::new(image.clone());
    brush.sampler.x_extend = Extend::Repeat;
    brush.sampler.y_extend = Extend::Reflect;
    brush.sampler.quality = ImageQuality::High;
    let mut scene = Scene::new();
    scene.fill(
        Fill::NonZero,
        Affine::translate((4., 3.)),
        &brush,
        None,
        &Rect::new(0., 0., 20., 12.),
    );
    let mut quad = ImageQuad::new(Quad::new([4., 3., 20., 12.], [1.; 4]), image);
    quad.uv = [0., 0., 10., 6.];
    quad.extend = [Extend::Repeat, Extend::Reflect];
    quad.quality = ImageQuality::High;
    let mut renderer = pollster::block_on(Renderer::new()).unwrap();
    let expected = renderer
        .render_items(&[Draw::Image(&quad)], [32, 24], [0.; 4], 0.)
        .unwrap();
    assert_eq!(
        expected,
        renderer
            .render_scene(&scene, [32, 24], [0.; 4], false)
            .unwrap()
    );
    assert_eq!(renderer.image_cache_stats().2, 1);
}

#[test]
#[ignore = "requires a working wgpu adapter"]
fn source_eviction_releases_all_bindings_without_corrupting_in_flight_draws() {
    let mut scene = Scene::new();
    let mut first = None;
    for x in 0..300_u32 {
        let image = ImageData {
            data: Blob::new(Arc::new(vec![(x % 251) as u8, 80, 200, 255])),
            format: ImageFormat::Rgba8,
            alpha_type: ImageAlphaType::AlphaPremultiplied,
            width: 1,
            height: 1,
        };
        if x == 0 {
            first = Some(image.clone());
        }
        let mut brush = ImageBrush::new(image);
        brush.sampler.quality = ImageQuality::Low;
        scene.draw_image((&brush).into(), Affine::translate(((2 * x) as f64, 0.)));
        brush.sampler.quality = ImageQuality::High;
        brush.sampler.x_extend = peniko::Extend::Reflect;
        scene.draw_image((&brush).into(), Affine::translate(((2 * x + 1) as f64, 0.)));
    }
    let mut renderer = pollster::block_on(Renderer::new()).unwrap();
    let actual = renderer
        .render_scene(&scene, [600, 1], [0.; 4], false)
        .unwrap();
    for x in 0..600 {
        assert_eq!(
            &actual[x * 4..x * 4 + 4],
            [((x / 2) % 251) as u8, 80, 200, 255]
        );
    }
    assert_eq!(
        (
            renderer.image_cache_stats().0,
            renderer.image_cache_stats().1,
            renderer.image_cache_stats().2
        ),
        (256, 1024, 300)
    );
    assert_eq!(renderer.image_binding_stats(), (512, 600));
    let mut return_to_first = Scene::new();
    return_to_first.draw_image((&ImageBrush::new(first.unwrap())).into(), Affine::IDENTITY);
    renderer
        .render_scene(&return_to_first, [1, 1], [0.; 4], false)
        .unwrap();
    assert_eq!(renderer.image_cache_stats().2, 301);
    assert_eq!(renderer.image_binding_stats(), (511, 601));
}

#[test]
#[ignore = "requires a working wgpu adapter"]
fn trimming_image_gpu_cache_preserves_pixels_and_reuploads_on_next_change() {
    let mut renderer = pollster::block_on(Renderer::new()).unwrap();
    let mut oracle = pollster::block_on(Renderer::new()).unwrap();
    let image = ImageData {
        data: Blob::new(Arc::new([220u8, 30, 60, 255].repeat(16))),
        format: ImageFormat::Rgba8,
        alpha_type: ImageAlphaType::AlphaPremultiplied,
        width: 4,
        height: 4,
    };
    let mut scene = Scene::new();
    scene.draw_image((&image).into(), Affine::translate((3., 4.)));
    let original = renderer
        .render_scene(&scene, [32, 32], [1.; 4], true)
        .unwrap();
    let warm = renderer.image_cache_stats();
    assert_eq!(warm.0, 1);
    assert!(warm.1 > 0);
    assert!(renderer.image_binding_stats().0 > 0);
    renderer.trim_image_cache();
    assert_eq!(renderer.image_cache_stats(), (0, 0, warm.2, warm.3));
    assert_eq!(renderer.image_binding_stats().0, 0);
    assert_eq!(
        renderer
            .render_scene(&scene, [32, 32], [1.; 4], true)
            .unwrap(),
        original
    );
    assert_eq!(
        renderer.image_cache_stats().2,
        warm.2,
        "unchanged pixels required an upload"
    );
    let mut moved = Scene::new();
    moved.draw_image((&image).into(), Affine::translate((12., 9.)));
    let actual = renderer
        .render_scene(&moved, [32, 32], [1.; 4], true)
        .unwrap();
    let expected = oracle
        .render_scene(&moved, [32, 32], [1.; 4], false)
        .unwrap();
    assert_eq!(actual, expected);
    assert_eq!(renderer.image_cache_stats().2, warm.2 + 1);
    assert_eq!(renderer.image_cache_stats().0, 1);
}
