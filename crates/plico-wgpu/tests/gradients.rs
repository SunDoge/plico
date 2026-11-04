#![cfg(feature = "paint-scene")]
use kurbo::{Affine, Rect, Shape};
use peniko::{Color, Fill, Gradient, InterpolationAlphaSpace};
use plico_renderer::{PaintScene, Scene};
use plico_wgpu::Renderer;
#[test]
#[ignore = "requires a working wgpu adapter"]
fn path_gradient_pixels_and_fractional_damage_match_full_render() {
    let mut partial = pollster::block_on(Renderer::new()).unwrap();
    let mut full = pollster::block_on(Renderer::new()).unwrap();
    for frame in 0..6 {
        let gradient = Gradient::new_linear((0., 0.), (32., 0.))
            .with_stops([
                Color::from_rgba8(255, 0, 0, 0),
                Color::from_rgba8(0, 0, 255, 255),
            ])
            .with_interpolation_alpha_space(if frame % 2 == 0 {
                InterpolationAlphaSpace::Premultiplied
            } else {
                InterpolationAlphaSpace::Unpremultiplied
            });
        let mut scene = Scene::new();
        if frame < 5 {
            scene.fill(
                Fill::NonZero,
                Affine::translate((frame as f64 * 0.25, 0.)),
                &gradient,
                None,
                &Rect::new(0., 0., 32., 24.).to_path(0.1),
            );
        }
        let actual = partial
            .render_scene(&scene, [64, 32], [0.; 4], true)
            .unwrap();
        let expected = full.render_scene(&scene, [64, 32], [0.; 4], false).unwrap();
        assert_eq!(actual, expected, "frame {frame}");
        if frame == 0 {
            let pixel = &actual[(12 * 64 + 16) * 4..(12 * 64 + 16) * 4 + 4];
            assert_eq!(pixel[0], 0);
            assert!(pixel[2].abs_diff(131) <= 1);
            assert_eq!(pixel[2], pixel[3]);
        }
    }
}

#[test]
#[ignore = "requires a working wgpu adapter"]
fn transformed_multistop_rectangle_gradient_matches_source_coordinates_and_damage() {
    let mut partial = pollster::block_on(Renderer::new()).unwrap();
    let mut full = pollster::block_on(Renderer::new()).unwrap();
    let gradient = Gradient::new_linear((0., 0.), (20., 0.)).with_stops([
        (0., Color::BLACK),
        (0.5, Color::WHITE),
        (1., Color::BLACK),
    ]);
    let brush_transform =
        Affine::translate((7., 5.)) * Affine::rotate(0.35) * Affine::scale_non_uniform(1.3, 0.8);
    for frame in 0..5 {
        let transform = Affine::translate((frame as f64 * 0.25, 0.));
        let mut scene = Scene::new();
        if frame < 4 {
            scene.fill(
                Fill::NonZero,
                transform,
                &gradient,
                Some(brush_transform),
                &Rect::new(0., 0., 64., 40.),
            );
        }
        let actual = partial
            .render_scene(&scene, [80, 48], [0.; 4], true)
            .unwrap();
        let expected = full.render_scene(&scene, [80, 48], [0.; 4], false).unwrap();
        assert_eq!(actual, expected, "frame={frame}");
        if frame == 0 {
            let inverse = (transform * brush_transform).inverse();
            for y in 2..38 {
                for x in 2..62 {
                    let p = inverse * kurbo::Point::new(x as f64 + 0.5, y as f64 + 0.5);
                    let t = (p.x / 20.).clamp(0., 1.);
                    let shade = ((1. - (2. * t - 1.).abs()) * 255.).round() as u8;
                    let pixel = &actual[(y * 80 + x) * 4..(y * 80 + x) * 4 + 4];
                    assert!(
                        pixel[0].abs_diff(shade) <= 1,
                        "{x},{y}: {pixel:?} expected={shade}"
                    );
                    assert_eq!(pixel[0], pixel[1]);
                    assert_eq!(pixel[1], pixel[2]);
                    assert_eq!(pixel[3], 255);
                }
            }
        }
    }
    // Singular brush transforms must return an error, retaining prior pixels.
    let mut invalid = Scene::new();
    invalid.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        &gradient,
        Some(Affine::scale(0.)),
        &Rect::new(0., 0., 64., 40.),
    );
    assert!(
        partial
            .render_scene(&invalid, [80, 48], [0.; 4], true)
            .is_err()
    );
}

#[test]
#[ignore = "requires a working wgpu adapter"]
fn gradient_stroke_and_rounded_rectangle_use_the_same_brush_space_as_paths() {
    let mut renderer = pollster::block_on(Renderer::new()).unwrap();
    let gradient = Gradient::new_linear((0., 0.), (12., 0.))
        .with_stops([(0., Color::BLACK), (0.4, Color::WHITE), (1., Color::BLACK)])
        .with_extend(peniko::Extend::Reflect);
    let transform = Affine::translate((20., 14.)) * Affine::rotate(0.15);
    let brush_transform = Affine::translate((3., 2.)) * Affine::scale(1.5);
    let rounded = Rect::new(0., 0., 40., 26.).to_rounded_rect(6.);
    for stroke in [false, true] {
        let mut primitive = Scene::new();
        let mut path = Scene::new();
        if stroke {
            let style = kurbo::Stroke::new(3.);
            primitive.stroke(
                &style,
                transform,
                &gradient,
                Some(brush_transform),
                &rounded,
            );
            path.stroke(
                &style,
                transform,
                &gradient,
                Some(brush_transform),
                &rounded.to_path(0.1),
            );
        } else {
            primitive.fill(
                Fill::NonZero,
                transform,
                &gradient,
                Some(brush_transform),
                &rounded,
            );
            path.fill(
                Fill::NonZero,
                transform,
                &gradient,
                Some(brush_transform),
                &rounded.to_path(0.1),
            );
        }
        let expected = renderer
            .render_scene(&path, [96, 64], [0.; 4], false)
            .unwrap();
        let actual = renderer
            .render_scene(&primitive, [96, 64], [0.; 4], false)
            .unwrap();
        assert_eq!(actual, expected);
        assert!(actual.as_chunks::<4>().0.iter().any(|p| p[3] > 0));
    }
}

#[test]
#[ignore = "requires a working wgpu adapter"]
fn radial_alpha_brush_transform_and_damage_match_reference() {
    let mut partial = pollster::block_on(Renderer::new()).unwrap();
    let mut full = pollster::block_on(Renderer::new()).unwrap();
    let gradient = Gradient::new_radial((32., 24.), 20.).with_stops([
        Color::from_rgba8(255, 0, 0, 0),
        Color::from_rgba8(0, 0, 255, 200),
    ]);
    let brush_transform = Affine::translate((3., 1.)) * Affine::scale_non_uniform(1.1, 0.8);
    for frame in 0..6 {
        let transform = Affine::translate((frame as f64 * 0.25, 0.));
        let mut scene = Scene::new();
        if frame < 5 {
            scene.fill(
                Fill::NonZero,
                transform,
                &gradient,
                Some(brush_transform),
                &Rect::new(0., 0., 72., 48.),
            );
        }
        let actual = partial
            .render_scene(&scene, [80, 56], [0.; 4], true)
            .unwrap();
        let expected = full.render_scene(&scene, [80, 56], [0.; 4], false).unwrap();
        assert_eq!(actual, expected, "radial damage frame={frame}");
        if frame == 0 {
            let inverse = (transform * brush_transform).inverse();
            for y in 2..46 {
                for x in 2..70 {
                    let p = inverse * kurbo::Point::new(x as f64 + 0.5, y as f64 + 0.5);
                    let alpha = (((p - kurbo::Point::new(32., 24.)).hypot() / 20.).min(1.) * 200.)
                        .round() as u8;
                    let pixel = &actual[(y * 80 + x) * 4..(y * 80 + x) * 4 + 4];
                    assert_eq!(&pixel[..2], &[0, 0]);
                    assert!(pixel[2].abs_diff(alpha) <= 1 && pixel[3].abs_diff(alpha) <= 1);
                }
            }
        }
    }
}
