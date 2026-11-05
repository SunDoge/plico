//! Run joining (mygo joinLast): adjacent runs differing only in class, each
//! below the join area, merge into one general-pipeline call. Large runs
//! keep their specialized shader, and texture boundaries never join.
use peniko::{Blob, ImageAlphaType, ImageData, ImageFormat};
use plico_wgpu::{Draw, Quad, Renderer, image::ImageQuad};
use std::sync::Arc;

fn image(rgba: [u8; 4]) -> ImageData {
    ImageData {
        data: Blob::new(Arc::new(rgba.to_vec())),
        format: ImageFormat::Rgba8,
        alpha_type: ImageAlphaType::AlphaPremultiplied,
        width: 1,
        height: 1,
    }
}

#[test]
#[ignore = "requires a working wgpu adapter"]
fn interleaved_small_classes_join_into_one_call_without_pixel_changes() {
    let mut renderer = pollster::block_on(Renderer::new()).unwrap();
    let size = [128, 128];
    // shadow-then-fill pairs, a row at a time: the classic interleave mygo
    // joinLast exists for. Without joining this draws two calls per pair.
    let mut quads = Vec::new();
    for row in 0..8u16 {
        let y = f32::from(row) * 14.;
        let mut shadow = Quad::new([6., y, 48., 12.], [0., 0., 0., 0.45]);
        shadow.shadow_sigma = 1.5;
        let fill = Quad::new([8., y + 2., 44., 8.], [0.25, 0.55, 0.95, 1.]);
        quads.push(shadow);
        quads.push(fill);
    }
    let pixels = renderer.render(&quads, size, [0.; 4], 0.).unwrap();
    let stats = renderer.batch_stats();
    assert_eq!(stats.instances, 16);
    assert_eq!(
        stats.draw_calls, 1,
        "all sixteen small interleaved quads join into one call"
    );
    // Joining draws through the general pipeline; force-general must give
    // the exact same bytes (it also joins, as class is not the boundary).
    renderer.force_general_pipelines(true);
    let general = renderer.render(&quads, size, [0.; 4], 0.).unwrap();
    assert_eq!(pixels, general);
}

#[test]
#[ignore = "requires a working wgpu adapter"]
fn large_runs_keep_their_class_instead_of_joining() {
    let mut renderer = pollster::block_on(Renderer::new()).unwrap();
    let size = [256, 160];
    // A big plain sheet (above the 128x128 join area), then a small general
    // shadow: the plain run must keep the specialized pipeline, and the
    // small shadow cannot drag it into a joined general run.
    let sheet = Quad::new([0., 0., 160., 140.], [0.9, 0.92, 0.95, 1.]);
    let mut shadow = Quad::new([168., 4., 48., 10.], [0., 0., 0., 0.5]);
    shadow.shadow_sigma = 1.;
    let pixels = renderer
        .render(&[sheet, shadow], size, [0.; 4], 0.)
        .unwrap();
    let stats = renderer.batch_stats();
    assert_eq!(
        stats.draw_calls, 2,
        "a large run never joins its small neighbor"
    );
    assert_eq!(
        stats.plain_draw_calls, 1,
        "the large plain run stays specialized"
    );
    let inside = &pixels[(70 * 256 + 80) * 4..(70 * 256 + 80) * 4 + 4];
    for (actual, expected) in inside.iter().zip([230u8, 235, 243, 255]) {
        assert!(actual.abs_diff(expected) <= 1);
    }
    // A small plain quad first, then the large sheet, then the small shadow:
    // the two plain quads are one plain class run (dot + sheet), and the
    // large area keeps that run specialized; the trailing small shadow run
    // is a different class and cannot join across a non-small boundary.
    let dot = Quad::new([164., 20., 12., 12.], [1., 0.4, 0.2, 1.]);
    let _ = renderer
        .render(&[dot, sheet, shadow], size, [0.; 4], 0.)
        .unwrap();
    assert_eq!(renderer.batch_stats().draw_calls, 2);
    assert_eq!(renderer.batch_stats().plain_draw_calls, 1);
}

#[test]
#[ignore = "requires a working wgpu adapter"]
fn texture_boundaries_never_join() {
    let mut renderer = pollster::block_on(Renderer::new()).unwrap();
    let size = [128, 64];
    let red = image([255, 0, 0, 255]);
    let blue = image([0, 0, 255, 255]);
    // Two small plain fills with image quads of two textures between them:
    // texture changes split the runs; the plain fills cannot join across.
    let left = Quad::new([0., 0., 12., 12.], [0., 0., 0., 1.]);
    let right = Quad::new([44., 0., 12., 12.], [0., 0., 0., 1.]);
    let a = ImageQuad::new(Quad::new([16., 0., 10., 10.], [1.; 4]), red);
    let b = ImageQuad::new(Quad::new([28., 0., 10., 10.], [1.; 4]), blue);
    renderer
        .render_items(
            &[
                Draw::Quad(&left),
                Draw::Image(&a),
                Draw::Image(&b),
                Draw::Quad(&right),
            ],
            size,
            [0.; 4],
            0.,
        )
        .unwrap();
    let stats = renderer.batch_stats();
    assert_eq!(stats.draw_calls, 4, "texture changes split every run");
    assert_eq!(
        stats.plain_draw_calls, 2,
        "the two plain fills stay separate"
    );
}
