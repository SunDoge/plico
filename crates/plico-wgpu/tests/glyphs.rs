#![cfg(all(feature = "paint-scene", feature = "system-text", target_os = "linux"))]
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
