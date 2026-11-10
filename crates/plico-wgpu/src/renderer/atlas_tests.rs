use super::*;
#[test]
#[ignore = "requires system fonts and a working wgpu adapter"]
fn child_layer_cannot_recycle_parent_glyph_uvs() {
    use kurbo::{Affine, Rect};
    use plico_renderer::{
        PaintCommand, PaintScene, Scene, paragraph::ParagraphRequest, style::Paint,
    };
    use std::sync::Arc;
    let mut renderer = pollster::block_on(Renderer::new()).unwrap();
    let request = |text: &str, color| {
        Arc::new(
            ParagraphRequest::from_paint(
                &Paint {
                    text: Some(text.into()),
                    font_size: 1000.,
                    text_color: color,
                    ..Default::default()
                },
                None,
            )
            .unwrap(),
        )
    };
    let first = request("W", peniko::Color::from_rgba8(200, 0, 0, 255));
    let second = request("Z", peniko::Color::from_rgba8(0, 0, 200, 255));
    let w = renderer.paragraph_layout(first.clone(), 1.).unwrap();
    let z = renderer.paragraph_layout(second.clone(), 1.).unwrap();
    let origin = |layout: &crate::text::GlyphLayout, x: f64| {
        let glyph = &layout.glyphs[0];
        Affine::translate((
            x - glyph.x - glyph.ink_bounds[0],
            8. - glyph.y - glyph.ink_bounds[1],
        ))
    };
    let mut scene = Scene::new();
    scene.commands.push(PaintCommand::Paragraph {
        request: first,
        transform: origin(&w, 8.),
    });
    scene.push_layer(
        peniko::BlendMode::default(),
        0.5,
        Affine::IDENTITY,
        &Rect::new(0., 0., 256., 64.),
        None,
        None,
    );
    scene.commands.push(PaintCommand::Paragraph {
        request: second,
        transform: origin(&z, 64.),
    });
    scene.pop_layer();
    let expected = renderer
        .render_scene(&scene, [256, 64], [1.; 4], false)
        .unwrap();
    let mut fill = crate::text::TextRequest::new("MNABCDEFGH");
    fill.size = 1000.;
    let fill = renderer.layout_text(&fill).unwrap();
    let atlas = renderer.atlas.as_mut().unwrap();
    atlas.clear();
    atlas.get(&w.glyphs[0], 0).unwrap();
    let mut full = false;
    'fill: for glyph in &fill.glyphs {
        for phase in 0..4 {
            match atlas.get(glyph, phase) {
                Ok(_) => {}
                Err(plico_native::text::Error::AtlasFull) => {
                    full = true;
                    break 'fill;
                }
                Err(error) => panic!("{error}"),
            }
        }
    }
    assert!(full);
    let actual = renderer
        .render_scene(&scene, [256, 64], [1.; 4], false)
        .unwrap();
    assert_eq!(
        actual, expected,
        "child layer invalidated already prepared parent UVs"
    );
    assert!(renderer.text_stats().recycled_frames > 0);
}

#[test]
#[ignore = "requires system fonts and a working wgpu adapter"]
fn full_atlas_recycles_before_draw_without_stale_uvs() {
    use crate::{
        Draw,
        text::{TextRequest, TextRun},
    };
    let mut renderer = pollster::block_on(Renderer::new()).unwrap();
    let mut first = TextRequest::new("W");
    first.size = 1000.;
    let layout = renderer.layout_text(&first).unwrap();
    let glyph = &layout.glyphs[0];
    let origin = [
        8. - glyph.x - glyph.ink_bounds[0],
        8. - glyph.y - glyph.ink_bounds[1],
    ];
    let draws = [Draw::Text(TextRun::new(&layout, origin, [0., 0., 0., 1.]))];
    let expected = renderer
        .render_items(&draws, [256, 64], [1.; 4], 0.)
        .unwrap();
    let mut request = TextRequest::new("MNABCDEFGH");
    request.size = 1000.;
    let large = renderer.layout_text(&request).unwrap();
    let atlas = renderer.atlas.as_mut().unwrap();
    // Simulate another document evicting the first frame's glyphs before
    // filling the fixed storage; the positioned layout itself stays alive.
    atlas.clear();
    let mut full = false;
    'fill: for glyph in &large.glyphs {
        for phase in 0..4 {
            match atlas.get(glyph, phase) {
                Ok(_) => {}
                Err(plico_native::text::Error::AtlasFull) => {
                    full = true;
                    break 'fill;
                }
                Err(e) => panic!("{e}"),
            }
        }
    }
    assert!(full, "fixture did not exhaust the atlas");
    let pixels = renderer
        .render_items(&draws, [256, 64], [1.; 4], 0.)
        .unwrap();
    assert_eq!(
        pixels, expected,
        "cache recycling used stale glyph rectangles"
    );
    let stats = renderer.text_stats();
    assert_eq!(stats.recycled_frames, 1);
    assert_eq!(stats.atlas_cpu_bytes, 16 * 1024 * 1024);
    assert_eq!(stats.atlas_gpu_bytes, 16 * 1024 * 1024);
}
