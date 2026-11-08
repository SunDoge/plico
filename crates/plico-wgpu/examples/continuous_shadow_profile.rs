//! Fixed full-redraw probe; CPU preparation + GPU completion, without readback.
//! Wall time, not GPU timestamps or real-window presentation latency.
//! Fill/clip contours stay continuous; only shadow smoothing changes.
//! Run with `cargo run -p plico-wgpu --example continuous_shadow_profile`.
use kurbo::{Affine, Rect, Shape};
use peniko::{Color, Fill};
use plico_renderer::{
    PaintScene, Scene, ShadowProfile, corner_shape::corner_shape, style::CornerRadii,
};
use std::time::Instant;

fn panel(scene: &mut Scene, rect: Rect, radius: f64, smoothing: f32) {
    let contour = corner_shape(rect, CornerRadii::uniform(radius as f32), 1.);
    let mut outside = rect.inflate(40., 40.).to_path(0.1);
    outside.extend(
        contour
            .to_path(0.1)
            .reverse_subpaths()
            .elements()
            .iter()
            .copied(),
    );
    scene.push_clip_layer(Affine::IDENTITY, &outside);
    for (offset, sigma, opacity) in [(1., 1.5, 0.12), (4., 5., 0.14)] {
        scene.draw_box_shadow_profile(
            Affine::translate((0., offset)),
            rect,
            Color::from_rgba8(0, 0, 0, (opacity * 255.) as u8),
            ShadowProfile {
                radius,
                std_dev: sigma,
                corner_smoothing: smoothing,
            },
        );
    }
    scene.pop_layer();
    scene.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        Color::WHITE,
        None,
        &contour,
    );
}
fn frame(scale: f64, smoothing: f32, n: usize) -> Scene {
    let mut scene = Scene::new();
    panel(&mut scene, Rect::new(36., 32., 604., 368.), 24., smoothing);
    for row in 0..3 {
        for col in 0..4 {
            let x = 64. + f64::from(col) * 132.;
            let y = 76. + f64::from(row) * 84. + (n % 2) as f64 * 0.25;
            panel(
                &mut scene,
                Rect::new(x, y, x + 112., y + 48.),
                12.,
                smoothing,
            );
        }
    }
    let mut transformed = Scene::new();
    transformed.append_scene(scene, Affine::scale(scale));
    transformed
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::LowPower,
        ..Default::default()
    }))?;
    let (device, queue) =
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default()))?;
    let mut renderer = pollster::block_on(plico_wgpu::Renderer::from_device(
        adapter,
        device.clone(),
        queue,
    ))?;
    println!("adapter: {:?}", renderer.adapter_info());
    for scale in [1., 2.] {
        for smoothing in [0., 1.] {
            let size = [(640. * scale) as u32, (400. * scale) as u32];
            let target = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("shadow cost probe"),
                size: wgpu::Extent3d {
                    width: size[0],
                    height: size[1],
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[],
            });
            let view = target.create_view(&wgpu::TextureViewDescriptor::default());
            for n in 0..3 {
                renderer.render_scene_to_view(
                    &frame(scale, smoothing, n),
                    size,
                    [0.92, 0.93, 0.95, 1.],
                    false,
                    &view,
                    wgpu::TextureFormat::Rgba8Unorm,
                )?;
                device.poll(wgpu::PollType::wait_indefinitely())?;
            }
            let mut times = Vec::new();
            for n in 0..8 {
                let start = Instant::now();
                renderer.render_scene_to_view(
                    &frame(scale, smoothing, n),
                    size,
                    [0.92, 0.93, 0.95, 1.],
                    false,
                    &view,
                    wgpu::TextureFormat::Rgba8Unorm,
                )?;
                device.poll(wgpu::PollType::wait_indefinitely())?;
                times.push(start.elapsed().as_secs_f64() * 1000.);
            }
            times.sort_by(f64::total_cmp);
            println!(
                "scale={scale} smoothing={smoothing} median_ms={:.3} max_ms={:.3} batch={:?} layers={:?}",
                (times[3] + times[4]) * 0.5,
                times[7],
                renderer.batch_stats(),
                renderer.layer_pool_stats()
            );
        }
    }
    Ok(())
}
