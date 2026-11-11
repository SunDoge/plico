use kurbo::{Affine, Rect, RoundedRect, Stroke};
use peniko::{BlendMode, Color};
use plico_renderer::{PaintScene, Scene};
use plico_wgpu::{Draw, Quad, Renderer};

fn ring(rect: [f32; 4], radius: f32) -> Quad {
    let mut quad = Quad::new(rect, [0.; 4]);
    quad.radii = [radius; 4];
    quad.border_width = 2.;
    quad.border_color = [0.2, 0.6, 1., 0.85];
    quad
}

#[test]
#[ignore = "requires a working wgpu adapter"]
fn large_border_only_quads_skip_their_transparent_interior_exactly() {
    let cases = [
        ring([31.25, 27.5, 901.5, 611.25], 0.),
        ring([31.25, 27.5, 901.5, 611.25], 72.),
    ];
    let mut split = pollster::block_on(Renderer::new()).unwrap();
    let mut whole = pollster::block_on(Renderer::new()).unwrap();
    whole.force_no_ring_splitting(true);
    for quad in &cases {
        let expected = whole
            .render_items(&[Draw::Quad(quad)], [1024, 720], [0.03, 0.04, 0.06, 1.], 0.)
            .unwrap();
        let actual = split
            .render_items(&[Draw::Quad(quad)], [1024, 720], [0.03, 0.04, 0.06, 1.], 0.)
            .unwrap();
        assert_eq!(actual, expected);
        assert_eq!(whole.batch_stats().ring_split_quads, 0);
        assert_eq!(whole.batch_stats().instances, 1);
        assert_eq!(whole.batch_stats().draw_calls, 1);
        assert_eq!(split.batch_stats().ring_split_quads, 1);
        assert_eq!(split.batch_stats().instances, 4);
        assert_eq!(split.batch_stats().draw_calls, 4);
    }
}

#[test]
#[ignore = "requires a working wgpu adapter"]
fn small_and_nontransparent_fills_remain_single_draws() {
    let small = ring([0., 0., 64., 64.], 8.);
    let mut filled = ring([0., 0., 900., 600.], 32.);
    filled.colors = [[0.1, 0.2, 0.3, 0.01]; 2];
    let mut renderer = pollster::block_on(Renderer::new()).unwrap();
    for quad in [&small, &filled] {
        renderer
            .render_items(&[Draw::Quad(quad)], [1024, 720], [0.; 4], 0.)
            .unwrap();
        assert_eq!(renderer.batch_stats().ring_split_quads, 0);
        assert_eq!(renderer.batch_stats().instances, 1);
        assert_eq!(renderer.batch_stats().draw_calls, 1);
    }
}

#[test]
#[ignore = "requires a working wgpu adapter"]
fn split_rings_preserve_damage_and_cropped_layer_coordinates() {
    let mut partial = pollster::block_on(Renderer::new()).unwrap();
    let mut oracle = pollster::block_on(Renderer::new()).unwrap();
    oracle.force_no_ring_splitting(true);
    for frame in 0..6 {
        let mut scene = Scene::new();
        scene.push_layer(
            BlendMode::default(),
            0.8,
            Affine::IDENTITY,
            &Rect::new(80., 50., 940., 650.),
            None,
            None,
        );
        scene.stroke(
            &Stroke::new(2.),
            Affine::translate((frame as f64 * 0.375, 0.)),
            Color::from_rgba8(64, 160, 255, 220),
            None,
            &RoundedRect::from_rect(Rect::new(96., 64., 920., 632.), 48.),
        );
        scene.pop_layer();
        let actual = partial
            .render_scene(&scene, [1024, 720], [0.03, 0.04, 0.06, 1.], true)
            .unwrap();
        let expected = oracle
            .render_scene(&scene, [1024, 720], [0.03, 0.04, 0.06, 1.], false)
            .unwrap();
        assert_eq!(actual, expected, "frame {frame}");
        assert_eq!(partial.batch_stats().ring_split_quads, 1);
        assert_eq!(oracle.batch_stats().ring_split_quads, 0);
    }
}

#[test]
#[ignore = "manual GPU profile"]
fn profile_large_border_only_panel() {
    fn median(samples: &mut [f64]) -> f64 {
        samples.sort_by(f64::total_cmp);
        let middle = samples.len() / 2;
        if samples.len().is_multiple_of(2) {
            (samples[middle - 1] + samples[middle]) * 0.5
        } else {
            samples[middle]
        }
    }

    let quad = ring([8., 8., 1904., 1064.], 48.);
    let mut renderer = pollster::block_on(Renderer::new()).unwrap();
    let mut whole = Vec::new();
    let mut split = Vec::new();
    let mut whole_wall = Vec::new();
    let mut split_wall = Vec::new();
    for sample in 0..50 {
        let disabled = sample % 2 == 0;
        renderer.force_no_ring_splitting(disabled);
        let started = std::time::Instant::now();
        renderer
            .render_items(
                &[Draw::Quad(&quad)],
                [1920, 1080],
                [0.03, 0.04, 0.06, 1.],
                0.,
            )
            .unwrap();
        if sample < 10 {
            continue;
        }
        let wall = started.elapsed().as_secs_f64() * 1e3;
        let gpu = renderer.frame_budget().gpu_ms;
        if disabled {
            whole_wall.push(wall);
            whole.extend(gpu);
        } else {
            split_wall.push(wall);
            split.extend(gpu);
        }
    }
    let whole_gpu = if whole.is_empty() {
        "n/a".into()
    } else {
        format!("{:.3}", median(&mut whole))
    };
    let split_gpu = if split.is_empty() {
        "n/a".into()
    } else {
        format!("{:.3}", median(&mut split))
    };
    eprintln!(
        "ring whole_wall_ms={:.3} split_wall_ms={:.3} whole_gpu_ms={} split_gpu_ms={} split_instances={} split_draws={}",
        median(&mut whole_wall),
        median(&mut split_wall),
        whole_gpu,
        split_gpu,
        renderer.batch_stats().instances,
        renderer.batch_stats().draw_calls,
    );
}
