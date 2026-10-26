#![cfg(feature = "raster")]
use plico_renderer::{PaintScene, Scene};
use plico_skia::{Error, SkiaSurface};
use vello_common::{
    kurbo::{Affine, Rect},
    peniko::{Color, Fill},
};

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
        vello_common::peniko::BlendMode::default(),
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
    use vello_common::{
        kurbo::{Line, Stroke},
        peniko::BlendMode,
    };
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
    use std::sync::Arc;
    use vello_common::peniko::{
        Blob, ImageAlphaType, ImageBrush, ImageData, ImageFormat, ImageQuality,
    };
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
            vello_common::peniko::BlendMode::default(),
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
