#![cfg(feature = "wgpu")]
//! Material regressions against a deliberately detailed synthetic backdrop.
use plico::prelude::*;
use plico_desktop::{NativeApp, RenderBackend, Source, capture};
#[derive(Default)]
struct Probe {
    refraction: f32,
}
impl plico_dev::App for Probe {
    #[component]
    fn show(&mut self, cx: &mut UiContext) {
        column(cx)
            .position(plico::Position::Relative)
            .width_percent(100.)
            .height_percent(100.)
            .children(|cx| {
                for y in 0..12 {
                    for x in 0..16 {
                        cx.keyed((y * 16 + x) as u64, |cx| {
                            column(cx)
                                .absolute((x * 20) as f32, (y * 20) as f32)
                                .width(20.)
                                .height(20.)
                                .background(Color::Literal(if (x + y) % 2 == 0 {
                                    0x334d6c
                                } else {
                                    0xf2c99d
                                }))
                                .end();
                        });
                    }
                }
                column(cx)
                    .absolute(40., 40.)
                    .width(240.)
                    .height(160.)
                    .radius(18.)
                    .corner_smoothing(1.)
                    .elevation(Elevation::PANEL)
                    .material(LiquidGlass {
                        refraction: self.refraction,
                        ..Default::default()
                    })
                    .end();
            });
    }
}
#[test]
#[ignore = "requires system fonts and a working wgpu adapter"]
fn curved_edges_refract_detailed_background_while_center_and_shadow_contract_remain_stable() {
    let render = |refraction| {
        let mut source = Source::new(NativeApp(Probe { refraction }));
        let image = capture(&mut source)
            .size(320, 240)
            .renderer(RenderBackend::Wgpu)
            .render()
            .unwrap();
        assert!(
            source
                .adapter
                .placed
                .iter()
                .any(|n| n.paint.shadows.len() == 2)
        );
        image
    };
    let flat = render(0.);
    let curved = render(1.6);
    let pixel = |frame: &plico_desktop::CapturedFrame, x: usize, y: usize| {
        frame.rgba()[(y * 320 + x) * 4..(y * 320 + x) * 4 + 4].to_vec()
    };
    let changed = (48..76)
        .flat_map(|x| (88..152).map(move |y| (x, y)))
        .filter(|&(x, y)| pixel(&flat, x, y) != pixel(&curved, x, y))
        .count();
    assert!(
        changed > 500,
        "flat-edge refraction is missing: {changed} changed pixels"
    );
    for y in 108..132 {
        for x in 140..180 {
            assert_eq!(
                pixel(&flat, x, y),
                pixel(&curved, x, y),
                "lens changed the flat center"
            );
        }
    }
    assert_eq!(
        curved,
        render(1.6),
        "idle glass must not shimmer between captures"
    );
}

#[derive(Default)]
struct EdgeProbe {
    side: Option<ScrollEdgeSide>,
}
impl plico_dev::App for EdgeProbe {
    #[component]
    fn show(&mut self, cx: &mut UiContext) {
        column(cx)
            .position(Position::Relative)
            .width_percent(100.)
            .height_percent(100.)
            .children(|cx| {
                for y in 0..12 {
                    for x in 0..16 {
                        cx.keyed((y * 16 + x) as u64, |cx| {
                            column(cx)
                                .absolute((x * 20) as f32, (y * 20) as f32)
                                .width(20.)
                                .height(20.)
                                .background(Color::Literal(if (x + y) % 2 == 0 {
                                    0x202020
                                } else {
                                    0xf0f0f0
                                }))
                                .end();
                        });
                    }
                }
                let options = BackdropBlur {
                    radius: if self.side.is_some() { 8. } else { 0. },
                    ..Default::default()
                };
                if let Some(side) = self.side {
                    scroll_edge(cx, side, options)
                        .absolute(0., 0.)
                        .width_percent(100.)
                        .height_percent(100.);
                } else {
                    backdrop_blur(cx, options)
                        .absolute(0., 0.)
                        .width_percent(100.)
                        .height_percent(100.);
                }
            });
    }
}
#[test]
#[ignore = "requires system fonts and a working wgpu adapter"]
fn typed_scroll_edges_blur_the_selected_side_and_preserve_the_opposite_side() {
    let render = |side| {
        let mut source = Source::new(NativeApp(EdgeProbe { side }));
        capture(&mut source)
            .size(320, 240)
            .renderer(RenderBackend::Wgpu)
            .render()
            .unwrap()
    };
    let base = render(None);
    let red =
        |image: &plico_desktop::CapturedFrame, x: usize, y: usize| image.rgba()[(y * 320 + x) * 4];
    for (side, strong, weak) in [
        (ScrollEdgeSide::Top, (19, 0), (19, 239)),
        (ScrollEdgeSide::Bottom, (19, 239), (19, 0)),
        (ScrollEdgeSide::Left, (0, 19), (319, 19)),
        (ScrollEdgeSide::Right, (319, 19), (0, 19)),
    ] {
        let actual = render(Some(side));
        assert!(
            red(&actual, strong.0, strong.1).abs_diff(red(&base, strong.0, strong.1)) > 20,
            "{side:?} did not blur"
        );
        assert!(
            red(&actual, weak.0, weak.1).abs_diff(red(&base, weak.0, weak.1)) <= 1,
            "{side:?} changed its far edge"
        );
    }
}

#[derive(Default)]
struct ContourProbe {
    shader: Option<plico::ShaderAsset>,
    radius: f32,
    smoothing: f32,
}
impl plico_dev::App for ContourProbe {
    #[component]
    fn show(&mut self, cx: &mut UiContext) {
        column(cx)
            .width_percent(100.)
            .height_percent(100.)
            .position(Position::Relative)
            .background(Color::Literal(0))
            .children(|cx| {
                if let Some(asset) = self.shader {
                    shader(cx, &asset)
                        .absolute(16., 16.)
                        .width(96.)
                        .height(64.)
                        .values(&[
                            self.radius,
                            0.,
                            0.,
                            0.,
                            0.,
                            0.,
                            0.,
                            0.,
                            0.,
                            0.,
                            0.,
                            0.,
                            self.smoothing,
                        ]);
                } else {
                    column(cx)
                        .absolute(16., 16.)
                        .width(96.)
                        .height(64.)
                        .radius(self.radius)
                        .corner_smoothing(self.smoothing)
                        .background(Color::Literal(0xffffff))
                        .end();
                }
            });
    }
}
#[test]
#[ignore = "requires system fonts and a working wgpu adapter"]
fn glass_contour_matches_shared_mygo_geometry_including_cramped_corners() {
    let original = plico::materials::liquid_glass_shader()
        .source()
        .replace("fn plico_effect(", "fn material_effect(");
    let program = format!(
        "{original}\n{}",
        r#"
fn plico_effect(uv:vec2<f32>)->vec4<f32> {
    let size=plico.resolution_time_scale.xy/plico.resolution_time_scale.w;
    let radius=min(plico.values[0].x,min(size.x,size.y)*0.5);
    let field=glass_contour_field((uv-vec2<f32>(0.5))*size,size,radius,plico.values[3].x);
    let coverage=select(0.0,1.0,field<=0.0);
    return vec4<f32>(vec3<f32>(coverage),1.0);
}"#
    );
    let asset = plico::ShaderAsset::new(Box::leak(program.into_boxed_str())).read_backdrop();
    let render = |shader, radius, smoothing, scale| {
        let mut source = Source::new(NativeApp(ContourProbe {
            shader,
            radius,
            smoothing,
        }));
        capture(&mut source)
            .size(128, 96)
            .scale(scale)
            .renderer(RenderBackend::Wgpu)
            .render()
            .unwrap()
    };
    for (radius, smoothing, scale) in [
        (18., 1., 1.),
        (28., 1., 2.),
        (40., 1., 1.),
        (28., 0.5, 1.),
        (18., 0., 1.),
    ] {
        let expected = render(None, radius, smoothing, scale);
        let actual = render(Some(asset), radius, smoothing, scale);
        let mut wrong = 0;
        for (reference, field) in expected
            .rgba()
            .as_chunks::<4>()
            .0
            .iter()
            .zip(actual.rgba().as_chunks::<4>().0)
        {
            if (reference[0] >= 250 && field[0] < 128) || (reference[0] <= 5 && field[0] >= 128) {
                wrong += 1;
            }
        }
        assert!(
            wrong <= 8,
            "radius={radius}, smoothing={smoothing}, scale={scale}: {wrong} interior/exterior disagreements"
        );
    }
}
