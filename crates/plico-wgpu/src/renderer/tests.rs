#[test]
fn interior_clip_preserves_edges_masks_and_nonfill_instances() {
    use bytemuck::Zeroable;
    let mut base = super::Instance::zeroed();
    base.rect = [10., 10., 20., 20.];
    base.clip = [0., 0., 40., 40.];
    base.clip_params[0] = 1.;
    for (rect, expected) in [
        ([10., 10., 20., 20.], -1.),
        ([3., 3., 34., 34.], -1.),
        ([2.99, 3., 34., 34.], 1.),
        ([10., 10., 27.01, 20.], 1.),
        ([0., 0., 40., 40.], 1.),
    ] {
        let mut i = base;
        i.rect = rect;
        super::skip_interior_clip(&mut i);
        assert_eq!(i.clip_params[0], expected, "{rect:?}");
    }
    for kind in 0..5 {
        let mut i = base;
        match kind {
            0 => i.clip_params[2] = 1., // rounded/path mask
            1 => i.config[1] = 1.,      // material
            2 => i.config[2] = 1.,      // shadow
            3 => i.color2[0] = 1.,      // gradient
            _ => i.color[3] = 3.,       // deferred layer marker
        }
        super::skip_interior_clip(&mut i);
        assert_eq!(i.clip_params[0], 1.);
    }
}

#[test]
#[ignore = "requires a working wgpu adapter"]
fn interior_clip_matches_reference_pixels() {
    let mut renderer = pollster::block_on(super::Renderer::new()).unwrap();
    for general in [false, true] {
        renderer.force_general_pipelines(general);
        for aa in [false, true] {
            for clip in [
                [0., 0., 96., 80.],
                [4.25, 5.75, 75.5, 60.25],
                [15.25, 8.5, 20.5, 40.25],
            ] {
                let quads: Vec<_> = (0..8)
                    .map(|i| {
                        let mut q = crate::Quad::new(
                            [8.25 + i as f32 * 8., 12.5, 18.25, 33.75],
                            [0.2, 0.6, 0.9, 0.6],
                        );
                        q.radii = [0., 3., 7., 9.];
                        q.clip = Some(clip);
                        q.clip_antialias = aa;
                        q
                    })
                    .collect();
                renderer.profile_no_clip_skip = true;
                let reference = renderer
                    .render(&quads, [96, 80], [0.1, 0.2, 0.3, 1.], 0.)
                    .unwrap();
                assert_eq!(renderer.batch_stats().clip_coverage_skips, 0);
                renderer.profile_no_clip_skip = false;
                let actual = renderer
                    .render(&quads, [96, 80], [0.1, 0.2, 0.3, 1.], 0.)
                    .unwrap();
                if clip[0] == 0. {
                    assert_eq!(renderer.batch_stats().clip_coverage_skips, 8);
                }
                assert_eq!(actual, reference, "general={general} aa={aa} clip={clip:?}");
            }
        }
    }
}

#[test]
#[ignore = "manual timing experiment; includes GPU completion wait, no CI threshold"]
fn profile_interior_clip() {
    let mut renderer = pollster::block_on(super::Renderer::new()).unwrap();
    let size = [1536, 1024];
    let texture = renderer.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("interior clip benchmark"),
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
    let view = texture.create_view(&Default::default());
    let quads: Vec<_> = (0..2000)
        .map(|i| {
            let mut q = crate::Quad::new(
                [
                    8. + (i % 40) as f32 * 37.,
                    8. + (i / 40) as f32 * 19.,
                    32.,
                    14.,
                ],
                [0.2, 0.5, 0.8, 0.8],
            );
            q.clip_antialias = true;
            q
        })
        .collect();
    let items: Vec<_> = quads.iter().map(crate::Draw::Quad).collect();
    eprintln!("adapter={:?}", renderer.adapter_info());
    for general in [false, true] {
        renderer.force_general_pipelines(general);
        let mut reference = Vec::new();
        let mut optimized = Vec::new();
        let mut skipped = 0;
        // Alternate the paths per pair, and reverse order to avoid one path
        // always paying the first-use/clock-ramp cost. No pixel readback.
        for pair in 0..120 {
            for off in if pair % 2 == 0 {
                [true, false]
            } else {
                [false, true]
            } {
                renderer.profile_no_clip_skip = off;
                let start = std::time::Instant::now();
                renderer
                    .render_to_view(
                        &items,
                        size,
                        [1.; 4],
                        0.,
                        &view,
                        wgpu::TextureFormat::Rgba8Unorm,
                    )
                    .unwrap();
                renderer
                    .device
                    .poll(wgpu::PollType::wait_indefinitely())
                    .unwrap();
                if !off {
                    skipped = renderer.batch_stats().clip_coverage_skips;
                    assert_eq!(skipped, 2000);
                }
                if pair >= 20 {
                    if off { &mut reference } else { &mut optimized }
                        .push(start.elapsed().as_secs_f64() * 1e6);
                }
            }
        }
        reference.sort_by(f64::total_cmp);
        optimized.sort_by(f64::total_cmp);
        eprintln!(
            "general={general} completed_frame_us reference_p50={:.2} reference_p95={:.2} optimized_p50={:.2} optimized_p95={:.2} skipped={}",
            reference[50], reference[95], optimized[50], optimized[95], skipped
        );
    }
}
#[test]
#[ignore = "requires a working wgpu adapter"]
fn abandoned_prepared_frames_preserve_partial_damage_and_pixels() {
    use super::*;
    use plico_renderer::{PaintScene, Scene};
    let mut renderer = pollster::block_on(Renderer::new()).unwrap();
    let size = [128, 128];
    let background = [0., 0., 0., 1.];
    let baseline = Scene::new();
    renderer
        .render_scene(&baseline, size, background, true)
        .unwrap();
    let probe = renderer
        .prepare_scene_frame(&baseline, size, background)
        .unwrap();
    assert!(renderer.prepared_frame_unchanged(&probe, true));
    assert!(!renderer.prepared_frame_unchanged(&probe, false));
    let other_background = renderer
        .prepare_scene_frame(&baseline, size, [1.; 4])
        .unwrap();
    assert!(!renderer.prepared_frame_unchanged(&other_background, true));
    renderer.invalidate_scene_damage();
    assert!(!renderer.prepared_frame_unchanged(&probe, true));
    renderer
        .render_scene(&baseline, size, background, true)
        .unwrap();
    let mut abandoned = Scene::new();
    abandoned.fill(
        peniko::Fill::NonZero,
        kurbo::Affine::IDENTITY,
        peniko::Color::WHITE,
        None,
        &kurbo::Rect::new(0., 0., 128., 128.),
    );
    drop(
        renderer
            .prepare_scene_frame(&abandoned, size, [1.; 4])
            .unwrap(),
    );
    assert!(
        renderer
            .plan_scene_frame(&baseline, size, background, true)
            .unwrap()
            .unchanged(),
        "a prepared but unplanned frame must not alter retained history/background"
    );
    let mut latest = Scene::new();
    latest.fill(
        peniko::Fill::NonZero,
        kurbo::Affine::IDENTITY,
        peniko::Color::from_rgb8(255, 0, 0),
        None,
        &kurbo::Rect::new(8., 8., 20., 20.),
    );
    let prepared = renderer
        .prepare_scene_frame(&latest, size, background)
        .unwrap();
    let plan = renderer.plan_prepared_frame(prepared, true);
    let coverage = renderer.scene_damage_coverage(size);
    assert!(coverage > 0. && coverage < 0.1, "coverage {coverage}");
    let encoder = renderer.encode_scene_plan(plan).unwrap();
    let actual = renderer.read_frame(encoder, size).unwrap();
    let mut reference = pollster::block_on(Renderer::new()).unwrap();
    assert_eq!(
        actual,
        reference
            .render_scene(&latest, size, background, false)
            .unwrap()
    );
}

#[test]
#[ignore = "requires a working wgpu adapter"]
fn stable_native_raster_windows_preserve_clipped_gpu_pixels() {
    use super::*;
    use crate::{Draw, image::ImageQuad, native_raster_window::cache_window};
    use peniko::{Blob, ImageAlphaType, ImageData, ImageFormat};
    use std::sync::Arc;
    let mut renderer = pollster::block_on(Renderer::new()).unwrap();
    let ink = [0., 0., 1024., 64.];
    let visible = [160., 8., 224., 48.];
    let window = cache_window(ink, visible);
    assert_ne!(window, ink);
    let make_image = |rect: [f64; 4], ratio: f32, shift: f32| {
        let [left, top, right, bottom] = rect.map(|v| v as u32);
        let pixels = (top..bottom)
            .flat_map(|y| {
                (left..right).flat_map(move |x| {
                    // Stable native-pixel coordinates, including translucent edges.
                    let a = ((x * 13 + y * 7) % 128 + 128) as u8;
                    [((x * 3) % 128) as u8, ((y * 5) % 128) as u8, 30, a]
                })
            })
            .collect::<Vec<_>>();
        let mut quad = Quad::new(
            [
                left as f32 * ratio - 130. + shift,
                top as f32 * ratio + 5. + shift,
                (right - left) as f32 * ratio,
                (bottom - top) as f32 * ratio,
            ],
            [1.; 4],
        );
        quad.clip = Some([
            visible[0] as f32 * ratio - 130. + shift,
            visible[1] as f32 * ratio + 5. + shift,
            (visible[2] - visible[0]) as f32 * ratio,
            (visible[3] - visible[1]) as f32 * ratio,
        ]);
        ImageQuad::new(
            quad,
            ImageData {
                data: Blob::new(Arc::new(pixels)),
                width: right - left,
                height: bottom - top,
                format: ImageFormat::Rgba8,
                alpha_type: ImageAlphaType::AlphaPremultiplied,
            },
        )
    };
    for ratio in [0.75, 1., 1.25] {
        for shift in [0., 0.25, 0.5, 0.75] {
            let full = make_image(ink, ratio, shift);
            let cached = make_image(window, ratio, shift);
            let expected = renderer
                .render_items(&[Draw::Image(&full)], [256, 128], [0.; 4], 0.)
                .unwrap();
            let actual = renderer
                .render_items(&[Draw::Image(&cached)], [256, 128], [0.; 4], 0.)
                .unwrap();
            assert!(
                actual.iter().zip(expected).all(|(a, b)| a.abs_diff(b) <= 1),
                "bitmap window changed GPU coverage/sampling at {ratio} / {shift}"
            );
        }
    }
}

#[test]
#[ignore = "requires a working wgpu adapter"]
fn coverage_masks_use_one_gpu_byte_per_pixel_without_aliasing_rgba_images() {
    use peniko::{Blob, ImageAlphaType, ImageData, ImageFormat};
    let mut renderer = pollster::block_on(Renderer::new()).unwrap();
    let image = ImageData {
        data: Blob::new(std::sync::Arc::new(vec![10, 20, 30, 40, 50, 60, 70, 80])),
        format: ImageFormat::Rgba8,
        alpha_type: ImageAlphaType::AlphaPremultiplied,
        width: 2,
        height: 1,
    };
    renderer
        .images
        .get_mask(
            &renderer.device,
            &renderer.queue,
            &renderer.atlas_texture.layout,
            &image,
        )
        .unwrap();
    assert_eq!(renderer.image_cache_stats().1, 2);
    renderer
        .images
        .get(
            &renderer.device,
            &renderer.queue,
            &renderer.atlas_texture.layout,
            &image,
            crate::image::ImageSampling {
                quality: peniko::ImageQuality::Low,
                extend: [peniko::Extend::Pad; 2],
                mipmaps: false,
            },
        )
        .unwrap();
    assert_eq!(
        (
            renderer.image_cache_stats().0,
            renderer.image_cache_stats().1
        ),
        (2, 10)
    );
}

use super::*;
#[test]
#[ignore = "requires a working wgpu adapter"]
fn accepted_material_replacement_invalidates_layers_and_damage_but_rejection_does_not() {
    use plico_renderer::{PaintScene, Scene};
    let mut renderer = pollster::block_on(Renderer::new()).unwrap();
    let mut scene = Scene::new();
    scene.push_layer(
        peniko::Compose::Plus,
        0.5,
        kurbo::Affine::IDENTITY,
        &kurbo::Rect::new(0., 0., 16., 16.),
        None,
        None,
    );
    scene.fill(
        peniko::Fill::NonZero,
        kurbo::Affine::IDENTITY,
        peniko::Color::WHITE,
        None,
        &kurbo::Rect::new(0., 0., 16., 16.),
    );
    scene.pop_layer();
    let original = renderer
        .render_scene(&scene, [32, 32], [0.; 4], true)
        .unwrap();
    assert_eq!(
        renderer
            .render_scene(&scene, [32, 32], [0.; 4], true)
            .unwrap(),
        original
    );
    assert_eq!(renderer.scene_damage_coverage([32, 32]), 0.);
    let layers = renderer.layer_pool_stats();
    assert!(layers.entries > 0);
    let compilations = renderer.pipeline_compilations();
    assert!(pollster::block_on(renderer.set_material("invalid WGSL")).is_err());
    assert!(!pollster::block_on(renderer.set_material(DEFAULT_MATERIAL)).unwrap());
    assert_eq!(renderer.layer_pool_stats().entries, layers.entries);
    assert_eq!(renderer.pipeline_compilations(), compilations);
    assert_eq!(
        renderer
            .render_scene(&scene, [32, 32], [0.; 4], true)
            .unwrap(),
        original
    );
    assert_eq!(renderer.scene_damage_coverage([32, 32]), 0.);
    assert!(
        pollster::block_on(
            renderer.set_material("fn plico_material(m: Material)->vec4f {return m.color;}")
        )
        .unwrap()
    );
    assert_eq!(renderer.layer_pool_stats().entries, 0);
    assert!(renderer.plus_pipeline.is_none());
    assert_eq!(
        renderer
            .render_scene(&scene, [32, 32], [0.; 4], true)
            .unwrap(),
        original
    );
    assert_eq!(renderer.scene_damage_coverage([32, 32]), 1.);
    assert!(renderer.layer_pool_stats().allocations > layers.allocations);
    assert!(renderer.plus_pipeline.is_some());
}

#[test]
#[ignore = "requires a working wgpu adapter"]
fn cpu_mip_fallback_matches_gpu_for_borrowed_and_converted_uploads() {
    use peniko::{Blob, ImageAlphaType, ImageData, ImageFormat};
    use std::sync::Arc;
    let mut gpu = pollster::block_on(Renderer::new()).unwrap();
    let mut cpu = pollster::block_on(Renderer::new()).unwrap();
    cpu.images.force_cpu_mips();
    for format in [ImageFormat::Rgba8, ImageFormat::Bgra8] {
        for alpha in [ImageAlphaType::Alpha, ImageAlphaType::AlphaPremultiplied] {
            let source: Vec<u8> = (0..65 * 67)
                .flat_map(|i| {
                    let mut pixel = [((i * 13) % 128) as u8, 20, 3, 128];
                    if format == ImageFormat::Bgra8 {
                        pixel.swap(0, 2);
                    }
                    pixel
                })
                .collect();
            let image = crate::image::ImageQuad {
                quality: peniko::ImageQuality::High,
                ..crate::image::ImageQuad::new(
                    Quad::new([0., 0., 7., 5.], [1.; 4]),
                    ImageData {
                        data: Blob::new(Arc::new(source)),
                        width: 65,
                        height: 67,
                        format,
                        alpha_type: alpha,
                    },
                )
            };
            let expected = cpu
                .render_items(&[crate::Draw::Image(&image)], [7, 5], [0.; 4], 0.)
                .unwrap();
            let actual = gpu
                .render_items(&[crate::Draw::Image(&image)], [7, 5], [0.; 4], 0.)
                .unwrap();
            assert!(
                actual.iter().zip(expected).all(|(a, b)| a.abs_diff(b) <= 3),
                "{format:?} {alpha:?}"
            );
            assert!(
                actual
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .all(|pixel| pixel[3] == 128)
            );
        }
    }
}

#[test]
#[ignore = "manual GPU staging comparison; run with --ignored --nocapture"]
fn profile_explicit_staging_against_queue_write_texture() {
    use std::time::Instant;
    let renderer = pollster::block_on(Renderer::new()).unwrap();
    eprintln!("adapter: {:?}", renderer.adapter_info());
    for [width, height] in [[4096u32, 3072u32], [4095, 3071]] {
        let source: Vec<u8> = (0..width as usize * height as usize * 4)
            .map(|i| (i % 251) as u8)
            .collect();
        for mapped in [false, true] {
            let mut samples = Vec::new();
            for index in 0..7 {
                let oracle = renderer.device.create_buffer(&wgpu::BufferDescriptor {
                    label: None,
                    size: 768,
                    usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                    mapped_at_creation: false,
                });
                renderer
                    .device
                    .poll(wgpu::PollType::wait_indefinitely())
                    .unwrap();
                let start = Instant::now();
                let texture = renderer.device.create_texture(&wgpu::TextureDescriptor {
                    label: Some("upload staging comparison"),
                    size: wgpu::Extent3d {
                        width,
                        height,
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: wgpu::TextureFormat::Rgba8Unorm,
                    usage: wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::COPY_SRC,
                    view_formats: &[],
                });
                let mut encoder = renderer.device.create_command_encoder(&Default::default());
                let destination = wgpu::TexelCopyTextureInfo {
                    texture: &texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                };
                if mapped {
                    let pitch = (width * 4).div_ceil(256) * 256;
                    let upload = renderer.device.create_buffer(&wgpu::BufferDescriptor {
                        label: Some("explicit staging comparison"),
                        size: u64::from(pitch) * u64::from(height),
                        usage: wgpu::BufferUsages::COPY_SRC,
                        mapped_at_creation: true,
                    });
                    {
                        let mut data = upload.slice(..).get_mapped_range_mut().unwrap();
                        for (row, input) in source.chunks_exact(width as usize * 4).enumerate() {
                            let start = row * pitch as usize;
                            data.slice(start..start + input.len())
                                .copy_from_slice(input);
                        }
                    }
                    upload.unmap();
                    encoder.copy_buffer_to_texture(
                        wgpu::TexelCopyBufferInfo {
                            buffer: &upload,
                            layout: wgpu::TexelCopyBufferLayout {
                                offset: 0,
                                bytes_per_row: Some(pitch),
                                rows_per_image: None,
                            },
                        },
                        destination,
                        texture.size(),
                    );
                } else {
                    renderer.queue.write_texture(
                        destination,
                        &source,
                        wgpu::TexelCopyBufferLayout {
                            offset: 0,
                            bytes_per_row: Some(width * 4),
                            rows_per_image: None,
                        },
                        texture.size(),
                    );
                }
                let points = [[0, 0], [width / 2, height / 2], [width - 1, height - 1]];
                for (slot, [x, y]) in points.into_iter().enumerate() {
                    encoder.copy_texture_to_buffer(
                        wgpu::TexelCopyTextureInfo {
                            texture: &texture,
                            mip_level: 0,
                            origin: wgpu::Origin3d { x, y, z: 0 },
                            aspect: wgpu::TextureAspect::All,
                        },
                        wgpu::TexelCopyBufferInfo {
                            buffer: &oracle,
                            layout: wgpu::TexelCopyBufferLayout {
                                offset: slot as u64 * 256,
                                bytes_per_row: Some(256),
                                rows_per_image: None,
                            },
                        },
                        wgpu::Extent3d {
                            width: 1,
                            height: 1,
                            depth_or_array_layers: 1,
                        },
                    );
                }
                renderer.queue.submit([encoder.finish()]);
                let cpu = start.elapsed();
                let start = Instant::now();
                let (tx, rx) = std::sync::mpsc::channel();
                oracle
                    .slice(..)
                    .map_async(wgpu::MapMode::Read, move |result| {
                        tx.send(result).unwrap();
                    });
                renderer
                    .device
                    .poll(wgpu::PollType::wait_indefinitely())
                    .unwrap();
                rx.recv().unwrap().unwrap();
                let wait = start.elapsed();
                let actual = oracle.slice(..).get_mapped_range().unwrap();
                for (slot, [x, y]) in points.into_iter().enumerate() {
                    let offset = (y as usize * width as usize + x as usize) * 4;
                    assert_eq!(
                        &actual[slot * 256..slot * 256 + 4],
                        &source[offset..offset + 4]
                    );
                }
                if index == 0 {
                    eprintln!("{width}x{height} mapped={mapped} first: cpu={cpu:?}, wait={wait:?}");
                } else {
                    samples.push((cpu, wait));
                }
            }
            samples.sort_by_key(|sample| sample.0);
            let mut waits: Vec<_> = samples.iter().map(|sample| sample.1).collect();
            waits.sort();
            let mut totals: Vec<_> = samples.iter().map(|sample| sample.0 + sample.1).collect();
            totals.sort();
            eprintln!(
                "{width}x{height} mapped={mapped} paired_total_upper_median={:?}",
                totals[3]
            );
            eprintln!(
                "{width}x{height} mapped={mapped} warm n=6: cpu_upper_median={:?}, wait_upper_median={:?}",
                samples[3].0, waits[3]
            );
        }
    }
}

#[test]
#[ignore = "manual GPU upload profile; run with --ignored --nocapture"]
fn profile_image_upload_cpu_and_gpu_wait() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("off")),
        )
        .with_span_events(tracing_subscriber::fmt::format::FmtSpan::CLOSE)
        .with_ansi(false)
        .with_test_writer()
        .try_init();
    use peniko::{Blob, ImageAlphaType, ImageData, ImageFormat};
    use std::{sync::Arc, time::Instant};
    let mut renderer = pollster::block_on(Renderer::new()).unwrap();
    assert!(crate::mipmap::MipGenerator::supported(&renderer.device));
    eprintln!("adapter: {:?}", renderer.adapter_info());
    for size in [[4096, 3072], [4095, 3071]] {
        let pixels = vec![128u8; size[0] as usize * size[1] as usize * 4];
        for (mode, cpu, mipmaps) in [
            ("base-only", false, false),
            ("cpu-mips", true, true),
            ("gpu-mips", false, true),
        ] {
            renderer.trim_image_cache();
            // Reset the test override and pipeline outside the measured interval.
            renderer.images = Default::default();
            if cpu {
                renderer.images.force_cpu_mips();
            }
            let mut samples = Vec::new();
            for index in 0..7 {
                let source = ImageData {
                    data: Blob::new(Arc::new(pixels.clone())),
                    width: size[0],
                    height: size[1],
                    format: ImageFormat::Rgba8,
                    alpha_type: ImageAlphaType::AlphaPremultiplied,
                };
                renderer
                    .device
                    .poll(wgpu::PollType::wait_indefinitely())
                    .unwrap();
                let start = Instant::now();
                let image = renderer
                    .images
                    .get(
                        &renderer.device,
                        &renderer.queue,
                        &renderer.atlas_texture.layout,
                        &source,
                        crate::image::ImageSampling {
                            quality: peniko::ImageQuality::High,
                            extend: [peniko::Extend::Pad; 2],
                            mipmaps,
                        },
                    )
                    .unwrap();
                // This profile bypasses frame encoding, so explicitly submit
                // the cache's deferred mip work. Production prepends it to the
                // frame's single submission.
                if let Some(mips) = renderer.images.pending_mip_commands(&renderer.device) {
                    renderer.queue.submit([mips]);
                    renderer.images.commit_pending_mips();
                } else {
                    renderer.queue.submit([]);
                }
                let cpu_time = start.elapsed();
                let wait = Instant::now();
                renderer
                    .device
                    .poll(wgpu::PollType::wait_indefinitely())
                    .unwrap();
                let gpu_wait = wait.elapsed();
                std::hint::black_box(image);
                if index == 0 {
                    eprintln!(
                        "{size:?} mode={mode} first: cpu={cpu_time:?}, subsequent_wait={gpu_wait:?}"
                    );
                } else {
                    samples.push((cpu_time, gpu_wait));
                }
            }
            samples.sort_by_key(|sample| sample.0);
            let mut waits: Vec<_> = samples.iter().map(|sample| sample.1).collect();
            waits.sort();
            eprintln!(
                "{size:?} mode={mode} warm n=6: cpu_upper_median={:?}, wait_upper_median={:?}",
                samples[3].0, waits[3]
            );
        }
    }
}

#[test]
#[ignore = "requires a working wgpu adapter"]
fn gpu_mip_levels_match_cpu_area_reference_and_preserve_premultiplication() {
    let renderer = pollster::block_on(Renderer::new()).unwrap();
    assert!(crate::mipmap::MipGenerator::supported(&renderer.device));
    let generator = crate::mipmap::MipGenerator::new(&renderer.device);
    for size in [[64u32, 64u32], [65, 67], [257, 193], [1, 129], [3, 1]] {
        let source: Vec<u8> = (0..size[0] * size[1])
            .flat_map(|i| {
                let alpha = (i % 251) as u8;
                [
                    ((i * 17) % (u32::from(alpha) + 1)) as u8,
                    alpha / 2,
                    0,
                    alpha,
                ]
            })
            .collect();
        let levels = 32 - size[0].max(size[1]).leading_zeros();
        let texture = renderer.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("mip CPU oracle"),
            size: wgpu::Extent3d {
                width: size[0],
                height: size[1],
                depth_or_array_layers: 1,
            },
            mip_level_count: levels,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::STORAGE_BINDING
                | wgpu::TextureUsages::COPY_DST
                | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        renderer.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &source,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(size[0] * 4),
                rows_per_image: None,
            },
            texture.size(),
        );
        let mut mip_encoder = renderer.device.create_command_encoder(&Default::default());
        generator.encode(&renderer.device, &mut mip_encoder, &texture);
        renderer.queue.submit([mip_encoder.finish()]);
        let mut previous_gpu = source.clone();
        let mut reference = source;
        let mut previous_size = size;
        for level in 1..levels {
            let target = [(size[0] >> level).max(1), (size[1] >> level).max(1)];
            let pitch = (target[0] * 4).div_ceil(256) * 256;
            let buffer = renderer.device.create_buffer(&wgpu::BufferDescriptor {
                label: None,
                size: u64::from(pitch) * u64::from(target[1]),
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                mapped_at_creation: false,
            });
            let mut encoder = renderer.device.create_command_encoder(&Default::default());
            encoder.copy_texture_to_buffer(
                wgpu::TexelCopyTextureInfo {
                    texture: &texture,
                    mip_level: level,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                wgpu::TexelCopyBufferInfo {
                    buffer: &buffer,
                    layout: wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(pitch),
                        rows_per_image: None,
                    },
                },
                wgpu::Extent3d {
                    width: target[0],
                    height: target[1],
                    depth_or_array_layers: 1,
                },
            );
            renderer.queue.submit([encoder.finish()]);
            let (tx, rx) = std::sync::mpsc::channel();
            buffer
                .slice(..)
                .map_async(wgpu::MapMode::Read, move |result| {
                    tx.send(result).unwrap();
                });
            renderer
                .device
                .poll(wgpu::PollType::wait_indefinitely())
                .unwrap();
            rx.recv().unwrap().unwrap();
            let mapped = buffer.slice(..).get_mapped_range().unwrap();
            let actual: Vec<u8> = mapped
                .chunks_exact(pitch as usize)
                .flat_map(|row| row[..target[0] as usize * 4].iter().copied())
                .collect();
            let step = crate::image::area_reduce(&previous_gpu, previous_size, target);
            reference = crate::image::area_reduce(&reference, previous_size, target);
            assert!(
                actual.iter().zip(&step).all(|(a, b)| a.abs_diff(*b) <= 1),
                "step {size:?} level {level}"
            );
            assert!(
                actual
                    .iter()
                    .zip(&reference)
                    .all(|(a, b)| a.abs_diff(*b) <= 3),
                "chain {size:?} level {level}"
            );
            for pixel in actual.as_chunks::<4>().0 {
                assert!(pixel[..3].iter().all(|c| *c <= pixel[3]));
            }
            previous_gpu = actual;
            previous_size = target;
        }
    }
}

#[test]
#[ignore = "requires a working wgpu adapter"]
fn mip_uploads_remain_retryable_and_join_the_frame_submission() {
    use crate::{Draw, image::ImageQuad};
    use peniko::{Blob, ImageAlphaType, ImageData, ImageFormat};
    use std::sync::Arc;

    let mut renderer = pollster::block_on(Renderer::new()).unwrap();
    assert!(crate::mipmap::MipGenerator::supported(&renderer.device));
    let image = ImageQuad::new(
        Quad::new([0., 0., 16., 16.], [1.; 4]),
        ImageData {
            data: Blob::new(Arc::new(vec![128; 65 * 67 * 4])),
            width: 65,
            height: 67,
            format: ImageFormat::Rgba8,
            alpha_type: ImageAlphaType::AlphaPremultiplied,
        },
    );
    let draws = [Draw::Image(&image)];

    let abandoned = renderer
        .encode_frame(&draws, [32, 32], [0.; 4], 0.)
        .unwrap();
    assert_eq!(renderer.images.pending_mip_count(), 1);
    assert!(!renderer.frame_prefix_commands.is_empty());
    renderer.frame_prefix_commands.clear();
    drop(abandoned);

    let retry = renderer
        .encode_frame(&draws, [32, 32], [0.; 4], 0.)
        .unwrap();
    assert_eq!(renderer.images.pending_mip_count(), 1);
    assert!(!renderer.frame_prefix_commands.is_empty());
    let pixels = renderer.read_frame(retry, [32, 32]).unwrap();
    assert_eq!(pixels.len(), 32 * 32 * 4);
    assert_eq!(renderer.images.pending_mip_count(), 0);
    assert_eq!(renderer.frame_budget().submissions, 1);
    assert_eq!(renderer.frame_budget().command_buffers, 2);
}

#[test]
#[ignore = "requires a working wgpu adapter"]
fn readback_trim_preserves_target_and_recreates_only_for_capture() {
    let mut renderer = pollster::block_on(Renderer::new()).unwrap();
    assert_eq!(renderer.target_storage_bytes(), (0, 0));
    let quad = Quad::new([0., 0., 17., 19.], [1., 0., 0., 1.]);
    let expected = renderer.render(&[quad], [17, 19], [0.; 4], 0.).unwrap();
    assert_eq!(renderer.target_storage_bytes(), (17 * 19 * 4, 256 * 19));
    let view = renderer.target.as_ref().unwrap().view.clone();
    renderer.trim_readback_buffer();
    assert_eq!(renderer.target_storage_bytes(), (17 * 19 * 4, 0));
    let encoder = renderer
        .encode_frame(&[crate::Draw::Quad(&quad)], [17, 19], [0.; 4], 0.)
        .unwrap();
    renderer.queue.submit([encoder.finish()]);
    assert_eq!(renderer.target_storage_bytes(), (17 * 19 * 4, 0));
    assert_eq!(renderer.target.as_ref().unwrap().view, view);
    assert_eq!(
        renderer.render(&[quad], [17, 19], [0.; 4], 0.).unwrap(),
        expected
    );
    assert_eq!(renderer.target_storage_bytes(), (17 * 19 * 4, 256 * 19));
    renderer.trim_readback_buffer();
    renderer.trim_readback_buffer();
    assert_eq!(renderer.target_storage_bytes().1, 0);
}

#[test]
#[ignore = "requires a working wgpu adapter"]
fn cache_trim_dispatches_completed_work_and_preserves_retained_pixels() {
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };
    let mut renderer = pollster::block_on(Renderer::new()).unwrap();
    let quad = Quad::new([0., 0., 8., 8.], [1., 0., 0., 1.]);
    // Readback has already waited for this submission, so no sleeps or
    // timing assumptions are needed to observe its completion callback.
    let expected = renderer.render(&[quad], [16, 16], [0.; 4], 0.).unwrap();
    let target = renderer.target.as_ref().unwrap().view.clone();
    let completed = Arc::new(AtomicBool::new(false));
    let notify = completed.clone();
    renderer
        .queue
        .on_submitted_work_done(move || notify.store(true, Ordering::SeqCst));
    assert!(!completed.load(Ordering::SeqCst));
    renderer.trim_caches().unwrap();
    assert!(
        completed.load(Ordering::SeqCst),
        "trim must process completed submission callbacks"
    );
    assert_eq!(renderer.target.as_ref().unwrap().view, target);
    assert_eq!(renderer.target_storage_bytes().1, 0);
    assert_eq!(renderer.instance_storage_bytes().1, 0);
    let (size, pixels) = renderer.read_retained_rgba().unwrap().unwrap();
    assert_eq!(size, [16, 16]);
    assert_eq!(pixels, expected);
}

#[test]
#[ignore = "requires a working wgpu adapter"]
fn geometry_trim_releases_peak_capacity_preserves_pixels_and_keeps_pipelines_hot() {
    use plico_renderer::{PaintScene, Scene};
    let mut renderer = pollster::block_on(Renderer::new()).unwrap();
    let mut scene = Scene::new();
    for _ in 0..1024 {
        scene.fill(
            peniko::Fill::NonZero,
            kurbo::Affine::IDENTITY,
            peniko::Color::WHITE,
            None,
            &kurbo::Rect::new(1., 1., 8., 8.),
        );
    }
    let size = [16, 16];
    let expected = renderer.render_scene(&scene, size, [0.; 4], true).unwrap();
    let baseline_bytes = INITIAL_INSTANCE_CAPACITY as u64 * std::mem::size_of::<Instance>() as u64;
    assert!(renderer.instance_storage_bytes().0 > baseline_bytes);
    assert!(renderer.instance_storage_bytes().1 > 0);
    let compilations = renderer.pipeline_compilations();
    let plan = renderer
        .plan_scene_frame(&scene, size, [0.; 4], false)
        .unwrap();
    let encoder = renderer.encode_scene_plan(plan).unwrap();
    let target = renderer.target.as_ref().unwrap().view.clone();
    renderer.trim_geometry_buffers();
    assert_eq!(renderer.instance_storage_bytes(), (baseline_bytes, 0));
    assert_eq!(renderer.target.as_ref().unwrap().view, target);
    assert!(
        renderer
            .plan_scene_frame(&scene, size, [0.; 4], true)
            .unwrap()
            .unchanged()
    );
    // Replacing the renderer's buffer cannot invalidate an encoder that
    // already captured the old buffer and has not been submitted yet.
    assert_eq!(renderer.read_frame(encoder, size).unwrap(), expected);
    let baseline_buffer = renderer.instances.clone();
    renderer.trim_geometry_buffers();
    assert_eq!(
        renderer.instances, baseline_buffer,
        "repeated trim must not reallocate the startup buffer"
    );
    assert_eq!(renderer.instance_storage_bytes(), (baseline_bytes, 0));
    assert_eq!(
        renderer.render_scene(&scene, size, [0.; 4], false).unwrap(),
        expected
    );
    assert!(renderer.instance_storage_bytes().0 > baseline_bytes);
    assert_eq!(renderer.pipeline_compilations(), compilations);
}

#[test]
#[ignore = "requires a working wgpu adapter"]
fn encoded_layers_release_scene_owners_before_submission_and_after_failure() {
    use kurbo::{Affine, Rect};
    use plico_renderer::{PaintScene, Scene};
    let mut renderer = pollster::block_on(Renderer::new()).unwrap();
    let mut scene = Scene::new();
    for alpha in [0.8, 0.7] {
        scene.push_layer(
            peniko::BlendMode::default(),
            alpha,
            Affine::IDENTITY,
            &Rect::new(0., 0., 32., 32.),
            None,
            None,
        );
    }
    scene.fill(
        peniko::Fill::NonZero,
        Affine::IDENTITY,
        peniko::Color::WHITE,
        None,
        &Rect::new(4., 4., 20., 20.),
    );
    let source = plico_renderer::ShaderEffectSource::with_backdrop(
        "fn plico_effect(uv:vec2f)->vec4f { return plico_backdrop_at(uv); }",
        true,
    )
    .unwrap();
    let mut text = plico_renderer::TextContext::new();
    let mut paint = plico_renderer::widget::PaintContext::new(32., 32., 1., &mut text);
    paint.draw_shader_effect(
        plico_renderer::ShaderEffectId::new(),
        source,
        0.,
        Vec::<f32>::new().into(),
    );
    scene.append_scene(paint.finish(), Affine::IDENTITY);
    scene.pop_layer();
    scene.pop_layer();
    let expected = renderer
        .render_scene(&scene, [32, 32], [0.; 4], false)
        .unwrap();
    assert_eq!(
        renderer.frame_budget().submissions,
        1,
        "isolated and backdrop-reading child layers share the frame submission"
    );
    assert_eq!(
        renderer.frame_budget().command_buffers,
        1,
        "isolated and backdrop-reading child layers share the root encoder"
    );
    let broken = crate::image::ImageQuad::new(
        Quad::new([0., 0., 1., 1.], [1.; 4]),
        peniko::ImageData {
            data: peniko::Blob::new(std::sync::Arc::new(vec![255u8])),
            format: peniko::ImageFormat::Rgba8,
            alpha_type: peniko::ImageAlphaType::AlphaPremultiplied,
            width: 1,
            height: 1,
        },
    );
    for fail in [false, true] {
        let prepared = renderer.prepare_scene(&scene, [32, 32]).unwrap();
        let mut draws = prepared.draws();
        let weak = draws
            .iter()
            .find_map(|draw| match draw {
                crate::Draw::Layer(layer) => Some(std::rc::Rc::downgrade(&layer.scene)),
                _ => None,
            })
            .expect("nested scene must produce an isolated layer");
        if fail {
            draws.push(crate::Draw::Image(&broken));
        }
        let encoder = renderer.encode_frame(&draws, [32, 32], [0.; 4], 0.);
        assert_eq!(encoder.is_err(), fail);
        drop(draws);
        drop(prepared);
        renderer.trim_raster_cache();
        assert!(
            weak.upgrade().is_none(),
            "pass-local layer references outlived cache trim (failure={fail})"
        );
        if let Ok(encoder) = encoder {
            // GPU commands own everything needed even after all CPU layer
            // owners were released, before this encoder is submitted.
            assert_eq!(renderer.read_frame(encoder, [32, 32]).unwrap(), expected);
        }
    }
    assert_eq!(
        renderer
            .render_scene(&scene, [32, 32], [0.; 4], false)
            .unwrap(),
        expected
    );
}

#[test]
#[ignore = "requires a working wgpu adapter"]
fn encoded_frames_release_temporary_image_owners_on_success_and_failure() {
    use crate::{Draw, image::ImageQuad};
    use peniko::{Blob, ImageAlphaType, ImageData, ImageFormat};
    use std::sync::Arc;
    let instance = wgpu::Instance::new(crate::instance_descriptor());
    let mut renderer =
        pollster::block_on(Renderer::new_compatible(&instance, None, true)).unwrap();
    let image = ImageQuad::new(
        Quad::new([0., 0., 8., 8.], [1.; 4]),
        ImageData {
            data: Blob::new(Arc::new(vec![255u8, 0, 0, 255])),
            width: 1,
            height: 1,
            format: ImageFormat::Rgba8,
            alpha_type: ImageAlphaType::AlphaPremultiplied,
        },
    );
    let pixels = renderer
        .render_items(&[Draw::Image(&image)], [16, 16], [0.; 4], 0.)
        .unwrap();
    assert_eq!(&pixels[..4], &[255, 0, 0, 255]);
    assert!(renderer.frame_images.is_empty());
    assert!(renderer.frame_image_slots.is_empty());
    // A second upload fails after the first image has entered the pass table.
    let mut truncated = ImageQuad::new(image.quad, image.image.clone());
    truncated.image.width = 2;
    assert!(
        renderer
            .encode_frame(
                &[Draw::Image(&image), Draw::Image(&truncated)],
                [16, 16],
                [0.; 4],
                0.,
            )
            .is_err()
    );
    assert!(renderer.frame_images.is_empty());
    assert!(renderer.frame_image_slots.is_empty());
    assert!(
        renderer.pending_gpu_slot.is_none(),
        "a failed shared encoder must release its timestamp slot"
    );
    assert_eq!(
        renderer
            .render_items(&[Draw::Image(&image)], [16, 16], [0.; 4], 0.)
            .unwrap(),
        pixels
    );
}

#[cfg(feature = "window")]
#[test]
#[ignore = "requires a working wgpu adapter"]
fn startup_frame_is_complete_before_exposure_without_readback_allocation() {
    use plico_renderer::{PaintScene, Scene};
    let mut scene = Scene::new();
    scene.fill(
        peniko::Fill::NonZero,
        kurbo::Affine::IDENTITY,
        peniko::Color::from_rgba8(55, 120, 190, 210),
        None,
        &kurbo::Rect::new(3., 2., 41., 28.).to_rounded_rect(7.),
    );
    let mut reference = pollster::block_on(Renderer::new()).unwrap();
    let expected = reference
        .render_scene(&scene, [48, 32], [0.; 4], false)
        .unwrap();
    let mut renderer = pollster::block_on(Renderer::new()).unwrap();
    let plan = renderer
        .plan_scene_frame(&scene, [48, 32], [0.; 4], true)
        .unwrap();
    assert!(!plan.unchanged());
    let copy = renderer
        .prime_scene_plan(plan, wgpu::TextureFormat::Bgra8Unorm)
        .unwrap();
    assert_eq!(
        renderer.target_storage_bytes().1,
        0,
        "startup must not allocate screenshot storage"
    );
    assert!(
        renderer.blitter.is_some(),
        "the present pipeline must be ready before exposure"
    );
    let actual = renderer.read_frame(copy, [48, 32]).unwrap();
    assert_eq!(actual, expected);
    assert!(
        renderer
            .plan_scene_frame(&scene, [48, 32], [0.; 4], true)
            .unwrap()
            .unchanged(),
        "startup retains the complete target, not a duplicate image"
    );
}
#[cfg(feature = "window")]
#[test]
#[ignore = "requires a working wgpu adapter"]
fn failed_acquisition_discards_encoded_work_and_requires_a_fresh_frame() {
    use plico_renderer::Scene;
    let scene = Scene::new();
    let mut renderer = pollster::block_on(Renderer::new()).unwrap();
    for _ in 0..32 {
        let plan = renderer
            .plan_scene_frame(&scene, [32, 24], [0.2, 0.3, 0.4, 1.], true)
            .unwrap();
        assert!(
            !plan.unchanged(),
            "unsubmitted work is never a retained frame"
        );
        let encoder = renderer.encode_scene_plan(plan).unwrap();
        renderer.abandon_scene_frame(encoder);
        assert_eq!(renderer.target_storage_bytes().1, 0);
    }
    let plan = renderer
        .plan_scene_frame(&scene, [32, 24], [0.2, 0.3, 0.4, 1.], true)
        .unwrap();
    assert!(!plan.unchanged());
    let encoder = renderer.encode_scene_plan(plan).unwrap();
    let actual = renderer.read_frame(encoder, [32, 24]).unwrap();
    let mut reference = pollster::block_on(Renderer::new()).unwrap();
    let expected = reference
        .render_scene(&scene, [32, 24], [0.2, 0.3, 0.4, 1.], false)
        .unwrap();
    assert_eq!(
        actual, expected,
        "acquisition recovery matches a fresh complete frame"
    );
    assert!(
        renderer
            .plan_scene_frame(&scene, [32, 24], [0.2, 0.3, 0.4, 1.], true)
            .unwrap()
            .unchanged()
    );
}

#[test]
#[ignore = "requires a working wgpu adapter"]
fn trimming_idle_target_releases_storage_and_rebuilds_unchanged_scene_pixels() {
    use plico_renderer::{PaintScene, Scene};
    let mut renderer = pollster::block_on(Renderer::new()).unwrap();
    let mut scene = Scene::new();
    scene.fill(
        peniko::Fill::NonZero,
        kurbo::Affine::IDENTITY,
        peniko::Color::from_rgba8(35, 120, 210, 180),
        None,
        &kurbo::Rect::new(7., 9., 112., 83.).to_rounded_rect(15.),
    );
    let size = [128, 96];
    let background = [0.2, 0.3, 0.4, 0.5];
    let before = renderer
        .render_scene(&scene, size, background, false)
        .unwrap();
    assert!(renderer.target_storage_bytes().0 > 0);
    assert!(renderer.target_storage_bytes().1 > 0);
    assert!(
        renderer
            .plan_scene_frame(&scene, size, background, true)
            .unwrap()
            .unchanged()
    );
    let retained = renderer.read_retained_rgba().unwrap().unwrap();
    assert_eq!(retained, (size, before.clone()));
    assert!(
        renderer
            .plan_scene_frame(&scene, size, background, true)
            .unwrap()
            .unchanged(),
        "readback cannot invalidate an otherwise unchanged frame"
    );
    renderer.trim_frame_target();
    assert_eq!(renderer.target_storage_bytes(), (0, 0));
    assert!(renderer.read_retained_rgba().unwrap().is_none());
    assert_eq!(
        renderer.target_storage_bytes(),
        (0, 0),
        "snapshot cannot recreate a trimmed target"
    );
    assert!(
        !renderer
            .plan_scene_frame(&scene, size, background, true)
            .unwrap()
            .unchanged()
    );
    let after = renderer
        .render_scene(&scene, size, background, true)
        .unwrap();
    assert_eq!(after, before);
    assert!(
        renderer
            .plan_scene_frame(&scene, size, background, true)
            .unwrap()
            .unchanged()
    );
}

#[test]
#[ignore = "requires a working wgpu adapter"]
fn scene_present_skip_requires_valid_retained_target() {
    use plico_renderer::Scene;
    let mut renderer = pollster::block_on(Renderer::new()).unwrap();
    let texture = renderer.device.create_texture(&wgpu::TextureDescriptor {
        label: None,
        size: wgpu::Extent3d {
            width: 32,
            height: 32,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let view = texture.create_view(&Default::default());
    let scene = Scene::new();
    let draw = |renderer: &mut Renderer, background| {
        renderer
            .render_scene_to_view(
                &scene,
                [32, 32],
                background,
                true,
                &view,
                wgpu::TextureFormat::Rgba8Unorm,
            )
            .unwrap()
    };
    assert!(draw(&mut renderer, [0.; 4]));
    assert!(!draw(&mut renderer, [0.; 4]));
    renderer.invalidate_scene_damage();
    assert!(draw(&mut renderer, [0.; 4]));
    assert!(draw(&mut renderer, [1.; 4]));
    renderer.render_items(&[], [32, 32], [0.; 4], 0.).unwrap();
    assert!(draw(&mut renderer, [1.; 4]));
    assert!(!draw(&mut renderer, [1.; 4]));
}

#[test]
#[ignore = "requires a working wgpu adapter"]
fn gpu_present_preserves_encoded_pixels_on_unorm_and_srgb_targets() {
    let mut renderer = pollster::block_on(Renderer::new()).unwrap();
    let mut q = Quad::new([2.25, 3.5, 24., 23.], [0.15, 0.35, 0.65, 0.7]);
    q.radii = [5.; 4];
    q.colors[1] = [0.65, 0.2, 0.1, 0.3];
    let mut previous: Option<([u32; 2], wgpu::TextureView)> = None;
    for (index, (size, format)) in [
        ([32, 32], wgpu::TextureFormat::Rgba8Unorm),
        ([32, 32], wgpu::TextureFormat::Rgba8Unorm),
        ([64, 48], wgpu::TextureFormat::Rgba8Unorm),
        ([32, 32], wgpu::TextureFormat::Rgba8UnormSrgb),
        ([32, 32], wgpu::TextureFormat::Bgra8Unorm),
        ([32, 32], wgpu::TextureFormat::Bgra8UnormSrgb),
    ]
    .into_iter()
    .enumerate()
    {
        // Changed content catches a stale binding after target replacement.
        q.colors[0][0] = 0.1 + index as f32 * 0.1;
        let expected = renderer.render(&[q], size, [0.; 4], 0.).unwrap();
        let source = renderer.target.as_ref().unwrap().view.clone();
        if let Some((old_size, old_view)) = &previous {
            assert_eq!(*old_size == size, *old_view == source);
        }
        previous = Some((size, source));
        let texture = renderer.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("present oracle"),
            size: wgpu::Extent3d {
                width: size[0],
                height: size[1],
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        renderer
            .render_to_view(
                &[crate::Draw::Quad(&q)],
                size,
                [0.; 4],
                0.,
                &texture.create_view(&Default::default()),
                format,
            )
            .unwrap();
        let buffer = renderer.device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: 256 * u64::from(size[1]),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = renderer.device.create_command_encoder(&Default::default());
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(256),
                    rows_per_image: Some(size[1]),
                },
            },
            wgpu::Extent3d {
                width: size[0],
                height: size[1],
                depth_or_array_layers: 1,
            },
        );
        renderer.queue.submit([encoder.finish()]);
        let (tx, rx) = std::sync::mpsc::channel();
        buffer.slice(..).map_async(wgpu::MapMode::Read, move |r| {
            tx.send(r).unwrap();
        });
        renderer
            .device
            .poll(wgpu::PollType::wait_indefinitely())
            .unwrap();
        rx.recv().unwrap().unwrap();
        let data = buffer.slice(..).get_mapped_range().unwrap();
        for (y, row) in data.as_chunks::<256>().0.iter().enumerate() {
            for (x, pixel) in row[..size[0] as usize * 4]
                .as_chunks::<4>()
                .0
                .iter()
                .enumerate()
            {
                let mut actual: [u8; 4] = *pixel;
                if matches!(
                    format,
                    wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Bgra8UnormSrgb
                ) {
                    actual.swap(0, 2);
                }
                for c in 0..4 {
                    assert!(
                        actual[c].abs_diff(expected[(y * size[0] as usize + x) * 4 + c]) <= 1,
                        "{format:?} pixel {x},{y} differs: {actual:?}"
                    );
                }
            }
        }
        drop(data);
        buffer.unmap();
    }
}
#[test]
#[ignore = "requires a working wgpu adapter"]
fn tiled_gaussian_matches_direct_sampling_for_edges_and_partial_tiles() {
    use kurbo::{Affine, Rect};
    use peniko::{BlendMode, Color, Fill};
    use plico_renderer::{
        PaintScene, Scene,
        filter::{BlurEdge, GaussianBlur},
    };
    use std::sync::Arc;
    let mut tiled = pollster::block_on(Renderer::new()).unwrap();
    let mut reference = pollster::block_on(Renderer::new()).unwrap();
    tiled.blur = Some(crate::blur::Blur::tiled(&tiled.device));
    for sigma in [0.3, 3., 18., 64.] {
        for edge in [
            BlurEdge::Transparent,
            BlurEdge::Clamp,
            BlurEdge::Repeat,
            BlurEdge::Mirror,
        ] {
            let mut scene = Scene::new();
            scene.push_layer(
                BlendMode::default(),
                0.7,
                Affine::IDENTITY,
                &Rect::new(1.5, 2.25, 46.5, 30.75),
                Some(Arc::new(GaussianBlur { sigma, edge }.into_filter())),
                None,
            );
            scene.fill(
                Fill::NonZero,
                Affine::IDENTITY,
                Color::from_rgba8(200, 30, 100, 173),
                None,
                &Rect::new(-200., -300., 17.5, 400.),
            );
            scene.pop_layer();
            let a = tiled
                .render_scene(&scene, [49, 35], [0.; 4], false)
                .unwrap();
            let b = reference
                .render_scene(&scene, [49, 35], [0.; 4], false)
                .unwrap();
            for (i, (a, b)) in a.iter().zip(&b).enumerate() {
                assert!(
                    a.abs_diff(*b) <= 1,
                    "sigma={sigma} edge={edge:?} byte={i}: {a}/{b}"
                );
            }
        }
    }
}
#[test]
#[ignore = "manual GPU timing experiment; not a CI performance threshold"]
fn profile_gaussian_workspace_cost() {
    use kurbo::{Affine, Rect};
    use peniko::{BlendMode, Color, Fill};
    use plico_renderer::{
        PaintScene, Scene,
        filter::{BlurEdge, GaussianBlur},
    };
    use std::{sync::Arc, time::Instant};
    fn percentiles(values: &mut [f64]) -> (f64, f64) {
        values.sort_by(f64::total_cmp);
        (
            values[values.len() / 2],
            values[(values.len() * 95 / 100).min(values.len() - 1)],
        )
    }
    let instance = wgpu::Instance::new(crate::instance_descriptor());
    let mut renderer = pollster::block_on(Renderer::new_compatible(&instance, None, true)).unwrap();
    if std::env::var_os("PLICO_TEST_TILED_BLUR").is_some() {
        renderer.blur = Some(crate::blur::Blur::tiled(&renderer.device));
        eprintln!("test-only tiled kernel selected");
    }
    eprintln!("adapter={:?}", renderer.adapter_info());
    for (width, height, sigma) in [(128, 96, 3.), (384, 256, 3.), (384, 256, 18.)] {
        let size = [width, height];
        let texture = renderer.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Gaussian timing destination"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let view = texture.create_view(&Default::default());
        // Build outside all timing intervals. Both policies replay identical
        // scene snapshots; neither performs CPU readback or swapchain present.
        let scenes: Vec<_> = (0..32)
            .map(|frame| {
                let mut scene = Scene::new();
                scene.push_layer(
                    BlendMode::default(),
                    0.8,
                    Affine::IDENTITY,
                    &Rect::new(8., 8., width as f64 - 8., height as f64 - 8.),
                    Some(Arc::new(
                        GaussianBlur {
                            sigma,
                            edge: BlurEdge::Transparent,
                        }
                        .into_filter(),
                    )),
                    None,
                );
                scene.fill(
                    Fill::NonZero,
                    Affine::IDENTITY,
                    Color::from_rgb8(50, 120, 230),
                    None,
                    &Rect::new(
                        0.,
                        0.,
                        width as f64 * 0.6 + frame as f64 * 0.25,
                        height as f64,
                    ),
                );
                scene.pop_layer();
                scene
            })
            .collect();
        // Reversed second pair reduces attribution to first-run warming.
        for clear in [false, true, true, false] {
            renderer.trim_layer_pool();
            for scene in scenes.iter().take(8) {
                if clear {
                    renderer.trim_layer_pool();
                }
                renderer
                    .render_scene_to_view(
                        scene,
                        size,
                        [1.; 4],
                        false,
                        &view,
                        wgpu::TextureFormat::Rgba8Unorm,
                    )
                    .unwrap();
                renderer
                    .device
                    .poll(wgpu::PollType::wait_indefinitely())
                    .unwrap();
            }
            let before = renderer.layer_pool_stats();
            let mut submission = Vec::new();
            let mut completion = Vec::new();
            let mut gpu = Vec::new();
            for scene in &scenes {
                let start = Instant::now();
                if clear {
                    renderer.trim_layer_pool();
                }
                renderer
                    .render_scene_to_view(
                        scene,
                        size,
                        [1.; 4],
                        false,
                        &view,
                        wgpu::TextureFormat::Rgba8Unorm,
                    )
                    .unwrap();
                submission.push(start.elapsed().as_secs_f64() * 1000.);
                renderer
                    .device
                    .poll(wgpu::PollType::wait_indefinitely())
                    .unwrap();
                completion.push(start.elapsed().as_secs_f64() * 1000.);
                if let Some(time) = renderer.layer_pool.iter().flatten().find_map(|entry| {
                    entry
                        .blur
                        .as_ref()
                        .and_then(|(work, _)| work.timing.as_ref())
                }) {
                    let (tx, rx) = std::sync::mpsc::channel();
                    time.readback
                        .slice(..)
                        .map_async(wgpu::MapMode::Read, move |r| {
                            let _ = tx.send(r);
                        });
                    renderer
                        .device
                        .poll(wgpu::PollType::wait_indefinitely())
                        .unwrap();
                    rx.recv().unwrap().unwrap();
                    let data = time.readback.slice(..).get_mapped_range().unwrap();
                    let ticks: Vec<_> = data
                        .as_chunks::<8>()
                        .0
                        .iter()
                        .map(|b| u64::from_ne_bytes(*b))
                        .collect();
                    gpu.push(
                        (ticks[1].wrapping_sub(ticks[0]) as f64
                            + ticks[3].wrapping_sub(ticks[2]) as f64)
                            * renderer.queue.get_timestamp_period() as f64
                            / 1e6,
                    );
                    drop(data);
                    time.readback.unmap();
                }
            }
            let stats = renderer.layer_pool_stats();
            let (submit50, submit95) = percentiles(&mut submission);
            let (done50, done95) = percentiles(&mut completion);
            if !gpu.is_empty() {
                let (p50, p95) = percentiles(&mut gpu);
                eprintln!(
                    "blur_gpu_ms {width}x{height} sigma={sigma} clear={clear} p50={p50:.3} p95={p95:.3}"
                );
            } else {
                eprintln!("GPU timestamps unavailable on this adapter");
            }
            eprintln!(
                "blur {width}x{height} sigma={sigma} clear={clear} submission_ms p50={submit50:.3} p95={submit95:.3} completion_ms p50={done50:.3} p95={done95:.3} workspace_allocations={} resident_bytes={}",
                stats.filter_allocations - before.filter_allocations,
                stats.retained_bytes
            );
        }
    }
}

#[test]
#[ignore = "requires a working wgpu adapter"]
fn frame_budget_reports_stages_and_collects_gpu_timestamps() {
    let instance = wgpu::Instance::new(crate::instance_descriptor());
    let mut renderer =
        pollster::block_on(super::Renderer::new_compatible(&instance, None, true)).unwrap();
    let pixels = renderer
        .render_items(&[], [32, 32], [0., 0., 0., 1.], 0.)
        .unwrap();
    assert_eq!(pixels.len(), 32 * 32 * 4);
    let budget = renderer.frame_budget();
    assert_eq!(budget.instances, 0);
    assert_eq!(budget.draw_calls, 0);
    assert_eq!(budget.submissions, 1);
    assert_eq!(budget.command_buffers, 1);
    assert!(budget.upload_bytes >= 48, "frame uniform upload is counted");
    assert!((budget.prepare_ms + budget.encode_ms + budget.submit_ms).is_finite());
    if renderer.gpu_timer.is_some() {
        assert!(
            budget.gpu_ms.is_some(),
            "timestamp queries are available but no sample was collected"
        );
    }
}
