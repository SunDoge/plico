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
    scene.draw_box_shadow(
        Affine::IDENTITY,
        Rect::new(0., 0., 4., 4.),
        Color::BLACK,
        1.,
        2.,
    );
    assert!(matches!(
        surface.render_scene(&scene, Color::WHITE),
        Err(Error::Unsupported {
            command: "BoxShadow"
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
    let icon = SvgImage::parse(r#"<svg xmlns="http://www.w3.org/2000/svg" width="8" height="8"><defs><linearGradient id="g"><stop stop-color="red"/><stop offset="1" stop-color="blue"/></linearGradient></defs><rect width="8" height="8" fill="url(#g)"/></svg>"#).unwrap();
    let mut scene = Scene::new();
    scene.draw_svg(icon.document().clone(), Affine::IDENTITY);
    assert!(matches!(
        surface.render_scene(&scene, Color::WHITE),
        Err(Error::Unsupported {
            command: "SVG gradient/pattern"
        })
    ));
    assert!(matches!(surface.read_pixels(), Err(Error::NoFrame)));
}
