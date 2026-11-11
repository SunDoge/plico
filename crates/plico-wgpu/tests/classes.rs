//! Class specialization: adjacent solids and ordinary gradient/border fills
//! draw through small dedicated fragment pipelines (mygo's ClassPlain and
//! ClassFill). Specialized paths must be pixel-identical to the general shader,
//! and class runs must never reorder painter sequence.
use peniko::{Blob, ImageAlphaType, ImageData, ImageFormat};
use plico_wgpu::{Draw, Quad, Renderer, mask::CoverageMask};
use std::sync::Arc;

fn fixture() -> Vec<Quad> {
    // Plain fills: solid, unbordered, no mask. Includes rounded and clipped
    // quads, the shapes that dominate real frames. The sheet alone covers
    // more than the join area (128x128 px), so its run keeps the
    // specialized pipeline instead of merging into a small general run.
    let mut sheet = Quad::new([4., 4., 150., 120.], [0.94, 0.96, 0.99, 1.]);
    sheet.radii = [10., 10., 0., 0.];
    let mut clipped = Quad::new([8., 56., 64., 16.], [0.2, 0.5, 0.9, 1.]);
    clipped.clip = Some([8., 60., 40., 8.]);
    clipped.clip_antialias = true;
    let translucent = Quad::new([60., 60., 30., 30.], [0.9, 0.2, 0.2, 0.4]);
    let square = Quad::new([4., 96., 20., 20.], [0., 0., 0., 1.]);
    // General-path quads: border, gradient, shadow.
    let mut bordered = Quad::new([160., 8., 20., 20.], [0.; 4]);
    bordered.radii = [4.; 4];
    bordered.border_width = 1.5;
    bordered.border_color = [0., 0.4, 0.8, 1.];
    // This run exceeds joinArea so ClassFill remains specialized instead of
    // joining the following small shadow through the general pipeline.
    let mut gradient = Quad::new([8., 12., 140., 130.], [1., 0.4, 0.2, 1.]);
    gradient.colors[1] = [0.2, 0.4, 1., 1.];
    let mut shadow = Quad::new([56., 128., 64., 8.], [0., 0., 0., 0.5]);
    shadow.shadow_sigma = 1.;
    vec![
        sheet,
        clipped,
        translucent,
        square,
        bordered,
        gradient,
        shadow,
    ]
}

#[test]
#[ignore = "requires a working wgpu adapter"]
fn specialized_fill_pipelines_are_pixel_identical_to_the_general_shader() {
    let mut renderer = pollster::block_on(Renderer::new()).unwrap();
    let size = [200, 150];
    let quads = fixture();
    let specialized = renderer.render(&quads, size, [0.05; 4], 0.).unwrap();
    let stats = renderer.batch_stats();
    assert_eq!(
        stats.plain_draw_calls, 1,
        "adjacent plain fills share one run"
    );
    assert_eq!(
        stats.fill_draw_calls, 1,
        "adjacent border/gradient fills share one specialized run"
    );
    assert!(
        stats.draw_calls < stats.instances as u64,
        "runs are split by class"
    );
    renderer.force_general_pipelines(true);
    let general = renderer.render(&quads, size, [0.05; 4], 0.).unwrap();
    assert_eq!(renderer.batch_stats().plain_draw_calls, 0);
    assert_eq!(renderer.batch_stats().fill_draw_calls, 0);
    assert_eq!(
        specialized, general,
        "the plain/fill class shaders must reproduce the general pipeline exactly"
    );
    // Force-general must be reversible: switching back restores class runs.
    renderer.force_general_pipelines(false);
    renderer.render(&quads, size, [0.05; 4], 0.).unwrap();
    assert_eq!(renderer.batch_stats().plain_draw_calls, 1);
    assert_eq!(renderer.batch_stats().fill_draw_calls, 1);
}

#[test]
#[ignore = "requires a working wgpu adapter"]
fn class_runs_never_reorder_painter_sequence() {
    let mut renderer = pollster::block_on(Renderer::new()).unwrap();
    let size = [32, 32];
    // Two translucent plain fills overlapping a bordered general fill between
    // them: class splitting must not merge the plain pair across the border.
    let below = Quad::new([0., 0., 32., 32.], [1., 0., 0., 0.5]);
    let mut frame = Quad::new([4., 4., 24., 24.], [0.; 4]);
    frame.border_width = 2.;
    frame.border_color = [0., 0., 1., 1.];
    let above = Quad::new([8., 8., 16., 16.], [0., 1., 0., 0.5]);
    let pixels = renderer
        .render(&[below, frame, above], size, [0.; 4], 0.)
        .unwrap();
    let stats = renderer.batch_stats();
    // All three quads are below the join area, so the plain/general/plain
    // interleave merges into one general-pipeline call (mygo joinLast).
    // The pixel checks below are what prove painter order survived it.
    assert_eq!(stats.draw_calls, 1, "small class runs join into one call");
    assert_eq!(stats.plain_draw_calls, 0);
    let px = |x: u32, y: u32| {
        let p: [u8; 4] = pixels[((y * 32 + x) * 4) as usize..][..4]
            .try_into()
            .unwrap();
        p
    };
    // The green overlay must stay on top of the red; the border ring must
    // stay above the red it covers. Painter order is asserted from pixels.
    for (a, b) in px(16, 16).iter().zip([64, 128, 0, 192]) {
        assert!(a.abs_diff(b) <= 2, "{:?}", px(16, 16));
    }
    for (a, b) in px(4, 16).iter().zip([0, 0, 255, 255]) {
        assert!(a.abs_diff(b) <= 2, "{:?}", px(4, 16));
    }
    for (a, b) in px(16, 2).iter().zip([128, 0, 0, 128]) {
        assert!(a.abs_diff(b) <= 2, "{:?}", px(16, 2));
    }
    for (a, b) in px(16, 4).iter().zip([0, 0, 255, 255]) {
        assert!(a.abs_diff(b) <= 2, "{:?}", px(16, 4));
    }
}

#[test]
#[ignore = "requires a working wgpu adapter"]
fn plain_classification_covers_solid_fills_and_masks() {
    let mut renderer = pollster::block_on(Renderer::new()).unwrap();
    let size = [16, 16];
    // Solid fill only: exactly one plain run and nothing else.
    let solid = Quad::new([0., 0., 16., 16.], [0.1, 0.2, 0.3, 1.]);
    renderer.render(&[solid], size, [0.; 4], 0.).unwrap();
    assert_eq!(renderer.batch_stats().plain_draw_calls, 1);
    assert_eq!(renderer.batch_stats().draw_calls, 1);

    // A coverage mask is clip state, not a different paint class. It keeps the
    // minimal plain pipeline while sampling the same mask as the general path.
    let mask = CoverageMask {
        image: ImageData {
            data: Blob::new(Arc::new(vec![128; 16 * 16 * 4])),
            format: ImageFormat::Rgba8,
            alpha_type: ImageAlphaType::AlphaPremultiplied,
            width: 16,
            height: 16,
        },
        rect: [0., 0., 16., 16.],
    };
    let masked = renderer
        .render_items(
            &[Draw::SetMask(&mask), Draw::Quad(&solid), Draw::ResetMask],
            size,
            [0.; 4],
            0.,
        )
        .unwrap();
    assert_eq!(renderer.batch_stats().plain_draw_calls, 1);
    renderer.force_general_pipelines(true);
    let masked_general = renderer
        .render_items(
            &[Draw::SetMask(&mask), Draw::Quad(&solid), Draw::ResetMask],
            size,
            [0.; 4],
            0.,
        )
        .unwrap();
    assert_eq!(masked, masked_general);
    assert_eq!(renderer.batch_stats().plain_draw_calls, 0);
    renderer.force_general_pipelines(false);

    // A gradient fill (different endpoint colors) must not take the plain path.
    let mut gradient = Quad::new([0., 0., 16., 16.], [1., 0., 0., 1.]);
    gradient.colors[1] = [0., 0., 1., 1.];
    renderer.render(&[gradient], size, [0.; 4], 0.).unwrap();
    assert_eq!(renderer.batch_stats().plain_draw_calls, 0);
    assert_eq!(renderer.batch_stats().fill_draw_calls, 1);
    // A shadow quad is never plain.
    let mut shadow = Quad::new([0., 0., 16., 16.], [0., 0., 0., 0.5]);
    shadow.shadow_sigma = 1.;
    renderer.render(&[shadow], size, [0.; 4], 0.).unwrap();
    assert_eq!(renderer.batch_stats().plain_draw_calls, 0);
    assert_eq!(renderer.batch_stats().fill_draw_calls, 0);

    // Runtime materials are application-defined and stay on the general path.
    let mut material = Quad::new([0., 0., 16., 16.], [0.2, 0.3, 0.4, 1.]);
    material.material = true;
    renderer.render(&[material], size, [0.; 4], 0.).unwrap();
    assert_eq!(renderer.batch_stats().plain_draw_calls, 0);
    assert_eq!(renderer.batch_stats().fill_draw_calls, 0);
}
