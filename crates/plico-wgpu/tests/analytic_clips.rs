use kurbo::{Affine, Rect, RoundedRect};
use peniko::{Color, Fill};
use plico_renderer::{PaintScene, Scene};
use plico_wgpu::{Quad, Renderer};

const CLIP_RECT: [f32; 4] = [31.25, 27.5, 961.25, 664.75];
const CLIP_RADIUS: f32 = 42.;
const FILL: [f32; 4] = [35. / 255., 110. / 255., 225. / 255., 220. / 255.];

fn clipped_scene() -> Scene {
    clipped_scene_at(0.)
}

fn clipped_scene_at(offset: f64) -> Scene {
    let mut scene = Scene::new();
    let clip = RoundedRect::from_rect(
        Rect::new(
            CLIP_RECT[0] as f64 + offset,
            CLIP_RECT[1] as f64,
            (CLIP_RECT[0] + CLIP_RECT[2]) as f64 - offset,
            (CLIP_RECT[1] + CLIP_RECT[3]) as f64,
        ),
        CLIP_RADIUS as f64,
    );
    scene.push_clip_layer(Affine::IDENTITY, &clip);
    scene.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        Color::from_rgba8(35, 110, 225, 220),
        None,
        &Rect::new(0., 0., 1024., 720.),
    );
    scene.pop_layer();
    scene
}

#[test]
#[ignore = "requires a working wgpu adapter"]
fn leaf_rounded_clip_matches_direct_geometry_without_a_mask() {
    let scene = clipped_scene();
    let mut analytic = pollster::block_on(Renderer::new()).unwrap();
    let mut masked = pollster::block_on(Renderer::new()).unwrap();
    masked.force_no_analytic_clip_reduction(true);

    let actual = analytic
        .render_scene(&scene, [1024, 720], [0.03, 0.04, 0.06, 1.], false)
        .unwrap();
    let prepared = analytic.prepare_scene(&scene, [1024, 720]).unwrap();
    let draws = prepared.draws();
    let replayed = analytic
        .render_items(&draws, [1024, 720], [0.03, 0.04, 0.06, 1.], 0.)
        .unwrap();
    let masked_pixels = masked
        .render_scene(&scene, [1024, 720], [0.03, 0.04, 0.06, 1.], false)
        .unwrap();
    let mut direct = pollster::block_on(Renderer::new()).unwrap();
    let mut quad = Quad::new(CLIP_RECT, FILL);
    quad.radii = [CLIP_RADIUS; 4];
    let expected = direct
        .render(&[quad], [1024, 720], [0.03, 0.04, 0.06, 1.], 0.)
        .unwrap();

    let differences = actual
        .iter()
        .zip(&expected)
        .enumerate()
        .filter_map(|(index, (&actual, &expected))| {
            (actual != expected).then_some((index, actual, expected))
        })
        .collect::<Vec<_>>();
    assert!(
        differences.is_empty(),
        "{} differing bytes, first {:?}",
        differences.len(),
        &differences[..differences.len().min(16)]
    );
    assert_eq!(
        replayed, expected,
        "public prepared draw replay lost its clip"
    );
    assert_ne!(
        masked_pixels, expected,
        "legacy mask path unexpectedly retained the double-AA edge"
    );
    assert_eq!(analytic.batch_stats().analytic_clip_draws, 1);
    assert_eq!(masked.batch_stats().analytic_clip_draws, 0);
    assert_eq!(analytic.raster_cache_stats()[0], (0, 0));
    assert!(masked.raster_cache_stats()[0].0 > 0);
}

#[test]
#[ignore = "requires a working wgpu adapter"]
fn nested_clip_keeps_the_existing_mask_path() {
    let mut scene = Scene::new();
    scene.push_clip_layer(
        Affine::IDENTITY,
        &RoundedRect::from_rect(Rect::new(8., 8., 120., 88.), 18.),
    );
    scene.push_clip_layer(Affine::IDENTITY, &Rect::new(16., 12., 112., 80.));
    scene.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        Color::WHITE,
        None,
        &Rect::new(0., 0., 128., 96.),
    );
    scene.pop_layer();
    scene.pop_layer();

    let mut renderer = pollster::block_on(Renderer::new()).unwrap();
    renderer
        .render_scene(&scene, [128, 96], [0.; 4], false)
        .unwrap();
    assert_eq!(renderer.batch_stats().analytic_clip_draws, 0);
    assert!(renderer.raster_cache_stats()[0].0 > 0);
}

#[test]
#[ignore = "requires a working wgpu adapter"]
fn rounded_clip_never_becomes_a_rectangular_occluder() {
    let mut scene = Scene::new();
    scene.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        Color::from_rgb8(220, 30, 40),
        None,
        &Rect::new(0., 0., 96., 72.),
    );
    scene.push_clip_layer(
        Affine::IDENTITY,
        &RoundedRect::from_rect(Rect::new(8., 8., 88., 64.), 24.),
    );
    scene.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        Color::from_rgb8(20, 80, 230),
        None,
        &Rect::new(0., 0., 96., 72.),
    );
    scene.pop_layer();

    let mut renderer = pollster::block_on(Renderer::new()).unwrap();
    let pixels = renderer
        .render_scene(&scene, [96, 72], [0.; 4], false)
        .unwrap();
    assert_eq!(&pixels[..4], &[220, 30, 40, 255]);
    assert_eq!(renderer.batch_stats().occlusion_culled_draws, 0);
    assert_eq!(renderer.batch_stats().analytic_clip_draws, 1);
}

#[test]
#[ignore = "manual GPU profile"]
fn profile_leaf_rounded_clip() {
    fn median(values: &mut [f64]) -> f64 {
        values.sort_by(f64::total_cmp);
        let middle = values.len() / 2;
        if values.len().is_multiple_of(2) {
            (values[middle - 1] + values[middle]) * 0.5
        } else {
            values[middle]
        }
    }

    let scene = clipped_scene();
    let mut analytic = pollster::block_on(Renderer::new()).unwrap();
    let mut masked = pollster::block_on(Renderer::new()).unwrap();
    masked.force_no_analytic_clip_reduction(true);
    let mut analytic_gpu = Vec::new();
    let mut masked_gpu = Vec::new();
    let mut analytic_wall = Vec::new();
    let mut masked_wall = Vec::new();
    for sample in 0..80 {
        let (renderer, gpu, wall) = if sample % 2 == 0 {
            (&mut masked, &mut masked_gpu, &mut masked_wall)
        } else {
            (&mut analytic, &mut analytic_gpu, &mut analytic_wall)
        };
        let started = std::time::Instant::now();
        renderer
            .render_scene(&scene, [1024, 720], [0.03, 0.04, 0.06, 1.], false)
            .unwrap();
        if sample >= 20 {
            wall.push(started.elapsed().as_secs_f64() * 1e3);
            gpu.extend(renderer.frame_budget().gpu_ms);
        }
    }
    eprintln!(
        "rounded clip masked_wall_ms={:.3} analytic_wall_ms={:.3} masked_gpu_ms={} analytic_gpu_ms={}",
        median(&mut masked_wall),
        median(&mut analytic_wall),
        if masked_gpu.is_empty() {
            "n/a".into()
        } else {
            format!("{:.3}", median(&mut masked_gpu))
        },
        if analytic_gpu.is_empty() {
            "n/a".into()
        } else {
            format!("{:.3}", median(&mut analytic_gpu))
        },
    );

    let mut analytic_prepare = Vec::new();
    let mut masked_prepare = Vec::new();
    for sample in 0..60 {
        let scene = clipped_scene_at(sample as f64 * 0.137);
        let measure = |renderer: &mut Renderer, samples: &mut Vec<f64>| {
            let started = std::time::Instant::now();
            renderer.prepare_scene(&scene, [1024, 720]).unwrap();
            samples.push(started.elapsed().as_secs_f64() * 1e3);
        };
        if sample % 2 == 0 {
            measure(&mut masked, &mut masked_prepare);
            measure(&mut analytic, &mut analytic_prepare);
        } else {
            measure(&mut analytic, &mut analytic_prepare);
            measure(&mut masked, &mut masked_prepare);
        }
    }
    eprintln!(
        "dynamic rounded clip masked_prepare_ms={:.3} analytic_prepare_ms={:.3}",
        median(&mut masked_prepare),
        median(&mut analytic_prepare),
    );
}
