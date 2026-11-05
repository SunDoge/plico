//! Manual measurements only: production scene planning, no full-frame readback
//! in timed loops. Includes the same full-screen composition as window present.
use super::Renderer;
use kurbo::{Affine, Circle, Rect};
use peniko::{Color, Fill};
use plico_renderer::{PaintCommand, PaintScene, Scene, paragraph::ParagraphRequest, style::Paint};
use std::{sync::Arc, time::Instant};

fn label(scene: &mut Scene, text: String, x: f64, y: f64) {
    let request = ParagraphRequest::from_paint(
        &Paint {
            text: Some(text.into()),
            font_size: 16.,
            ..Default::default()
        },
        Some(250.),
    )
    .unwrap();
    scene.commands.push(PaintCommand::Paragraph {
        request: Arc::new(request),
        transform: Affine::translate((x, y)),
    });
}
fn fixtures(workload: &str, scale: f64) -> Vec<Scene> {
    let mut body = Scene::new();
    for row in 0..18 {
        for col in 0..4 {
            let x = 24. + col as f64 * 310.;
            let y = 112. + row as f64 * 36.;
            body.fill(
                Fill::NonZero,
                Affine::IDENTITY,
                Color::from_rgb8(236, 239, 244),
                None,
                &Rect::new(x, y, x + 294., y + 31.).to_rounded_rect(5.),
            );
            label(
                &mut body,
                format!("Item {:02} · 中文 / 日本語", row * 4 + col),
                x + 8.,
                y + 5.,
            );
        }
    }
    (0..24)
        .map(|frame| {
            let mut root = Scene::new();
            let background = if workload == "full" {
                Color::from_rgb8(248 - frame, 248, 248)
            } else {
                Color::from_rgb8(248, 248, 248)
            };
            root.fill(
                Fill::NonZero,
                Affine::IDENTITY,
                background,
                None,
                &Rect::new(0., 0., 1280., 800.),
            );
            label(&mut root, "Plico damage dashboard".into(), 24., 16.);
            let scroll = if workload == "scroll" {
                f64::from(frame) * 1.25
            } else {
                0.
            };
            root.push_clip_layer(Affine::IDENTITY, &Rect::new(0., 104., 1280., 792.));
            root.append_cached_scene(body.clone(), Affine::translate((0., -scroll)));
            root.pop_layer();
            root.fill(
                Fill::NonZero,
                Affine::IDENTITY,
                Color::from_rgb8(220, 225, 234),
                None,
                &Rect::new(24., 60., 424., 68.),
            );
            let value = if workload == "progress" {
                f64::from(frame) * 12. + 40.
            } else {
                200.
            };
            root.fill(
                Fill::NonZero,
                Affine::IDENTITY,
                Color::from_rgb8(45, 110, 220),
                None,
                &Rect::new(24., 60., 24. + value, 68.),
            );
            for (cx, cy) in [(620., 52.), (930., 52.), (1220., 52.)] {
                for dot in 0..8 {
                    let angle = f64::from(dot) * std::f64::consts::TAU / 8.;
                    let phase = if workload == "spinners" {
                        f64::from(frame) * 0.35
                    } else {
                        0.
                    };
                    let alpha = (60. + 195. * (phase - angle).sin().max(0.)) as u8;
                    root.fill(
                        Fill::NonZero,
                        Affine::IDENTITY,
                        Color::from_rgba8(45, 110, 220, alpha),
                        None,
                        &Circle::new((cx + angle.cos() * 14., cy + angle.sin() * 14.), 3.),
                    );
                }
            }
            if scale == 1. {
                root
            } else {
                let mut physical = Scene::new();
                physical.append_cached_scene(root, Affine::scale(scale));
                physical
            }
        })
        .collect()
}
struct Timer {
    queries: wgpu::QuerySet,
    resolve: wgpu::Buffer,
    readback: wgpu::Buffer,
}
impl Timer {
    fn new(device: &wgpu::Device) -> Self {
        assert!(device.features().contains(wgpu::Features::TIMESTAMP_QUERY));
        Self {
            queries: device.create_query_set(&wgpu::QuerySetDescriptor {
                label: None,
                ty: wgpu::QueryType::Timestamp,
                count: 2,
            }),
            resolve: device.create_buffer(&wgpu::BufferDescriptor {
                label: None,
                size: 16,
                usage: wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC,
                mapped_at_creation: false,
            }),
            readback: device.create_buffer(&wgpu::BufferDescriptor {
                label: None,
                size: 16,
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                mapped_at_creation: false,
            }),
        }
    }
    fn stamp(&self, device: &wgpu::Device, index: u32) -> wgpu::CommandEncoder {
        let mut encoder = device.create_command_encoder(&Default::default());
        {
            let _pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("damage profile boundary"),
                timestamp_writes: Some(wgpu::ComputePassTimestampWrites {
                    query_set: &self.queries,
                    beginning_of_pass_write_index: Some(index),
                    end_of_pass_write_index: None,
                }),
            });
        }
        if index == 1 {
            encoder.resolve_query_set(&self.queries, 0..2, &self.resolve, 0);
            encoder.copy_buffer_to_buffer(&self.resolve, 0, &self.readback, 0, 16);
        }
        encoder
    }
    fn read_ms(&self, renderer: &Renderer) -> f64 {
        let (tx, rx) = std::sync::mpsc::channel();
        self.readback
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |result| {
                tx.send(result).unwrap();
            });
        renderer
            .device
            .poll(wgpu::PollType::wait_indefinitely())
            .unwrap();
        rx.recv().unwrap().unwrap();
        let bytes = self.readback.slice(..).get_mapped_range().unwrap();
        let first = u64::from_ne_bytes(bytes[0..8].try_into().unwrap());
        let last = u64::from_ne_bytes(bytes[8..16].try_into().unwrap());
        drop(bytes);
        self.readback.unmap();
        last.wrapping_sub(first) as f64 * f64::from(renderer.queue.get_timestamp_period()) / 1e6
    }
}
fn percentile(samples: &mut [f64], p: usize) -> f64 {
    samples.sort_by(f64::total_cmp);
    samples[(samples.len() * p / 100).min(samples.len() - 1)]
}
#[test]
#[ignore = "manual hardware GPU damage benchmark; not a CI speed threshold"]
fn compare_damage_cpu_gpu_and_pixels() {
    profile_compare(false);
}
#[test]
#[ignore = "manual GPU batch-culling A/B benchmark; not a CI speed threshold"]
fn compare_batch_culling_cpu_gpu_and_pixels() {
    profile_compare(true);
}
fn profile_compare(batch_ab: bool) {
    let instance = wgpu::Instance::new(crate::instance_descriptor());
    let mut renderer = pollster::block_on(Renderer::new_compatible(&instance, None, true)).unwrap();
    eprintln!("damage_adapter={:?}", renderer.adapter_info());
    let timer = Timer::new(&renderer.device);
    for scale in [1., 2.] {
        let size = [(1280. * scale) as u32, (800. * scale) as u32];
        let output = renderer.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("damage profile window-sized destination"),
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
        let destination = output.create_view(&Default::default());
        for workload in ["idle", "progress", "spinners", "scroll", "full"] {
            let scenes = fixtures(workload, scale);
            // Pixel oracle outside measurements, including frame removal/reset.
            let mut oracle = pollster::block_on(Renderer::new()).unwrap();
            renderer.invalidate_scene_damage();
            for scene in scenes.iter().take(8).chain([&Scene::new()]) {
                let actual = renderer.render_scene(scene, size, [1.; 4], true).unwrap();
                let expected = oracle.render_scene(scene, size, [1.; 4], false).unwrap();
                assert!(
                    actual == expected,
                    "damage pixel mismatch: {workload}, scale={scale}"
                );
            }
            for round in 0..4 {
                let modes = if round % 2 == 0 {
                    [false, true]
                } else {
                    [true, false]
                };
                for mode in modes {
                    renderer.profile_no_batch_cull = batch_ab && !mode;
                    let damage = batch_ab || mode;
                    renderer.invalidate_scene_damage();
                    for scene in &scenes {
                        renderer
                            .render_scene_to_view(
                                scene,
                                size,
                                [1.; 4],
                                damage,
                                &destination,
                                wgpu::TextureFormat::Rgba8Unorm,
                            )
                            .unwrap();
                    }
                    renderer
                        .device
                        .poll(wgpu::PollType::wait_indefinitely())
                        .unwrap();
                    let (mut cpu, mut gpu, mut complete) = (Vec::new(), Vec::new(), Vec::new());
                    let (mut skipped, mut instances, mut draws, mut coverage) = (0, 0, 0, 0.);
                    for frame in 0..96 {
                        let scene = &scenes[frame % scenes.len()];
                        let start = Instant::now();
                        let Some(mut encoder) = renderer
                            .encode_scene_for_profile(scene, size, damage)
                            .unwrap()
                        else {
                            cpu.push(start.elapsed().as_secs_f64() * 1000.);
                            gpu.push(0.);
                            complete.push(start.elapsed().as_secs_f64() * 1000.);
                            skipped += 1;
                            continue;
                        };
                        if renderer.blitter.is_none() {
                            renderer.blitter = Some(crate::blit::Blitter::new(
                                &renderer.device,
                                wgpu::TextureFormat::Rgba8Unorm,
                            ));
                        }
                        renderer.blitter.as_mut().unwrap().encode(
                            &renderer.device,
                            &mut encoder,
                            &renderer.target.as_ref().unwrap().view,
                            &destination,
                        );
                        let encode_ms = start.elapsed().as_secs_f64() * 1000.;
                        let prefix = timer.stamp(&renderer.device, 0);
                        let suffix = timer.stamp(&renderer.device, 1);
                        let submit = Instant::now();
                        // All three buffers are queued together: timestamps do not
                        // include CPU preparation or an intentional host idle gap.
                        renderer
                            .queue
                            .submit([prefix.finish(), encoder.finish(), suffix.finish()]);
                        cpu.push(encode_ms + submit.elapsed().as_secs_f64() * 1000.);
                        gpu.push(timer.read_ms(&renderer));
                        complete.push(start.elapsed().as_secs_f64() * 1000.);
                        let stats = renderer.batch_stats();
                        instances += stats.instances;
                        draws += stats.draw_calls;
                        coverage += renderer.scene_damage_coverage(size);
                    }
                    eprintln!(
                        "{},{workload},{scale},{round},{mode},{:.6},{:.6},{:.6},{:.6},{:.6},{skipped},{instances},{draws},{:.6}",
                        if batch_ab {
                            "batch_sample"
                        } else {
                            "damage_sample"
                        },
                        percentile(&mut cpu, 50),
                        percentile(&mut cpu, 95),
                        percentile(&mut gpu, 50),
                        percentile(&mut gpu, 95),
                        percentile(&mut complete, 50),
                        coverage / 96.
                    );
                }
            }
        }
    }
}

#[test]
#[ignore = "requires a working wgpu adapter"]
fn full_fallback_preserves_history_for_small_and_unchanged_frames() {
    let mut partial = pollster::block_on(Renderer::new()).unwrap();
    let mut full = pollster::block_on(Renderer::new()).unwrap();
    let mut scene = Scene::new();
    let make = |color| {
        let mut scene = Scene::new();
        scene.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            color,
            None,
            &Rect::new(0., 0., 80., 100.),
        );
        scene
    };
    for color in [Color::BLACK, Color::from_rgb8(30, 90, 200)] {
        scene = make(color);
        let actual = partial
            .render_scene(&scene, [100, 100], [1.; 4], true)
            .unwrap();
        let expected = full
            .render_scene(&scene, [100, 100], [1.; 4], false)
            .unwrap();
        assert!(actual == expected);
        assert_eq!(
            partial.batch_stats().clears,
            0,
            "broad damage should use a full pass"
        );
        assert_eq!(partial.batch_stats().draw_calls, 1);
    }
    scene.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        Color::BLACK,
        None,
        &Rect::new(90., 20., 94., 24.),
    );
    let actual = partial
        .render_scene(&scene, [100, 100], [1.; 4], true)
        .unwrap();
    let expected = full
        .render_scene(&scene, [100, 100], [1.; 4], false)
        .unwrap();
    assert!(actual == expected);
    assert!(partial.scene_damage_coverage([100, 100]) < 0.5);
    assert!(
        partial.batch_stats().clears > 0,
        "full fallback must not invalidate next frame's history"
    );
    let unchanged = partial
        .render_scene(&scene, [100, 100], [1.; 4], true)
        .unwrap();
    assert!(unchanged == actual);
    assert_eq!(partial.batch_stats().draw_calls, 0);
}
