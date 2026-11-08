use kurbo::{Affine, PathEl, Point, Rect, Shape};
use peniko::Color;
use plico_renderer::{
    PaintScene, Scene, ShadowProfile, corner_shape::corner_shape, style::CornerRadii,
};
use plico_wgpu::Renderer;

// Independent reference: flatten the CPU contour, intersect each horizontal
// slice, and integrate the Gaussian at 1024 vertical samples (GPU uses 32).
fn segments(shape: &impl Shape) -> Vec<(Point, Point)> {
    let mut result = Vec::new();
    let mut start = Point::ZERO;
    let mut current = start;
    kurbo::flatten(shape.path_elements(0.001), 0.001, |el| match el {
        PathEl::MoveTo(p) => {
            start = p;
            current = p;
        }
        PathEl::LineTo(p) => {
            result.push((current, p));
            current = p;
        }
        PathEl::ClosePath => {
            result.push((current, start));
            current = start;
        }
        _ => unreachable!("flatten returns lines"),
    });
    result
}
fn cdf(x: f64) -> f64 {
    let a = x.abs();
    let t = 1. / (1. + 0.2316419 * a);
    let tail = (-0.5 * a * a).exp() / (2. * std::f64::consts::PI).sqrt()
        * t
        * (0.319381530
            + t * (-0.356563782 + t * (1.781477937 + t * (-1.821255978 + t * 1.330274429))));
    if x >= 0. { 1. - tail } else { tail }
}
fn reference(lines: &[(Point, Point)], p: Point, sigma: f64) -> f64 {
    let step = 8. * sigma / 1024.;
    let mut sum = 0.;
    for n in 0..1024 {
        let y = p.y - 4. * sigma + (f64::from(n) + 0.5) * step;
        let mut lo = f64::INFINITY;
        let mut hi = f64::NEG_INFINITY;
        for &(a, b) in lines {
            if (a.y <= y && b.y > y) || (b.y <= y && a.y > y) {
                let x = a.x + (b.x - a.x) * (y - a.y) / (b.y - a.y);
                lo = lo.min(x);
                hi = hi.max(x);
            }
        }
        if lo <= hi {
            let z = (y - p.y) / sigma;
            sum += (cdf((hi - p.x) / sigma) - cdf((lo - p.x) / sigma)) * (-0.5 * z * z).exp()
                / (2. * std::f64::consts::PI).sqrt()
                / sigma
                * step;
        }
    }
    sum
}

#[test]
#[ignore = "requires a working wgpu adapter"]
fn continuous_shadow_pixels_match_cpu_contour_gaussian_across_scales_and_blends() {
    let mut renderer = pollster::block_on(Renderer::new()).unwrap();
    let mut global_worst = 0_f64;
    for scale in [1., 1.5, 2.] {
        for (w, h, radius) in [(80., 64., 24.), (80., 30., 14.), (40., 40., 20.)] {
            for smoothing in [0., 0.5, 1.] {
                for sigma in [0.75, 5., 12.] {
                    let rect = Rect::new(40., 40., 40. + w, 40. + h);
                    let mut local = Scene::new();
                    local.draw_box_shadow_profile(
                        Affine::IDENTITY,
                        rect,
                        Color::BLACK,
                        ShadowProfile {
                            radius,
                            std_dev: sigma,
                            corner_smoothing: smoothing,
                        },
                    );
                    let mut scene = Scene::new();
                    scene.append_scene(local, Affine::scale(scale));
                    let size = (160. * scale) as u32;
                    let pixels = renderer
                        .render_scene(&scene, [size, size], [1.; 4], true)
                        .unwrap();
                    let shape = corner_shape(
                        Affine::scale(scale).transform_rect_bbox(rect),
                        CornerRadii::uniform((radius * scale) as f32),
                        smoothing,
                    );
                    let lines = segments(&shape);
                    let mut worst = 0_f64;
                    for y in [32., 39., 41., 44., 48., 54., 62., 70.] {
                        for x in [32., 39., 41., 44., 48., 54., 62., 80.] {
                            let px = (x * scale) as u32;
                            let py = (y * scale) as u32;
                            let p = Point::new(f64::from(px) + 0.5, f64::from(py) + 0.5);
                            let expected = (1. - reference(&lines, p, sigma * scale)) * 255.;
                            let actual = f64::from(pixels[((py * size + px) * 4) as usize]);
                            worst = worst.max((expected - actual).abs());
                        }
                    }
                    global_worst = global_worst.max(worst);
                    assert!(
                        worst < 3.,
                        "error {worst:.3}, scale {scale}, size {w}x{h}, radius {radius}, smoothing {smoothing}, sigma {sigma}"
                    );
                }
            }
        }
    }
    eprintln!("max CPU-reference error: {global_worst:.3}/255");
}

#[test]
#[ignore = "requires a working wgpu adapter"]
fn zero_blur_continuous_shadow_tracks_fill_and_inset_hole_complements_it() {
    let mut renderer = pollster::block_on(Renderer::new()).unwrap();
    for scale in [1., 1.5, 2.] {
        for smoothing in [0., 0.5, 1.] {
            let rect = Rect::new(16., 16., 80., 72.);
            let mut outer = Scene::new();
            let profile = ShadowProfile {
                radius: 20.,
                std_dev: 0.,
                corner_smoothing: smoothing,
            };
            outer.draw_box_shadow_profile(Affine::scale(scale), rect, Color::BLACK, profile);
            let mut inset = Scene::new();
            inset.draw_inset_box_shadow_profile(
                Affine::scale(scale),
                Rect::new(0., 0., 96., 96.),
                rect,
                Color::BLACK,
                profile,
            );
            let size = (96. * scale) as u32;
            let a = renderer
                .render_scene(&outer, [size, size], [1.; 4], true)
                .unwrap();
            let b = renderer
                .render_scene(&inset, [size, size], [1.; 4], true)
                .unwrap();
            let mut fill = Scene::new();
            fill.fill(
                peniko::Fill::NonZero,
                Affine::scale(scale),
                Color::BLACK,
                None,
                &corner_shape(rect, CornerRadii::uniform(20.), smoothing),
            );
            let expected = renderer
                .render_scene(&fill, [size, size], [1.; 4], true)
                .unwrap();
            let mut total = 0_u64;
            for ((a, b), expected) in a
                .as_chunks::<4>()
                .0
                .iter()
                .zip(b.as_chunks::<4>().0.iter())
                .zip(expected.as_chunks::<4>().0.iter())
            {
                assert!(
                    (i32::from(a[0]) + i32::from(b[0]) - 255).abs() <= 1,
                    "complement mismatch"
                );
                total += u64::from(a[0].abs_diff(expected[0]));
            }
            assert!(
                total as f64 / f64::from(size * size) < 0.6,
                "AA fill mismatch {total}"
            );
        }
    }
}

#[test]
#[ignore = "requires a working wgpu adapter"]
fn continuous_shadows_preserve_partial_damage_and_do_not_allocate_layer_targets() {
    let mut partial = pollster::block_on(Renderer::new()).unwrap();
    let mut full = pollster::block_on(Renderer::new()).unwrap();
    for frame in 0..16 {
        let mut scene = Scene::new();
        if frame < 15 {
            scene.push_clip_layer(Affine::IDENTITY, &Rect::new(12., 8., 92., 88.));
            let rect = Rect::new(20., 20., 80., 72.);
            let transform = Affine::translate((f64::from(frame) * 0.25, 0.));
            let profile = ShadowProfile {
                radius: 18.,
                std_dev: 3.,
                corner_smoothing: if frame % 2 == 0 { 0.5 } else { 1. },
            };
            scene.draw_box_shadow_profile(
                transform,
                rect,
                Color::from_rgba8(0, 0, 0, 100),
                profile,
            );
            scene.push_clip_layer(
                transform,
                &corner_shape(rect, CornerRadii::uniform(18.), 1.),
            );
            scene.draw_inset_box_shadow_profile(
                transform,
                rect,
                rect + kurbo::Vec2::new(0., 2.),
                Color::BLACK,
                profile,
            );
            scene.pop_layer();
            scene.pop_layer();
        }
        let actual = partial
            .render_scene(&scene, [112, 104], [1.; 4], true)
            .unwrap();
        let expected = full
            .render_scene(&scene, [112, 104], [1.; 4], false)
            .unwrap();
        assert_eq!(actual, expected, "frame {frame}");
        assert_eq!(partial.layer_pool_stats().allocations, 0);
        assert_eq!(partial.layer_pool_stats().retained_bytes, 0);
    }
}
