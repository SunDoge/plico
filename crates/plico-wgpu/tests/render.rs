use plico_wgpu::{DEFAULT_MATERIAL, Quad, Renderer};

fn pixel(image: &[u8], size: [u32; 2], x: u32, y: u32) -> [u8; 4] {
    let start = ((y * size[0] + x) * 4) as usize;
    image[start..start + 4].try_into().unwrap()
}
fn near(actual: [u8; 4], expected: [u8; 4]) {
    for (a, e) in actual.into_iter().zip(expected) {
        assert!(a.abs_diff(e) <= 1, "{actual:?} != {expected:?}");
    }
}

/// Explicit hardware test, excluded from routine CI. No software/hardware output
/// equality assumption: all checks concern interior pixels and specified coverage.
#[test]
#[ignore = "requires a working wgpu adapter; run with --ignored"]
fn quads_alpha_clip_border_resize_and_runtime_material_are_correct() {
    let mut renderer = pollster::block_on(Renderer::new()).unwrap();
    eprintln!("adapter: {:?}", renderer.adapter_info());
    let size = [32, 32];
    let full = Quad::new([0., 0., 32., 32.], [1., 0., 0., 1.]);
    let image = renderer.render(&[full], size, [0.; 4], 0.).unwrap();
    near(pixel(&image, size, 16, 16), [255, 0, 0, 255]);
    let fraction = Quad::new([10.25, 2., 10., 20.], [1., 0., 0., 1.]);
    let image = renderer.render(&[fraction], size, [0.; 4], 0.).unwrap();
    near(pixel(&image, size, 10, 10), [191, 0, 0, 191]);
    near(pixel(&image, size, 20, 10), [64, 0, 0, 64]);
    let mut half = Quad::new([8., 8., 16., 16.], [0., 1., 0., 0.5]);
    half.clip = Some([8., 8., 8., 8.]);
    let image = renderer.render(&[full, half], size, [0.; 4], 0.).unwrap();
    near(pixel(&image, size, 10, 10), [127, 128, 0, 255]);
    near(pixel(&image, size, 20, 10), [255, 0, 0, 255]);
    let mut rounded = Quad::new([2., 2., 28., 28.], [1., 0., 0., 1.]);
    rounded.radii = [8.; 4];
    rounded.border_width = 2.;
    rounded.border_color = [0., 0., 1., 1.];
    let image = renderer.render(&[rounded], size, [0.; 4], 0.).unwrap();
    near(pixel(&image, size, 2, 2), [0; 4]);
    near(pixel(&image, size, 16, 2), [0, 0, 255, 255]);
    near(pixel(&image, size, 16, 16), [255, 0, 0, 255]);
    let mut gradient = full;
    gradient.colors[1] = [0., 0., 1., 1.];
    let image = renderer.render(&[gradient], size, [0.; 4], 0.).unwrap();
    near(pixel(&image, size, 10, 7), [195, 0, 60, 255]);
    let resized = renderer.render(&[full; 70], [64, 48], [0.; 4], 0.).unwrap();
    assert_eq!(resized.len(), 64 * 48 * 4);
    near(pixel(&resized, [64, 48], 60, 40), [0; 4]);
    let source = "fn plico_material(m: Material) -> vec4f { return vec4f(m.parameter,clamp(m.time,0.,1.),0.,1.); }";
    assert!(pollster::block_on(renderer.set_material(source)).unwrap());
    assert!(!pollster::block_on(renderer.set_material(source)).unwrap());
    assert_eq!(renderer.pipeline_compilations(), 2);
    let mut custom = full;
    custom.material = true;
    custom.parameter = 0.25;
    let before = renderer.render(&[custom], size, [0.; 4], 0.).unwrap();
    let after = renderer.render(&[custom], size, [0.; 4], 1.).unwrap();
    near(pixel(&before, size, 10, 10), [64, 0, 0, 255]);
    near(pixel(&after, size, 10, 10), [64, 255, 0, 255]);
    for bad in [
        "invalid syntax".to_owned(),
        format!("{source}\n@group(0) @binding(1) var extra:texture_2d<f32>;"),
        "x".repeat(65 * 1024),
    ] {
        assert!(pollster::block_on(renderer.set_material(&bad)).is_err());
        assert_eq!(renderer.pipeline_compilations(), 2);
        assert_eq!(
            renderer.render(&[custom], size, [0.; 4], 1.).unwrap(),
            after,
            "failed shader replacement changed the working pipeline"
        );
    }
    assert!(pollster::block_on(renderer.set_material(DEFAULT_MATERIAL)).unwrap());
    assert!(renderer.render(&[], [0, 1], [0.; 4], 0.).is_err());
    assert!(renderer.render(&[], [u32::MAX, 1], [0.; 4], 0.).is_err());
    assert!(renderer.render(&[], size, [0.; 4], f32::NAN).is_err());
    let mut invalid = full;
    invalid.rect[0] = f32::NAN;
    assert!(renderer.render(&[invalid], size, [0.; 4], 0.).is_err());
    let mut empty = full;
    empty.rect[2] = 0.;
    near(
        pixel(
            &renderer.render(&[empty], size, [0.; 4], 0.).unwrap(),
            size,
            10,
            10,
        ),
        [0; 4],
    );
}

#[cfg(all(target_os = "linux", feature = "system-text"))]
#[test]
#[ignore = "requires system fonts and a working wgpu adapter"]
fn text_is_ordered_dpi_aware_culled_and_upload_stable() {
    use plico_wgpu::{
        Draw,
        text::{TextRequest, TextRun},
    };
    let mut renderer = pollster::block_on(Renderer::new()).unwrap();
    let size = [420, 96];
    let mut request = TextRequest::new("Sss 中文 日本語 café e\u{301} 👩‍💻");
    request.family = "Noto Sans, Noto Sans CJK SC, sans-serif";
    request.size = 14.;
    let layout = renderer.layout_text(&request).unwrap();
    assert_eq!(layout.missing_glyphs, 0);
    request.scale = 2.;
    let double = renderer.layout_text(&request).unwrap();
    assert_eq!(double.scale, 2.);
    assert!((layout.logical_size[0] - double.logical_size[0]).abs() < 6.);
    assert!((layout.baseline - double.baseline).abs() < 2.);
    let outside = renderer
        .render_items(
            &[Draw::Text(TextRun::new(
                &layout,
                [10_000., 10_000.],
                [0., 0., 0., 1.],
            ))],
            size,
            [1.; 4],
            0.,
        )
        .unwrap();
    assert!(outside.iter().all(|p| *p == 255));
    assert_eq!(
        renderer.text_stats().uploaded_bytes,
        0,
        "invisible glyphs were rasterized/uploaded"
    );
    let run = TextRun::new(&layout, [8., 8.], [0., 0., 0., 1.]);
    let pixels = renderer
        .render_items(&[Draw::Text(run)], size, [1.; 4], 0.)
        .unwrap();
    assert!(pixels.as_chunks::<4>().0.iter().any(|p| p[0] < 100));
    let warm = renderer.text_stats();
    assert_eq!(
        pixels,
        renderer
            .render_items(&[Draw::Text(run)], size, [1.; 4], 0.)
            .unwrap()
    );
    assert_eq!(warm.uploaded_bytes, renderer.text_stats().uploaded_bytes);
    // Changing monochrome tint does not change the glyph cache or its uploaded bytes.
    let mut red = run;
    red.color = [1., 0., 0., 1.];
    let red_pixels = renderer
        .render_items(&[Draw::Text(red)], size, [1.; 4], 0.)
        .unwrap();
    assert_ne!(red_pixels, pixels);
    assert_eq!(warm.uploaded_bytes, renderer.text_stats().uploaded_bytes);
    let cover = Quad::new([0., 0., 420., 96.], [0., 0., 1., 1.]);
    let covered = renderer
        .render_items(&[Draw::Text(run), Draw::Quad(&cover)], size, [1.; 4], 0.)
        .unwrap();
    assert!(
        covered
            .as_chunks::<4>()
            .0
            .iter()
            .all(|p| *p == [0, 0, 255, 255]),
        "text ignored painter order"
    );
    for offset in [0., 0.25, 0.5, 0.75] {
        let _ = renderer
            .render_items(
                &[Draw::Text(TextRun::new(
                    &layout,
                    [8. + offset, 8.],
                    run.color,
                ))],
                size,
                [1.; 4],
                0.,
            )
            .unwrap();
    }
    let warm = renderer.text_stats();
    for frame in 0..60 {
        let offset = (frame % 4) as f64 / 4.;
        let _ = renderer
            .render_items(
                &[Draw::Text(TextRun::new(
                    &layout,
                    [8. + offset, 8. + offset],
                    run.color,
                ))],
                size,
                [1.; 4],
                0.,
            )
            .unwrap();
    }
    assert_eq!(renderer.text_stats().cached_glyphs, warm.cached_glyphs);
    assert_eq!(renderer.text_stats().uploaded_bytes, warm.uploaded_bytes);
}

/// Integer physical-pixel scrolls must translate coverage, not reshape or
/// change glyph weight, even when layout was built at a fractional DPI.
#[cfg(all(target_os = "linux", feature = "system-text"))]
#[test]
#[ignore = "requires system fonts and a working wgpu adapter"]
fn native_text_scroll_preserves_pixels_and_atlas_at_fractional_dpi() {
    use plico_wgpu::{
        Draw,
        text::{TextRequest, TextRun},
    };
    let mut renderer = pollster::block_on(Renderer::new()).unwrap();
    let size = [400, 160];
    for scale in [1., 1.25, 1.5, 2.] {
        let mut request = TextRequest::new("Sss 中文 日本語");
        request.family = "Noto Sans, Noto Sans CJK SC, sans-serif";
        request.size = 14.;
        request.scale = scale;
        let layout = renderer.layout_text(&request).unwrap();
        assert_eq!(layout.missing_glyphs, 0);
        let origin = [32.25, 48.];
        let baseline = renderer
            .render_items(
                &[Draw::Text(TextRun::new(&layout, origin, [0., 0., 0., 1.]))],
                size,
                [1.; 4],
                0.,
            )
            .unwrap();
        assert!(
            baseline
                .as_chunks::<4>()
                .0
                .iter()
                .any(|pixel| pixel[0] < 100)
        );
        let warm = renderer.text_stats();
        for [dx, dy] in [[7, 9], [-40, -31], [0, 17], [0, 0]] {
            let image = renderer
                .render_items(
                    &[Draw::Text(TextRun::new(
                        &layout,
                        [origin[0] + f64::from(dx), origin[1] + f64::from(dy)],
                        [0., 0., 0., 1.],
                    ))],
                    size,
                    [1.; 4],
                    0.,
                )
                .unwrap();
            for y in 0..size[1] as i32 {
                for x in 0..size[0] as i32 {
                    let (sx, sy) = (x - dx, y - dy);
                    if sx >= 0 && sy >= 0 && sx < size[0] as i32 && sy < size[1] as i32 {
                        assert_eq!(
                            pixel(&image, size, x as u32, y as u32),
                            pixel(&baseline, size, sx as u32, sy as u32),
                            "scale={scale}, scroll=({dx},{dy}), pixel=({x},{y})",
                        );
                    }
                }
            }
            assert_eq!(renderer.text_stats().cached_glyphs, warm.cached_glyphs);
            assert_eq!(renderer.text_stats().uploaded_bytes, warm.uploaded_bytes);
        }
    }
}
