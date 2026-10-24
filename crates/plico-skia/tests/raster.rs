#![cfg(feature = "raster")]
use plico_renderer::{PaintScene, Scene};
use plico_skia::{Error, SkiaSurface};
use vello_common::{
    kurbo::{Affine, Rect, Stroke},
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
    scene.stroke(
        &Stroke::new(2.),
        Affine::IDENTITY,
        Color::BLACK,
        None,
        &Rect::new(0., 0., 4., 4.),
    );
    assert!(matches!(
        surface.render_scene(&scene, Color::WHITE),
        Err(Error::Unsupported { command: "Stroke" })
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
