#![cfg(feature = "raster")]
use kurbo::{Affine, Rect};
use peniko::Color;
use plico_renderer::{
    Scene, TextContext,
    paragraph::{ParagraphLayout, ParagraphRequest},
    style::Paint,
};
use plico_skia::SkiaSurface;
use std::sync::Arc;

fn request(text: &str, width: Option<f32>) -> Arc<ParagraphRequest> {
    Arc::new(
        ParagraphRequest::from_paint(
            &Paint {
                text: Some(text.into()),
                font_size: 24.,
                wrap_text: true,
                ..Default::default()
            },
            width,
        )
        .unwrap(),
    )
}
#[test]
fn native_paragraph_measures_paints_and_hits_without_parley_layouts() {
    let mut surface = SkiaSurface::new(400, 200).unwrap();
    let mut text = TextContext::new();
    surface.configure_text(&mut text);
    assert_eq!(text.raster_backend_name(), "skparagraph");
    let req = request("Hello 世界 👋", Some(300.));
    let p = text.native_paragraph(req.clone()).unwrap();
    let m = p.metrics();
    assert!(m.width > 20. && m.width <= 300. && m.height > 0.);
    assert!(m.baselines[0].unwrap() > 0.);
    let geometry = ParagraphLayout::Native(p);
    for range in [6..12, 13..17] {
        let rectangles = geometry.selection_rects(range.clone());
        assert!(!rectangles.is_empty());
        for rect in rectangles {
            let byte = geometry.hit_test((rect[0] + rect[2]) / 2., (rect[1] + rect[3]) / 2.);
            assert!(req.text.is_char_boundary(byte));
            assert!(
                (range.start..=range.end).contains(&byte),
                "{range:?}: {byte}"
            );
        }
    }
    let mut scene = Scene::new();
    scene
        .commands
        .push(plico_renderer::PaintCommand::Paragraph {
            request: req,
            transform: Affine::translate((10., 10.)),
        });
    surface.draw_scene(&scene, Color::WHITE).unwrap();
    assert!(
        surface
            .read_pixels()
            .unwrap()
            .as_chunks::<4>()
            .0
            .iter()
            .any(|p| *p != [255, 255, 255, 255])
    );
    assert_eq!(
        text.cache_entries(),
        (0, 0),
        "ordinary Skia paragraphs must not shape via Parley"
    );
}
#[test]
fn native_wrapping_ellipsis_and_cached_geometry_agree() {
    let surface = SkiaSurface::new(300, 300).unwrap();
    let mut text = TextContext::new();
    surface.configure_text(&mut text);
    let req = request("one two three four five six seven eight", Some(80.));
    let first = text.native_paragraph(req.clone()).unwrap();
    let again = text.native_paragraph(req.clone()).unwrap();
    assert!(std::rc::Rc::ptr_eq(&first, &again));
    assert!(first.metrics().height > 50.);
    let mut clipped = (*req).clone();
    clipped.max_lines = 1;
    clipped.ellipsis = true;
    let clipped = text.native_paragraph(Arc::new(clipped)).unwrap();
    assert!(clipped.metrics().height < first.metrics().height);
    assert!(clipped.metrics().width <= 81.);
}
#[test]
fn native_paragraph_damage_keeps_pixels_outside_clip_unchanged() {
    let mut surface = SkiaSurface::new(400, 200).unwrap();
    let mut text = TextContext::new();
    surface.configure_text(&mut text);
    let mut scene = Scene::new();
    scene
        .commands
        .push(plico_renderer::PaintCommand::Paragraph {
            request: request("moving text", Some(300.)),
            transform: Affine::translate((10., 10.)),
        });
    surface.draw_scene(&scene, Color::WHITE).unwrap();
    let before = surface.read_pixels().unwrap();
    surface
        .draw_scene_damaged(
            &Scene::new(),
            Color::WHITE,
            plico_renderer::DamageRegion::Rect(Rect::new(0., 0., 100., 100.)),
        )
        .unwrap();
    let after = surface.read_pixels().unwrap();
    for (i, (a, b)) in before
        .as_chunks::<4>()
        .0
        .iter()
        .zip(after.as_chunks::<4>().0.iter())
        .enumerate()
    {
        if i % 400 >= 100 || i / 400 >= 100 {
            assert_eq!(a, b);
        }
    }
}

#[test]
fn taffy_measure_baseline_and_scene_all_use_native_paragraphs() {
    let mut surface = SkiaSurface::new(400, 300).unwrap();
    let mut text = TextContext::new();
    surface.configure_text(&mut text);
    let mut tree = taffy::TaffyTree::<Paint>::new();
    let root = tree
        .new_leaf_with_context(
            taffy::Style {
                size: taffy::Size {
                    width: taffy::Dimension::length(140.),
                    height: taffy::Dimension::auto(),
                },
                ..Default::default()
            },
            Paint {
                text: Some("Native paragraph wraps into several lines 世界".into()),
                font_size: 24.,
                wrap_text: true,
                ..Default::default()
            },
        )
        .unwrap();
    let nodes = plico_renderer::layout::compute_and_walk_with_scroll(
        &mut tree,
        root,
        400.,
        300.,
        &mut text,
        &Default::default(),
    );
    assert_eq!(nodes.len(), 1);
    let p = plico_renderer::scene::layout_node_paragraph(&mut text, &nodes[0]).unwrap();
    assert!(matches!(p, ParagraphLayout::Native(_)));
    assert!((nodes[0].content_size[1] - p.metrics().height).abs() <= 1.);
    let mut scene = Scene::new();
    plico_renderer::scene::build_scene_scaled(
        &mut scene,
        &nodes,
        &mut text,
        400,
        300,
        Color::WHITE,
        1.75,
    );
    assert!(
        scene
            .commands
            .iter()
            .any(|c| matches!(c, plico_renderer::PaintCommand::Paragraph { .. }))
    );
    assert!(
        !scene
            .commands
            .iter()
            .any(|c| matches!(c, plico_renderer::PaintCommand::GlyphRun { .. }))
    );
    surface.draw_scene(&scene, Color::WHITE).unwrap();
    assert_eq!(text.cache_entries(), (0, 0));
}

#[test]
fn native_ink_damage_matches_full_redraw_across_fractional_motion_and_removal() {
    let (mut tracked, mut full) = (
        SkiaSurface::new(400, 200).unwrap(),
        SkiaSurface::new(400, 200).unwrap(),
    );
    let mut text = TextContext::new();
    tracked.configure_text(&mut text);
    for i in 0..60 {
        let mut scene = Scene::new();
        if i % 7 != 0 {
            scene
                .commands
                .push(plico_renderer::PaintCommand::Paragraph {
                    request: request("sWjg 世界 👋", Some(260.)),
                    transform: Affine::translate((5. + i as f64 * 0.25, 10.)) * Affine::scale(1.25),
                });
        }
        tracked.draw_scene_tracked(&scene, Color::WHITE).unwrap();
        full.draw_scene(&scene, Color::WHITE).unwrap();
        let same = tracked.read_pixels().unwrap() == full.read_pixels().unwrap();
        assert!(same, "native paragraph damage frame {i}");
    }
}

#[test]
fn font_registered_before_native_engine_install_is_available_and_cached() {
    let surface = SkiaSurface::new(300, 100).unwrap();
    let mut text = TextContext::new();
    let asset = plico_renderer::text::FontAsset::from_static(include_bytes!(
        "../../../backend/examples/web-preview/public/DejaVuSans.ttf"
    ))
    .with_family("PlicoParagraphFixture");
    let paint = Paint {
        text: Some("font revision".into()),
        ..Default::default()
    };
    let before = text.paragraph_request(&paint, None).unwrap();
    assert!(text.register_font(&asset));
    let after = text.paragraph_request(&paint, None).unwrap();
    assert_ne!(before.fingerprint(), after.fingerprint());
    surface.configure_text(&mut text);
    let mut req = (*request("Fixture text", None)).clone();
    req.family = "PlicoParagraphFixture".into();
    let req = Arc::new(req);
    let first = text.native_paragraph(req.clone()).unwrap();
    assert!(first.metrics().width > 10.);
    assert!(!text.register_font(&asset));
    assert!(std::rc::Rc::ptr_eq(
        &first,
        &text.native_paragraph(req).unwrap()
    ));
}

#[test]
fn native_runtime_selection_drag_and_copy_use_painted_geometry() {
    use plico_core::prelude::*;
    use plico_renderer::{
        ClipboardRequest, FrameSource, KeyEvent, KeyLocation, KeyPhase, Modifiers, Point,
        PointerButton, PointerEvent, PointerPhase, PointerProperties, UiEvent,
    };
    struct Document;
    impl plico_runtime::View for Document {
        fn show(&mut self, cx: &mut UiContext) {
            column(cx)
                .width(280.)
                .selectable(true)
                .gap(12)
                .children(|cx| {
                    text(cx, "First 中文 paragraph").width_percent(100.);
                    text(cx, "Second 👋 paragraph").width_percent(100.);
                });
        }
    }
    let mut source = plico_runtime::Source::new(Document);
    let mut tcx = TextContext::new();
    SkiaSurface::new(400, 200).unwrap().configure_text(&mut tcx);
    source.build_frame(&mut tcx, 400, 200);
    assert!(source.last_error.is_none());
    let origins: Vec<_> = source
        .adapter
        .placed
        .iter()
        .filter(|p| p.paint.text.is_some())
        .map(|p| {
            [
                p.content_origin[0] as f64 + 1.,
                p.content_origin[1] as f64 + 5.,
            ]
        })
        .collect();
    assert_eq!(origins.len(), 2);
    for (phase, pos) in [
        (PointerPhase::Down, origins[0]),
        (PointerPhase::Move, [origins[1][0] + 60., origins[1][1]]),
        (PointerPhase::Up, [origins[1][0] + 60., origins[1][1]]),
    ] {
        source.handle_event(UiEvent::Pointer(PointerEvent {
            phase,
            position: Point {
                x: pos[0],
                y: pos[1],
            },
            button: Some(PointerButton::Primary),
            buttons: 1,
            modifiers: Modifiers::empty(),
            properties: PointerProperties::default(),
        }));
    }
    source.build_frame(&mut tcx, 400, 200);
    assert!(
        source
            .adapter
            .selected_text()
            .unwrap()
            .starts_with("First 中文 paragraph\n\n")
    );
    let key = |key: &str| {
        UiEvent::Key(KeyEvent {
            phase: KeyPhase::Down,
            key: key.into(),
            key_without_modifiers: key.into(),
            code: key.into(),
            text: None,
            text_with_all_modifiers: None,
            location: KeyLocation::Standard,
            modifiers: Modifiers::CONTROL,
            repeat: false,
            synthetic: false,
        })
    };
    source.handle_event(key("a"));
    source.build_frame(&mut tcx, 400, 200);
    assert_eq!(
        source.handle_event(key("c")).clipboard,
        Some(ClipboardRequest::Write(
            "First 中文 paragraph\n\nSecond 👋 paragraph".into()
        ))
    );
    assert_eq!(tcx.cache_entries(), (0, 0));
}
