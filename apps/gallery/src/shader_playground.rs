//! Native WGSL adaptations of glsl-playground; only one study animates at a time.
use crate::heading;
use plico::prelude::*;

#[cfg(not(target_arch = "wasm32"))]
static SEARCH_ICON: plico::SvgAsset = plico::SvgAsset::new(
    r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round"><circle cx="10.5" cy="10.5" r="6.5"/><path d="m16 16 4.5 4.5"/></svg>"#,
);

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Playground {
    pub study: String,
    pub paused: bool,
    pub energy: f64,
    pub refraction: f64,
    pub blur: f64,
    pub drop: f64,
    pub query: String,
    #[serde(skip)]
    motion: DropMotion,
}
impl Default for Playground {
    fn default() -> Self {
        Self {
            study: "wave".into(),
            paused: false,
            energy: 0.65,
            refraction: 28.,
            blur: 5.,
            drop: 0.,
            query: String::new(),
            motion: DropMotion::default(),
        }
    }
}

// Transient motion is excluded from serialized recipes, as with SpringDemo.
#[derive(Clone, Debug, Default)]
struct DropMotion {
    target: f64,
    drag: Option<(f64, f64)>,
}
impl PartialEq for DropMotion {
    fn eq(&self, other: &Self) -> bool {
        self.target == other.target && self.drag == other.drag
    }
}
#[cfg(not(target_arch = "wasm32"))]
impl DropMotion {
    fn sample(&mut self, cx: &mut UiContext, target: f64, paused: bool) -> (f32, bool) {
        let target = target as f32;
        let value = if self.drag.is_some() || paused || cx.display_preferences().reduce_motion {
            cx.jump("shader.drop", target)
        } else {
            cx.spring(
                "shader.drop",
                target,
                SpringOptions {
                    stiffness: 200.,
                    damping: 22.,
                    ..Default::default()
                },
            )
        };
        self.target = f64::from(target);
        let active = !cx.settled("shader.drop");
        (value.clamp(0., 1.03), active)
    }
}

#[component]
#[rustfmt::skip]
pub fn page(cx: &mut UiContext, state: &mut Playground) {
    heading(cx, "Shader playground", "Light, glass and liquid motion · adapted from aaaa-zhen/glsl-playground");
    row(cx).gap(8).wrap(true) + |cx| {
        for (id, label) in [
            ("wave", "Voice wave"),
            ("dots", "Fluid dots"),
            ("lens", "Liquid lens"),
            ("search", "Liquid search"),
            ("edge", "Edge light"),
        ] {
            cx.keyed(id, |cx| {
                let control = button(cx).text(label);
                let control = if state.study == id { control.primary() } else { control.ghost() };
                if control.clicked() {
                    state.study = id.into();
                }
            });
        }
    };
    column(cx).height(0.).grow(1.).scroll_y(true).gap(16) + |cx| {
        row(cx).gap(12).wrap(true).align(Align::Center) + |cx| {
            if button(cx).text(if state.paused { "Play" } else { "Pause" }).clicked() {
                state.paused = !state.paused;
            }
            if state.study == "wave" || state.study == "edge" {
                text(cx, "Intensity").color(Color::Muted);
                slider(cx, "Effect intensity", &mut state.energy, 0.0..=1.0).width(160.);
            }
            if state.study == "lens" || state.study == "search" {
                row(cx).gap(8).align(Align::Center) + |cx| {
                    text(cx, "Refraction").color(Color::Muted);
                    slider(cx, "Lens refraction", &mut state.refraction, 0.0..=60.0).width(130.);
                };
                row(cx).gap(8).align(Align::Center) + |cx| {
                    text(cx, "Frost").color(Color::Muted);
                    slider(cx, "Lens frost", &mut state.blur, 0.0..=24.0).width(130.);
                };
            }
            if state.study == "search" {
                if button(cx).text(if state.drop < 0.5 { "Release drop" } else { "Attach drop" }).clicked() {
                    state.drop = if state.drop < 0.5 { 1. } else { 0. };
                }
                slider(cx, "Drop progress", &mut state.drop, 0.0..=1.0).width(130.);
            }
        };
        #[cfg(not(target_arch = "wasm32"))]
        {
            let (drop_progress, drop_active) = if state.study == "search" {
                state.motion.sample(cx, state.drop, state.paused)
            } else {
                (state.drop as f32, false)
            };
            column(cx).position(Position::Relative).width_percent(100.).height(400.).shrink(0.).radius(24.)
                .corner_smoothing(1.).clip(true).background(Color::Literal(0x050609)) + |cx|
            {
                match state.study.as_str() {
                    "dots" => {
                        shader(cx, &plico::effects::fluid_dots()).width_percent(100.).height_percent(100.)
                            .animated(true).paused(state.paused);
                    }
                    "lens" | "search" => {
                        // Real scene content: the lens samples these nodes, not an image facsimile.
                        column(cx).absolute(0., 0.).width_percent(100.).height_percent(100.)
                            .background(Color::Literal(0xd8e6ea)).padding(28).gap(20) + |cx|
                        {
                            text(cx, "Plico / Optical studies").font_size(14).weight(600)
                                .color(Color::Literal(0x244750));
                            text(cx, "A different\npoint of view.").font_size(42).weight(650)
                                .color(Color::Literal(0x183d48));
                            row(cx).gap(20) + |cx| {
                                for color in
                                    [0xff754d, 0x2578a0, 0xf5cc66, 0x4c9381]
                                {
                                    cx.keyed(color as u64, |cx| {
                                        column(cx).width(90.).height(90.).radius(45.)
                                            .background(Color::Literal(color)) + |_cx|
                                        {};
                                    });
                                }
                            };
                            text(cx, "REAL BACKDROP · sharp type outside, refracted detail inside")
                                .font_size(12).color(Color::Literal(0x244750));
                        };
                        let searching = state.study == "search";
                        let drop = drop_progress;
                        let values = if searching {
                            [
                                0.5,
                                0.035 + drop * 0.64,
                                0.075 + drop * 0.25,
                                0.10 - drop * 0.025,
                                12.,
                                66. - drop * 42.,
                                state.refraction as f32,
                                state.blur as f32,
                                0.95,
                                0.12,
                                1.05,
                                60.,
                                1.,
                                0.,
                                0.,
                                0.,
                            ]
                        } else {
                            [
                                0.52,
                                0.52,
                                0.30,
                                0.34,
                                -1.,
                                0.,
                                state.refraction as f32,
                                state.blur as f32,
                                0.94,
                                0.12,
                                1.12,
                                100.,
                                0.,
                                0.45,
                                0.,
                                0.,
                            ]
                        };
                        shader(cx, &plico::effects::liquid_lens()).absolute(0., 0.).width_percent(100.)
                            .height_percent(100.).values(&values).animated(!searching).paused(state.paused);
                        if searching {
                            button_base(cx, "Pull liquid drop").absolute(0., 0.).width_percent(100.).height(90.)
                                .padding(0).children(|cx| {
                                if cx.pressed() {
                                    if let Some((_, y)) = cx.pointer_position() {
                                        let (start_y, start_drop) = *state.motion.drag
                                            .get_or_insert((y, state.drop));
                                        let next = (start_drop + (y - start_y) / 256.).clamp(0., 1.);
                                        if next != state.drop {
                                            state.drop = next;
                                            cx.request_rebuild();
                                        }
                                    }
                                } else if state.motion.drag.take().is_some() {
                                    state.drop = if state.drop > 0.3 { 1. } else { 0. };
                                    cx.request_rebuild();
                                }
                            });
                        }
                        // A real focusable text input; the glass is its separate paint layer.
                        if searching && drop_progress >= 0.99 && !drop_active {
                            column(cx).absolute(0., 244.).width_percent(100.).align(Align::Center) + |cx| {
                                row(cx).width_percent(65.).height(52.).radius(26.).padding_x(18).gap(10)
                                    .align(Align::Center).focus_within_border(2., Color::Accent) + |cx|
                                {
                                    svg(cx, &SEARCH_ICON).size(18., 18.)
                                        .color(Color::Literal(0xc8d2e3));
                                    text_input_string(cx, "Search", &mut state.query).unstyled().placeholder("Search anything").width(0.).grow(1.)
                                        .height(44.).padding(0).color(Color::Literal(0xf5f8ff));
                                };
                            };
                        }
                    }
                    "edge" => {
                        column(cx).width_percent(100.).height_percent(100.).align(Align::Center)
                            .justify(Justify::Center).gap(10) + |cx|
                        {
                            text(cx, "How can I help?").font_size(30).weight(550)
                                .color(Color::Literal(0xf6f6fc));
                            text(cx, "Procedural light · continuous-curvature corners").font_size(13)
                                .color(Color::Literal(0xaab3c8));
                        };
                        shader(cx, &plico::effects::edge_light()).absolute(0., 0.).width_percent(100.)
                            .height_percent(100.).values(&[state.energy as f32, 0.1]).animated(true)
                            .paused(state.paused);
                    }
                    _ => {
                        shader(cx, &plico::effects::voice_wave()).width_percent(100.).height_percent(100.)
                            .values(&[state.energy as f32]).animated(true).paused(state.paused);
                    }
                }
            };
        }
        #[cfg(target_arch = "wasm32")]
        text(cx, "These WGSL studies require the native wgpu host.").color(Color::Muted);
        text(cx, match state.study.as_str() {
            "dots" => "Six dots merge in pairs, gather and burst using damped spring equations.",
            "lens" => "Curved refraction, spectral separation, a dark upper mirror and an equatorial highlight.",
            "search" => "Drag down from the top edge, release or scrub the drop: gradient-aware smooth union blends the top edge into a capsule. The final field accepts text.",
            "edge" => "Two light lobes follow a parabolic corner SDF. Procedural noise replaces the reference's 3D texture.",
            _ => "Four spectral ribbons with Lorentzian glow and a soft envelope. Intensity controls the wave amplitude.",
        }).color(Color::Muted).font_size(13).width_percent(100.);
    };
}

#[cfg(test)]
mod tests {
    #[test]
    fn framework_playground_assets_validate() {
        for asset in [
            plico::effects::voice_wave(),
            plico::effects::fluid_dots(),
            plico::effects::liquid_lens(),
            plico::effects::edge_light(),
        ] {
            plico_renderer::shader::ShaderEffectSource::with_backdrop(
                asset.source(),
                asset.reads_backdrop(),
            )
            .unwrap_or_else(|error| panic!("{}: {error}", asset.source_path().unwrap()));
        }
    }
}
