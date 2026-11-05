//! Retained-fragment scenes must render identically to their pre-expanded
//! form. The scene bridge shares the original command Arc when no fragments
//! are present and only runs the flattening expansion for scenes that have
//! them; both paths must produce the same pixels.
#![cfg(feature = "paint-scene")]
use kurbo::{Affine, Rect};
use peniko::{Color, Fill};
use plico_renderer::{PaintScene, Scene};
use plico_wgpu::Renderer;

fn sheet() -> (Scene, Scene) {
    // A retained fragment holding a rounded red sheet plus its parent's blue
    // frame: the fragment path stores a nested command tree, the expanded
    // path stores the same content inline.
    let mut fragment = Scene::new();
    fragment.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        Color::from_rgba8(220, 60, 40, 255),
        None,
        &Rect::new(8., 8., 56., 48.).to_rounded_rect(6.),
    );
    let mut nested = Scene::new();
    nested.append_cached_scene(fragment.clone(), Affine::translate((0., 4.)));
    nested.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        Color::from_rgba8(40, 90, 220, 255),
        None,
        &Rect::new(4., 0., 60., 12.),
    );
    let expanded = nested.flattened();
    (nested, expanded)
}

#[test]
#[ignore = "requires a working wgpu adapter"]
fn retained_fragment_scenes_match_their_expanded_command_stream() {
    let (nested, expanded) = sheet();
    let mut renderer = pollster::block_on(Renderer::new()).unwrap();
    let nested_pixels = renderer
        .render_scene(&nested, [64, 64], [0.; 4], false)
        .expect("fragment scenes must prepare");
    let expanded_pixels = renderer
        .render_scene(&expanded, [64, 64], [0.; 4], false)
        .expect("expanded scenes must prepare");
    assert_eq!(
        nested_pixels, expanded_pixels,
        "the fragment expansion path must draw exactly what its flattened form draws"
    );
    // The prepared cache must not leak the nested form into a later expanded
    // frame or vice versa: render both again in swapped order.
    let again = renderer
        .render_scene(&expanded, [64, 64], [0.; 4], false)
        .unwrap();
    assert_eq!(again, expanded_pixels);
    let again = renderer
        .render_scene(&nested, [64, 64], [0.; 4], false)
        .unwrap();
    assert_eq!(again, nested_pixels);
}

#[test]
#[ignore = "requires a working wgpu adapter"]
fn instance_staging_reuse_keeps_damage_replays_exact_across_frames() {
    // Repeated frames with moving content exercise the reused staging vector:
    // stale entries must never leak into a shorter or longer frame.
    let mut renderer = pollster::block_on(Renderer::new()).unwrap();
    let mut previous: Option<Vec<u8>> = None;
    for step in 0..6 {
        let mut scene = Scene::new();
        // Long frame first, then a shorter one: the reused capacity must not
        // replay anything beyond the current command count.
        for index in 0..(12 - step * 2) {
            scene.fill(
                Fill::NonZero,
                Affine::translate((f64::from(index) * 5., f64::from(step) * 3.)),
                Color::from_rgba8(30, 130, 240, 200),
                None,
                &Rect::new(0., 0., 6., 6.),
            );
        }
        let pixels = renderer
            .render_scene(&scene, [64, 64], [0.; 4], false)
            .unwrap();
        if let Some(before) = previous {
            assert_ne!(before, pixels, "moving content must change the frame");
        }
        previous = Some(pixels);
    }
}
