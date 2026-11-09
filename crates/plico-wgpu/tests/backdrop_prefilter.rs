use kurbo::{Affine, Rect};
use peniko::{Color, Fill};
use plico_renderer::{PaintScene, Scene, ShaderEffectId, ShaderEffectSource, TextContext};
use plico_wgpu::Renderer;

fn effect(
    text: &mut TextContext,
    width: f32,
    height: f32,
    scale: f64,
    source: &str,
    prefilter: bool,
    values: &[f32],
) -> Scene {
    let source = ShaderEffectSource::with_backdrop(source, true)
        .unwrap()
        .with_backdrop_prefilter(prefilter.then_some(values.first().copied().unwrap_or(128.)))
        .unwrap();
    let mut paint = plico_renderer::widget::PaintContext::new(width, height, scale, text);
    paint.draw_shader_effect(ShaderEffectId::new(), source, 0., values.to_vec().into());
    paint.finish()
}
const BLUR: &str = "fn plico_effect(uv:vec2f)->vec4f { return plico_backdrop_blur(uv,plico.values[0].x*plico.resolution_time_scale.w); }";

#[test]
#[ignore = "requires a working wgpu adapter"]
fn pyramid_removes_sparse_kernel_aliasing_on_fine_stripes_across_scales() {
    let mut renderer = pollster::block_on(Renderer::new()).unwrap();
    let mut text = TextContext::new();
    for scale in [1., 1.5, 2.] {
        for (width, height) in [(128., 96.), (127., 95.)] {
            let mut deviations = [0_f64; 2];
            for (index, prefilter) in [false, true].into_iter().enumerate() {
                let mut scene = Scene::new();
                let size = [
                    (f64::from(width) * scale).round() as u32,
                    (f64::from(height) * scale).round() as u32,
                ];
                // Physical one-pixel stripes avoid overlapping fractional-AA
                // fills obscuring the filtering signal at 1.5x.
                for x in 0..size[0] {
                    scene.fill(
                        Fill::NonZero,
                        Affine::IDENTITY,
                        if x % 2 == 0 {
                            Color::BLACK
                        } else {
                            Color::WHITE
                        },
                        None,
                        &Rect::new(f64::from(x), 0., f64::from(x + 1), f64::from(size[1])),
                    );
                }
                scene.append_scene(
                    effect(&mut text, width, height, scale, BLUR, prefilter, &[8.]),
                    Affine::scale(scale),
                );
                let size = [
                    (f64::from(width) * scale).round() as u32,
                    (f64::from(height) * scale).round() as u32,
                ];
                let pixels = renderer.render_scene(&scene, size, [0.; 4], false).unwrap();
                let y = size[1] / 2;
                let mut samples = 0;
                for x in size[0] / 4..size[0] * 3 / 4 {
                    let pixel = &pixels[((y * size[0] + x) * 4) as usize..][..4];
                    assert_eq!(pixel[3], 255);
                    deviations[index] += (f64::from(pixel[0]) - 127.5).powi(2);
                    samples += 1;
                }
                deviations[index] = (deviations[index] / f64::from(samples)).sqrt();
                let stats = renderer.batch_stats();
                assert_eq!(stats.backdrop_copies, 1);
                if prefilter {
                    assert!(stats.backdrop_mip_passes > 0);
                    assert!(
                        stats.backdrop_bytes <= u64::from(size[0]) * u64::from(size[1]) * 8,
                        "pyramid must stay below twice base payload"
                    );
                } else {
                    assert_eq!(stats.backdrop_mip_passes, 0);
                    assert_eq!(
                        stats.backdrop_bytes,
                        u64::from(size[0]) * u64::from(size[1]) * 4
                    );
                }
            }
            assert!(
                deviations[1] < 3.,
                "prefilter RMS {}, scale {scale} {width}x{height}",
                deviations[1]
            );
            assert!(
                deviations[1] < deviations[0] * 0.3,
                "aliasing did not improve: {deviations:?}"
            );
            eprintln!("scale={scale} size={width}x{height} stripe RMS: {deviations:?}");
        }
    }
}

#[test]
#[ignore = "requires a working wgpu adapter"]
fn explicit_lod_preserves_premultiplied_color_odd_axes_and_releases_removed_surfaces() {
    let mut renderer = pollster::block_on(Renderer::new()).unwrap();
    let mut text = TextContext::new();
    for (width, height) in [(127., 95.), (127., 1.), (1., 95.)] {
        let mut scene = Scene::new();
        for y in 0..height as u32 {
            for x in 0..width as u32 {
                let color = if (x + y) % 2 == 0 {
                    Color::from_rgba8(255, 0, 0, 128)
                } else {
                    Color::from_rgba8(0, 0, 255, 128)
                };
                scene.fill(
                    Fill::NonZero,
                    Affine::IDENTITY,
                    color,
                    None,
                    &Rect::new(
                        f64::from(x),
                        f64::from(y),
                        f64::from(x + 1),
                        f64::from(y + 1),
                    ),
                );
            }
        }
        scene.append_scene(
            effect(
                &mut text,
                width,
                height,
                1.,
                "fn plico_effect(uv:vec2f)->vec4f { return plico_backdrop_at_lod(uv,20.); }",
                true,
                &[],
            ),
            Affine::IDENTITY,
        );
        let size = [width as u32, height as u32];
        let pixels = renderer.render_scene(&scene, size, [0.; 4], true).unwrap();
        for pixel in pixels.as_chunks::<4>().0 {
            for (actual, expected) in pixel.iter().zip([64_u8, 0, 64, 128]) {
                assert!(actual.abs_diff(expected) <= 2, "{pixel:?}, size {size:?}");
            }
        }
        renderer
            .render_scene(&Scene::new(), size, [0.; 4], true)
            .unwrap();
        assert_eq!(
            renderer.effect_cache_stats().0,
            0,
            "no historical effect targets after removal"
        );
    }
}

#[test]
#[ignore = "requires a working wgpu adapter"]
fn prefiltered_blur_keeps_crop_mapping_parent_mask_and_moving_partial_damage() {
    let mut partial = pollster::block_on(Renderer::new()).unwrap();
    let mut full = pollster::block_on(Renderer::new()).unwrap();
    let mut text = TextContext::new();
    for n in 0..12 {
        let mut scene = Scene::new();
        scene.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            Color::from_rgb8(40, 80, 160),
            None,
            &Rect::new(0., 0., 96., 80.),
        );
        scene.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            Color::WHITE,
            None,
            &Rect::new(42., 0., 43., 80.),
        );
        if n < 11 {
            scene.push_layer(
                peniko::BlendMode::default(),
                0.8,
                Affine::IDENTITY,
                &Rect::new(12., 10., 84., 72.),
                None,
                None,
            );
            scene.push_clip_layer(
                Affine::IDENTITY,
                &Rect::new(16., 16., 76., 64.).to_rounded_rect(12.),
            );
            scene.append_scene(
                effect(&mut text, 100., 90., 1., BLUR, true, &[8.]),
                Affine::translate((-4. + f64::from(n) * 0.25, -3.)),
            );
            scene.pop_layer();
            scene.pop_layer();
        }
        let actual = partial
            .render_scene(&scene, [96, 80], [0.; 4], true)
            .unwrap();
        let expected = full.render_scene(&scene, [96, 80], [0.; 4], false).unwrap();
        assert_eq!(actual, expected, "frame {n}");
        assert_eq!(&actual[..4], &[40, 80, 160, 255], "ancestor clip");
    }
}

#[test]
#[ignore = "requires a working wgpu adapter"]
fn pyramid_blur_step_response_stays_close_to_a_gaussian() {
    let mut renderer = pollster::block_on(Renderer::new()).unwrap();
    let mut text = TextContext::new();
    for sigma in [4., 12., 32.] {
        let mut rms = [0_f64; 2];
        let mut worst = 0_f64;
        for (i, prefilter) in [false, true].into_iter().enumerate() {
            let mut scene = Scene::new();
            scene.fill(
                Fill::NonZero,
                Affine::IDENTITY,
                Color::BLACK,
                None,
                &Rect::new(0., 0., 128., 128.),
            );
            scene.fill(
                Fill::NonZero,
                Affine::IDENTITY,
                Color::WHITE,
                None,
                &Rect::new(128., 0., 256., 128.),
            );
            scene.append_scene(
                effect(&mut text, 256., 128., 1., BLUR, prefilter, &[sigma]),
                Affine::IDENTITY,
            );
            let pixels = renderer
                .render_scene(&scene, [256, 128], [0.; 4], false)
                .unwrap();
            let mut count = 0;
            for x in 32..224 {
                let z = (f64::from(x) + 0.5 - 128.) / f64::from(sigma);
                let t = 1. / (1. + 0.2316419 * z.abs());
                let tail = 0.3989422804
                    * (-0.5 * z * z).exp()
                    * t
                    * (0.319381530
                        + t * (-0.356563782
                            + t * (1.781477937 + t * (-1.821255978 + t * 1.330274429))));
                let cdf = if z >= 0. { 1. - tail } else { tail };
                let error = (f64::from(pixels[((64 * 256 + x) * 4) as usize]) - cdf * 255.).abs();
                rms[i] += error * error;
                count += 1;
                if prefilter {
                    worst = worst.max(error);
                }
            }
            rms[i] = (rms[i] / f64::from(count)).sqrt();
        }
        eprintln!("sigma={sigma} edge RMS {rms:?}, pyramid max={worst:.3}");
        assert!(worst < 6., "step response error {worst}");
        assert!(
            rms[1] < rms[0],
            "pyramid should improve Gaussian response {rms:?}"
        );
    }
}

#[test]
#[ignore = "manual full-redraw wall-time probe including GPU readback; not a CI threshold"]
fn profile_many_backdrop_pyramids() {
    let mut renderer = pollster::block_on(Renderer::new()).unwrap();
    let mut text = TextContext::new();
    eprintln!("adapter={:?}", renderer.adapter_info());
    for prefilter in [false, true] {
        let mut scene = Scene::new();
        for x in 0..640 {
            scene.fill(
                Fill::NonZero,
                Affine::IDENTITY,
                if x % 2 == 0 {
                    Color::BLACK
                } else {
                    Color::WHITE
                },
                None,
                &Rect::new(f64::from(x), 0., f64::from(x + 1), 400.),
            );
        }
        for y in 0..4 {
            for x in 0..4 {
                scene.append_scene(
                    effect(&mut text, 128., 64., 1., BLUR, prefilter, &[8.]),
                    Affine::translate((12. + f64::from(x) * 156., 12. + f64::from(y) * 96.)),
                );
            }
        }
        for _ in 0..3 {
            renderer
                .render_scene(&scene, [640, 400], [1.; 4], false)
                .unwrap();
        }
        let mut times = Vec::new();
        for _ in 0..12 {
            let start = std::time::Instant::now();
            renderer
                .render_scene(&scene, [640, 400], [1.; 4], false)
                .unwrap();
            times.push(start.elapsed().as_secs_f64() * 1000.);
        }
        times.sort_by(f64::total_cmp);
        eprintln!(
            "prefilter={prefilter} median_ms={:.3} max_ms={:.3} stats={:?}",
            times[6],
            times[11],
            renderer.batch_stats()
        );
    }
}

#[test]
#[ignore = "requires a working wgpu adapter"]
fn progressive_sigma_crosses_mip_boundaries_without_a_pixel_jump() {
    let mut renderer = pollster::block_on(Renderer::new()).unwrap();
    let mut text = TextContext::new();
    for boundary in [2_f32, 4., 8., 16., 32.] {
        let mut results = Vec::new();
        for sigma in [boundary - 0.001, boundary + 0.001] {
            let mut scene = Scene::new();
            for x in 0..127 {
                scene.fill(
                    Fill::NonZero,
                    Affine::IDENTITY,
                    if x % 6 < 3 {
                        Color::BLACK
                    } else {
                        Color::WHITE
                    },
                    None,
                    &Rect::new(f64::from(x), 0., f64::from(x + 1), 95.),
                );
            }
            scene.append_scene(
                effect(&mut text, 127., 95., 1., BLUR, true, &[sigma]),
                Affine::IDENTITY,
            );
            results.push(
                renderer
                    .render_scene(&scene, [127, 95], [0.; 4], false)
                    .unwrap(),
            );
        }
        let max = results[0]
            .iter()
            .zip(&results[1])
            .map(|(a, b)| a.abs_diff(*b))
            .max()
            .unwrap();
        assert!(max <= 1, "mip boundary {boundary}, delta {max}");
    }
}
