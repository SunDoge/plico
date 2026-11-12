//! Class specialization: adjacent solids, ordinary gradient/border fills and
//! box shadows draw through small dedicated pipelines (mygo's ClassPlain,
//! ClassFill and ClassShadow). Specialized paths must be pixel-identical to the
//! general shader, and class runs must never reorder painter sequence.
use peniko::{Blob, ImageAlphaType, ImageData, ImageFormat};
use plico_wgpu::{Draw, Quad, Renderer, image::ImageQuad, mask::CoverageMask};
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
    // Other specialized classes: border/gradient fill and box shadows.
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
    let mut inset = Quad::new([120., 72., 60., 60.], [0., 0., 0., 0.6]);
    inset.radii = [8.; 4];
    inset.shadow_sigma = 2.;
    inset.shadow_hole = Some([5., 5., 50., 50.]);
    vec![
        sheet,
        clipped,
        translucent,
        square,
        bordered,
        gradient,
        shadow,
        inset,
    ]
}

fn warm_classes(renderer: &mut Renderer, quads: &[Quad], size: [u32; 2]) -> Vec<u8> {
    let mut last = Default::default();
    for _ in 0..2_000 {
        let pixels = renderer.render(quads, size, [0.05; 4], 0.).unwrap();
        let stats = renderer.batch_stats();
        last = stats;
        if stats.plain_draw_calls == 1 && stats.fill_draw_calls == 1 && stats.shadow_draw_calls == 1
        {
            return pixels;
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    panic!("class pipeline warmup did not finish: {last:?}");
}

fn image_fixture() -> ImageQuad {
    let pixels = (0..64)
        .flat_map(|i| {
            [
                ((i * 47) % 256) as u8,
                ((i * 83 + 11) % 256) as u8,
                ((i * 29 + 37) % 256) as u8,
                255,
            ]
        })
        .collect::<Vec<_>>();
    let mut image = ImageQuad::new(
        Quad::new([8., 8., 176., 128.], [1.; 4]),
        ImageData {
            data: Blob::new(Arc::new(pixels)),
            format: ImageFormat::Rgba8,
            alpha_type: ImageAlphaType::AlphaPremultiplied,
            width: 8,
            height: 8,
        },
    );
    image.quad.radii = [18., 4., 12., 24.];
    image.quad.clip = Some([12., 10., 164., 120.]);
    image.quad.clip_antialias = true;
    image.quality = peniko::ImageQuality::High;
    image
}

#[test]
#[ignore = "requires a working wgpu adapter"]
fn specialized_image_pipeline_is_pixel_identical_to_general_sampling() {
    let mut renderer = pollster::block_on(Renderer::new()).unwrap();
    let image = image_fixture();
    let items = [Draw::Image(&image)];
    let mut specialized = Vec::new();
    for _ in 0..2_000 {
        specialized = renderer
            .render_items(&items, [192, 144], [0.03; 4], 0.)
            .unwrap();
        if renderer.batch_stats().image_draw_calls == 1 {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    assert_eq!(renderer.batch_stats().image_draw_calls, 1);
    renderer.force_general_pipelines(true);
    let general = renderer
        .render_items(&items, [192, 144], [0.03; 4], 0.)
        .unwrap();
    assert_eq!(renderer.batch_stats().image_draw_calls, 0);
    assert_eq!(specialized, general);
}

#[cfg(all(target_os = "linux", feature = "system-text"))]
#[test]
#[ignore = "requires system fonts and a working wgpu adapter"]
fn specialized_glyph_pipelines_are_pixel_identical_to_general_sampling() {
    use plico_wgpu::text::{TextRequest, TextRun};

    let mut renderer = pollster::block_on(Renderer::new()).unwrap();
    // Each class must exceed joinArea independently; otherwise MyGO's
    // joinLast intentionally draws the adjacent glyph classes as one general
    // run, which is correct but cannot exercise these two pipelines.
    let mut request = TextRequest::new("AA🙂");
    request.family = "Noto Sans, Noto Sans CJK SC, Noto Color Emoji, sans-serif";
    request.size = 180.;
    let layout = renderer.layout_text(&request).unwrap();
    assert_eq!(layout.missing_glyphs, 0);
    let run = TextRun::new(&layout, [8., 8.], [0.1, 0.3, 0.8, 0.85]);
    let items = [Draw::Text(run)];
    let mut specialized = Vec::new();
    for _ in 0..2_000 {
        specialized = renderer
            .render_items(&items, [640, 240], [1.; 4], 0.)
            .unwrap();
        let stats = renderer.batch_stats();
        if stats.glyph_draw_calls > 0 && stats.color_glyph_draw_calls > 0 {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    let stats = renderer.batch_stats();
    assert!(stats.glyph_draw_calls > 0, "{stats:?}");
    assert!(stats.color_glyph_draw_calls > 0, "{stats:?}");
    renderer.force_general_pipelines(true);
    let general = renderer
        .render_items(&items, [640, 240], [1.; 4], 0.)
        .unwrap();
    assert_eq!(renderer.batch_stats().glyph_draw_calls, 0);
    assert_eq!(renderer.batch_stats().color_glyph_draw_calls, 0);
    assert_eq!(specialized, general);
}

#[cfg(all(target_os = "linux", feature = "system-text"))]
#[test]
#[ignore = "manual GPU profile"]
fn profile_glyph_and_image_class_gpu_cost() {
    use plico_wgpu::text::{TextRequest, TextRun};

    fn median(values: &mut [f64]) -> f64 {
        values.sort_by(f64::total_cmp);
        let middle = values.len() / 2;
        if values.len().is_multiple_of(2) {
            (values[middle - 1] + values[middle]) * 0.5
        } else {
            values[middle]
        }
    }

    let mut renderer = pollster::block_on(Renderer::new()).unwrap();
    let image = image_fixture();
    let image_items = [Draw::Image(&image)];
    let text = "Plico Aa 中文 0123456789 ".repeat(12);
    let mut request = TextRequest::new(&text);
    request.family = "Noto Sans, Noto Sans CJK SC, sans-serif";
    request.size = 64.;
    let layout = renderer.layout_text(&request).unwrap();
    assert_eq!(layout.missing_glyphs, 0);
    let run = TextRun::new(&layout, [8., 8.], [0.1, 0.3, 0.8, 0.9]);
    let text_items = [Draw::Text(run)];

    // Let every background pipeline arrive before alternating modes.
    for _ in 0..2_000 {
        renderer
            .render_items(&image_items, [192, 144], [0.; 4], 0.)
            .unwrap();
        if renderer.batch_stats().image_draw_calls > 0 {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    for (name, items, size) in [
        ("image", image_items.as_slice(), [192, 144]),
        ("glyph", text_items.as_slice(), [1920, 160]),
    ] {
        let mut general = Vec::new();
        let mut classes = Vec::new();
        for sample in 0..42 {
            let force = sample % 2 == 0;
            renderer.force_general_pipelines(force);
            renderer.render_items(items, size, [0.; 4], 0.).unwrap();
            if sample >= 10
                && let Some(gpu) = renderer.frame_budget().gpu_ms
            {
                if force {
                    general.push(gpu);
                } else {
                    classes.push(gpu);
                }
            }
        }
        assert!(!general.is_empty() && !classes.is_empty());
        eprintln!(
            "{name} general_gpu_ms={:.3} class_gpu_ms={:.3}",
            median(&mut general),
            median(&mut classes)
        );
    }
}

#[test]
#[ignore = "requires a working wgpu adapter"]
fn specialized_class_pipelines_are_pixel_identical_to_the_general_shader() {
    let mut renderer = pollster::block_on(Renderer::new()).unwrap();
    let size = [200, 150];
    let quads = fixture();
    let specialized = warm_classes(&mut renderer, &quads, size);
    let stats = renderer.batch_stats();
    assert_eq!(
        stats.plain_draw_calls, 1,
        "adjacent plain fills share one run"
    );
    assert_eq!(
        stats.fill_draw_calls, 1,
        "adjacent border/gradient fills share one specialized run"
    );
    assert_eq!(stats.shadow_draw_calls, 1);
    assert!(
        stats.draw_calls < stats.instances as u64,
        "runs are split by class"
    );
    renderer.force_general_pipelines(true);
    let general = renderer.render(&quads, size, [0.05; 4], 0.).unwrap();
    assert_eq!(renderer.batch_stats().plain_draw_calls, 0);
    assert_eq!(renderer.batch_stats().fill_draw_calls, 0);
    assert_eq!(renderer.batch_stats().shadow_draw_calls, 0);
    assert_eq!(
        specialized, general,
        "the plain/fill class shaders must reproduce the general pipeline exactly"
    );
    // Force-general must be reversible: switching back restores class runs.
    renderer.force_general_pipelines(false);
    renderer.render(&quads, size, [0.05; 4], 0.).unwrap();
    assert_eq!(renderer.batch_stats().plain_draw_calls, 1);
    assert_eq!(renderer.batch_stats().fill_draw_calls, 1);
    assert_eq!(renderer.batch_stats().shadow_draw_calls, 1);
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
    warm_classes(&mut renderer, &fixture(), [200, 150]);
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
    assert_eq!(renderer.batch_stats().shadow_draw_calls, 1);

    // Runtime materials are application-defined and stay on the general path.
    let mut material = Quad::new([0., 0., 16., 16.], [0.2, 0.3, 0.4, 1.]);
    material.material = true;
    renderer.render(&[material], size, [0.; 4], 0.).unwrap();
    assert_eq!(renderer.batch_stats().plain_draw_calls, 0);
    assert_eq!(renderer.batch_stats().fill_draw_calls, 0);
    assert_eq!(renderer.batch_stats().shadow_draw_calls, 0);
}
