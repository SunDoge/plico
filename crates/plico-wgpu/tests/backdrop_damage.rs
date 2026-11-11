use kurbo::{Affine, Rect};
use peniko::{Color, Fill};
use plico_renderer::{PaintScene, Scene, ShaderEffectId, ShaderEffectSource, TextContext};
use plico_wgpu::Renderer;

fn pane(text: &mut TextContext, id: ShaderEffectId, rect: Rect, scale: f64, time: f32) -> Scene {
    let source=ShaderEffectSource::with_backdrop("fn plico_effect(uv:vec2f)->vec4f { let c=plico_backdrop_blur(uv,3.*plico.resolution_time_scale.w); return vec4f(c.rgb*vec3f(0.8,0.9,1.),c.a); }",true).unwrap().with_backdrop_prefilter(Some(3.)).unwrap();
    let mut paint = plico_renderer::widget::PaintContext::new(
        rect.width() as f32,
        rect.height() as f32,
        scale,
        text,
    );
    paint.draw_shader_effect(id, source, time, Vec::<f32>::new().into());
    let mut scene = Scene::new();
    scene.append_scene(
        paint.finish(),
        Affine::scale(scale) * Affine::translate((rect.x0, rect.y0)),
    );
    scene
}
fn fill(scene: &mut Scene, rect: Rect, color: Color, scale: f64) {
    scene.fill(Fill::NonZero, Affine::scale(scale), color, None, &rect);
}
fn compare(
    partial: &mut Renderer,
    full: &mut Renderer,
    scene: &Scene,
    size: [u32; 2],
    frame: usize,
) {
    let a = partial.render_scene(scene, size, [1.; 4], true).unwrap();
    let b = full.render_scene(scene, size, [1.; 4], false).unwrap();
    let mismatch = a.iter().zip(&b).position(|(a, b)| a != b);
    assert!(
        mismatch.is_none(),
        "frame {frame}, first differing byte {mismatch:?}"
    );
}

#[test]
#[ignore = "requires a working wgpu adapter"]
fn stable_backdrop_snapshots_reuse_textures_until_the_pool_is_trimmed() {
    for separate in [false, true] {
        let mut renderer = pollster::block_on(Renderer::new()).unwrap();
        renderer.force_separate_backdrops(separate);
        let mut text = TextContext::new();
        let ids = [ShaderEffectId::new(), ShaderEffectId::new()];
        let mut scene = Scene::new();
        fill(
            &mut scene,
            Rect::new(0., 0., 180., 100.),
            Color::from_rgb8(70, 110, 160),
            1.,
        );
        for (id, x) in ids.into_iter().zip([12., 100.]) {
            scene.append_scene(
                pane(&mut text, id, Rect::new(x, 18., x + 64., 82.), 1., 0.),
                Affine::IDENTITY,
            );
        }

        let expected = renderer
            .render_scene(&scene, [180, 100], [1.; 4], false)
            .unwrap();
        let first = renderer.layer_pool_stats();
        let textures = renderer.batch_stats().backdrop_textures;
        assert!(textures > 0);
        assert_eq!(first.backdrop_allocations, textures);
        assert_eq!(first.backdrop_entries as u64, textures);
        assert!(first.backdrop_bytes > 0);
        assert_eq!(first.backdrop_uniform_entries, 2);
        assert_eq!(first.backdrop_uniform_allocations, 2);

        for _ in 0..3 {
            assert_eq!(
                renderer
                    .render_scene(&scene, [180, 100], [1.; 4], false)
                    .unwrap(),
                expected
            );
        }
        let reused = renderer.layer_pool_stats();
        assert_eq!(reused.backdrop_allocations, first.backdrop_allocations);
        assert!(reused.backdrop_reuses >= first.backdrop_reuses + textures * 3);
        assert_eq!(
            reused.backdrop_uniform_allocations,
            first.backdrop_uniform_allocations
        );
        assert!(reused.backdrop_uniform_reuses >= first.backdrop_uniform_reuses + 6);

        renderer.trim_layer_pool();
        let trimmed = renderer.layer_pool_stats();
        assert_eq!(trimmed.backdrop_entries, 0);
        assert_eq!(trimmed.backdrop_bytes, 0);
        assert_eq!(trimmed.backdrop_uniform_entries, 0);
        assert_eq!(
            renderer
                .render_scene(&scene, [180, 100], [1.; 4], false)
                .unwrap(),
            expected
        );
        assert!(renderer.layer_pool_stats().backdrop_allocations > reused.backdrop_allocations);
    }
}

#[test]
#[ignore = "requires a working wgpu adapter"]
fn unchanged_backdrops_idle_and_unrelated_changes_do_not_resample_them() {
    let mut partial = pollster::block_on(Renderer::new()).unwrap();
    let mut full = pollster::block_on(Renderer::new()).unwrap();
    let mut text = TextContext::new();
    let id = ShaderEffectId::new();
    for frame in 0..8 {
        let mut scene = Scene::new();
        fill(
            &mut scene,
            Rect::new(0., 0., 320., 200.),
            Color::from_rgb8(90, 120, 150),
            1.,
        );
        fill(
            &mut scene,
            Rect::new(200., 50., 204., 68.),
            if frame < 2 {
                Color::BLACK
            } else if frame % 2 == 0 {
                Color::WHITE
            } else {
                Color::BLACK
            },
            1.,
        );
        scene.append_scene(
            pane(&mut text, id, Rect::new(20., 20., 84., 84.), 1., 0.),
            Affine::IDENTITY,
        );
        compare(&mut partial, &mut full, &scene, [320, 200], frame);
        if frame > 0 {
            assert_eq!(
                partial.batch_stats().backdrop_copies,
                0,
                "unrelated change resampled glass"
            );
            assert!(partial.scene_damage_coverage([320, 200]) < 0.01);
        }
        if frame == 1 {
            assert_eq!(
                partial.batch_stats().instances,
                0,
                "identical frame must not prepare instances (offscreen readback may submit an empty pass)"
            );
        }
    }
}
#[test]
#[ignore = "requires a working wgpu adapter"]
fn background_and_foreground_edits_restore_the_entire_input_without_feedback_at_multiple_dpi() {
    let mut partial = pollster::block_on(Renderer::new()).unwrap();
    let mut full = pollster::block_on(Renderer::new()).unwrap();
    let mut text = TextContext::new();
    let id = ShaderEffectId::new();
    for scale in [1., 1.5, 2.] {
        for frame in 0..12 {
            let mut scene = Scene::new();
            fill(
                &mut scene,
                Rect::new(0., 0., 320., 200.),
                Color::from_rgb8(90, 120, 150),
                scale,
            );
            fill(
                &mut scene,
                Rect::new(28., 30., 34., 78.),
                if frame % 3 == 0 {
                    Color::WHITE
                } else {
                    Color::BLACK
                },
                scale,
            );
            scene.append_scene(
                pane(&mut text, id, Rect::new(20., 20., 100., 100.), scale, 0.),
                Affine::IDENTITY,
            );
            if frame < 11 {
                fill(
                    &mut scene,
                    Rect::new(44. + frame as f64, 40., 48. + frame as f64, 65.),
                    Color::from_rgba8(255, 0, 0, 180),
                    scale,
                );
            }
            let size = [(320. * scale) as u32, (200. * scale) as u32];
            compare(&mut partial, &mut full, &scene, size, frame);
            if frame > 0 {
                assert_eq!(partial.batch_stats().backdrop_copies, 1);
                assert!(
                    partial.scene_damage_coverage(size) < 0.15,
                    "glass edit became full frame"
                );
            }
        }
    }
}
#[test]
#[ignore = "requires a working wgpu adapter"]
fn overlapping_dependency_chains_and_removed_panes_match_full_replay() {
    let mut partial = pollster::block_on(Renderer::new()).unwrap();
    let mut full = pollster::block_on(Renderer::new()).unwrap();
    let mut text = TextContext::new();
    let ids = [
        ShaderEffectId::new(),
        ShaderEffectId::new(),
        ShaderEffectId::new(),
    ];
    for frame in 0..16 {
        let mut scene = Scene::new();
        fill(
            &mut scene,
            Rect::new(0., 0., 400., 240.),
            Color::from_rgb8(50, 100, 180),
            1.,
        );
        fill(
            &mut scene,
            Rect::new(28., 30., 32., 38.),
            if frame % 2 == 0 {
                Color::BLACK
            } else {
                Color::WHITE
            },
            1.,
        );
        for (i, id) in ids.iter().enumerate() {
            if frame > 10 && i == 1 {
                continue;
            }
            let x = 20. + i as f64 * 46.;
            let y = 20. + i as f64 * 12.;
            scene.append_scene(
                pane(&mut text, *id, Rect::new(x, y, x + 64., y + 64.), 1., 0.),
                Affine::IDENTITY,
            );
        }
        compare(&mut partial, &mut full, &scene, [400, 240], frame);
        if frame > 0 {
            assert!(partial.scene_damage_coverage([400, 240]) < 0.3);
        }
    }
}
#[test]
#[ignore = "requires a working wgpu adapter"]
fn masked_cropped_opacity_groups_do_not_read_stale_foreground_outside_the_output_clip() {
    let mut partial = pollster::block_on(Renderer::new()).unwrap();
    let mut full = pollster::block_on(Renderer::new()).unwrap();
    let mut text = TextContext::new();
    let id = ShaderEffectId::new();
    for scale in [1., 1.5, 2.] {
        for frame in 0..12 {
            let mut scene = Scene::new();
            fill(
                &mut scene,
                Rect::new(0., 0., 320., 200.),
                Color::from_rgb8(50, 90, 130),
                scale,
            );
            scene.push_layer(
                peniko::BlendMode::default(),
                0.8,
                Affine::scale(scale),
                &Rect::new(14., 14., 114., 110.),
                None,
                None,
            );
            fill(
                &mut scene,
                Rect::new(14., 14., 114., 110.),
                Color::from_rgba8(100, 200, 50, 180),
                scale,
            );
            scene.push_clip_layer(
                Affine::scale(scale),
                &Rect::new(30., 30., 90., 80.).to_rounded_rect(12.),
            );
            scene.append_scene(
                pane(&mut text, id, Rect::new(16., 16., 112., 108.), scale, 0.),
                Affine::IDENTITY,
            );
            scene.pop_layer();
            if frame % 2 == 0 {
                fill(
                    &mut scene,
                    Rect::new(25., 27., 100., 35.),
                    Color::from_rgba8(220, 10, 90, 200),
                    scale,
                );
            }
            scene.pop_layer();
            let size = [(320. * scale) as u32, (200. * scale) as u32];
            compare(&mut partial, &mut full, &scene, size, frame);
            if frame > 0 {
                assert!(partial.scene_damage_coverage(size) < 0.3);
            }
        }
    }
}

fn blur_surface(
    text: &mut TextContext,
    id: ShaderEffectId,
    rect: Rect,
    scale: f64,
    sigma: f32,
) -> Scene {
    let source=ShaderEffectSource::with_backdrop("fn plico_effect(uv:vec2f)->vec4f { return plico_backdrop_blur(uv,plico.values[0].x*plico.resolution_time_scale.w); }",true).unwrap().with_backdrop_prefilter(Some(sigma)).unwrap();
    let mut paint = plico_renderer::widget::PaintContext::new(
        rect.width() as f32,
        rect.height() as f32,
        scale,
        text,
    );
    paint.draw_shader_effect(id, source, 0., vec![sigma].into());
    let mut scene = Scene::new();
    scene.append_scene(
        paint.finish(),
        Affine::scale(scale) * Affine::translate((rect.x0, rect.y0)),
    );
    scene
}
#[test]
#[ignore = "requires a working wgpu adapter"]
fn same_stage_arrays_preserve_private_uv_mips_and_interleaved_foreground_exactly() {
    let mut batched = pollster::block_on(Renderer::new()).unwrap();
    let mut reference = pollster::block_on(Renderer::new()).unwrap();
    reference.force_separate_backdrops(true);
    let mut text = TextContext::new();
    let ids: Vec<_> = (0..8).map(|_| ShaderEffectId::new()).collect();
    for scale in [1., 1.5, 2.] {
        for odd in [false, true] {
            for frame in 0..4 {
                let mut scene = Scene::new();
                for x in 0..320 {
                    fill(
                        &mut scene,
                        Rect::new(f64::from(x), 0., f64::from(x + 1), 200.),
                        Color::from_rgba8(
                            (x % 251) as u8,
                            ((x * 3 + frame * 31) % 253) as u8,
                            ((x * 7) % 255) as u8,
                            190,
                        ),
                        scale,
                    );
                }
                let w = if odd { 59. } else { 60. };
                let h = if odd { 47. } else { 48. };
                for (i, id) in ids.iter().enumerate() {
                    let x = 14. + (i % 4) as f64 * 76.;
                    let y = 20. + (i / 4) as f64 * 86.;
                    let rect = Rect::new(x, y, x + w, y + h);
                    scene.append_scene(
                        blur_surface(&mut text, *id, rect, scale, [0., 3., 8., 6.][i % 4]),
                        Affine::IDENTITY,
                    );
                    // This label belongs to the already-rendered pane, and
                    // cannot change the input of later disjoint panes.
                    fill(
                        &mut scene,
                        Rect::new(x + 8., y + 8., x + 20., y + 20.),
                        Color::from_rgba8(255, 20, 10, 150),
                        scale,
                    );
                }
                let size = [(320. * scale) as u32, (200. * scale) as u32];
                let a = batched.render_scene(&scene, size, [1.; 4], false).unwrap();
                let b = reference
                    .render_scene(&scene, size, [1.; 4], false)
                    .unwrap();
                let mismatch = a.iter().zip(&b).position(|(a, b)| a != b);
                assert!(
                    mismatch.is_none(),
                    "array sampling differs at {mismatch:?}, scale {scale}, odd {odd}, frame {frame}"
                );
                assert_eq!(batched.batch_stats().backdrop_shared_reads, 8);
                assert_eq!(batched.batch_stats().backdrop_textures, 1);
                assert_eq!(reference.batch_stats().backdrop_textures, 8);
                assert!(
                    batched.batch_stats().backdrop_mip_passes
                        < reference.batch_stats().backdrop_mip_passes
                );
            }
        }
    }
}
#[test]
#[ignore = "requires a working wgpu adapter"]
fn intervening_writes_and_overlaps_split_background_stages_instead_of_reordering() {
    let mut batched = pollster::block_on(Renderer::new()).unwrap();
    let mut reference = pollster::block_on(Renderer::new()).unwrap();
    reference.force_separate_backdrops(true);
    let mut text = TextContext::new();
    let ids = [
        ShaderEffectId::new(),
        ShaderEffectId::new(),
        ShaderEffectId::new(),
    ];
    for overlap in [false, true] {
        batched.invalidate_scene_damage();
        for frame in 0..6 {
            let mut scene = Scene::new();
            fill(
                &mut scene,
                Rect::new(0., 0., 240., 160.),
                Color::from_rgb8(80, 100, 200),
                1.,
            );
            scene.append_scene(
                blur_surface(&mut text, ids[0], Rect::new(10., 10., 74., 74.), 1., 8.),
                Affine::IDENTITY,
            );
            let x = if overlap { 50. } else { 90. };
            fill(
                &mut scene,
                Rect::new(x, 10., x + 64., 74.),
                if frame % 2 == 0 {
                    Color::WHITE
                } else {
                    Color::BLACK
                },
                1.,
            );
            scene.append_scene(
                blur_surface(&mut text, ids[1], Rect::new(x, 10., x + 64., 74.), 1., 8.),
                Affine::IDENTITY,
            );
            scene.append_scene(
                blur_surface(&mut text, ids[2], Rect::new(164., 84., 228., 148.), 1., 8.),
                Affine::IDENTITY,
            );
            let a = batched
                .render_scene(&scene, [240, 160], [1.; 4], true)
                .unwrap();
            let b = reference
                .render_scene(&scene, [240, 160], [1.; 4], false)
                .unwrap();
            assert!(
                a == b,
                "stage split mismatch, overlap {overlap}, frame {frame}"
            );
            if frame == 0 {
                assert_eq!(
                    batched.batch_stats().backdrop_shared_reads,
                    2,
                    "independent third pane may share first snapshot"
                );
                assert_eq!(
                    batched.batch_stats().backdrop_textures,
                    2,
                    "dependent pane requires its own snapshot"
                );
            } else {
                assert_eq!(
                    batched.batch_stats().backdrop_shared_reads,
                    0,
                    "unchanged independent panes must be skipped"
                );
            }
        }
    }
}

#[test]
#[ignore = "requires a working wgpu adapter"]
fn grouped_arrays_inside_cropped_opacity_and_distinct_masks_match_separate_snapshots() {
    let mut batched = pollster::block_on(Renderer::new()).unwrap();
    let mut reference = pollster::block_on(Renderer::new()).unwrap();
    reference.force_separate_backdrops(true);
    let mut text = TextContext::new();
    let ids = [ShaderEffectId::new(), ShaderEffectId::new()];
    for scale in [1., 1.5, 2.] {
        for frame in 0..4 {
            let mut scene = Scene::new();
            fill(
                &mut scene,
                Rect::new(0., 0., 320., 180.),
                Color::from_rgb8(100, 130, 160),
                scale,
            );
            scene.push_layer(
                peniko::BlendMode::default(),
                0.7,
                Affine::scale(scale),
                &Rect::new(14., 14., 280., 130.),
                None,
                None,
            );
            fill(
                &mut scene,
                Rect::new(14., 14., 280., 130.),
                Color::from_rgba8(80, 200, 40, 180),
                scale,
            );
            for (i, id) in ids.iter().enumerate() {
                let x = 22. + i as f64 * 138.;
                let rect = Rect::new(x, 24., x + 96., 104.);
                scene.push_clip_layer(
                    Affine::scale(scale),
                    &Rect::new(x + 2., 26., x + 92., 100.).to_rounded_rect(10. + i as f64 * 8.),
                );
                scene.append_scene(
                    blur_surface(&mut text, *id, rect, scale, 4. + frame as f32),
                    Affine::IDENTITY,
                );
                scene.pop_layer();
                fill(
                    &mut scene,
                    Rect::new(x + 10., 40., x + 16., 65.),
                    Color::BLACK,
                    scale,
                );
            }
            scene.pop_layer();
            let size = [(320. * scale) as u32, (180. * scale) as u32];
            let a = batched.render_scene(&scene, size, [1.; 4], true).unwrap();
            let b = reference
                .render_scene(&scene, size, [1.; 4], false)
                .unwrap();
            let mismatch = a.iter().zip(&b).position(|(a, b)| a != b);
            assert!(
                mismatch.is_none(),
                "group opacity diff {mismatch:?}, scale {scale}, frame {frame}"
            );
            assert_eq!(batched.batch_stats().backdrop_shared_reads, 2);
        }
    }
}

#[test]
#[ignore = "requires a working wgpu adapter"]
fn sparse_damage_can_change_array_members_without_retaining_old_slice_contents() {
    let mut batched = pollster::block_on(Renderer::new()).unwrap();
    let mut reference = pollster::block_on(Renderer::new()).unwrap();
    reference.force_separate_backdrops(true);
    let mut text = TextContext::new();
    let ids: Vec<_> = (0..4).map(|_| ShaderEffectId::new()).collect();
    for scale in [1., 1.5, 2.] {
        for frame in 0..24 {
            let mut scene = Scene::new();
            fill(
                &mut scene,
                Rect::new(0., 0., 400., 160.),
                Color::from_rgb8(90, 130, 180),
                scale,
            );
            for i in 0..4 {
                let x = 16. + i as f64 * 90.;
                fill(
                    &mut scene,
                    Rect::new(x + 22., 36., x + 26., 48.),
                    if i == (frame / 2) % 4 {
                        Color::from_rgb8((frame * 11) as u8, 200, 40)
                    } else {
                        Color::BLACK
                    },
                    scale,
                );
            }
            for (i, id) in ids.iter().enumerate() {
                let x = 16. + i as f64 * 90.;
                scene.append_scene(
                    blur_surface(&mut text, *id, Rect::new(x, 20., x + 64., 84.), scale, 8.),
                    Affine::IDENTITY,
                );
            }
            let x = 16. + ((frame / 3) % 4) as f64 * 90.;
            fill(
                &mut scene,
                Rect::new(x + 9., 40., x + 12., 58.),
                if frame % 2 == 0 {
                    Color::WHITE
                } else {
                    Color::BLACK
                },
                scale,
            );
            let size = [(400. * scale) as u32, (160. * scale) as u32];
            let a = batched.render_scene(&scene, size, [1.; 4], true).unwrap();
            let b = reference
                .render_scene(&scene, size, [1.; 4], false)
                .unwrap();
            let mismatch = a.iter().zip(&b).position(|(a, b)| a != b);
            assert!(
                mismatch.is_none(),
                "sparse batch diff {mismatch:?}, scale {scale}, frame {frame}"
            );
            if frame > 0 {
                assert!(batched.scene_damage_coverage(size) < 0.5);
            }
        }
    }
}

#[test]
#[ignore = "manual interleaved full/partial and separate/batched timing, no image readback"]
fn profile_backdrop_architecture() {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::LowPower,
        ..Default::default()
    }))
    .unwrap();
    eprintln!("adapter={:?}", adapter.get_info());
    let (device, queue) =
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).unwrap();
    let mut renderers: Vec<_> = (0..4)
        .map(|i| {
            let mut r = pollster::block_on(Renderer::from_device(
                adapter.clone(),
                device.clone(),
                queue.clone(),
            ))
            .unwrap();
            r.force_separate_backdrops(i % 2 == 0);
            r
        })
        .collect();
    let targets: Vec<_> = (0..4)
        .map(|_| {
            device.create_texture(&wgpu::TextureDescriptor {
                label: Some("backdrop architecture probe"),
                size: wgpu::Extent3d {
                    width: 640,
                    height: 400,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[],
            })
        })
        .collect();
    let views: Vec<_> = targets
        .iter()
        .map(|t| t.create_view(&Default::default()))
        .collect();
    let mut text = TextContext::new();
    let ids: Vec<_> = (0..16).map(|_| ShaderEffectId::new()).collect();
    let mut base = Scene::new();
    for x in 0..640 {
        fill(
            &mut base,
            Rect::new(f64::from(x), 0., f64::from(x + 1), 400.),
            if x % 2 == 0 {
                Color::BLACK
            } else {
                Color::WHITE
            },
            1.,
        );
    }
    for (i, id) in ids.iter().enumerate() {
        let x = 12. + (i % 4) as f64 * 156.;
        let y = 12. + (i / 4) as f64 * 96.;
        base.append_scene(
            blur_surface(&mut text, *id, Rect::new(x, y, x + 128., y + 64.), 1., 8.),
            Affine::IDENTITY,
        );
    }
    for inside in [false, true] {
        let frames: Vec<_> = (0..2)
            .map(|i| {
                let mut s = base.clone();
                let (x, y) = if inside { (30., 30.) } else { (630., 380.) };
                fill(
                    &mut s,
                    Rect::new(x, y, x + 3., y + 14.),
                    if i == 0 { Color::BLACK } else { Color::WHITE },
                    1.,
                );
                s
            })
            .collect();
        let mut times: Vec<Vec<f64>> = (0..4).map(|_| Vec::new()).collect();
        for n in 0..38 {
            for k in 0..4 {
                let i = if n % 2 == 0 { k } else { 3 - k };
                let start = std::time::Instant::now();
                renderers[i]
                    .render_scene_to_view(
                        &frames[n % 2],
                        [640, 400],
                        [1.; 4],
                        i >= 2,
                        &views[i],
                        wgpu::TextureFormat::Rgba8Unorm,
                    )
                    .unwrap();
                device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
                if n >= 6 {
                    times[i].push(start.elapsed().as_secs_f64() * 1000.);
                }
            }
        }
        for (i, r) in renderers.iter().enumerate() {
            times[i].sort_by(f64::total_cmp);
            eprintln!(
                "inside={inside} partial={} batched={} median_ms={:.3} max_ms={:.3} coverage={:.6} stats={:?} pool={:?}",
                i >= 2,
                i % 2 == 1,
                (times[i][15] + times[i][16]) * 0.5,
                times[i][31],
                r.scene_damage_coverage([640, 400]),
                r.batch_stats(),
                r.layer_pool_stats()
            );
        }
    }
}

#[test]
#[ignore = "requires a working wgpu adapter"]
fn batched_capture_never_includes_offscreen_future_members_at_aa_boundaries() {
    let mut batched = pollster::block_on(Renderer::new()).unwrap();
    let mut reference = pollster::block_on(Renderer::new()).unwrap();
    reference.force_separate_backdrops(true);
    let mut text = TextContext::new();
    let ids: Vec<_> = (0..8).map(|_| ShaderEffectId::new()).collect();
    let positions = [
        (10., 10.),
        (90., 10.),
        (320.5, 10.),
        (-64.5, 10.),
        (10., 160.5),
        (10., -64.5),
        (320., 10.),
        (10., 160.),
    ];
    let mut scene = Scene::new();
    fill(
        &mut scene,
        Rect::new(0., 0., 320., 160.),
        Color::from_rgb8(80, 120, 160),
        1.,
    );
    for (i, (x, y)) in positions.into_iter().enumerate() {
        scene.append_scene(
            blur_surface(&mut text, ids[i], Rect::new(x, y, x + 64., y + 64.), 1., 8.),
            Affine::IDENTITY,
        );
    }
    let a = batched
        .render_scene(&scene, [320, 160], [1.; 4], false)
        .unwrap();
    let b = reference
        .render_scene(&scene, [320, 160], [1.; 4], false)
        .unwrap();
    assert!(a == b);
    assert_eq!(batched.batch_stats().backdrop_copies, 2);
    assert_eq!(batched.batch_stats().backdrop_shared_reads, 2);
}
