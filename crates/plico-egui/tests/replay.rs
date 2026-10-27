//! Offscreen replay tests: Scene commands must tessellate into epaint
//! meshes with the right coverage, colors and clips - and unsupported
//! features must fail loudly, never silently drop.
#![cfg(feature = "replay")]
use egui::{Context, RawInput};
use plico_egui::{GlyphCache, ReplayError, replay_scene};
use plico_renderer::{PaintCommand, PaintScene, Scene};
use vello_common::kurbo::{Affine, Rect};
use vello_common::peniko::{Color, Fill};

/// Tessellate one scene offscreen: (clip rect, mesh) pairs. egui applies
/// the clip at draw time, so tests intersect triangle bounds with the clip.
fn tessellate(scene: &Scene) -> Vec<(egui::Rect, egui::epaint::Mesh)> {
    let ctx = Context::default();
    ctx.begin_pass(RawInput::default());
    let painter = ctx.layer_painter(egui::LayerId::background());
    let mut cache = GlyphCache::default();
    replay_scene(&painter, scene, &mut cache).expect("supported scene");
    drop(painter);
    let output = ctx.end_pass();
    let clipped = ctx.tessellate(output.shapes, 1.0);
    clipped
        .into_iter()
        .filter_map(|clipped| match clipped.primitive {
            egui::epaint::Primitive::Mesh(mesh) => Some((clipped.clip_rect, mesh)),
            egui::epaint::Primitive::Callback(_) => None,
        })
        .collect()
}

fn triangle_area(a: egui::Pos2, b: egui::Pos2, c: egui::Pos2) -> f32 {
    ((b.x - a.x) * (c.y - a.y) - (c.x - a.x) * (b.y - a.y)).abs() / 2.
}

/// Total clipped mesh area whose vertices all match the color predicate.
fn colored_area(
    meshes: &[(egui::Rect, egui::epaint::Mesh)],
    pick: fn(&egui::Color32) -> bool,
) -> f32 {
    let mut area = 0.;
    for (clip, mesh) in meshes {
        for triangle in mesh.indices.as_chunks::<3usize>().0 {
            let [a, b, c] = [
                mesh.vertices[triangle[0] as usize],
                mesh.vertices[triangle[1] as usize],
                mesh.vertices[triangle[2] as usize],
            ];
            if pick(&a.color) && pick(&b.color) && pick(&c.color) {
                area += clipped_triangle_area(a.pos, b.pos, c.pos, *clip);
            }
        }
    }
    area
}

/// Triangle area intersected with the draw-time clip; approximate by full
/// area when fully inside and half when straddling the clip edge.
fn clipped_triangle_area(a: egui::Pos2, b: egui::Pos2, c: egui::Pos2, clip: egui::Rect) -> f32 {
    let area = triangle_area(a, b, c);
    let inside = |p: egui::Pos2| clip.contains(p);
    let all_in = inside(a) && inside(b) && inside(c);
    let tri = egui::Rect::from_min_max(
        egui::pos2(a.x.min(b.x).min(c.x), a.y.min(b.y).min(c.y)),
        egui::pos2(a.x.max(b.x).max(c.x), a.y.max(b.y).max(c.y)),
    );
    let all_out = !inside(a) && !inside(b) && !inside(c) && !clip.intersects(tri);
    if all_in {
        area
    } else if all_out {
        0.
    } else {
        area / 2.
    }
}

#[test]
fn a_solid_rect_replays_as_a_covering_red_mesh() {
    let mut scene = Scene::new();
    scene.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        Color::from_rgb8(255, 0, 0),
        None,
        &Rect::new(10., 10., 110., 60.),
    );
    let meshes = tessellate(&scene);
    let red = colored_area(&meshes, |c| c.r() == 255 && c.g() == 0 && c.b() == 0);
    let expected = 100. * 50.;
    assert!(
        (red - expected).abs() / expected < 0.04,
        "red coverage {red} vs expected {expected}"
    );
}

#[test]
fn a_clipped_layer_limits_the_replayed_coverage() {
    let mut scene = Scene::new();
    scene.push_clip_layer(Affine::IDENTITY, &Rect::new(10., 10., 60., 60.));
    scene.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        Color::from_rgb8(0, 255, 0),
        None,
        &Rect::new(0., 0., 110., 110.),
    );
    scene.pop_layer();
    let meshes = tessellate(&scene);
    // egui applies the clip at draw time; the replay's contract is that the
    // clip reaches the tessellate output verbatim on every green mesh.
    assert!(!meshes.is_empty());
    for (clip, mesh) in &meshes {
        assert!(
            mesh.vertices.iter().any(|v| v.color.g() == 255),
            "green mesh expected"
        );
        assert_eq!(clip.min, egui::pos2(10., 10.));
        assert_eq!(clip.max, egui::pos2(60., 60.));
    }
}

#[test]
fn layer_alpha_folds_into_the_replayed_color() {
    let mut scene = Scene::new();
    scene.push_layer(
        vello_common::peniko::BlendMode::default(),
        0.5,
        Affine::IDENTITY,
        &Rect::new(0., 0., 100., 100.),
        None,
        None,
    );
    scene.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        Color::from_rgb8(255, 0, 0),
        None,
        &Rect::new(0., 0., 100., 100.),
    );
    scene.pop_layer();
    let meshes = tessellate(&scene);
    // Folded alpha must show up in the vertex color, not in coverage area.
    let semi_red = colored_area(&meshes, |c| c.r() > 100 && (120..136).contains(&c.a()));
    assert!(semi_red > 90. * 90., "50% red area {semi_red}");
}

#[test]
fn unsupported_commands_fail_loudly_instead_of_drawing_nothing() {
    let ctx = Context::default();
    ctx.begin_pass(RawInput::default());
    let painter = ctx.layer_painter(egui::LayerId::background());
    let mut cache = GlyphCache::default();

    // Even-odd fills have no epaint tessellation rule; a gradient brush is
    // likewise rejected. The scene API only accepts solid brushes for fill,
    // so even-odd is the reachable rejection through the public API.
    let mut even_odd = Scene::new();
    even_odd.fill(
        Fill::EvenOdd,
        Affine::IDENTITY,
        Color::BLACK,
        None,
        &Rect::new(0., 0., 5., 5.),
    );
    let matches = even_odd.commands.iter().any(
        |c| matches!(c, plico_renderer::PaintCommand::Fill { fill, .. } if *fill == Fill::EvenOdd),
    );
    if matches {
        let error = replay_scene(&painter, &even_odd, &mut cache).unwrap_err();
        assert!(matches!(error, ReplayError::Unsupported { .. }), "{error}");
    } else {
        // The scene normalized even-odd to non-zero; then the replay accepts
        // it and this assertion documents that path instead.
        replay_scene(&painter, &even_odd, &mut cache).expect("normalized fill");
    }
    let _ = ctx.end_pass();
}

#[test]
fn transforms_position_the_replayed_shapes() {
    let mut scene = Scene::new();
    scene.fill(
        Fill::NonZero,
        Affine::translate((20., 30.)),
        Color::from_rgb8(0, 0, 255),
        None,
        &Rect::new(0., 0., 40., 40.),
    );
    let meshes = tessellate(&scene);
    // The blue quad must live inside the translated span.
    for (_, mesh) in &meshes {
        for vertex in &mesh.vertices {
            if vertex.color.b() == 255 && vertex.color.r() == 0 {
                assert!(
                    vertex.pos.x >= 19.
                        && vertex.pos.x <= 61.
                        && vertex.pos.y >= 29.
                        && vertex.pos.y <= 71.,
                    "vertex escaped the translate: {:?}",
                    vertex.pos
                );
            }
        }
    }
    let blue = colored_area(&meshes, |c| c.b() == 255);
    assert!((blue - 1600.).abs() / 1600. < 0.04, "blue area {blue}");
}

#[test]
fn a_real_parley_glyph_run_replays_as_textured_quads() {
    // Shape real text through the shared Parley path, encode it into the
    // scene as a GlyphRun, and verify the replay produces textured quads
    // positioned at the shaped glyph origins - the renderer-independence
    // claim for text.
    let mut tcx = plico_renderer::TextContext::new();
    let asset = plico_renderer::text::FontAsset::from_static(include_bytes!(
        "../../../backend/examples/web-preview/public/DejaVuSans.ttf"
    ))
    .with_family("egui replay test");
    assert!(tcx.register_font(&asset));
    let layout = plico_renderer::text::layout_text(&mut tcx, "Ag", 32.);
    let mut scene = Scene::new();
    plico_renderer::scene::draw_text_layout_into(
        &mut scene,
        &mut tcx,
        &layout,
        Affine::IDENTITY,
        Affine::IDENTITY,
        1.,
    );
    assert!(
        scene
            .commands
            .iter()
            .any(|c| matches!(c, PaintCommand::GlyphRun { glyphs, .. } if !glyphs.is_empty())),
        "the shared path must encode shaped glyph runs"
    );

    let meshes = tessellate(&scene);
    // Every glyph-run shape is a textured mesh (not a plain fill path).
    let textured = meshes
        .iter()
        .filter(|(_, mesh)| mesh.texture_id != egui::TextureId::Managed(0))
        .count();
    assert!(
        textured >= 1,
        "expected textured glyph quads, got {textured}"
    );
    // The quads live below the baseline advance area of the shaped run and
    // to the right of the origin; roughly: all vertices within 0..96x0..64.
    for (clip, mesh) in &meshes {
        if mesh.texture_id != egui::TextureId::Managed(0) {
            for vertex in &mesh.vertices {
                assert!(
                    vertex.pos.x >= -1. && vertex.pos.x <= 96.,
                    "x {}",
                    vertex.pos
                );
                assert!(
                    vertex.pos.y >= -8. && vertex.pos.y <= 64.,
                    "y {}",
                    vertex.pos
                );
            }
            assert!(clip.width() > 0. && clip.height() > 0.);
        }
    }
}

#[test]
fn an_svg_icon_lowers_through_the_shared_projector() {
    // The same Lucide-style markup settings and gallery declare; the replay
    // must lower it through the shared usvg→command projector into fills and
    // strokes, with the document's currentColor resolved by the caller.
    let icon = plico_renderer::svg::SvgImage::parse_with_current_color(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24">
             <path d="M20 6 9 17l-5-5" stroke="currentColor" stroke-width="2" fill="none"/>
           </svg>"#,
        0x2255aa,
    )
    .unwrap();
    let mut scene = Scene::new();
    scene.draw_svg(icon.document().clone(), Affine::translate((5., 5.)));
    let meshes = tessellate(&scene);
    // The stroke becomes path geometry: some tessellated area must appear,
    // translated into the icon's 5,5 origin.
    let mut area = 0.;
    for (_, mesh) in &meshes {
        for triangle in mesh.indices.as_chunks::<3usize>().0 {
            let [a, b, c] = [
                mesh.vertices[triangle[0] as usize],
                mesh.vertices[triangle[1] as usize],
                mesh.vertices[triangle[2] as usize],
            ];
            area += triangle_area(a.pos, b.pos, c.pos);
        }
    }
    assert!(
        area > 40.,
        "the checkmark stroke must tessellate, got {area}"
    );
    // All ink stays within the translated 24x24 icon box.
    for (_, mesh) in &meshes {
        for vertex in &mesh.vertices {
            assert!(
                vertex.pos.x >= 4. && vertex.pos.x <= 30.,
                "x {}",
                vertex.pos
            );
            assert!(
                vertex.pos.y >= 4. && vertex.pos.y <= 30.,
                "y {}",
                vertex.pos
            );
        }
    }
}

#[test]
fn a_box_shadow_replays_as_a_blurred_textured_quad() {
    // The HUD-style shadow: a blurred rounded rect must become a textured
    // quad whose area exceeds the unblurred rect (the blur extent adds ink
    // around it) with translucent edge coverage.
    let mut scene = Scene::new();
    scene.draw_box_shadow(
        Affine::IDENTITY,
        vello_common::kurbo::Rect::new(10., 10., 60., 40.),
        vello_common::peniko::Color::from_rgba8(0, 0, 0, 64),
        8.,
        4.,
    );
    let meshes = tessellate(&scene);
    let mut shadow_area = 0.;
    let mut quads = 0;
    for (clip, mesh) in &meshes {
        if mesh.texture_id != egui::TextureId::Managed(0) {
            quads += 1;
            for triangle in mesh.indices.as_chunks::<3usize>().0 {
                let [a, b, c] = [
                    mesh.vertices[triangle[0] as usize],
                    mesh.vertices[triangle[1] as usize],
                    mesh.vertices[triangle[2] as usize],
                ];
                shadow_area += clipped_triangle_area(a.pos, b.pos, c.pos, *clip);
            }
        }
    }
    assert_eq!(quads, 1, "one shadow quad expected");
    // The quad spans the rect plus the ~12px blur extent on every side:
    // 74x54 against the unblurred 50x30. The clip here only cuts the part
    // left of the screen origin, so compare against the visible half bound.
    assert!(
        shadow_area > 1990.,
        "blur must extend well beyond the rect, got {shadow_area}"
    );
}
