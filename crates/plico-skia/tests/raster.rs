#![cfg(feature = "raster")]
use kurbo::{Affine, Rect};
use peniko::{Color, Fill};
use plico_renderer::{PaintScene, Scene};
use plico_skia::{Error, SkiaSurface};

fn pixel(bytes: &[u8], x: usize, y: usize) -> &[u8] {
    &bytes[(y * 16 + x) * 4..(y * 16 + x + 1) * 4]
}

#[test]
fn transformed_clips_and_frame_reset_preserve_pixels() {
    let mut surface = SkiaSurface::new(16, 16).unwrap();
    let mut scene = Scene::new();
    scene.push_clip_layer(Affine::translate((4., 4.)), &Rect::new(0., 0., 4., 4.));
    scene.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        Color::from_rgb8(255, 0, 0),
        None,
        &Rect::new(0., 0., 16., 16.),
    );
    scene.pop_layer();
    let bytes = surface.render_scene(&scene, Color::WHITE).unwrap();
    assert_eq!(pixel(&bytes, 5, 5), [255, 0, 0, 255]);
    assert_eq!(pixel(&bytes, 2, 2), [255; 4]);
    let bytes = surface
        .render_scene(&Scene::new(), Color::TRANSPARENT)
        .unwrap();
    assert!(
        bytes.iter().all(|v| *v == 0),
        "previous frame must be cleared"
    );
}

#[test]
fn failures_are_explicit_and_do_not_poison_the_next_frame() {
    let mut surface = SkiaSurface::new(16, 16).unwrap();
    let mut scene = Scene::new();
    scene.push_clip_layer(Affine::IDENTITY, &Rect::new(0., 0., 1., 1.));
    scene.push_layer(
        peniko::BlendMode::default(),
        1.,
        Affine::IDENTITY,
        &Rect::new(0., 0., 1., 1.),
        Some(std::sync::Arc::new(
            vello_common::filter_effects::Filter::from_function(
                vello_common::filter_effects::FilterFunction::Blur { radius: 1. },
            ),
        )),
        None,
    );
    assert!(matches!(
        surface.render_scene(&scene, Color::WHITE),
        Err(Error::Unsupported {
            command: "layer filter"
        })
    ));
    assert!(matches!(
        surface.write_png(std::path::Path::new("must-not-exist.png")),
        Err(Error::NoFrame)
    ));
    let bytes = surface.render_scene(&Scene::new(), Color::WHITE).unwrap();
    assert_eq!(pixel(&bytes, 15, 15), [255; 4]);
    let mut unbalanced = Scene::new();
    unbalanced.pop_layer();
    assert!(matches!(
        surface.render_scene(&unbalanced, Color::WHITE),
        Err(Error::ClipStack)
    ));
    let mut unclosed = Scene::new();
    unclosed.push_clip_layer(Affine::IDENTITY, &Rect::new(0., 0., 2., 2.));
    assert!(matches!(
        surface.render_scene(&unclosed, Color::WHITE),
        Err(Error::ClipStack)
    ));
}

#[test]
fn invalid_dimensions_are_rejected_before_allocation() {
    for (width, height) in [(0, 16), (16, 0), (u32::MAX, 1), (1, u32::MAX)] {
        assert!(matches!(
            SkiaSurface::new(width, height),
            Err(Error::InvalidSize { .. })
        ));
    }
}

#[test]
fn fill_transform_and_premultiplied_rgba_are_preserved() {
    let mut scene = Scene::new();
    scene.fill(
        Fill::NonZero,
        Affine::translate((4., 6.)),
        Color::from_rgba8(200, 100, 50, 128),
        None,
        &Rect::new(0., 0., 3., 3.),
    );
    let mut surface = SkiaSurface::new(16, 16).unwrap();
    let bytes = surface.render_scene(&scene, Color::TRANSPARENT).unwrap();
    assert_eq!(pixel(&bytes, 0, 0), [0; 4]);
    for (actual, expected) in pixel(&bytes, 5, 7).iter().zip([100u8, 50, 25, 128]) {
        assert!(actual.abs_diff(expected) <= 1, "{actual} != {expected}");
    }
}

#[test]
fn stroke_and_isolated_alpha_layer_preserve_coverage() {
    use kurbo::{Line, Stroke};
    use peniko::BlendMode;
    let mut scene = Scene::new();
    scene.push_layer(
        BlendMode::default(),
        0.5,
        Affine::IDENTITY,
        &Rect::new(0., 0., 8., 16.),
        None,
        None,
    );
    scene.stroke(
        &Stroke::new(4.),
        Affine::IDENTITY,
        Color::BLACK,
        None,
        &Line::new((0., 8.), (16., 8.)),
    );
    scene.pop_layer();
    let mut surface = SkiaSurface::new(16, 16).unwrap();
    let bytes = surface.render_scene(&scene, Color::WHITE).unwrap();
    for channel in &pixel(&bytes, 4, 8)[..3] {
        assert!(channel.abs_diff(127) <= 1);
    }
    assert_eq!(pixel(&bytes, 12, 8), [255; 4]);
    assert_eq!(pixel(&bytes, 4, 2), [255; 4]);
}

#[test]
fn svg_transform_opacity_and_rejection_are_explicit() {
    use plico_renderer::svg::SvgImage;
    let icon = SvgImage::parse(r#"<svg xmlns="http://www.w3.org/2000/svg" width="8" height="8"><g opacity="0.5"><path d="M1 4H7" fill="none" stroke="red" stroke-width="2"/></g></svg>"#).unwrap();
    let mut scene = Scene::new();
    scene.draw_svg(icon.document().clone(), Affine::translate((4., 4.)));
    let mut surface = SkiaSurface::new(16, 16).unwrap();
    for _ in 0..2 {
        let pixels = surface.render_scene(&scene, Color::WHITE).unwrap();
        let p = pixel(&pixels, 8, 8);
        assert_eq!(p[0], 255);
        assert!(p[1].abs_diff(127) <= 1 && p[2].abs_diff(127) <= 1);
        assert_eq!(pixel(&pixels, 2, 8), [255; 4]);
    }
    let icon = SvgImage::parse(r#"<svg xmlns="http://www.w3.org/2000/svg" width="8" height="8"><defs><pattern id="p" width="4" height="4" patternUnits="userSpaceOnUse"><rect width="4" height="4" fill="red"/></pattern></defs><rect width="8" height="8" fill="url(#p)"/></svg>"#).unwrap();
    let mut scene = Scene::new();
    scene.draw_svg(icon.document().clone(), Affine::IDENTITY);
    assert!(matches!(
        surface.render_scene(&scene, Color::WHITE),
        Err(Error::Unsupported {
            command: "SVG pattern"
        })
    ));
    assert!(matches!(surface.read_pixels(), Err(Error::NoFrame)));
}

#[test]
fn box_shadow_blurs_matches_solid_at_zero_std_dev_and_follows_transform() {
    // A zero-blur shadow must equal the solid rounded rect it stands for.
    let mut solid = Scene::new();
    solid.draw_box_shadow(
        Affine::IDENTITY,
        Rect::new(4., 4., 12., 12.),
        Color::BLACK,
        1.,
        0.,
    );
    let mut surface = SkiaSurface::new(16, 16).unwrap();
    let bytes = surface.render_scene(&solid, Color::WHITE).unwrap();
    assert_eq!(pixel(&bytes, 8, 8), [0, 0, 0, 255]);
    assert_eq!(pixel(&bytes, 2, 2), [255; 4]);

    // Gaussian falloff: the centre stays covered, pixels two sigma past the
    // rect match the background, and the edge in between fades.
    let mut blurred = Scene::new();
    blurred.draw_box_shadow(
        Affine::IDENTITY,
        Rect::new(4., 4., 12., 12.),
        Color::BLACK,
        1.,
        1.,
    );
    let bytes = surface.render_scene(&blurred, Color::WHITE).unwrap();
    assert!(
        pixel(&bytes, 8, 8)[0] <= 40,
        "centre must stay covered: {:?}",
        pixel(&bytes, 8, 8)
    );
    let centre = pixel(&bytes, 8, 8)[0];
    let near_edge = pixel(&bytes, 6, 8)[0];
    let far = pixel(&bytes, 14, 8)[0];
    assert!(
        near_edge >= centre,
        "coverage must fall off away from the shape"
    );
    assert!(
        far >= 240,
        "two sigma past the rect is the background: {far}"
    );
    assert!(far > near_edge, "falloff must be visible");

    // The transform positions the shadow; a translated shadow keeps its shape.
    let mut moved = Scene::new();
    moved.draw_box_shadow(
        Affine::translate((2., 2.)),
        Rect::new(4., 4., 12., 12.),
        Color::BLACK,
        1.,
        1.,
    );
    let bytes = surface.render_scene(&moved, Color::WHITE).unwrap();
    assert!(
        pixel(&bytes, 8, 8)[0] <= 40,
        "translated centre: {:?}",
        pixel(&bytes, 8, 8)
    );
    assert!(
        pixel(&bytes, 3, 8)[0] >= 240,
        "old centre fades out: {:?}",
        pixel(&bytes, 3, 8)
    );
}

#[test]
fn svg_gradients_preserve_color_opacity_and_local_transforms() {
    use plico_renderer::svg::SvgImage;
    let icon = SvgImage::parse(r#"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16"><defs><linearGradient id="g" gradientUnits="userSpaceOnUse" x1="0" x2="8" gradientTransform="translate(2 0)"><stop stop-color="red"/><stop offset="1" stop-color="blue"/></linearGradient></defs><g transform="translate(2 2)"><rect x="2" width="8" height="8" fill="url(#g)" fill-opacity="0.5"/></g></svg>"#).unwrap();
    let mut scene = Scene::new();
    scene.draw_svg(icon.document().clone(), Affine::IDENTITY);
    let mut surface = SkiaSurface::new(16, 16).unwrap();
    let bytes = surface.render_scene(&scene, Color::WHITE).unwrap();
    let left = pixel(&bytes, 4, 5);
    let right = pixel(&bytes, 11, 5);
    assert!(left[0] > left[2] + 80, "{left:?}");
    assert!(right[2] > right[0] + 80, "{right:?}");
    assert!(left[1].abs_diff(127) <= 1);
    assert_eq!(pixel(&bytes, 1, 5), [255; 4]);

    let icon = SvgImage::parse(r#"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16"><defs><radialGradient id="r" cx=".5" cy=".5" r=".5" fx=".25" fy=".5"><stop stop-color="red"/><stop offset="1" stop-color="blue"/></radialGradient></defs><rect width="16" height="16" fill="url(#r)"/></svg>"#).unwrap();
    scene.reset();
    scene.draw_svg(icon.document().clone(), Affine::IDENTITY);
    let bytes = surface.render_scene(&scene, Color::WHITE).unwrap();
    let focus = pixel(&bytes, 4, 8);
    let edge = pixel(&bytes, 15, 8);
    assert!(focus[0] > focus[2] + 150, "{focus:?}");
    assert!(edge[2] > edge[0] + 150, "{edge:?}");
}

#[test]
fn raster_images_preserve_transform_clip_alpha_and_format_changes() {
    use peniko::{Blob, ImageAlphaType, ImageBrush, ImageData, ImageFormat, ImageQuality};
    use std::sync::Arc;
    let data = ImageData {
        data: Blob::new(Arc::new(vec![255u8, 0, 0, 128])),
        format: ImageFormat::Rgba8,
        alpha_type: ImageAlphaType::Alpha,
        width: 1,
        height: 1,
    };
    let mut brush = ImageBrush::new(data);
    brush.sampler.quality = ImageQuality::Low;
    let mut surface = SkiaSurface::new(16, 16).unwrap();
    let make_scene = |brush: &ImageBrush| {
        let mut scene = Scene::new();
        scene.push_clip_layer(Affine::IDENTITY, &Rect::new(6., 6., 10., 10.));
        scene.draw_image(
            brush.into(),
            Affine::translate((4., 4.)) * Affine::scale(8.),
        );
        scene.pop_layer();
        scene
    };
    let bytes = surface
        .render_scene(&make_scene(&brush), Color::WHITE)
        .unwrap();
    assert_eq!(pixel(&bytes, 7, 7), [255, 127, 127, 255]);
    assert_eq!(pixel(&bytes, 5, 5), [255; 4]);
    // Same pixel Blob with another format must not reuse the RGBA interpretation.
    brush.image.format = ImageFormat::Bgra8;
    let bytes = surface
        .render_scene(&make_scene(&brush), Color::WHITE)
        .unwrap();
    assert_eq!(pixel(&bytes, 7, 7), [127, 127, 255, 255]);
    brush.sampler.alpha = 0.5;
    let bytes = surface
        .render_scene(&make_scene(&brush), Color::WHITE)
        .unwrap();
    assert!(pixel(&bytes, 7, 7)[0].abs_diff(191) <= 1);
    assert_eq!(pixel(&bytes, 7, 7)[2], 255);
}

#[test]
fn damaged_draw_matches_full_draw_for_moving_overlapping_content() {
    use plico_renderer::DamageRegion;
    let make_scene = |x: f64| {
        let mut scene = Scene::new();
        scene.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            Color::from_rgb8(0, 120, 255),
            None,
            &Rect::new(x, 2., x + 4., 10.),
        );
        scene.push_layer(
            peniko::BlendMode::default(),
            0.5,
            Affine::IDENTITY,
            &Rect::new(0., 0., 16., 16.),
            None,
            None,
        );
        scene.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            Color::from_rgb8(255, 0, 0),
            None,
            &Rect::new(4., 4., 14., 12.),
        );
        scene.pop_layer();
        scene
    };
    let mut partial = SkiaSurface::new(16, 16).unwrap();
    let mut oracle = SkiaSurface::new(16, 16).unwrap();
    let before = partial.render_scene(&make_scene(2.), Color::WHITE).unwrap();
    let scene = make_scene(6.);
    // Includes the old location as well as the new location.
    partial
        .draw_scene_damaged(
            &scene,
            Color::WHITE,
            DamageRegion::Rect(Rect::new(2., 2., 10., 10.)),
        )
        .unwrap();
    let after = partial.read_pixels().unwrap();
    assert_eq!(after, oracle.render_scene(&scene, Color::WHITE).unwrap());
    for y in 0..16 {
        for x in 0..16 {
            if !(2..10).contains(&x) || !(2..10).contains(&y) {
                assert_eq!(pixel(&before, x, y), pixel(&after, x, y));
            }
        }
    }
}

#[test]
fn damage_falls_back_on_initial_frame_background_change_and_invalid_bounds() {
    use plico_renderer::DamageRegion;
    let mut surface = SkiaSurface::new(16, 16).unwrap();
    let scene = Scene::new();
    surface
        .draw_scene_damaged(&scene, Color::WHITE, DamageRegion::Empty)
        .unwrap();
    assert!(surface.read_pixels().unwrap().iter().all(|v| *v == 255));
    surface
        .draw_scene_damaged(&scene, Color::TRANSPARENT, DamageRegion::Empty)
        .unwrap();
    assert!(surface.read_pixels().unwrap().iter().all(|v| *v == 0));
    surface
        .draw_scene_damaged(
            &scene,
            Color::WHITE,
            DamageRegion::Rect(Rect::new(f64::NAN, 0., 1., 1.)),
        )
        .unwrap();
    assert!(surface.read_pixels().unwrap().iter().all(|v| *v == 255));
    let before = surface.read_pixels().unwrap();
    surface
        .draw_scene_damaged(
            &scene,
            Color::WHITE,
            DamageRegion::Rect(Rect::new(20., 20., 25., 25.)),
        )
        .unwrap();
    assert_eq!(surface.read_pixels().unwrap(), before);
}

#[test]
fn tracked_targets_match_full_redraw_across_reordered_acquisition_and_removal() {
    use plico_renderer::DamageRegion;
    let mut targets: Vec<_> = (0..3).map(|_| SkiaSurface::new(32, 32).unwrap()).collect();
    let mut oracle = SkiaSurface::new(32, 32).unwrap();
    let mut partial_frames = 0;
    for frame in 0..120 {
        let mut scene = Scene::new();
        let x = (frame % 8) as f64;
        if frame % 13 != 0 {
            scene.fill(
                Fill::NonZero,
                Affine::translate((x, 0.)),
                Color::from_rgb8(0, 140, 255),
                None,
                &Rect::new(0., 2., 4., 8.),
            );
        }
        scene.push_clip_layer(Affine::IDENTITY, &Rect::new(2., 1., 15., 14.));
        scene.push_layer(
            peniko::BlendMode::default(),
            if frame % 9 == 0 { 0.2 } else { 0.6 },
            Affine::IDENTITY,
            &Rect::new(3., 3., 12., 12.),
            None,
            None,
        );
        scene.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            Color::from_rgb8(240, 80, 40),
            None,
            &Rect::new(0., 0., 16., 16.),
        );
        scene.pop_layer();
        scene.pop_layer();
        let background = if frame < 60 {
            Color::WHITE
        } else {
            Color::TRANSPARENT
        };
        // Reacquisition is deliberately neither round-robin nor every frame.
        let target = &mut targets[[0, 2, 2, 1, 0, 1, 2][frame % 7]];
        let damage = target.draw_scene_tracked(&scene, background).unwrap();
        partial_frames += usize::from(matches!(damage, DamageRegion::Rect(_)));
        assert_eq!(
            target.read_pixels().unwrap(),
            oracle.render_scene(&scene, background).unwrap(),
            "frame {frame}, damage {damage:?}"
        );
        assert_eq!(
            target.draw_scene_tracked(&scene, background).unwrap(),
            DamageRegion::Empty
        );
    }
    assert!(
        partial_frames > 50,
        "must actually exercise partial drawing"
    );
}

#[test]
fn explicit_draw_invalidates_automatic_damage_history() {
    use plico_renderer::DamageRegion;
    let mut surface = SkiaSurface::new(16, 16).unwrap();
    let scene = Scene::new();
    assert_eq!(
        surface.draw_scene_tracked(&scene, Color::WHITE).unwrap(),
        DamageRegion::Full
    );
    assert_eq!(
        surface.draw_scene_tracked(&scene, Color::WHITE).unwrap(),
        DamageRegion::Empty
    );
    let mut overlay = Scene::new();
    overlay.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        Color::BLACK,
        None,
        &Rect::new(0., 0., 16., 16.),
    );
    surface.draw_scene(&overlay, Color::WHITE).unwrap();
    assert_eq!(
        surface.draw_scene_tracked(&scene, Color::WHITE).unwrap(),
        DamageRegion::Full
    );
    assert!(surface.read_pixels().unwrap().iter().all(|v| *v == 255));
}

#[test]
fn tracked_text_fractional_motion_color_and_strokes_match_full_pixels() {
    use peniko::{Blob, FontData, Style};
    use plico_renderer::{DamageRegion, Glyph};
    let font = FontData::new(
        Blob::new(std::sync::Arc::new(
            include_bytes!("../../../backend/examples/web-preview/public/DejaVuSans.ttf").to_vec(),
        )),
        0,
    );
    let face = skia_safe::FontMgr::new()
        .new_from_bytes(font.data.as_ref(), 0)
        .unwrap();
    let skfont = skia_safe::Font::new(face, 24.);
    let ids = skfont.text_to_glyphs_vec("sWjg");
    let mut partial = SkiaSurface::new(96, 80).unwrap();
    let mut full = SkiaSurface::new(96, 80).unwrap();
    let mut small_frames = 0;
    for frame in 0..80 {
        let mut scene = Scene::new();
        if frame % 19 != 0 {
            scene.draw_glyphs(
                &font,
                24.,
                frame % 2 == 0,
                &[],
                Default::default(),
                &Style::Fill(Fill::NonZero),
                if frame % 3 == 0 {
                    Color::BLACK
                } else {
                    Color::from_rgb8(80, 120, 240)
                },
                1.,
                Affine::translate(((frame % 16) as f64 * 0.25, 0.)),
                None,
                ids.iter().enumerate().map(|(i, id)| Glyph {
                    id: *id as u32,
                    x: i as f32 * 16.,
                    y: 40.,
                }),
            );
        }
        scene.stroke(
            &kurbo::Stroke::new(2.).with_join(kurbo::Join::Miter),
            Affine::translate(((frame % 9) as f64 * 0.5, 0.)),
            Color::from_rgba8(240, 50, 70, 128),
            None,
            &Rect::new(50., 15., 70., 55.),
        );
        let damage = partial.draw_scene_tracked(&scene, Color::WHITE).unwrap();
        small_frames += usize::from(matches!(damage, DamageRegion::Rect(_)));
        assert_eq!(
            partial.read_pixels().unwrap(),
            full.render_scene(&scene, Color::WHITE).unwrap(),
            "frame {frame}, damage={damage:?}"
        );
    }
    assert!(small_frames > 60);
}

#[test]
fn tracked_failure_forces_full_recovery_and_dense_cooldown_is_bounded() {
    use plico_renderer::{DamageRegion, PaintCommand};
    let mut partial = SkiaSurface::new(16, 16).unwrap();
    let mut full = SkiaSurface::new(16, 16).unwrap();
    partial
        .draw_scene_tracked(&Scene::new(), Color::WHITE)
        .unwrap();
    let mut invalid = Scene::new();
    invalid.commands.push(PaintCommand::PopLayer);
    assert!(matches!(
        partial.draw_scene_tracked(&invalid, Color::WHITE),
        Err(Error::ClipStack)
    ));
    assert_eq!(
        partial
            .draw_scene_tracked(&Scene::new(), Color::WHITE)
            .unwrap(),
        DamageRegion::Full
    );
    let mut recovered_partial = false;
    for frame in 0..30 {
        let mut scene = Scene::new();
        let r = if frame < 4 {
            Rect::new(0., 0., 16., 16.)
        } else {
            Rect::new(4., 4., 8., 8.)
        };
        scene.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            if frame % 2 == 0 {
                Color::BLACK
            } else {
                Color::from_rgb8(240, 80, 40)
            },
            None,
            &r,
        );
        let damage = partial.draw_scene_tracked(&scene, Color::WHITE).unwrap();
        if frame >= 15 {
            recovered_partial |= matches!(damage, DamageRegion::Rect(_));
        }
        assert_eq!(
            partial.read_pixels().unwrap(),
            full.render_scene(&scene, Color::WHITE).unwrap()
        );
    }
    assert!(
        recovered_partial,
        "dense fallback must periodically probe and recover sparse drawing"
    );
}

#[test]
fn culling_nested_scopes_and_clipped_shadows_preserves_other_islands() {
    let mut partial = SkiaSurface::new(96, 64).unwrap();
    let mut full = SkiaSurface::new(96, 64).unwrap();
    for frame in 0..30 {
        let mut scene = Scene::new();
        scene.fill(
            Fill::NonZero,
            Affine::translate(((frame % 12) as f64 * 0.25, 0.)),
            Color::BLACK,
            None,
            &Rect::new(4., 4., 16., 16.),
        );
        scene.push_clip_layer(Affine::IDENTITY, &Rect::new(50., 4., 90., 58.));
        scene.push_layer(
            peniko::BlendMode::default(),
            0.7,
            Affine::IDENTITY,
            &Rect::new(52., 6., 88., 56.),
            None,
            None,
        );
        scene.push_clip_layer(Affine::IDENTITY, &Rect::new(54., 8., 86., 54.));
        scene.draw_box_shadow(
            Affine::IDENTITY,
            Rect::new(60., 20., 76., 40.),
            Color::BLACK,
            4.,
            3.,
        );
        scene.pop_layer();
        scene.pop_layer();
        scene.pop_layer();
        partial.draw_scene_tracked(&scene, Color::WHITE).unwrap();
        assert_eq!(
            partial.read_pixels().unwrap(),
            full.render_scene(&scene, Color::WHITE).unwrap(),
            "frame {frame}"
        );
    }
}

#[test]
#[ignore = "Skia AA investigation: dirty clip currently changes rounded stroke coverage by 1/255"]
fn translucent_layer_dirty_clip_precision_probe() {
    use kurbo::{BezPath, Cap, Join, Stroke};
    let mut path = BezPath::new();
    path.move_to((224.5, 456.));
    path.curve_to(
        (224.5, 452.9624338759306),
        (226.96243387593063, 450.5),
        (230., 450.5),
    );
    path.line_to((350., 450.5));
    path.curve_to(
        (353.0375661240694, 450.5),
        (355.5, 452.9624338759306),
        (355.5, 456.),
    );
    path.line_to((355.5, 477.));
    path.curve_to(
        (355.5, 480.0375661240694),
        (353.0375661240694, 482.5),
        (350., 482.5),
    );
    path.line_to((230., 482.5));
    path.curve_to(
        (226.96243387593063, 482.5),
        (224.5, 480.0375661240694),
        (224.5, 477.),
    );
    path.close_path();
    let mut scene = Scene::new();
    scene.push_clip_layer(Affine::IDENTITY, &Rect::new(92., 153., 1080., 766.));
    scene.push_layer(
        peniko::BlendMode::default(),
        0.45,
        Affine::IDENTITY,
        &Rect::new(0., 0., 1100., 820.),
        None,
        None,
    );
    scene.stroke(
        &Stroke::new(1.).with_join(Join::Round).with_caps(Cap::Round),
        Affine::IDENTITY,
        Color::from_rgb8(217, 217, 222),
        None,
        &path,
    );
    scene.pop_layer();
    scene.pop_layer();
    let mut partial = SkiaSurface::new(1100, 820).unwrap();
    let mut full = SkiaSurface::new(1100, 820).unwrap();
    partial.draw_scene(&scene, Color::WHITE).unwrap();
    partial
        .draw_scene_damaged(
            &scene,
            Color::WHITE,
            plico_renderer::DamageRegion::Rect(Rect::new(226., 86., 874., 734.)),
        )
        .unwrap();
    let a = partial.read_pixels().unwrap();
    let b = full.render_scene(&scene, Color::WHITE).unwrap();
    let n = a
        .as_chunks::<4>()
        .0
        .iter()
        .zip(b.as_chunks::<4>().0.iter())
        .filter(|(a, b)| a != b)
        .count();
    assert!(n == 0, "rounded stroke clip changed {n} pixels");
}

#[test]
fn sparse_damage_preserves_gaps_and_avoids_dense_bounding_box_fallback() {
    let scene = |step: usize| {
        let mut scene = Scene::new();
        scene.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            Color::BLACK,
            None,
            &Rect::new(40., 40., 60., 60.),
        );
        scene.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            if step.is_multiple_of(2) {
                Color::BLACK
            } else {
                Color::from_rgb8(255, 0, 0)
            },
            None,
            &Rect::new(2. + step as f64, 2., 8. + step as f64, 8.),
        );
        if step < 3 {
            scene.fill(
                Fill::NonZero,
                Affine::IDENTITY,
                if step.is_multiple_of(2) {
                    Color::BLACK
                } else {
                    Color::from_rgb8(0, 0, 255)
                },
                None,
                &Rect::new(92., 92., 98., 98.),
            );
        }
        scene
    };
    let mut partial = SkiaSurface::new(100, 100).unwrap();
    partial.set_multi_rect_damage(true);
    let mut full = SkiaSurface::new(100, 100).unwrap();
    partial.draw_scene_tracked(&scene(0), Color::WHITE).unwrap();
    for step in 1..4 {
        let current = scene(step);
        let damage = partial.draw_scene_tracked(&current, Color::WHITE).unwrap();
        assert!(matches!(damage, plico_renderer::DamageRegion::Rect(_)));
        assert_eq!(partial.last_damage_rect_count(), 2);
        assert!(partial.last_damage_coverage() < 0.03);
        assert_eq!(
            partial.read_pixels().unwrap(),
            full.render_scene(&current, Color::WHITE).unwrap()
        );
    }
    assert_eq!(
        partial.draw_scene_tracked(&scene(3), Color::WHITE).unwrap(),
        plico_renderer::DamageRegion::Empty
    );
    assert_eq!(partial.last_damage_coverage(), 0.);
}
