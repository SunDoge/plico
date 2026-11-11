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
                .placed_nodes()
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

#[test]
#[ignore = "requires a working wgpu adapter"]
fn regular_and_clear_glass_follow_mygo_tones_and_preserve_small_control_clarity() {
    use plico_desktop::HostView;
    use plico_renderer::{FrameSource, Scene, TextContext};
    struct ToneProbe {
        dark: bool,
        size: f32,
        background: u32,
        glass: LiquidGlass,
    }
    impl HostView for ToneProbe {
        fn theme(&self) -> plico::Theme {
            if self.dark {
                plico::Theme::dark()
            } else {
                plico::Theme::light()
            }
        }
        #[component]
        #[rustfmt::skip]
        fn show(&mut self, cx: &mut UiContext) {
            column(cx).width(self.size).height(self.size).background(Color::Literal(self.background)) + |cx| {
                column(cx).width_percent(100.).height_percent(100.).radius(16.).material(self.glass).end();
            };
        }
    }
    let mut renderer = pollster::block_on(plico_wgpu::Renderer::new()).unwrap();
    let mut render = |dark, size, background, mut glass: LiquidGlass| {
        glass.rim = 0.;
        let mut source = Source::with_clock(
            ToneProbe {
                dark,
                size,
                background,
                glass,
            },
            plico::VirtualClock::default(),
        );
        let mut text = TextContext::new();
        let mut scene = Scene::new();
        source
            .render_retained_frame(&mut scene, &mut text, (size as u32, size as u32), 1.)
            .unwrap();
        renderer
            .render_scene(&scene, [size as u32, size as u32], [0.; 4], false)
            .unwrap()
    };
    for (dark, background, glass, expected) in [
        (false, 0x000000, LiquidGlass::regular(), 138u8),
        (false, 0xffffff, LiquidGlass::regular(), 255),
        (true, 0x000000, LiquidGlass::regular(), 38),
        (true, 0xffffff, LiquidGlass::regular(), 130),
        (false, 0x000000, LiquidGlass::clear(), 32),
        (true, 0x000000, LiquidGlass::clear(), 32),
    ] {
        let pixels = render(dark, 80., background, glass);
        let center = (40 * 80 + 40) * 4;
        assert!(
            pixels[center].abs_diff(expected) <= 1,
            "dark={dark}, background={background:x}: {} != {expected}",
            pixels[center]
        );
        assert_eq!(pixels[center + 3], 255);
    }
    for dark in [false, true] {
        let regular = LiquidGlass::regular();
        let clearer = LiquidGlass {
            clarity: 1.,
            ..regular
        };
        assert_eq!(
            render(dark, 80., 0x345678, regular),
            render(dark, 80., 0x345678, clearer),
            "clarity must not flatten small controls"
        );
        let opaque = render(dark, 240., 0x000000, regular);
        let clear = render(dark, 240., 0x000000, clearer);
        let center = (120 * 240 + 120) * 4;
        assert!(
            clear[center] < opaque[center],
            "large pane must become clearer"
        );
        assert!(clear[center].abs_diff(if dark { 26 } else { 51 }) <= 1);
    }
}

#[test]
#[ignore = "requires a working wgpu adapter"]
fn primary_glass_buttons_keep_default_theme_label_contrast() {
    use plico_desktop::HostView;
    use plico_renderer::{FrameSource, Scene, TextContext};
    struct Primary(plico::Theme);
    impl HostView for Primary {
        fn theme(&self) -> plico::Theme {
            self.0
        }
        fn show(&mut self, cx: &mut UiContext) {
            column(cx)
                .width(120)
                .height(40)
                .background(Color::Panel)
                .children(|cx| {
                    button(cx)
                        .aria_label("Primary")
                        .primary()
                        .width(120)
                        .height(40)
                        .radius(20.)
                        .material(LiquidGlass::regular());
                });
        }
    }
    let luminance = |rgb: [u8; 3]| {
        let linear = |v: u8| {
            let v = f64::from(v) / 255.;
            if v <= 0.04045 {
                v / 12.92
            } else {
                ((v + 0.055) / 1.055).powf(2.4)
            }
        };
        linear(rgb[0]) * 0.2126 + linear(rgb[1]) * 0.7152 + linear(rgb[2]) * 0.0722
    };
    let mut renderer = pollster::block_on(plico_wgpu::Renderer::new()).unwrap();
    for preset in [
        plico::Theme::default(),
        plico::Theme::light(),
        plico::Theme::dark(),
    ] {
        let mut source = Source::with_clock(Primary(preset), plico::VirtualClock::default());
        let theme = source.theme();
        let label = theme.on_accent;
        let mut text = TextContext::new();
        let mut scene = Scene::new();
        source
            .render_retained_frame(&mut scene, &mut text, (120, 40), 1.)
            .unwrap();
        let pixels = renderer
            .render_scene(&scene, [120, 40], [0.; 4], false)
            .unwrap();
        let foreground = luminance([(label >> 16) as u8, (label >> 8) as u8, label as u8]);
        // Test the text's usable interior, including the reflection band;
        // antialiased capsule borders do not belong to the label background.
        for y in 10..30 {
            for x in 20..100 {
                let i = (y * 120 + x) * 4;
                let background = luminance([pixels[i], pixels[i + 1], pixels[i + 2]]);
                let ratio =
                    (background.max(foreground) + 0.05) / (background.min(foreground) + 0.05);
                assert!(ratio >= 4.5, "primary label contrast {ratio} at ({x},{y})");
            }
        }
    }
}
