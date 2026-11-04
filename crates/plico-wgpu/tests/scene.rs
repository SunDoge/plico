#![cfg(all(feature = "paint-scene", feature = "system-text", target_os = "linux"))]
use kurbo::{Affine, Circle, Rect};
use peniko::{Color as PaintColor, Fill};
use plico::prelude::*;
use plico_renderer::{
    FrameSource, PaintCommand, PaintScene, Scene, TextContext, paragraph::ParagraphRequest,
    style::Paint,
};
use plico_runtime::{NativeView, Source};
use plico_wgpu::{Error, Renderer};
use std::sync::Arc;
struct View {
    value: i32,
}
impl plico::View for View {
    fn theme(&self) -> Theme {
        Theme::light()
    }
    fn show(&mut self, cx: &mut UiContext<'_>) {
        column(cx)
            .padding(16)
            .gap(8)
            .background(Color::Panel)
            .children(|cx| {
                text!(cx, "Count: {}", self.value).font_size(24);
                button(cx).text("Increment").primary();
            });
    }
}
#[test]
#[ignore = "requires a working wgpu adapter and system fonts"]
fn real_source_shared_scene_uses_native_measurement_and_rejects_unsupported_paint() {
    let mut renderer = pollster::block_on(Renderer::new()).unwrap();
    let mut text = TextContext::new();
    renderer.configure_text(&mut text, 1.).unwrap();
    let mut source = Source::with_clock(
        NativeView(View { value: 0 }),
        plico::VirtualClock::default(),
    );
    let mut scene = Scene::new();
    source.build_and_paint(&mut scene, &mut text, 320, 180, 1.);
    assert!(source.last_error.is_none());
    assert!(text.take_native_paragraph_error().is_none());
    assert_eq!(text.cache_entries(), (0, 0), "Parley fallback was used");
    let flat = scene.flattened();
    assert!(
        flat.commands
            .iter()
            .any(|c| matches!(c, PaintCommand::Paragraph { .. }))
    );
    assert!(
        !flat
            .commands
            .iter()
            .any(|c| matches!(c, PaintCommand::GlyphRun { .. }))
    );
    let prepared = renderer.prepare_scene(&scene, [320, 180]).unwrap();
    let repeated = renderer.prepare_scene(&scene, [320, 180]).unwrap();
    assert!(std::rc::Rc::ptr_eq(&prepared, &repeated));
    assert_eq!(renderer.scene_prepare_stats(), (1, 1));
    let before = renderer
        .render_items(&prepared.draws(), [320, 180], [1.; 4], 0.)
        .unwrap();
    source.update(|v| v.0.value = 42);
    source.build_and_paint(&mut scene, &mut text, 320, 180, 1.);
    let prepared = renderer.prepare_scene(&scene, [320, 180]).unwrap();
    let after = renderer
        .render_items(&prepared.draws(), [320, 180], [1.; 4], 0.)
        .unwrap();
    assert_ne!(before, after);
    let generation = text.cache_generation();
    renderer.configure_text(&mut text, 1.25).unwrap();
    assert_ne!(generation, text.cache_generation());
    source.build_and_paint(&mut scene, &mut text, 320, 180, 1.25);
    assert!(text.take_native_paragraph_error().is_none());
    let prepared = renderer.prepare_scene(&scene, [400, 225]).unwrap();
    let dpi = renderer
        .render_items(&prepared.draws(), [400, 225], [1.; 4], 0.)
        .unwrap();
    assert_eq!(dpi.len(), 400 * 225 * 4);
    // A supported command followed by an unsupported one must not silently draw half a frame.
    let mut bad = Scene::new();
    bad.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        PaintColor::WHITE,
        None,
        &Rect::new(0., 0., 10., 10.),
    );
    bad.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        &peniko::Gradient::new_radial((20., 20.), 8.)
            .with_stops([PaintColor::BLACK, PaintColor::WHITE]),
        None,
        &Circle::new((20., 20.), 8.),
    );
    assert!(matches!(
        renderer.prepare_scene(&bad, [320, 180]),
        Err(Error::Unsupported { command: 1, .. })
    ));
    let mut bad = Scene::new();
    bad.commands.push(PaintCommand::PopLayer);
    assert!(renderer.prepare_scene(&bad, [320, 180]).is_err());
    let plain = ParagraphRequest::from_paint(
        &Paint {
            text: Some("tintable".into()),
            ..Default::default()
        },
        None,
    )
    .unwrap();
    let a = text.native_paragraph(Arc::new(plain.clone())).unwrap();
    let mut tinted = plain;
    tinted.color = [255, 0, 0, 255];
    let b = text.native_paragraph(Arc::new(tinted)).unwrap();
    assert!(
        std::rc::Rc::ptr_eq(&a, &b),
        "a tint change reshaped native text"
    );
    let request = Arc::new(
        ParagraphRequest::from_paint(
            &Paint {
                text: Some("unsupported".into()),
                text_max_lines: 1,
                text_clip_lines: true,
                ..Default::default()
            },
            None,
        )
        .unwrap(),
    );
    assert!(text.native_paragraph(request).is_none());
    assert!(text.take_native_paragraph_error().is_some());
}
#[test]
#[ignore = "requires a working wgpu adapter"]
fn shared_rect_clip_keeps_fractional_coverage_and_nested_scopes() {
    let mut renderer = pollster::block_on(Renderer::new()).unwrap();
    let mut scene = Scene::new();
    scene.push_clip_layer(Affine::IDENTITY, &Rect::new(8.25, 0., 24., 32.));
    scene.push_clip_layer(Affine::IDENTITY, &Rect::new(0., 0., 16., 32.));
    scene.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        PaintColor::from_rgb8(255, 0, 0),
        None,
        &Rect::new(0., 0., 32., 32.),
    );
    scene.pop_layer();
    scene.pop_layer();
    let prepared = renderer.prepare_scene(&scene, [32, 32]).unwrap();
    let pixels = renderer
        .render_items(&prepared.draws(), [32, 32], [0.; 4], 0.)
        .unwrap();
    let pixel = |x: usize| &pixels[(10 * 32 + x) * 4..(10 * 32 + x) * 4 + 4];
    assert_eq!(pixel(7), [0; 4]);
    assert!(pixel(8)[3].abs_diff(191) <= 1);
    assert_eq!(pixel(10), [255, 0, 0, 255]);
    assert_eq!(pixel(16), [0; 4]);
}

#[test]
#[ignore = "requires a working wgpu adapter"]
fn shared_gradient_respects_alpha_interpolation_space() {
    let mut renderer = pollster::block_on(Renderer::new()).unwrap();
    let mut gradient = peniko::Gradient::new_linear((0., 0.), (0., 32.)).with_stops([
        PaintColor::from_rgba8(255, 0, 0, 0),
        PaintColor::from_rgba8(0, 0, 255, 255),
    ]);
    let mut samples = Vec::new();
    for space in [
        peniko::InterpolationAlphaSpace::Premultiplied,
        peniko::InterpolationAlphaSpace::Unpremultiplied,
    ] {
        gradient.interpolation_alpha_space = space;
        let mut scene = Scene::new();
        scene.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            &gradient,
            None,
            &Rect::new(0., 0., 32., 32.),
        );
        let prepared = renderer.prepare_scene(&scene, [32, 32]).unwrap();
        let pixels = renderer
            .render_items(&prepared.draws(), [32, 32], [0.; 4], 0.)
            .unwrap();
        samples.push(pixels[(16 * 32 + 16) * 4..(16 * 32 + 16) * 4 + 4].to_vec());
    }
    assert!(samples[0][0] <= 1);
    assert!(samples[0][2].abs_diff(samples[0][3]) <= 1);
    assert!(samples[1][0] > 32);
    assert!(samples[1][2] < samples[1][3]);
}

#[test]
#[ignore = "requires a working wgpu adapter"]
fn prepared_damage_covers_move_removal_and_retained_snapshot_mutation() {
    use plico_renderer::{DamageRegion, damage::SceneDamageTracker};
    let mut renderer = pollster::block_on(Renderer::new()).unwrap();
    let mut tracker = SceneDamageTracker::default();
    tracker.set_multi_rect_damage(true);
    let size = [256, 128];
    let mut scene = Scene::new();
    let append = |scene: &mut Scene, x: f64| {
        scene.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            PaintColor::from_rgba8(200, 30, 60, 128),
            None,
            &Rect::new(x, 10., x + 20., 30.),
        );
    };
    append(&mut scene, 10.);
    let old = renderer.prepare_scene(&scene, size).unwrap();
    assert_eq!(old.update_damage(&mut tracker, size), DamageRegion::Full);
    assert_eq!(old.update_damage(&mut tracker, size), DamageRegion::Empty);
    // Mutating a Scene while a prepared snapshot is retained must use COW.
    scene.commands.clear();
    append(&mut scene, 100.);
    let moved = renderer.prepare_scene(&scene, size).unwrap();
    assert!(!std::rc::Rc::ptr_eq(&old, &moved));
    let damage = moved.update_damage(&mut tracker, size);
    assert!(damage.intersects(DamageRegion::Rect(Rect::new(10., 10., 30., 30.))));
    assert!(damage.intersects(DamageRegion::Rect(Rect::new(100., 10., 120., 30.))));
    assert!(damage.coverage(size[0], size[1]) < 0.2);
    let empty = renderer.prepare_scene(&Scene::new(), size).unwrap();
    let removed = empty.update_damage(&mut tracker, size);
    assert!(removed.intersects(DamageRegion::Rect(Rect::new(100., 10., 120., 30.))));
    assert_ne!(removed, DamageRegion::Full);
    assert_eq!(empty.update_damage(&mut tracker, size), DamageRegion::Empty);
    tracker.invalidate();
    assert_eq!(empty.update_damage(&mut tracker, size), DamageRegion::Full);
    assert_eq!(
        empty.update_damage(&mut tracker, [512, 128]),
        DamageRegion::Full
    );
}

#[test]
#[ignore = "requires a working wgpu adapter and system fonts"]
fn partial_replay_matches_full_for_transparency_text_motion_clips_and_resize() {
    let mut partial = pollster::block_on(Renderer::new()).unwrap();
    let mut full = pollster::block_on(Renderer::new()).unwrap();
    let mut text = TextContext::new();
    partial.configure_text(&mut text, 1.).unwrap();
    full.configure_text(&mut text, 1.).unwrap();
    for frame in 0..12 {
        let size = if frame < 9 { [256, 128] } else { [280, 140] };
        let background = if frame < 7 {
            [0.; 4]
        } else {
            [0.9, 0.8, 0.7, 0.6]
        };
        let mut scene = Scene::new();
        scene.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            PaintColor::from_rgba8(20, 70, 150, 120),
            None,
            &Rect::new(0., 0., 256., 128.),
        );
        if frame != 5 {
            let x = 10. + (frame % 4) as f64 * 33.25;
            scene.fill(
                Fill::NonZero,
                Affine::IDENTITY,
                PaintColor::from_rgba8(200, 40, 20, 100),
                None,
                &Rect::new(x, 10.25, x + 30., 40.5),
            );
        }
        scene.push_clip_layer(
            Affine::IDENTITY,
            &Rect::new(4.25, 3., 220. - frame as f64, 110.),
        );
        let request = ParagraphRequest::from_paint(
            &Paint {
                text: Some(format!("Hello 世界 {}", frame / 3).into()),
                ..Default::default()
            },
            Some(220.),
        )
        .unwrap();
        scene.commands.push(PaintCommand::Paragraph {
            request: Arc::new(request),
            transform: Affine::translate((12.25, 50. + (frame % 3) as f64 * 0.25)),
        });
        scene.pop_layer();
        // Unchanged translucent foreground still contributes over moved content.
        scene.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            PaintColor::from_rgba8(30, 200, 100, 80),
            None,
            &Rect::new(20., 20., 180., 90.),
        );
        let expected = full.render_scene(&scene, size, background, false).unwrap();
        let actual = partial
            .render_scene(&scene, size, background, true)
            .unwrap();
        assert_eq!(actual, expected, "partial mismatch at frame {frame}");
        if ![0, 7, 9].contains(&frame) {
            assert!(
                partial.scene_damage_coverage(size) < 1.,
                "frame {frame} unexpectedly repainted fully"
            );
        }
        assert_eq!(
            partial
                .render_scene(&scene, size, background, true)
                .unwrap(),
            expected,
            "unchanged frame changed pixels"
        );
        assert_eq!(partial.scene_damage_coverage(size), 0.);
        if frame == 6 {
            // A screenshot/raw API may replace the same persistent target.
            // Its next scene frame must be full, even when commands are equal.
            partial
                .render_items(&[], size, [1., 0., 1., 1.], 0.)
                .unwrap();
            assert_eq!(
                partial
                    .render_scene(&scene, size, background, true)
                    .unwrap(),
                expected
            );
            assert!(
                partial
                    .render_scene(&scene, size, [f32::NAN; 4], true)
                    .is_err()
            );
            assert_eq!(
                partial
                    .render_scene(&scene, size, background, true)
                    .unwrap(),
                expected
            );
        }
    }
}

#[test]
#[ignore = "requires a working wgpu adapter"]
fn images_preserve_alpha_channels_painter_order_cache_and_damage() {
    use peniko::{Blob, ImageAlphaType, ImageBrush, ImageData, ImageFormat, ImageQuality};
    let mut renderer = pollster::block_on(Renderer::new()).unwrap();
    let mut oracle = pollster::block_on(Renderer::new()).unwrap();
    for (format, alpha_type, red) in [
        (
            ImageFormat::Rgba8,
            ImageAlphaType::Alpha,
            vec![200, 0, 0, 128],
        ),
        (
            ImageFormat::Bgra8,
            ImageAlphaType::Alpha,
            vec![0, 0, 200, 128],
        ),
        (
            ImageFormat::Rgba8,
            ImageAlphaType::AlphaPremultiplied,
            vec![100, 0, 0, 128],
        ),
    ] {
        let mut pixels = red;
        pixels.extend([0, 255, 0, 255]);
        let image = ImageBrush::new(ImageData {
            data: Blob::new(Arc::new(pixels)),
            format,
            alpha_type,
            width: 2,
            height: 1,
        })
        .with_quality(ImageQuality::Low);
        let blue = ImageBrush::new(ImageData {
            data: Blob::new(Arc::new(vec![0, 0, 255, 255])),
            format: ImageFormat::Rgba8,
            alpha_type: ImageAlphaType::Alpha,
            width: 1,
            height: 1,
        });
        let mut previous_uploads = 0;
        for frame in 0..3 {
            let x = frame as f64 * 8.;
            let mut scene = Scene::new();
            scene.draw_image(
                (&image).into(),
                Affine::translate((x, 0.)) * Affine::scale_non_uniform(16., 24.),
            );
            scene.draw_image(
                (&blue).into(),
                Affine::translate((x + 4., 4.)) * Affine::scale(8.),
            );
            scene.fill(
                Fill::NonZero,
                Affine::IDENTITY,
                PaintColor::from_rgba8(0, 255, 0, 255),
                None,
                &Rect::new(x + 8., 8., x + 10., 10.),
            );
            let actual = renderer
                .render_scene(&scene, [64, 32], [0.; 4], true)
                .unwrap();
            let expected = oracle
                .render_scene(&scene, [64, 32], [0.; 4], false)
                .unwrap();
            assert_eq!(actual, expected);
            let pixel = |px: usize, py: usize| &actual[(py * 64 + px) * 4..(py * 64 + px) * 4 + 4];
            assert_eq!(pixel(x as usize + 2, 2), [100, 0, 0, 128]);
            assert_eq!(pixel(x as usize + 6, 6), [0, 0, 255, 255]);
            assert_eq!(pixel(x as usize + 9, 9), [0, 255, 0, 255]);
            let (_, _, uploads, _) = renderer.image_cache_stats();
            if frame > 0 {
                assert_eq!(uploads, previous_uploads, "unchanged resources reuploaded");
            }
            previous_uploads = uploads;
        }
        let mut crop = Scene::new();
        crop.fill(
            Fill::NonZero,
            Affine::scale_non_uniform(2., 1.),
            &image,
            Some(Affine::translate((-16., 0.)) * Affine::scale(16.)),
            &Rect::new(0., 0., 16., 16.),
        );
        let pixels = renderer
            .render_scene(&crop, [64, 32], [0.; 4], true)
            .unwrap();
        assert_eq!(
            &pixels[(5 * 64 + 12) * 4..(5 * 64 + 12) * 4 + 4],
            [0, 255, 0, 255],
            "brush crop or independent image scale was lost"
        );
        let (_, bytes, _, hits) = renderer.image_cache_stats();
        assert!(bytes <= 128 * 1024 * 1024);
        assert!(hits > 0);
    }
}

#[test]
#[ignore = "requires a working wgpu adapter"]
fn svg_fallback_matches_resvg_and_reuses_raster_across_position_changes() {
    use plico_renderer::{SvgDocument, svg::usvg};
    let document = SvgDocument(Arc::new(usvg::Tree::from_str(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="32" height="32">
        <defs><linearGradient id="g"><stop stop-color="#fc0"/><stop offset="1" stop-color="#08f"/></linearGradient>
        <mask id="m"><rect width="32" height="32" fill="white"/><circle cx="16" cy="16" r="4" fill="black"/></mask></defs>
        <circle cx="16" cy="16" r="14" fill="url(#g)" mask="url(#m)"/>
        <path d="M6 16L12 22L26 8" stroke="#222" stroke-width="2" fill="none"/></svg>"##,
        &usvg::Options::default()).unwrap()));
    let mut renderer = pollster::block_on(Renderer::new()).unwrap();
    for scale in [1., 1.25, 2.] {
        let side = (32. * scale) as u32;
        let mut scene = Scene::new();
        scene.draw_svg(document.clone(), Affine::scale(scale));
        let actual = renderer
            .render_scene(&scene, [side, side], [0.; 4], false)
            .unwrap();
        let mut reference = resvg::tiny_skia::Pixmap::new(side, side).unwrap();
        resvg::render(
            &document.0,
            resvg::tiny_skia::Transform::from_scale(scale as f32, scale as f32),
            &mut reference.as_mut(),
        );
        assert_eq!(actual, reference.data(), "SVG mismatch at scale {scale}");
        let uploads = renderer.image_cache_stats().2;
        let mut moved = Scene::new();
        moved.draw_svg(
            document.clone(),
            Affine::translate((2., 0.)) * Affine::scale(scale),
        );
        renderer
            .render_scene(&moved, [side, side], [0.; 4], true)
            .unwrap();
        assert_eq!(
            renderer.image_cache_stats().2,
            uploads,
            "translation rerasterized SVG"
        );
    }
}

#[test]
#[ignore = "requires a working wgpu adapter"]
fn image_cache_eviction_keeps_in_flight_painter_order_resources_alive() {
    use peniko::{Blob, ImageAlphaType, ImageBrush, ImageData, ImageFormat, ImageQuality};
    let mut renderer = pollster::block_on(Renderer::new()).unwrap();
    let mut scene = Scene::new();
    for x in 0..300_u32 {
        let brush = ImageBrush::new(ImageData {
            data: Blob::new(Arc::new(vec![(x % 251) as u8, 80, 200, 255])),
            format: ImageFormat::Rgba8,
            alpha_type: ImageAlphaType::Alpha,
            width: 1,
            height: 1,
        })
        .with_quality(ImageQuality::Low);
        scene.draw_image((&brush).into(), Affine::translate((x as f64, 0.)));
    }
    let actual = renderer
        .render_scene(&scene, [300, 1], [0.; 4], true)
        .unwrap();
    for x in 0..300_usize {
        assert_eq!(&actual[x * 4..x * 4 + 4], [(x % 251) as u8, 80, 200, 255]);
    }
    let (entries, bytes, uploads, _) = renderer.image_cache_stats();
    assert_eq!((entries, bytes, uploads), (256, 1024, 300));
}

#[test]
#[ignore = "requires a working wgpu adapter"]
fn image_filtering_is_premultiplied_and_invalid_resources_are_rejected() {
    use peniko::{Blob, ImageAlphaType, ImageBrush, ImageData, ImageFormat, ImageQuality};
    let mut renderer = pollster::block_on(Renderer::new()).unwrap();
    let mut brush = ImageBrush::new(ImageData {
        data: Blob::new(Arc::new(vec![200, 0, 0, 128, 0, 255, 0, 0])),
        format: ImageFormat::Rgba8,
        alpha_type: ImageAlphaType::Alpha,
        width: 2,
        height: 1,
    })
    .with_alpha(0.5);
    let mut scene = Scene::new();
    scene.draw_image((&brush).into(), Affine::scale(16.));
    let pixels = renderer
        .render_scene(&scene, [32, 16], [0.; 4], false)
        .unwrap();
    let pixel = |x: usize| &pixels[(8 * 32 + x) * 4..(8 * 32 + x) * 4 + 4];
    assert_eq!(pixel(2), [50, 0, 0, 64]);
    assert_eq!(
        pixel(15)[1],
        0,
        "transparent green contaminated interpolation"
    );
    assert!(pixel(15)[0].abs_diff(27) <= 1);
    assert!(pixel(15)[3].abs_diff(34) <= 1);
    brush.sampler.quality = ImageQuality::High;
    let mut invalid = Scene::new();
    invalid.draw_image((&brush).into(), Affine::IDENTITY);
    let cubic = renderer
        .render_scene(&invalid, [32, 16], [0.; 4], false)
        .unwrap();
    assert!(
        cubic.as_chunks::<4>().0.iter().all(|p| p[1] == 0),
        "transparent green contaminated bicubic interpolation"
    );
    brush.sampler.quality = ImageQuality::Medium;
    brush.image.width = 100;
    let mut invalid = Scene::new();
    invalid.draw_image((&brush).into(), Affine::IDENTITY);
    assert!(
        renderer
            .render_scene(&invalid, [32, 16], [0.; 4], true)
            .is_err()
    );
    assert_eq!(
        renderer
            .render_scene(&scene, [32, 16], [0.; 4], true)
            .unwrap(),
        pixels
    );
}

#[test]
#[ignore = "requires a working wgpu adapter"]
fn general_path_coverage_matches_tiny_skia_and_reuses_integer_motion() {
    use resvg::tiny_skia as sk;
    let mut shape = kurbo::BezPath::new();
    shape.move_to((8., 8.));
    shape.line_to((32., 8.));
    shape.quad_to((48., 8.), (48., 24.));
    shape.curve_to((48., 40.), (8., 40.), (8., 24.));
    shape.close_path();
    shape.move_to((16., 16.));
    shape.line_to((24., 16.));
    shape.line_to((24., 24.));
    shape.line_to((16., 24.));
    shape.close_path();
    let mut builder = sk::PathBuilder::new();
    builder.move_to(8., 8.);
    builder.line_to(32., 8.);
    builder.quad_to(48., 8., 48., 24.);
    builder.cubic_to(48., 40., 8., 40., 8., 24.);
    builder.close();
    builder.move_to(16., 16.);
    builder.line_to(24., 16.);
    builder.line_to(24., 24.);
    builder.line_to(16., 24.);
    builder.close();
    let path = builder.finish().unwrap();
    let mut renderer = pollster::block_on(Renderer::new()).unwrap();
    let mut paint = sk::Paint::default();
    paint.set_color_rgba8(180, 30, 80, 160);
    for scale in [1., 1.25, 2.] {
        let mut uploads = 0;
        for dx in [7.25, 8.25, 8.5] {
            let mut scene = Scene::new();
            scene.fill(
                Fill::EvenOdd,
                Affine::translate((dx, 4.5)) * Affine::scale(scale),
                PaintColor::from_rgba8(180, 30, 80, 160),
                None,
                &shape,
            );
            let pixels = renderer
                .render_scene(&scene, [128, 96], [0.; 4], true)
                .unwrap();
            let mut reference = sk::Pixmap::new(128, 96).unwrap();
            reference.fill_path(
                &path,
                &paint,
                sk::FillRule::EvenOdd,
                sk::Transform::from_row(scale as f32, 0., 0., scale as f32, dx as f32, 4.5),
                None,
            );
            assert_eq!(
                pixels,
                reference.data(),
                "path mismatch at scale={scale} dx={dx}"
            );
            if dx == 8.25 {
                assert_eq!(
                    renderer.image_cache_stats().2,
                    uploads,
                    "integer motion rerasterized path"
                );
            }
            uploads = renderer.image_cache_stats().2;
        }
    }
}

#[test]
#[ignore = "requires a working wgpu adapter"]
fn curved_dashed_stroke_matches_cpu_reference_under_rotation() {
    use resvg::tiny_skia as sk;
    let mut shape = kurbo::BezPath::new();
    shape.move_to((4., 8.));
    shape.curve_to((20., 0.), (30., 32.), (48., 8.));
    let style = kurbo::Stroke::new(3.)
        .with_caps(kurbo::Cap::Round)
        .with_dashes(1., [4., 2.]);
    let transform = Affine::translate((30.25, 15.5)) * Affine::rotate(0.4);
    let mut scene = Scene::new();
    scene.stroke(
        &style,
        transform,
        PaintColor::from_rgba8(40, 80, 220, 180),
        None,
        &shape,
    );
    let mut renderer = pollster::block_on(Renderer::new()).unwrap();
    let actual = renderer
        .render_scene(&scene, [128, 96], [0.; 4], true)
        .unwrap();
    let mut path = sk::PathBuilder::new();
    path.move_to(4., 8.);
    path.cubic_to(20., 0., 30., 32., 48., 8.);
    let mut paint = sk::Paint::default();
    paint.set_color_rgba8(40, 80, 220, 180);
    let stroke = sk::Stroke {
        width: 3.,
        line_cap: sk::LineCap::Round,
        dash: sk::StrokeDash::new(vec![4., 2.], 1.),
        ..Default::default()
    };
    let [a, b, c, d, e, f] = transform.as_coeffs();
    let mut reference = sk::Pixmap::new(128, 96).unwrap();
    reference.stroke_path(
        &path.finish().unwrap(),
        &paint,
        &stroke,
        sk::Transform::from_row(a as f32, b as f32, c as f32, d as f32, e as f32, f as f32),
        None,
    );
    assert!(
        actual
            .iter()
            .zip(reference.data())
            .all(|(a, b)| a.abs_diff(*b) <= 1)
    );
    assert!(actual.iter().any(|v| *v > 0));
}

#[test]
#[ignore = "requires a working wgpu adapter"]
fn nested_path_clip_masks_match_cpu_intersection_and_damage() {
    use resvg::tiny_skia as sk;
    let mut outer = kurbo::BezPath::new();
    outer.move_to((12., 8.));
    outer.line_to((60., 8.));
    outer.quad_to((72., 8.), (72., 20.));
    outer.line_to((72., 60.));
    outer.quad_to((72., 72.), (60., 72.));
    outer.line_to((12., 72.));
    outer.quad_to((0., 72.), (0., 60.));
    outer.line_to((0., 20.));
    outer.quad_to((0., 8.), (12., 8.));
    outer.close_path();
    let mut op = sk::PathBuilder::new();
    op.move_to(12., 8.);
    op.line_to(60., 8.);
    op.quad_to(72., 8., 72., 20.);
    op.line_to(72., 60.);
    op.quad_to(72., 72., 60., 72.);
    op.line_to(12., 72.);
    op.quad_to(0., 72., 0., 60.);
    op.line_to(0., 20.);
    op.quad_to(0., 8., 12., 8.);
    op.close();
    let op = op.finish().unwrap();
    let mut inner = kurbo::BezPath::new();
    inner.move_to((10., 10.));
    inner.line_to((75., 25.));
    inner.line_to((20., 75.));
    inner.close_path();
    let mut ip = sk::PathBuilder::new();
    ip.move_to(10., 10.);
    ip.line_to(75., 25.);
    ip.line_to(20., 75.);
    ip.close();
    let ip = ip.finish().unwrap();
    let mut partial = pollster::block_on(Renderer::new()).unwrap();
    let mut full = pollster::block_on(Renderer::new()).unwrap();
    for frame in 0..4 {
        let dx = frame as f64 + 4.25;
        let mut scene = Scene::new();
        scene.push_clip_layer(Affine::translate((dx, 0.5)), &outer);
        scene.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            PaintColor::from_rgba8(200, 30, 50, 255),
            None,
            &Rect::new(0., 0., 96., 96.),
        );
        scene.push_clip_layer(Affine::translate((0., frame as f64 * 0.25)), &inner);
        scene.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            PaintColor::from_rgba8(20, 80, 220, 255),
            None,
            &Rect::new(0., 0., 96., 96.),
        );
        scene.pop_layer();
        scene.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            PaintColor::from_rgba8(0, 255, 0, 255),
            None,
            &Rect::new(2., 2., 8., 8.),
        );
        scene.pop_layer();
        scene.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            PaintColor::from_rgba8(255, 0, 200, 255),
            None,
            &Rect::new(0., 0., 3., 3.),
        );
        let actual = partial
            .render_scene(&scene, [96, 96], [0.; 4], true)
            .unwrap();
        assert_eq!(
            actual,
            full.render_scene(&scene, [96, 96], [0.; 4], false).unwrap()
        );
        let mut mask = sk::Mask::new(96, 96).unwrap();
        mask.fill_path(
            &op,
            sk::FillRule::Winding,
            true,
            sk::Transform::from_translate(dx as f32, 0.5),
        );
        let mut both = mask.clone();
        both.intersect_path(
            &ip,
            sk::FillRule::Winding,
            true,
            sk::Transform::from_translate(0., frame as f32 * 0.25),
        );
        let mut expected = sk::Pixmap::new(96, 96).unwrap();
        let mut paint = sk::Paint::default();
        paint.set_color_rgba8(200, 30, 50, 255);
        let rect = sk::Rect::from_xywh(0., 0., 96., 96.).unwrap();
        expected.fill_rect(rect, &paint, sk::Transform::identity(), Some(&mask));
        paint.set_color_rgba8(20, 80, 220, 255);
        expected.fill_rect(rect, &paint, sk::Transform::identity(), Some(&both));
        paint.set_color_rgba8(0, 255, 0, 255);
        expected.fill_rect(
            sk::Rect::from_xywh(2., 2., 6., 6.).unwrap(),
            &paint,
            sk::Transform::identity(),
            Some(&mask),
        );
        paint.set_color_rgba8(255, 0, 200, 255);
        expected.fill_rect(
            sk::Rect::from_xywh(0., 0., 3., 3.).unwrap(),
            &paint,
            sk::Transform::identity(),
            None,
        );
        // tiny-skia uses integer source-over while the GPU blends float color
        // into UNORM after each draw. Separate coverage from color quantization.
        for (index, (actual, expected)) in actual.iter().zip(expected.data()).enumerate() {
            let tolerance = if index % 4 == 3 { 1 } else { 2 };
            assert!(
                actual.abs_diff(*expected) <= tolerance,
                "clip oracle mismatch frame={frame} byte={index}: {actual} != {expected}"
            );
        }
    }
}

#[test]
#[ignore = "requires a working wgpu adapter and system fonts"]
fn native_text_respects_nonrectangular_clip_without_whole_layout_rasterization() {
    let mut renderer = pollster::block_on(Renderer::new()).unwrap();
    let mut text = TextContext::new();
    renderer.configure_text(&mut text, 1.).unwrap();
    let mut scene = Scene::new();
    scene.push_clip_layer(Affine::IDENTITY, &Circle::new((32., 32.), 16.));
    let request = ParagraphRequest::from_paint(
        &Paint {
            text: Some("WWWWWWWW".into()),
            font_size: 24.,
            ..Default::default()
        },
        None,
    )
    .unwrap();
    scene.commands.push(PaintCommand::Paragraph {
        request: Arc::new(request),
        transform: Affine::translate((0., 16.)),
    });
    scene.pop_layer();
    let actual = renderer
        .render_scene(&scene, [96, 64], [0.; 4], true)
        .unwrap();
    assert!(actual.iter().any(|v| *v > 0));
    for y in 0..64 {
        for x in 0..96 {
            if !(15..49).contains(&x) || !(15..49).contains(&y) {
                assert_eq!(actual[(y * 96 + x) * 4 + 3], 0);
            }
        }
    }
    assert_eq!(text.cache_entries(), (0, 0));
}

#[test]
#[ignore = "requires a working wgpu adapter"]
fn disjoint_clip_masks_are_empty_and_pop_restores_the_parent() {
    let mut renderer = pollster::block_on(Renderer::new()).unwrap();
    let mut scene = Scene::new();
    scene.push_clip_layer(Affine::IDENTITY, &Circle::new((16., 16.), 8.));
    scene.push_clip_layer(Affine::IDENTITY, &Circle::new((70., 16.), 8.));
    scene.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        PaintColor::WHITE,
        None,
        &Rect::new(0., 0., 96., 32.),
    );
    scene.pop_layer();
    scene.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        PaintColor::from_rgba8(255, 0, 0, 255),
        None,
        &Rect::new(0., 0., 96., 32.),
    );
    scene.pop_layer();
    let pixels = renderer
        .render_scene(&scene, [96, 32], [0.; 4], true)
        .unwrap();
    assert_eq!(
        &pixels[(16 * 96 + 16) * 4..(16 * 96 + 16) * 4 + 4],
        [255, 0, 0, 255]
    );
    assert_eq!(
        &pixels[(16 * 96 + 70) * 4..(16 * 96 + 70) * 4 + 4],
        [0, 0, 0, 0]
    );
}

#[test]
#[ignore = "requires a working wgpu adapter and system fonts"]
fn native_input_uses_one_geometry_for_drawing_editing_and_ime_at_multiple_dpi() {
    use plico_renderer::{
        ImeEvent, KeyEvent, KeyLocation, KeyPhase, Modifiers, SemanticAction, SemanticRole, UiEvent,
    };
    #[derive(Default)]
    struct Form {
        value: String,
    }
    impl plico::View for Form {
        fn theme(&self) -> Theme {
            Theme::light()
        }
        fn show(&mut self, cx: &mut UiContext<'_>) {
            text_input_string(cx, "Native input", &mut self.value);
        }
    }
    let mut renderer = pollster::block_on(Renderer::new()).unwrap();
    for scale in [1., 1.25, 2.] {
        let mut text = TextContext::new();
        renderer.configure_text(&mut text, scale).unwrap();
        let mut source =
            Source::with_clock(NativeView(Form::default()), plico::VirtualClock::default());
        let mut scene = Scene::new();
        source.prepare_for_event(&mut text);
        source
            .render_retained_frame(&mut scene, &mut text, (320, 100), scale)
            .unwrap();
        let size = [(320. * scale) as u32, (100. * scale) as u32];
        renderer.render_scene(&scene, size, [1.; 4], true).unwrap();
        let id = source
            .adapter
            .semantics(&source.engine, 0)
            .node_by_role(SemanticRole::TextInput, "Native input", None)
            .unwrap()
            .id;
        assert!(source.handle_semantic_action(SemanticAction::Focus { target: id }));
        let value = "你好🙂 e\u{301} العربية";
        assert!(source.handle_event(UiEvent::Paste(value.into())).handled);
        source.prepare_for_event(&mut text);
        source
            .render_retained_frame(&mut scene, &mut text, (320, 100), scale)
            .unwrap();
        let mut oracle = pollster::block_on(Renderer::new()).unwrap();
        oracle
            .configure_text(&mut TextContext::new(), scale)
            .unwrap();
        let expected = oracle.render_scene(&scene, size, [1.; 4], false).unwrap();
        let actual = renderer.render_scene(&scene, size, [1.; 4], true).unwrap();
        assert_eq!(actual, expected, "input damage left stale pixels");
        assert_eq!(source.view.0.value, value);
        assert!(scene.flattened().commands.iter().any(|c|matches!(c,PaintCommand::Paragraph { request,.. } if request.text.as_ref()==value)),"committed input not painted");
        assert!(text.take_native_paragraph_error().is_none());
        assert_eq!(text.cache_entries(), (0, 0), "input fell back to Parley");
        assert!(
            !scene
                .flattened()
                .commands
                .iter()
                .any(|c| matches!(c, PaintCommand::GlyphRun { .. }))
        );
        let ime = source.ime_state().unwrap();
        assert_eq!(ime.surrounding_cursor, value.len());
        assert!((ime.cursor_area[2] - ime.cursor_area[0] - 1. / scale).abs() < 0.001);
        let size = [(320. * scale) as u32, (100. * scale) as u32];
        renderer.render_scene(&scene, size, [1.; 4], true).unwrap();
        source.handle_event(UiEvent::Ime(ImeEvent::Preedit {
            text: "にほん".into(),
            cursor: None,
        }));
        source.prepare_for_event(&mut text);
        source
            .render_retained_frame(&mut scene, &mut text, (320, 100), scale)
            .unwrap();
        assert_eq!(source.view.0.value, value, "preedit became committed text");
        assert!(scene.flattened().commands.iter().any(
            |c| matches!(c,PaintCommand::Paragraph {request,..} if request.text.ends_with("にほん"))
        ));
        renderer.render_scene(&scene, size, [1.; 4], true).unwrap();
        source.handle_event(UiEvent::Ime(ImeEvent::Commit("日本".into())));
        source.prepare_for_event(&mut text);
        source
            .render_retained_frame(&mut scene, &mut text, (320, 100), scale)
            .unwrap();
        assert_eq!(source.view.0.value, format!("{value}日本"));
        let key = |value: &str| {
            UiEvent::Key(KeyEvent {
                phase: KeyPhase::Down,
                key: value.into(),
                code: value.into(),
                key_without_modifiers: value.into(),
                text: None,
                text_with_all_modifiers: None,
                location: KeyLocation::Standard,
                modifiers: Modifiers::CONTROL,
                repeat: false,
                synthetic: false,
            })
        };
        source.handle_event(key("a"));
        source.prepare_for_event(&mut text);
        source
            .render_retained_frame(&mut scene, &mut text, (320, 100), scale)
            .unwrap();
        renderer.render_scene(&scene, size, [1.; 4], true).unwrap();
        assert!(
            matches!(source.handle_event(key("c")).clipboard,Some(plico_renderer::ClipboardRequest::Write(text)) if text==format!("{value}日本"))
        );
    }
}

#[test]
#[ignore = "requires a working wgpu adapter and system fonts"]
fn native_password_input_shapes_only_masked_presentation() {
    use plico_renderer::{SemanticAction, SemanticRole, UiEvent};
    struct Form {
        buffer: TextBuffer,
    }
    impl plico::View for Form {
        fn theme(&self) -> Theme {
            Theme::light()
        }
        fn show(&mut self, cx: &mut UiContext<'_>) {
            text_input(cx, "Password", &self.buffer);
        }
    }
    let mut renderer = pollster::block_on(Renderer::new()).unwrap();
    let mut text = TextContext::new();
    renderer.configure_text(&mut text, 1.).unwrap();
    let mut source = Source::with_clock(
        NativeView(Form {
            buffer: TextBuffer::secret(),
        }),
        plico::VirtualClock::default(),
    );
    let mut scene = Scene::new();
    source.build_and_paint(&mut scene, &mut text, 320, 100, 1.);
    let id = source
        .adapter
        .semantics(&source.engine, 0)
        .node_by_role(SemanticRole::TextInput, "Password", None)
        .unwrap()
        .id;
    source.handle_semantic_action(SemanticAction::Focus { target: id });
    source.handle_event(UiEvent::Paste("private🙂value".into()));
    source.build_and_paint(&mut scene, &mut text, 320, 100, 1.);
    let flat = scene.flattened();
    let paragraphs: Vec<_> = flat
        .commands
        .iter()
        .filter_map(|command| match command {
            PaintCommand::Paragraph { request, .. } => Some(request),
            _ => None,
        })
        .collect();
    assert!(!paragraphs.is_empty());
    assert!(
        paragraphs
            .iter()
            .all(|request| request.text.chars().all(|c| c == '•'))
    );
    assert_eq!(source.ime_state().unwrap().surrounding_text, "");
    assert_eq!(text.cache_entries(), (0, 0));
    renderer
        .render_scene(&scene, [320, 100], [1.; 4], true)
        .unwrap();
}
