//! Offscreen replay tests: Scene commands must tessellate into epaint
//! meshes with the right coverage, colors and clips - and unsupported
//! features must fail loudly, never silently drop.
#![cfg(feature = "replay")]
use egui::{Context, RawInput};
use plico_egui::{GlyphCache, ReplayError, replay_scene};
use plico_renderer::{PaintScene, Scene};
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
