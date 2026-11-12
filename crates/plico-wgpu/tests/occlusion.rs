use peniko::{Blob, ImageAlphaType, ImageData, ImageFormat};
use plico_renderer::{PaintScene, Scene};
use plico_wgpu::{Draw, Quad, Renderer, image::ImageQuad};
use std::sync::Arc;

fn image() -> ImageQuad {
    ImageQuad::new(
        Quad::new([40., 40., 64., 64.], [1.; 4]),
        ImageData {
            data: Blob::new(Arc::new(vec![255, 0, 0, 255])),
            format: ImageFormat::Rgba8,
            alpha_type: ImageAlphaType::AlphaPremultiplied,
            width: 1,
            height: 1,
        },
    )
}

#[test]
#[ignore = "requires a working wgpu adapter"]
fn opaque_cover_culls_hidden_work_before_resource_preparation_with_identical_pixels() {
    let hidden = Quad::new([24., 24., 120., 80.], [0.2, 0.7, 0.3, 0.5]);
    let hidden_image = image();
    let cover = Quad::new([0., 0., 192., 144.], [0.1, 0.2, 0.8, 1.]);
    let draws = [
        Draw::Quad(&hidden),
        Draw::Image(&hidden_image),
        Draw::Quad(&cover),
    ];
    let mut renderer = pollster::block_on(Renderer::new()).unwrap();

    let culled = renderer
        .render_items(&draws, [192, 144], [0.; 4], 0.)
        .unwrap();
    let stats = renderer.batch_stats();
    assert_eq!(stats.occlusion_culled_draws, 2);
    assert_eq!(stats.instances, 1);
    assert_eq!(renderer.image_cache_stats().2, 0, "hidden image uploaded");

    renderer.force_no_occlusion_culling(true);
    let complete = renderer
        .render_items(&draws, [192, 144], [0.; 4], 0.)
        .unwrap();
    assert_eq!(renderer.batch_stats().occlusion_culled_draws, 0);
    assert_eq!(renderer.batch_stats().instances, 3);
    assert_eq!(renderer.image_cache_stats().2, 1);
    assert_eq!(culled, complete);

    let mut invalid = image();
    invalid.alpha = f32::NAN;
    assert!(
        renderer
            .render_items(
                &[Draw::Image(&invalid), Draw::Quad(&cover)],
                [192, 144],
                [0.; 4],
                0.,
            )
            .is_err(),
        "occlusion must not hide invalid public input"
    );
}

#[test]
#[ignore = "requires a working wgpu adapter"]
fn hidden_isolated_layer_allocates_no_offscreen_target() {
    use kurbo::{Affine, Rect};
    use peniko::{BlendMode, Color, Fill};

    let mut scene = Scene::new();
    scene.push_layer(
        BlendMode::default(),
        0.6,
        Affine::IDENTITY,
        &Rect::new(20., 20., 140., 110.),
        None,
        None,
    );
    scene.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        Color::from_rgb8(240, 40, 80),
        None,
        &Rect::new(20., 20., 140., 110.),
    );
    scene.pop_layer();
    scene.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        Color::from_rgb8(20, 30, 50),
        None,
        &Rect::new(0., 0., 192., 144.),
    );

    let mut renderer = pollster::block_on(Renderer::new()).unwrap();
    let culled = renderer
        .render_scene(&scene, [192, 144], [0.; 4], false)
        .unwrap();
    assert_eq!(renderer.batch_stats().occlusion_culled_draws, 1);
    assert_eq!(renderer.layer_frame_bytes(), 0);

    let mut oracle = pollster::block_on(Renderer::new()).unwrap();
    oracle.force_no_occlusion_culling(true);
    let complete = oracle
        .render_scene(&scene, [192, 144], [0.; 4], false)
        .unwrap();
    assert!(oracle.layer_frame_bytes() > 0);
    assert_eq!(culled, complete);
}

#[test]
#[ignore = "manual GPU profile"]
fn profile_opaque_panel_stack() {
    fn median(values: &mut [f64]) -> f64 {
        values.sort_by(f64::total_cmp);
        let middle = values.len() / 2;
        if values.len().is_multiple_of(2) {
            (values[middle - 1] + values[middle]) * 0.5
        } else {
            values[middle]
        }
    }

    let size = [1920, 1080];
    let mut quads = (0..12)
        .map(|index| Quad::new([8., 8., 1904., 1064.], [index as f32 / 24., 0.2, 0.5, 0.75]))
        .collect::<Vec<_>>();
    quads.push(Quad::new(
        [0., 0., size[0] as f32, size[1] as f32],
        [0.08, 0.12, 0.2, 1.],
    ));
    let draws = quads.iter().map(Draw::Quad).collect::<Vec<_>>();
    let mut renderer = pollster::block_on(Renderer::new()).unwrap();
    let mut whole = Vec::new();
    let mut culled = Vec::new();
    for sample in 0..42 {
        let disabled = sample % 2 == 0;
        renderer.force_no_occlusion_culling(disabled);
        renderer.render_items(&draws, size, [0.; 4], 0.).unwrap();
        let stats = renderer.batch_stats();
        if disabled {
            assert_eq!(stats.occlusion_culled_draws, 0);
            assert_eq!(stats.instances, 13);
        } else {
            assert_eq!(stats.occlusion_culled_draws, 12);
            assert_eq!(stats.instances, 1);
        }
        if sample >= 10
            && let Some(gpu) = renderer.frame_budget().gpu_ms
        {
            if disabled {
                whole.push(gpu);
            } else {
                culled.push(gpu);
            }
        }
    }
    if whole.is_empty() || culled.is_empty() {
        eprintln!("GPU timestamps unavailable; rerun with PLICO_GPU_TIMER=1");
        return;
    }
    eprintln!(
        "opaque stack whole_gpu_ms={:.3} culled_gpu_ms={:.3}",
        median(&mut whole),
        median(&mut culled)
    );
}
