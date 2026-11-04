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
        PaintColor::BLACK,
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
