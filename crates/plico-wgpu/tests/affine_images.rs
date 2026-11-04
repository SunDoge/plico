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
