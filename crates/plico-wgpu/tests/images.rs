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
            quality: ImageQuality::High,
            alpha: 1.,
        };
        let actual = renderer
            .render_items(&[Draw::Image(&quad)], [8, 8], [0.; 4], 0.)
            .unwrap();
        assert!(actual.as_chunks::<4>().0.iter().all(|p| *p == expected));
    }
}
