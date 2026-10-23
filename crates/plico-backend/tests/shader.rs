use plico_backend::{Source, View};
use plico_core::prelude::*;
use plico_renderer::{FrameSource, Scene, TextContext};
use std::time::Duration;
static EFFECT: ShaderAsset = ShaderAsset::new(
    r#"
fn wabou_effect(_uv: vec2<f32>) -> vec4<f32> {
    return vec4<f32>(clamp(wabou.resolution_time_scale.z, 0.0, 1.0), wabou.values[0].y, 0.0, 1.0);
}
"#,
);
struct Demo {
    paused: bool,
    green: f32,
    shows: usize,
    invalid: bool,
    present: bool,
}
impl Default for Demo {
    fn default() -> Self {
        Self {
            paused: false,
            green: 0.25,
            shows: 0,
            invalid: false,
            present: true,
        }
    }
}
impl View for Demo {
    #[component]
    fn show(&mut self, cx: &mut UiContext) {
        self.shows += 1;
        if self.present {
            let asset = if self.invalid {
                &ShaderAsset::new("invalid WGSL")
            } else {
                &EFFECT
            };
            shader(cx, asset)
                .width(32.)
                .height(32.)
                .radius(0.)
                .values(&[0., self.green, 0., 1.])
                .animated(true)
                .paused(self.paused);
        }
    }
}
fn pixel(s: &mut Source<Demo>, text: &mut TextContext, scale: f64) -> [u8; 4] {
    let nodes = s.build_frame(text, 32, 32);
    let mut scene = Scene::default();
    s.paint_scene(&mut scene, &nodes, text, (32, 32), scale);
    let size = (32. * scale) as u32;
    plico_shell::renderer::render_to_image(&scene, size, size, s.base_color())
        .unwrap()
        .get_pixel(size / 2, size / 2)
        .0
}
#[test]
fn actual_gpu_shader_uses_values_clock_and_pause_without_layout_or_redeclaration() {
    let clock = plico_core::VirtualClock::default();
    let mut s = Source::with_clock(Demo::default(), clock.clone());
    let mut text = TextContext::new();
    assert_eq!(pixel(&mut s, &mut text, 1.), [0, 64, 0, 255]);
    assert!(s.has_anim());
    let shows = s.view.shows;
    clock.advance(Duration::from_millis(500)).unwrap();
    let color = pixel(&mut s, &mut text, 1.);
    assert!((color[0] as i32 - 128).abs() <= 1);
    assert_eq!(color[1], 64);
    assert_eq!(s.view.shows, shows);
    assert_eq!(s.adapter.stats.layout_runs, 0);
    assert!(s.adapter.placed.iter().all(|n| n.paint.text.is_none()));
    s.set_reduced_motion(true);
    let frozen = pixel(&mut s, &mut text, 1.);
    clock.advance(Duration::from_secs(2)).unwrap();
    assert_eq!(pixel(&mut s, &mut text, 1.), frozen);
    assert!(!s.has_anim());
    s.set_reduced_motion(false);
    assert_eq!(pixel(&mut s, &mut text, 1.), frozen);
    assert!(s.has_anim());
    s.view.paused = true;
    s.request_rebuild();
    let paused = pixel(&mut s, &mut text, 1.);
    assert!(!s.has_anim());
    clock.advance(Duration::from_secs(3)).unwrap();
    assert_eq!(pixel(&mut s, &mut text, 2.), paused);
    s.view.green = 0.75;
    s.request_rebuild();
    let color = pixel(&mut s, &mut text, 2.);
    assert_eq!(color[1], 191);
    assert_eq!(s.adapter.stats.layout_runs, 0);
    s.view.invalid = true;
    s.request_rebuild();
    assert_eq!(pixel(&mut s, &mut text, 1.)[1], 191);
    assert_eq!(s.adapter.shader_diagnostics().count(), 1);
    s.view.present = false;
    s.request_rebuild();
    s.build_frame(&mut text, 32, 32);
    assert_eq!(s.adapter.shader_diagnostics().count(), 0);
    assert!(!s.has_anim());
}
#[test]
fn invalid_shader_parameters_are_rejected_at_declaration() {
    let mut engine = plico_core::Engine::default();
    assert!(
        engine
            .frame_declarations(|cx| {
                shader(cx, &EFFECT).values(&[f32::NAN]);
            })
            .is_err()
    );
    assert!(
        engine
            .frame_declarations(|cx| {
                shader(cx, &EFFECT).values(&[0.; 257]);
            })
            .is_err()
    );
}

#[test]
fn complete_modules_and_unchanged_static_surfaces_use_the_same_public_path() {
    static MODULE: ShaderAsset = ShaderAsset::module(
        r#"
struct Uniforms { header: vec4<f32>, values: array<vec4<f32>,64> };
@group(0) @binding(0) var<uniform> p: Uniforms;
@vertex fn vs_main(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    let vertices=array<vec2<f32>,3>(vec2(-1.0,-1.0),vec2(3.0,-1.0),vec2(-1.0,3.0));
    return vec4<f32>(vertices[i],0.0,1.0);
}
@fragment fn fs_main() -> @location(0) vec4<f32> { return p.values[0]; }
"#,
    );
    struct Static;
    impl View for Static {
        #[component]
        fn show(&mut self, cx: &mut UiContext) {
            shader(cx, &MODULE)
                .width(16.)
                .height(16.)
                .radius(0.)
                .values(&[0., 0., 0., 1., 0.2, 0.4, 0.8, 1.]);
        }
    }
    let mut s = Source::new(Static);
    let mut text = TextContext::new();
    let nodes = s.build_frame(&mut text, 16, 16);
    assert!(
        s.adapter.shader_diagnostics().next().is_none(),
        "{:?}",
        s.adapter.shader_diagnostics().collect::<Vec<_>>()
    );
    let mut scene = Scene::default();
    s.paint_scene(&mut scene, &nodes, &mut text, (16, 16), 1.);
    let image = plico_shell::renderer::render_to_image(&scene, 16, 16, s.base_color()).unwrap();
    assert_eq!(image.get_pixel(8, 8).0, [51, 102, 204, 255]);
    assert!(!s.has_anim());
    s.build_frame(&mut text, 16, 16);
    assert_eq!(
        (
            s.adapter.stats.style_writes,
            s.adapter.stats.paint_writes,
            s.adapter.stats.layout_runs
        ),
        (0, 0, 0)
    );
    let snapshot = s.engine.snapshot();
    let restored: Vec<plico_core::ViewNode> =
        serde_json::from_str(&serde_json::to_string(&snapshot).unwrap()).unwrap();
    assert!(restored[0].shader.as_ref().unwrap().module);
    assert_eq!(
        restored[0].shader.as_ref().unwrap().values,
        [0., 0., 0., 1., 0.2, 0.4, 0.8, 1.]
    );
}

#[test]
fn theme_reveal_keeps_old_and_new_shader_uniforms_independent() {
    static SOLID: ShaderAsset =
        ShaderAsset::new("fn wabou_effect(_uv:vec2<f32>)->vec4<f32>{return wabou.values[0];}");
    #[derive(Default)]
    struct Themed {
        dark: bool,
    }
    impl View for Themed {
        #[component]
        fn show(&mut self, cx: &mut UiContext) {
            shader(cx, &SOLID)
                .width(64.)
                .height(64.)
                .radius(0.)
                .values(if self.dark {
                    &[0., 1., 0., 1.]
                } else {
                    &[1., 0., 0., 1.]
                });
        }
        fn theme(&self) -> plico_core::Theme {
            if self.dark {
                plico_core::Theme::dark()
            } else {
                plico_core::Theme::light()
            }
        }
    }
    fn draw(s: &mut Source<Themed>, text: &mut TextContext) -> Scene {
        let nodes = s.build_frame(text, 64, 64);
        let mut scene = Scene::default();
        s.paint_scene(&mut scene, &nodes, text, (64, 64), 1.);
        s.paint_debug_overlay(&mut scene, &nodes, text, 1.);
        scene
    }
    let clock = plico_core::VirtualClock::default();
    let mut s = Source::with_clock(Themed::default(), clock.clone());
    let mut text = TextContext::new();
    draw(&mut s, &mut text);
    s.view.dark = true;
    s.request_rebuild();
    draw(&mut s, &mut text);
    clock.advance(Duration::from_millis(100)).unwrap();
    let scene = draw(&mut s, &mut text);
    let image = plico_shell::renderer::render_to_image(&scene, 64, 64, s.base_color()).unwrap();
    assert_eq!(image.get_pixel(32, 32).0, [0, 255, 0, 255]);
    assert_eq!(image.get_pixel(2, 2).0, [255, 0, 0, 255]);
    clock.advance(Duration::from_millis(150)).unwrap();
    let scene = draw(&mut s, &mut text);
    let image = plico_shell::renderer::render_to_image(&scene, 64, 64, s.base_color()).unwrap();
    assert_eq!(image.get_pixel(2, 2).0, [0, 255, 0, 255]);
    assert!(!s.has_anim());
}
