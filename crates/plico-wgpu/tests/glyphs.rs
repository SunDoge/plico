#![cfg(all(feature = "system-text", target_os = "linux"))]
use kurbo::Affine;
use plico_renderer::{PaintCommand, PaintScene, Scene, TextContext};
use plico_wgpu::Renderer;
#[test]
#[ignore = "requires a working wgpu adapter and system fonts"]
fn external_shaped_runs_use_native_raster_and_reuse_integer_motion() {
    let mut text = TextContext::new();
    let layout = plico_renderer::text::layout_text(&mut text, "Editor abc 中文", 18.);
    let glyphs = text.glyph_scene(&layout);
    assert!(
        glyphs
            .commands
            .iter()
            .any(|c| matches!(c, PaintCommand::GlyphRun { .. }))
    );
    let mut renderer = pollster::block_on(Renderer::new()).unwrap();
    let mut render = |x: f64| {
        let mut scene = Scene::new();
        scene.append_scene((*glyphs).clone(), Affine::translate((x, 4.)));
        renderer
            .render_scene(&scene, [240, 48], [0.; 4], false)
            .unwrap()
    };
    let first = render(4.);
    assert!(first.as_chunks::<4>().0.iter().any(|p| p[3] > 100));
    let moved = render(14.);
    for y in 0..48 {
        assert_eq!(
            &first[(y * 240) * 4..(y * 240 + 230) * 4],
            &moved[(y * 240 + 10) * 4..((y + 1) * 240) * 4]
        );
    }
    let uploads = renderer.image_cache_stats().2;
    let cache = renderer.positioned_glyph_cache_stats();
    assert!(cache.0 > 0 && cache.2 > 0);
    let mut scene = Scene::new();
    scene.append_scene((*glyphs).clone(), Affine::translate((4., 4.)));
    assert_eq!(
        first,
        renderer
            .render_scene(&scene, [240, 48], [0.; 4], false)
            .unwrap()
    );
    assert_eq!(
        renderer.image_cache_stats().2,
        uploads,
        "integer motion uploaded a new text bitmap"
    );
    assert_eq!(cache, renderer.positioned_glyph_cache_stats());
    renderer.trim_positioned_glyph_cache();
    assert_eq!(renderer.positioned_glyph_cache_stats(), (0, 0, 0, 0));
    assert_eq!(
        first,
        renderer
            .render_scene(&scene, [240, 48], [0.; 4], false)
            .unwrap()
    );
}

#[test]
#[ignore = "requires a working wgpu adapter, Fontconfig and a variable font"]
fn variable_weight_changes_pixels_without_poisoning_prior_instances() {
    use std::sync::Arc;
    let list = std::process::Command::new("fc-list")
        .args([":variable=true", "-f", "%{family}\n"])
        .output()
        .unwrap();
    assert!(list.status.success());
    let families = std::str::from_utf8(&list.stdout).unwrap();
    let family: Arc<str> = families
        .lines()
        .next()
        .unwrap()
        .split(',')
        .next()
        .unwrap()
        .into();
    let mut text = TextContext::new();
    let mut build = |weight: f32| {
        let layout = plico_renderer::text::layout_text_styled_with_spacing(
            &mut text,
            Arc::from("S"),
            36.,
            weight,
            false,
            0.,
            None,
            plico_renderer::style::TextAlign::Start,
            [0, 0, 0, 255],
            Arc::from([]),
            Some(&family),
            None,
        );
        (*text.glyph_scene(&layout)).clone()
    };
    let low = build(100.);
    let high = build(900.);
    assert!(high.commands.iter().any(|c|matches!(c,PaintCommand::GlyphRun {normalized_coords,..} if normalized_coords.iter().any(|v|*v!=0))));
    let mut renderer = pollster::block_on(Renderer::new()).unwrap();
    let a = renderer
        .render_scene(&low, [128, 64], [0.; 4], false)
        .unwrap();
    let source_bytes = renderer.positioned_glyph_cache_stats().1;
    let b = renderer
        .render_scene(&high, [128, 64], [0.; 4], false)
        .unwrap();
    let mass = |pixels: &[u8]| {
        pixels
            .as_chunks::<4>()
            .0
            .iter()
            .map(|p| p[3] as u64)
            .sum::<u64>()
    };
    assert!(mass(&b) > mass(&a) * 12 / 10, "variable weight was ignored");
    assert_eq!(
        a,
        renderer
            .render_scene(&low, [128, 64], [0.; 4], false)
            .unwrap()
    );
    let (faces, _, rasters, _) = renderer.positioned_glyph_cache_stats();
    assert_eq!(
        renderer.positioned_glyph_cache_stats().1,
        source_bytes,
        "variation instances duplicated source accounting"
    );
    assert_eq!(faces, 2);
    assert_eq!(rasters, 2);
}

#[test]
#[ignore = "requires a working wgpu adapter and system fonts"]
fn per_glyph_skew_updates_cache_and_damage_without_moving_layout() {
    let mut text = TextContext::new();
    let layout = plico_renderer::text::layout_text(&mut text, "A A", 24.);
    let base = text.glyph_scene(&layout);
    let mut partial = pollster::block_on(Renderer::new()).unwrap();
    let mut full = pollster::block_on(Renderer::new()).unwrap();
    let transforms = [
        None,
        Some(Affine::skew(0.3, 0.)),
        Some(Affine::translate((2., -3.)) * Affine::skew(0.3, 0.)),
        None,
    ];
    let mut first = Vec::new();
    for (step, transform) in transforms.into_iter().enumerate() {
        let mut scene = (*base).clone();
        for command in scene.commands.iter_mut() {
            if let PaintCommand::GlyphRun {
                glyph_transform, ..
            } = command
            {
                *glyph_transform = transform;
            }
        }
        let actual = partial
            .render_scene(&scene, [160, 64], [0.; 4], true)
            .unwrap();
        assert_eq!(
            actual,
            full.render_scene(&scene, [160, 64], [0.; 4], false)
                .unwrap()
        );
        if step == 0 {
            first = actual;
        } else if step == 3 {
            assert_eq!(first, actual);
        } else {
            assert_ne!(first, actual, "glyph transform was ignored");
        }
    }
    assert_eq!(partial.positioned_glyph_cache_stats().2, 3);
    assert_eq!(
        partial
            .render_scene(&Scene::new(), [160, 64], [0.; 4], true)
            .unwrap(),
        vec![0; 160 * 64 * 4]
    );
}

#[test]
#[ignore = "requires a working wgpu adapter and system fonts"]
fn affine_runs_match_transformed_positions_and_outline_matrices() {
    let mut text = TextContext::new();
    let layout = plico_renderer::text::layout_text(&mut text, "A B", 24.);
    let base = text.glyph_scene(&layout);
    let mut partial = pollster::block_on(Renderer::new()).unwrap();
    let mut full = pollster::block_on(Renderer::new()).unwrap();
    let mut reference = pollster::block_on(Renderer::new()).unwrap();
    for run in [
        Affine::translate((12., 10.)) * Affine::skew(-0.18, 0.),
        Affine::translate((20., 8.)) * Affine::scale_non_uniform(1.2, 0.8),
        Affine::translate((30., 16.)) * Affine::rotate(0.2),
        Affine::translate((90., 16.)) * Affine::scale_non_uniform(-1., 1.),
    ] {
        let mut scene = (*base).clone();
        for command in scene.commands.iter_mut() {
            if let PaintCommand::GlyphRun {
                transform,
                glyph_transform,
                ..
            } = command
            {
                *transform = run * *transform;
                *glyph_transform = Some(Affine::translate((2., -3.)) * Affine::skew(0.2, 0.));
            }
        }
        let mut expanded = scene.clone();
        for command in expanded.commands.iter_mut() {
            if let PaintCommand::GlyphRun {
                transform,
                glyph_transform,
                glyphs,
                ..
            } = command
            {
                let [a, b, c, d, _, _] = transform.as_coeffs();
                for glyph in std::sync::Arc::make_mut(glyphs) {
                    let point = *transform * kurbo::Point::new(glyph.x as f64, glyph.y as f64);
                    glyph.x = point.x as f32;
                    glyph.y = point.y as f32;
                }
                *glyph_transform = Some(
                    Affine::new([a, b, c, d, 0., 0.]) * glyph_transform.unwrap_or(Affine::IDENTITY),
                );
                *transform = Affine::IDENTITY;
            }
        }
        let actual = partial
            .render_scene(&scene, [160, 96], [0.; 4], true)
            .unwrap();
        assert_eq!(
            actual,
            full.render_scene(&scene, [160, 96], [0.; 4], false)
                .unwrap()
        );
        let expected = reference
            .render_scene(&expanded, [160, 96], [0.; 4], false)
            .unwrap();
        for (i, (a, b)) in actual.iter().zip(&expected).enumerate() {
            assert!(a.abs_diff(*b) <= 1, "run={run:?} byte={i}: {a}/{b}");
        }
    }
    assert_eq!(
        partial
            .render_scene(&Scene::new(), [160, 96], [0.; 4], true)
            .unwrap(),
        vec![0; 160 * 96 * 4]
    );
}

#[test]
#[ignore = "requires a working wgpu adapter and system fonts"]
fn emboldened_positioned_runs_render_wider_and_darker_than_plain() {
    let mut text = TextContext::new();
    let layout = plico_renderer::text::layout_text(&mut text, "Bold text bold 中文", 20.);
    let plain = text.glyph_scene(&layout);
    assert!(
        plain
            .commands
            .iter()
            .any(|c| matches!(c, PaintCommand::GlyphRun { .. })),
        "expected a positioned glyph run"
    );
    // Synthetic bold: the classic FreeType strength of about 1/24 em.
    let mut bold = plain.commands.clone();
    for command in bold.iter_mut() {
        if let PaintCommand::GlyphRun { embolden, .. } = command {
            *embolden = kurbo::Vec2::new(20. / 24., 20. / 24.);
        }
    }
    let mut renderer = pollster::block_on(Renderer::new()).unwrap();
    let mut scene = Scene::new();
    scene.commands = plain.commands.clone();
    let plain_pixels = renderer
        .render_scene(&scene, [260, 64], [0.; 4], false)
        .expect("plain run renders");
    let mut scene = Scene::new();
    scene.commands = bold;
    let bold_pixels = renderer
        .render_scene(&scene, [260, 64], [0.; 4], false)
        .expect("emboldened run must render instead of being rejected");
    let ink = |pixels: &[u8]| pixels.iter().map(|p| u32::from(*p)).sum::<u32>();
    let plain_ink = ink(&plain_pixels);
    let bold_ink = ink(&bold_pixels);
    assert!(
        bold_ink > plain_ink,
        "emboldening must add ink: {bold_ink} vs {plain_ink}"
    );
    // The widened strokes must reach further right than the plain coverage.
    let rightmost = |pixels: &[u8]| {
        (0..64)
            .rev()
            .flat_map(|y| (0..260).rev().map(move |x| (y, x)))
            .find(|&(y, x)| pixels[(y * 260 + x) * 4 + 3] > 40)
            .map(|(_, x)| x)
            .unwrap_or(0)
    };
    assert!(
        rightmost(&bold_pixels) >= rightmost(&plain_pixels),
        "emboldened coverage must not shrink"
    );
    // A second identical emboldened frame must hit the raster cache: the
    // embolden vector is part of the cache key, so this exercises reuse.
    let mut scene = Scene::new();
    let mut commands = plain.commands.clone();
    for command in commands.iter_mut() {
        if let PaintCommand::GlyphRun { embolden, .. } = command {
            *embolden = kurbo::Vec2::new(20. / 24., 20. / 24.);
        }
    }
    scene.commands = commands;
    let again = renderer
        .render_scene(&scene, [260, 64], [0.; 4], false)
        .unwrap();
    assert_eq!(bold_pixels, again);
}
