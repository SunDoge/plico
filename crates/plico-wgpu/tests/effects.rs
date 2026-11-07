#![cfg(all(feature = "system-text", target_os = "linux"))]
use plico::prelude::*;
use plico::{ShaderAsset, VirtualClock};
use plico_renderer::{FrameSource, Scene, TextContext};
use plico_runtime::{NativeView, Source};
use plico_wgpu::Renderer;
use std::time::Duration;
static EFFECT: ShaderAsset = ShaderAsset::new(
    r#"
fn plico_effect(_uv:vec2f)->vec4f {
    return vec4f(clamp(plico.resolution_time_scale.z,0.,1.),plico.values[0].x,0.,1.);
}
"#,
);
struct Demo {
    paused: bool,
    green: f32,
    invalid: bool,
    present: bool,
    shows: usize,
}
impl Default for Demo {
    fn default() -> Self {
        Self {
            paused: false,
            green: 0.25,
            invalid: false,
            present: true,
            shows: 0,
        }
    }
}
impl plico::View for Demo {
    fn theme(&self) -> Theme {
        Theme::light()
    }
    fn show(&mut self, cx: &mut UiContext<'_>) {
        self.shows += 1;
        if self.present {
            let asset = if self.invalid {
                &ShaderAsset::module(
                    r#"
struct Uniforms { header: vec4f, enabled: u32 };
@group(0) @binding(0) var<uniform> u: Uniforms;
@vertex fn vs_main(@builtin(vertex_index) i: u32) -> @builtin(position) vec4f {
    let points = array<vec2f, 3>(vec2f(-1.,-1.), vec2f(3.,-1.), vec2f(-1.,3.));
    return vec4f(points[i], 0., 1.);
}
@fragment fn fs_main() -> @location(0) vec4f { return vec4f(f32(u.enabled), 0., 1., 1.); }
"#,
                )
                .with_source_path("effects/invalid-uniform.wgsl")
            } else {
                &EFFECT
            };
            shader(cx, asset)
                .width(32.)
                .height(32.)
                .radius(0.)
                .values(&[self.green])
                .animated(true)
                .paused(self.paused);
        }
    }
}
fn render(
    source: &mut Source<NativeView<Demo>>,
    renderer: &mut Renderer,
    text: &mut TextContext,
    scale: f64,
) -> [u8; 4] {
    let mut scene = Scene::new();
    source
        .render_retained_frame(&mut scene, text, (64, 64), scale)
        .unwrap();
    assert!(source.last_error.is_none());
    if !source.view.0.invalid {
        assert_eq!(
            source.adapter.shader_diagnostics().count(),
            0,
            "{:?}",
            source.adapter.shader_diagnostics().collect::<Vec<_>>()
        );
    }
    let size = (64. * scale) as u32;
    let pixels = renderer
        .render_scene(&scene, [size, size], source.base_color().components, true)
        .unwrap();
    let offset = ((8. * scale) as usize * size as usize + (8. * scale) as usize) * 4;
    pixels[offset..offset + 4].try_into().unwrap()
}
#[test]
#[ignore = "requires a working wgpu adapter and system fonts"]
fn public_effect_api_uses_virtual_time_pause_values_and_last_valid_source() {
    let clock = VirtualClock::default();
    let mut source = Source::with_clock(NativeView(Demo::default()), clock.clone());
    let mut renderer = pollster::block_on(Renderer::new()).unwrap();
    let mut text = TextContext::new();
    renderer.configure_text(&mut text, 1.).unwrap();
    assert_eq!(
        render(&mut source, &mut renderer, &mut text, 1.),
        [0, 64, 0, 255]
    );
    assert!(source.has_anim());
    assert_eq!(renderer.effect_cache_stats(), (1, 32 * 32 * 4, 1));
    let shows = source.view.0.shows;
    clock.advance(Duration::from_millis(500)).unwrap();
    let pixel = render(&mut source, &mut renderer, &mut text, 1.);
    assert!(pixel[0].abs_diff(128) <= 1);
    assert_eq!(pixel[1], 64);
    assert_eq!(
        source.view.0.shows, shows,
        "time-only effect redeclared the UI"
    );
    assert!(
        renderer.scene_damage_coverage([64, 64]) < 0.5,
        "local WGSL surface damaged the entire window"
    );
    source.update(|view| view.0.paused = true);
    let frozen = render(&mut source, &mut renderer, &mut text, 1.);
    clock.advance(Duration::from_secs(3)).unwrap();
    assert_eq!(render(&mut source, &mut renderer, &mut text, 1.), frozen);
    assert_eq!(renderer.scene_damage_coverage([64, 64]), 0.);
    assert!(!source.has_anim());
    source.update(|view| view.0.green = 0.75);
    assert_eq!(render(&mut source, &mut renderer, &mut text, 1.)[1], 191);
    source.update(|view| view.0.invalid = true);
    assert_eq!(render(&mut source, &mut renderer, &mut text, 1.)[1], 191);
    let diagnostic = source.adapter.shader_diagnostics().next().unwrap();
    assert!(
        diagnostic.starts_with("effects/invalid-uniform.wgsl:"),
        "{diagnostic}"
    );
    assert!(diagnostic.contains("only f32"), "{diagnostic}");
    assert_eq!(source.adapter.shader_diagnostics().count(), 1);
    renderer.configure_text(&mut text, 1.25).unwrap();
    assert_eq!(render(&mut source, &mut renderer, &mut text, 1.25)[1], 191);
    source.update(|view| view.0.present = false);
    render(&mut source, &mut renderer, &mut text, 1.25);
    assert!(!source.has_anim());
    assert_eq!(
        renderer.effect_cache_stats().0,
        0,
        "removed shader retained its output texture"
    );
}

fn effect_scene(
    text: &mut TextContext,
    source: plico_renderer::ShaderEffectSource,
    id: plico_renderer::ShaderEffectId,
    values: &[f32],
) -> Scene {
    let mut paint = plico_renderer::widget::PaintContext::new(16., 16., 1., text);
    paint.draw_shader_effect(id, source, 0., values.to_vec().into());
    paint.finish()
}
#[test]
#[ignore = "requires a working wgpu adapter"]
fn shader_versions_in_one_snapshot_keep_distinct_uniforms_and_premultiplied_outputs() {
    use plico_renderer::{PaintScene, ShaderEffectId, ShaderEffectSource};
    let source = ShaderEffectSource::new(
        "fn plico_effect(_uv:vec2f)->vec4f {return vec4f(plico.values[0].x,0.,0.,0.5);}",
    )
    .unwrap();
    let id = ShaderEffectId::new();
    let mut text = TextContext::new();
    let mut scene = Scene::new();
    scene.append_scene(
        effect_scene(&mut text, source.clone(), id, &[0.5]),
        kurbo::Affine::IDENTITY,
    );
    scene.append_scene(
        effect_scene(&mut text, source, id, &[0.25]),
        kurbo::Affine::translate((16., 0.)),
    );
    let mut renderer = pollster::block_on(Renderer::new()).unwrap();
    let pixels = renderer
        .render_scene(&scene, [32, 16], [0.; 4], true)
        .unwrap();
    assert_eq!(
        &pixels[(8 * 32 + 8) * 4..(8 * 32 + 8) * 4 + 4],
        [128, 0, 0, 128]
    );
    assert_eq!(
        &pixels[(8 * 32 + 24) * 4..(8 * 32 + 24) * 4 + 4],
        [64, 0, 0, 128]
    );
    assert_eq!(renderer.effect_cache_stats(), (2, 2 * 16 * 16 * 4, 1));
}
#[test]
#[ignore = "requires a working wgpu adapter"]
fn device_pipeline_rejection_is_reported_and_the_next_valid_effect_recovers() {
    use plico_renderer::{ShaderEffectId, ShaderEffectSource};
    let invalid = ShaderEffectSource::new_module(
        r#"
    struct VertexOutput { @builtin(position) position: vec4f, @location(0) uv: vec2f };
    @vertex fn vs_main(@builtin(vertex_index) i:u32)->VertexOutput {
        let p=array<vec2f,3>(vec2f(-1.,-1.),vec2f(3.,-1.),vec2f(-1.,3.));
        return VertexOutput(vec4f(p[i],0.,1.),p[i]);
    }
    // Stage linkage is checked by the device; each entry point is valid WGSL.
    @fragment fn fs_main(@location(0) uv:vec3f)->@location(0) vec4f {return vec4f(uv,1.);}
    "#,
    )
    .unwrap();
    let mut renderer = pollster::block_on(Renderer::new()).unwrap();
    let mut text = TextContext::new();
    let id = ShaderEffectId::new();
    let scene = effect_scene(&mut text, invalid, id, &[]);
    assert!(matches!(
        renderer.render_scene(&scene, [16, 16], [0.; 4], true),
        Err(plico_wgpu::Error::Shader { .. })
    ));
    let source =
        ShaderEffectSource::new("fn plico_effect(_uv:vec2f)->vec4f {return vec4f(0.,1.,0.,1.);}")
            .unwrap();
    let pixels = renderer
        .render_scene(
            &effect_scene(&mut text, source, id, &[]),
            [16, 16],
            [0.; 4],
            true,
        )
        .unwrap();
    assert_eq!(&pixels[..4], [0, 255, 0, 255]);
}

#[test]
#[ignore = "requires a working wgpu adapter"]
fn effects_compose_through_nonrectangular_clip_and_isolated_opacity() {
    use kurbo::{Affine, Circle};
    use plico_renderer::{PaintScene, ShaderEffectId, ShaderEffectSource};
    let source =
        ShaderEffectSource::new("fn plico_effect(_uv:vec2f)->vec4f {return vec4f(0.,0.,1.,1.);}")
            .unwrap();
    let mut text = TextContext::new();
    let child = effect_scene(&mut text, source, ShaderEffectId::new(), &[]);
    let mut scene = Scene::new();
    scene.push_layer(
        peniko::BlendMode::default(),
        0.5,
        Affine::IDENTITY,
        &Circle::new((12., 12.), 8.),
        None,
        None,
    );
    scene.append_scene(child, Affine::translate((4., 4.)));
    scene.pop_layer();
    let mut renderer = pollster::block_on(Renderer::new()).unwrap();
    let pixels = renderer
        .render_scene(&scene, [32, 32], [1.; 4], true)
        .unwrap();
    let pixel = |x: usize, y: usize| &pixels[(y * 32 + x) * 4..(y * 32 + x) * 4 + 4];
    assert!(
        pixel(12, 12)
            .iter()
            .zip([128_u8, 128, 255, 255])
            .all(|(a, b)| a.abs_diff(b) <= 1)
    );
    assert_eq!(pixel(4, 4), [255, 255, 255, 255]);
}

#[test]
#[ignore = "requires a working wgpu adapter"]
fn damage_skipped_effect_survives_cache_trim_and_is_released_on_removal() {
    use kurbo::{Affine, Rect};
    use peniko::{Color, Fill};
    use plico_renderer::{PaintScene, ShaderEffectId, ShaderEffectSource};
    let source =
        ShaderEffectSource::new("fn plico_effect(_uv:vec2f)->vec4f {return vec4f(0.,1.,0.,1.);}")
            .unwrap();
    let mut text = TextContext::new();
    let effect = effect_scene(&mut text, source, ShaderEffectId::new(), &[]);
    let mut partial = pollster::block_on(Renderer::new()).unwrap();
    let mut full = pollster::block_on(Renderer::new()).unwrap();
    for step in 0..6 {
        let mut scene = Scene::new();
        if step != 5 {
            scene.append_scene(
                effect.clone(),
                Affine::translate((if step == 4 { 4. } else { 0. }, 0.)),
            );
        }
        scene.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            Color::from_rgb8(step * 30, 0, 0),
            None,
            &Rect::new(48., 0., 64., 16.),
        );
        let actual = partial
            .render_scene(&scene, [64, 16], [0.; 4], true)
            .unwrap();
        let expected = full.render_scene(&scene, [64, 16], [0.; 4], false).unwrap();
        assert_eq!(actual, expected, "frame {step}");
        assert_eq!(
            partial.effect_cache_stats().0,
            usize::from(step != 3 && step != 5)
        );
        if step == 2 {
            // Retained pixels remain valid even if the derived surface cache is
            // evicted. A later unrelated damage must not erase the shader.
            partial.trim_effect_cache();
        }
    }
}

#[test]
#[ignore = "requires a working wgpu adapter"]
fn frame_preflight_skips_resources_and_shader_failure_recovers() {
    use plico_renderer::{PaintScene, ShaderEffectId, ShaderEffectSource};
    use plico_wgpu::Error;
    let mut renderer = pollster::block_on(Renderer::new()).unwrap();
    let mut good = Scene::new();
    good.fill(
        peniko::Fill::NonZero,
        kurbo::Affine::IDENTITY,
        peniko::Color::from_rgba8(255, 0, 0, 255),
        None,
        &kurbo::Rect::new(0., 0., 16., 16.),
    );
    let baseline = renderer
        .render_scene(&good, [16, 16], [0.; 4], true)
        .unwrap();
    let invalid = ShaderEffectSource::new_module(
        r#"
        struct VertexOutput { @builtin(position) position: vec4f, @location(0) uv: vec2f };
        @vertex fn vs_main(@builtin(vertex_index) i:u32)->VertexOutput {
            let p=array<vec2f,3>(vec2f(-1.,-1.),vec2f(3.,-1.),vec2f(-1.,3.));
            return VertexOutput(vec4f(p[i],0.,1.),p[i]);
        }
        @fragment fn fs_main(@location(0) uv:vec3f)->@location(0) vec4f {return vec4f(uv,1.);}
    "#,
    )
    .unwrap();
    let mut text = TextContext::new();
    let id = ShaderEffectId::new();
    let bad = effect_scene(&mut text, invalid, id, &[]);
    let prepared = renderer.scene_prepare_stats();
    assert!(matches!(
        renderer.prepare_scene(&bad, [0, 16]),
        Err(Error::InvalidInput { .. })
    ));
    assert_eq!(renderer.scene_prepare_stats(), prepared);

    for (size, background) in [
        ([0, 16], [0.; 4]),
        ([16, 0], [0.; 4]),
        ([u32::MAX, 16], [0.; 4]),
        ([16, 16], [f32::NAN, 0., 0., 0.]),
    ] {
        let prepared = renderer.scene_prepare_stats();
        assert!(matches!(
            renderer.render_scene(&bad, size, background, true),
            Err(Error::InvalidInput { .. })
        ));
        assert_eq!(
            renderer.scene_prepare_stats(),
            prepared,
            "invalid frame prepared its scene"
        );
        assert_eq!(
            renderer.effect_cache_stats(),
            (0, 0, 0),
            "invalid frame allocated effects"
        );
        let restored = renderer
            .render_scene(&good, [16, 16], [0.; 4], true)
            .unwrap();
        assert_eq!(restored, baseline);
    }
    assert!(matches!(
        renderer.render_scene(&bad, [16, 16], [0.; 4], true),
        Err(Error::Shader { .. })
    ));
    assert_eq!(
        renderer
            .render_scene(&good, [16, 16], [0.; 4], true)
            .unwrap(),
        baseline
    );
    let fixed =
        ShaderEffectSource::new("fn plico_effect(_uv:vec2f)->vec4f {return vec4f(0.,1.,0.,1.);}")
            .unwrap();
    let result = renderer
        .render_scene(
            &effect_scene(&mut text, fixed, id, &[]),
            [16, 16],
            [0.; 4],
            true,
        )
        .unwrap();
    assert!(
        result
            .as_chunks::<4>()
            .0
            .iter()
            .all(|p| *p == [0, 255, 0, 255])
    );
}

#[test]
#[ignore = "requires a working wgpu adapter and system fonts"]
fn named_public_parameters_pack_vectors_and_recover_transactionally() {
    static MODULE: ShaderAsset = ShaderAsset::module(
        r#"
struct U { size: vec2f, time: f32, progress: f32, color: vec3f, alpha: f32 };
@group(0) @binding(0) var<uniform> u: U;
@vertex fn vs_main(@builtin(vertex_index) i: u32) -> @builtin(position) vec4f {
    let p = array<vec2f,3>(vec2f(-1.,-1.),vec2f(3.,-1.),vec2f(-1.,3.));
    return vec4f(p[i],0.,1.);
}
@fragment fn fs_main() -> @location(0) vec4f { return vec4f(u.color*u.progress,u.alpha); }
"#,
    )
    .with_source_path("named.wgsl");
    struct Named {
        bad: bool,
        progress: f32,
        raw: bool,
    }
    impl plico::View for Named {
        fn theme(&self) -> Theme {
            Theme::light()
        }
        fn show(&mut self, cx: &mut UiContext<'_>) {
            let builder = shader(cx, &MODULE)
                .width(32.)
                .height(32.)
                .radius(0.)
                .param("progress", 0.1)
                .param("progress", self.progress)
                .param("alpha", 1.0);
            let builder = if self.raw {
                builder.values(&[1.])
            } else {
                builder
            };
            if self.bad {
                builder.param("color", [0.25, 0.5]);
            } else {
                builder.param("color", [0.25, 0.5, 0.75]);
            }
        }
    }
    let mut source = Source::new(NativeView(Named {
        bad: false,
        progress: 0.5,
        raw: false,
    }));
    let mut renderer = pollster::block_on(Renderer::new()).unwrap();
    let mut text = TextContext::new();
    renderer.configure_text(&mut text, 1.).unwrap();
    let mut draw = |source: &mut Source<NativeView<Named>>| {
        let mut scene = Scene::new();
        source
            .render_retained_frame(&mut scene, &mut text, (64, 64), 1.)
            .unwrap();
        assert!(source.last_error.is_none());
        let rgba = renderer
            .render_scene(&scene, [64, 64], [0.; 4], true)
            .unwrap();
        <[u8; 4]>::try_from(&rgba[(8 * 64 + 8) * 4..(8 * 64 + 8) * 4 + 4]).unwrap()
    };
    assert_eq!(draw(&mut source), [32, 64, 96, 255]);
    source.update(|s| {
        s.0.bad = true;
        s.0.progress = 1.;
    });
    assert_eq!(
        draw(&mut source),
        [32, 64, 96, 255],
        "invalid parameters changed the last valid uniforms"
    );
    let error = source.adapter.shader_diagnostics().next().unwrap();
    assert!(
        error.starts_with("named.wgsl:") && error.contains("expects 3"),
        "{error}"
    );
    source.update(|s| s.0.bad = false);
    assert_eq!(draw(&mut source), [64, 128, 191, 255]);
    assert!(source.adapter.shader_diagnostics().next().is_none());
    source.update(|s| s.0.raw = true);
    assert_eq!(draw(&mut source), [64, 128, 191, 255]);
    assert!(
        source
            .adapter
            .shader_diagnostics()
            .next()
            .unwrap()
            .contains("not both")
    );
    source.update(|s| s.0.raw = false);
    assert_eq!(draw(&mut source), [64, 128, 191, 255]);
    assert!(source.adapter.shader_diagnostics().next().is_none());
}

#[test]
#[ignore = "requires a working wgpu adapter"]
fn backdrop_copy_preserves_orientation_batch_boundaries_and_following_draws() {
    use kurbo::{Affine, Rect};
    use peniko::{Color, Fill, Gradient};
    use plico_renderer::{PaintScene, ShaderEffectId, ShaderEffectSource};
    let mut renderer = pollster::block_on(Renderer::new()).unwrap();
    let mut text = TextContext::new();
    let mut base = Scene::new();
    let gradient = Gradient::new_linear((0., 0.), (0., 16.)).with_stops([
        Color::from_rgba8(255, 0, 0, 255),
        Color::from_rgba8(0, 0, 255, 255),
    ]);
    base.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        &gradient,
        None,
        &Rect::new(0., 0., 16., 16.),
    );
    let mut expected = base.clone();
    let source = ShaderEffectSource::with_backdrop(
        "fn plico_effect(uv: vec2f) -> vec4f { return plico_backdrop_at(uv); }",
        true,
    )
    .unwrap();
    let mut actual = base;
    actual.append_scene(
        effect_scene(&mut text, source, ShaderEffectId::new(), &[]),
        Affine::IDENTITY,
    );
    for scene in [&mut actual, &mut expected] {
        scene.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            Color::from_rgba8(0, 255, 0, 255),
            None,
            &Rect::new(0., 0., 4., 4.),
        );
    }
    let actual = renderer
        .render_scene(&actual, [16, 16], [0.; 4], false)
        .unwrap();
    let expected = renderer
        .render_scene(&expected, [16, 16], [0.; 4], false)
        .unwrap();
    for (a, b) in actual.iter().zip(&expected) {
        assert!(a.abs_diff(*b) <= 1, "{a} != {b}");
    }
}

#[test]
#[ignore = "requires a working wgpu adapter"]
fn backdrop_subregion_uses_local_coordinates() {
    use kurbo::{Affine, Rect};
    use peniko::{Color, Fill, Gradient};
    use plico_renderer::{PaintScene, ShaderEffectId, ShaderEffectSource};
    let mut renderer = pollster::block_on(Renderer::new()).unwrap();
    let mut text = TextContext::new();
    let mut base = Scene::new();
    let gradient = Gradient::new_linear((0., 0.), (64., 48.)).with_stops([
        Color::from_rgba8(240, 30, 40, 255),
        Color::from_rgba8(20, 60, 230, 255),
    ]);
    base.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        &gradient,
        None,
        &Rect::new(0., 0., 64., 48.),
    );
    let expected = renderer
        .render_scene(&base, [64, 48], [0.; 4], false)
        .unwrap();
    let source = ShaderEffectSource::with_backdrop(
        "fn plico_effect(uv: vec2f) -> vec4f { return plico_backdrop_at(uv); }",
        true,
    )
    .unwrap();
    base.append_scene(
        effect_scene(&mut text, source, ShaderEffectId::new(), &[]),
        Affine::translate((23., 11.)),
    );
    let actual = renderer
        .render_scene(&base, [64, 48], [0.; 4], false)
        .unwrap();
    for (index, (a, b)) in actual.iter().zip(&expected).enumerate() {
        assert!(a.abs_diff(*b) <= 1, "channel {index}: {a} != {b}");
    }
}

#[test]
#[ignore = "requires a working wgpu adapter"]
fn builtin_liquid_glass_renders_a_local_pane() {
    use kurbo::{Affine, Rect};
    use peniko::{Color, Fill};
    use plico_renderer::{PaintScene, ShaderEffectId, ShaderEffectSource};
    let mut renderer = pollster::block_on(Renderer::new()).unwrap();
    let mut text = TextContext::new();
    let mut scene = Scene::new();
    scene.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        Color::from_rgba8(30, 90, 140, 255),
        None,
        &Rect::new(0., 0., 64., 48.),
    );
    let source =
        ShaderEffectSource::with_backdrop(plico_renderer::shader::LIQUID_GLASS_WGSL, true).unwrap();
    scene.append_scene(
        effect_scene(
            &mut text,
            source,
            ShaderEffectId::new(),
            &[6., 0.78, 0.48, 0., 2., 1.24, 1., 0., 0.975, 0.988, 1., 0.],
        ),
        Affine::translate((23., 11.)),
    );
    let pixels = renderer
        .render_scene(&scene, [64, 48], [0.; 4], false)
        .unwrap();
    assert_eq!(&pixels[..4], &[30, 90, 140, 255]);
    let center = (19 * 64 + 31) * 4;
    assert!(pixels[center] > 30 && pixels[center + 1] > 90 && pixels[center + 2] > 140);
    assert_eq!(pixels[center + 3], 255);
}

#[test]
#[ignore = "requires a working wgpu adapter"]
fn liquid_glass_bends_straight_edges_along_their_surface_normal() {
    use kurbo::{Affine, Rect};
    use peniko::{Color, Fill, Gradient};
    use plico_renderer::{PaintScene, ShaderEffectId, ShaderEffectSource};
    let mut renderer = pollster::block_on(Renderer::new()).unwrap();
    let mut text = TextContext::new();
    let mut base = Scene::new();
    let ramp = Gradient::new_linear((0., 0.), (160., 0.)).with_stops([
        Color::from_rgba8(0, 80, 120, 255),
        Color::from_rgba8(255, 80, 120, 255),
    ]);
    base.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        &ramp,
        None,
        &Rect::new(0., 0., 160., 120.),
    );
    let expected = renderer
        .render_scene(&base, [160, 120], [0.; 4], false)
        .unwrap();
    let source =
        ShaderEffectSource::with_backdrop(plico_renderer::shader::LIQUID_GLASS_WGSL, true).unwrap();
    let mut paint = plico_renderer::widget::PaintContext::new(160., 120., 1., &mut text);
    paint.draw_shader_effect(
        ShaderEffectId::new(),
        source,
        0.,
        vec![18., 0., 0., 0., 8., 1., 0., 36., 1., 1., 1., 0.].into(),
    );
    base.append_scene(paint.finish(), Affine::IDENTITY);
    let actual = renderer
        .render_scene(&base, [160, 120], [0.; 4], false)
        .unwrap();
    let red = |pixels: &[u8], x: usize, y: usize| pixels[(y * 160 + x) * 4];
    assert!(
        red(&actual, 80, 3).abs_diff(red(&expected, 80, 3)) <= 1,
        "the top edge must refract vertically, without shifting the horizontal ramp"
    );
    assert!(
        red(&actual, 4, 60) > red(&expected, 4, 60) + 4,
        "the left edge must visibly bend the horizontal ramp inward"
    );
    assert!(
        red(&actual, 80, 60).abs_diff(red(&expected, 80, 60)) <= 1,
        "the flat center must not zoom the whole scene"
    );
}
