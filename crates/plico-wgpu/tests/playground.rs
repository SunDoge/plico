#![cfg(all(feature = "system-text", target_os = "linux"))]
//! Real GPU conformance for the reusable shader studies (not screenshot hashes).
use plico::prelude::*;
use plico::{ShaderAsset, VirtualClock};
use plico_renderer::{FrameSource, Scene, TextContext};
use plico_runtime::{NativeView, Source};
use plico_wgpu::Renderer;

struct Study {
    asset: ShaderAsset,
    values: Vec<f32>,
    time: f32,
}
impl plico::View for Study {
    fn theme(&self) -> Theme {
        Theme::light()
    }
    fn show(&mut self, cx: &mut UiContext<'_>) {
        column(cx)
            .width(320.)
            .height(240.)
            .position(Position::Relative)
            .background(Color::Literal(0x15253b))
            .children(|cx| {
                for i in 0..8 {
                    column(cx)
                        .absolute(i as f32 * 40., 0.)
                        .width(20.)
                        .height(240.)
                        .background(Color::Literal(0xe9b257))
                        .children(|_| {});
                }
                shader(cx, &self.asset)
                    .absolute(0., 0.)
                    .width(320.)
                    .height(240.)
                    .values(&self.values)
                    .time(self.time)
                    .paused(true);
            });
    }
}
fn render(
    renderer: &mut Renderer,
    asset: ShaderAsset,
    values: &[f32],
    time: f32,
    scale: f64,
) -> Vec<u8> {
    let mut source = Source::with_clock(
        NativeView(Study {
            asset,
            values: values.to_vec(),
            time,
        }),
        VirtualClock::default(),
    );
    let mut text = TextContext::new();
    renderer.configure_text(&mut text, scale).unwrap();
    let mut scene = Scene::new();
    source
        .render_retained_frame(&mut scene, &mut text, (320, 240), scale)
        .unwrap();
    assert!(source.last_error().is_none(), "{:?}", source.last_error());
    assert_eq!(source.adapter.shader_diagnostics().count(), 0);
    renderer
        .render_scene(
            &scene,
            [(320. * scale) as u32, (240. * scale) as u32],
            source.base_color().components,
            true,
        )
        .unwrap()
}
fn different(a: &[u8], b: &[u8]) -> usize {
    a.as_chunks::<4>()
        .0
        .iter()
        .zip(b.as_chunks::<4>().0.iter())
        .filter(|(a, b)| a[..3].iter().zip(&b[..3]).any(|(a, b)| a.abs_diff(*b) > 8))
        .count()
}
#[test]
#[ignore = "requires a working wgpu adapter and system fonts"]
fn playground_shaders_render_motion_refraction_and_dpi() {
    let mut renderer = pollster::block_on(Renderer::new()).unwrap();
    for asset in [
        plico::effects::voice_wave(),
        plico::effects::fluid_dots(),
        plico::effects::edge_light(),
    ] {
        let a = render(&mut renderer, asset, &[0.8, 0.12], 0., 1.);
        let b = render(&mut renderer, asset, &[0.8, 0.12], 2.4, 1.);
        assert!(
            different(&a, &b) > 100,
            "{} must visibly animate",
            asset.source_path().unwrap()
        );
        assert!(
            a.as_chunks::<4>().0.iter().all(|p| p[3] == 255),
            "composited output stays opaque"
        );
        let hidpi = render(&mut renderer, asset, &[0.8, 0.12], 2.4, 2.);
        assert_eq!(hidpi.len(), 640 * 480 * 4);
        // Logical center remains stable after scaling (within sampling tolerance).
        let center = (120 * 320 + 160) * 4;
        let center2 = (240 * 640 + 320) * 4;
        assert!(
            b[center..center + 3]
                .iter()
                .zip(&hidpi[center2..center2 + 3])
                .all(|(a, b)| a.abs_diff(*b) < 35)
        );
    }
    let mut values = [0.5, 0.5, 0.32, 0.3, -1., 40., 0., 0., 0., 0.12, 1., 60.];
    let flat = render(
        &mut renderer,
        plico::effects::liquid_lens(),
        &values,
        0.,
        1.,
    );
    values[6] = 40.;
    values[7] = 8.;
    values[10] = 1.12;
    let bent = render(
        &mut renderer,
        plico::effects::liquid_lens(),
        &values,
        0.,
        1.,
    );
    assert!(
        different(&flat, &bent) > 2000,
        "lens must refract actual background stripes"
    );
    values[1] = 0.12;
    values[2] = 0.075;
    values[3] = 0.10;
    values[4] = 12.;
    let attached = render(
        &mut renderer,
        plico::effects::liquid_lens(),
        &values,
        0.,
        1.,
    );
    values[1] = 0.65;
    values[2] = 0.325;
    values[3] = 0.075;
    let capsule = render(
        &mut renderer,
        plico::effects::liquid_lens(),
        &values,
        0.,
        1.,
    );
    assert!(
        different(&attached, &capsule) > 2000,
        "drop and capsule have distinct silhouettes"
    );
    assert_eq!(
        &capsule[(230 * 320 + 300) * 4..(230 * 320 + 300) * 4 + 4],
        &flat[(230 * 320 + 300) * 4..(230 * 320 + 300) * 4 + 4],
        "distant background is preserved"
    );
}
