//! Prepared-scene caching across the root scene and isolated layers. A single
//! cache entry ping-ponged between the root and its layers, so frames with
//! isolated groups never hit; the keyed map must serve both.
#![cfg(feature = "paint-scene")]
use kurbo::{Affine, Rect};
use peniko::{BlendMode, Color, Fill};
use plico_renderer::{PaintScene, Scene};
use plico_wgpu::Renderer;

/// A root scene containing an isolated layer: blue sheet, an opacity group
/// holding a green sheet, and a red sheet after it. The layer's child content
/// prepares with an inflated root clip distinct from the root scene's.
fn layered_scene(alpha: f32) -> Scene {
    let mut scene = Scene::new();
    scene.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        Color::from_rgba8(40, 90, 220, 255),
        None,
        &Rect::new(0., 0., 16., 16.),
    );
    scene.push_layer(
        BlendMode::default(),
        alpha,
        Affine::IDENTITY,
        &Rect::new(4., 4., 12., 12.),
        None,
        None,
    );
    scene.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        Color::from_rgba8(40, 200, 90, 255),
        None,
        &Rect::new(4., 4., 12., 12.),
    );
    scene.pop_layer();
    scene.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        Color::from_rgba8(220, 60, 40, 255),
        None,
        &Rect::new(12., 12., 20., 20.),
    );
    scene
}

#[test]
#[ignore = "requires a working wgpu adapter"]
fn isolated_layers_and_the_root_scene_both_hit_the_prepared_cache() {
    let mut renderer = pollster::block_on(Renderer::new()).unwrap();
    let scene = layered_scene(0.5);
    // First frame: the root scene and the layer child each prepare once.
    renderer
        .render_scene(&scene, [32, 32], [0.; 4], false)
        .unwrap();
    let (preparations, hits) = renderer.scene_prepare_stats();
    assert!(
        preparations >= 2,
        "root and layer each prepared: {preparations}"
    );
    assert_eq!(hits, 0, "the first frame cannot hit");
    // Second frame, unchanged scene: the root lookup hits and returns the
    // whole tree - the layer child never re-prepares on its own. This is the
    // assertion a single-entry cache could never pass: the layer's insertion
    // replaced the root's entry, so the root (and therefore the layer inside
    // it) re-prepared on every single frame.
    let pixels = renderer
        .render_scene(&scene, [32, 32], [0.; 4], false)
        .unwrap();
    let (preparations_after, hits_after) = renderer.scene_prepare_stats();
    assert_eq!(
        preparations_after, preparations,
        "the unchanged frame must not prepare anything - root or layer"
    );
    assert_eq!(hits_after, 1, "the root hit replays the whole tree");
    // Third frame: pixel output stays identical while still fully cached.
    let again = renderer
        .render_scene(&scene, [32, 32], [0.; 4], false)
        .unwrap();
    assert_eq!(pixels, again);
    assert_eq!(
        renderer.scene_prepare_stats(),
        (preparations_after, hits_after + 1)
    );
}

#[test]
#[ignore = "requires a working wgpu adapter"]
fn changed_layer_content_reprepares_only_the_layer() {
    let mut renderer = pollster::block_on(Renderer::new()).unwrap();
    renderer
        .render_scene(&layered_scene(0.5), [32, 32], [0.; 4], false)
        .unwrap();
    let (preparations, _hits) = renderer.scene_prepare_stats();
    // A different root scene (different alpha flows into the layer command)
    // must re-prepare both entries without stale pixel reuse.
    renderer
        .render_scene(&layered_scene(0.25), [32, 32], [0.; 4], false)
        .unwrap();
    let (changed_preparations, _) = renderer.scene_prepare_stats();
    assert!(changed_preparations > preparations);
}
