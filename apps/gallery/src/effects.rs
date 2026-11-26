//! MyGO-inspired GPU surfaces: a frosted liquid-glass pane, the chat-glow
//! assistant orb and the wind example's flowing field, all authored in WGSL.
use crate::heading;
use plico::prelude::*;

#[cfg(not(target_arch = "wasm32"))]
static ORB: plico::ShaderAsset = plico::shader_asset!("../assets/shaders/orb.wgsl");
#[cfg(not(target_arch = "wasm32"))]
static WIND: plico::ShaderAsset = plico::shader_asset!("../assets/shaders/wind.wgsl");

#[cfg(not(target_arch = "wasm32"))]
static STRIPES: plico::ShaderAsset = plico::shader_asset!("../assets/shaders/stripes.wgsl");
#[cfg(not(target_arch = "wasm32"))]
static BLUR_DETAIL: plico::ShaderAsset =
    plico::shader_asset!("../assets/shaders/blur-detail.wgsl").prefilter_backdrop(8.);

/// The Effects page's serializable demonstration state.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Effects {
    pub paused: bool,
    pub frost: f64,
    pub glass_style: String,
    pub clarity: f64,
    pub bend: f64,
    pub sheen: f64,
    pub energy: f64,
    pub glow: f64,
    pub hue: f64,
    #[serde(default)]
    pub controls: GlassControls,
}

impl Default for Effects {
    fn default() -> Self {
        Self {
            paused: false,
            frost: 0.55,
            glass_style: "lens".into(),
            clarity: 0.5,
            bend: 18.,
            sheen: 0.8,
            energy: 0.6,
            glow: 0.7,
            hue: 0.15,
            controls: Default::default(),
        }
    }
}

#[derive(Clone, Default, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct GlassControls {
    pub tab: usize,
    pub checked: bool,
    pub enabled: bool,
    pub level: f64,
    pub applied: bool,
    pub confirmed: bool,
    pub mode: usize,
    menu: PopoverState,
    popover: PopoverState,
    select: PopoverState,
}

#[component]
fn glass_controls(cx: &mut UiContext, state: &mut GlassControls, paused: bool) {
    use plico::widgets::liquid::{self, ActionState};
    liquid::card(cx).width_percent(100.).children(|cx| {
        row(cx).align(Align::Center).gap(12).children(|cx| {
            text(cx, "Liquid controls").font_size(18).weight(650);
            liquid::badge_with_speed(cx, "Live", 52., if paused { 0. } else { 0.4 });
        });
        liquid::tabs(
            cx,
            &mut state.tab,
            &["Overview", "Details", "Activity"],
            88.,
        );
        row(cx)
            .wrap(true)
            .align(Align::Center)
            .gap(12)
            .children(|cx| {
                liquid::glass_checkbox(cx, "Show labels", &mut state.checked);
                liquid::glass_switch(cx, "Highlight", &mut state.enabled);
                liquid::glass_slider(cx, "Intensity", &mut state.level, 0.0..=100., 1., 164.);
            });
        row(cx)
            .wrap(true)
            .align(Align::Center)
            .gap(8)
            .children(|cx| {
                if liquid::action_button(
                    cx,
                    if state.applied { "Applied" } else { "Apply" },
                    if state.applied {
                        ActionState::Success
                    } else {
                        ActionState::Idle
                    },
                ) {
                    state.applied = true;
                }
                if liquid::slide_to_confirm(
                    cx,
                    if state.confirmed {
                        "Confirmed"
                    } else {
                        "Slide to confirm"
                    },
                    if state.confirmed {
                        ActionState::Success
                    } else {
                        ActionState::Idle
                    },
                    204.,
                ) {
                    state.confirmed = true;
                }
                liquid::button_group(cx, false).children(|cx| {
                    if liquid::group_item(cx, "Reset action") {
                        state.applied = false;
                    }
                    liquid::group_separator(cx, false);
                    if liquid::group_item(cx, "Reset slide") {
                        state.confirmed = false;
                    }
                });
            });
        row(cx).wrap(true).gap(10).children(|cx| {
            liquid::select(
                cx,
                &mut state.select,
                "Mode",
                &mut state.mode,
                &[
                    SelectOption::new("regular", 0, "Regular glass"),
                    SelectOption::new("clear", 1, "Clear glass"),
                ],
            )
            .width(150.)
            .response();
            liquid::dropdown_menu(cx, &mut state.menu, "Actions").children(|cx| {
                if menu_item(cx, "Reset intensity").clicked() {
                    state.level = 0.;
                }
            });
            liquid::popover(cx, &mut state.popover, "Selection details").children(|cx| {
                text!(cx, "Intensity: {:.0}%", state.level);
            });
        });
    });
}

#[component]
#[rustfmt::skip]
pub fn page(ui: &mut UiContext, state: &mut Effects) {
    heading(ui, "WGSL effects",
        "Three MyGO-inspired GPU surfaces: frosted glass, an assistant orb and a flowing wind field.");
    column(ui).height(0.).grow(1.).scroll_y(true).padding_right(24).gap(14) + |ui| {
        row(ui).gap(12).wrap(true).align(Align::Center) + |ui| {
            if button(ui).text(if state.paused { "Resume" } else { "Pause" }).ghost().clicked() {
                state.paused = !state.paused;
            }
            text(ui, "Frost").font_size(12).color(Color::Muted);
            slider(ui, "Glass frost", &mut state.frost, 0.0..=1.0).step(0.05).width(110.);
            text(ui, "Bend").font_size(12).color(Color::Muted);
            slider(ui, "Glass bend", &mut state.bend, 0.0..=40.0).step(1.0).width(110.);
            text(ui, "Sheen").font_size(12).color(Color::Muted);
            slider(ui, "Glass sheen", &mut state.sheen, 0.0..=1.0).step(0.05).width(110.);
        };
        row(ui).gap(10).wrap(true).align(Align::Center) + |ui| {
            for (name, value) in [
                ("Custom", "custom"),
                ("Regular", "regular"),
                ("Clear", "clear"),
                ("Lens", "lens"),
            ] {
                ui.keyed(value, |ui| {
                    let control = button(ui).text(name).radius(18.).height(36.)
                        .material(LiquidGlass::regular());
                    let control = if state.glass_style == value { control.primary() } else { control.ghost() };
                    if control.clicked() {
                        state.glass_style = value.into();
                    }
                });
            }
            text(ui, "Pane clarity").font_size(12).color(Color::Muted);
            slider(ui, "Pane clarity", &mut state.clarity, 0.0..=1.0).step(0.05).width(140.);
        };
        #[cfg(not(target_arch = "wasm32"))]
        {
            let material = match state.glass_style.as_str() {
                "regular" => LiquidGlass {
                    clarity: state.clarity as f32,
                    ..LiquidGlass::regular()
                },
                "clear" => LiquidGlass {
                    clarity: state.clarity as f32,
                    ..LiquidGlass::clear()
                },
                "lens" => LiquidGlass::lens(),
                _ => LiquidGlass::default(),
            };
            // Contrasting silhouettes make the pane's blur and rim refraction
            // visible; the shader reads everything painted before it.
            // Continuous corners carry a longer shoulder than circular arcs.
            // Use the pane token rather than the old circular 18-DIP radius so
            // the large surface reads rounded instead of almost rectangular.
            column(ui).position(Position::Relative).width_percent(100.).height(300.).shrink(0.).clip(true)
                .radius(24.).corner_smoothing(1.).background(Color::Literal(0x3c4045)) + |ui|
            {
                column(ui).absolute(-50., -70.).width(280.).height(280.).radius(140.)
                    .background(Color::Literal(0xcbbfb3)) + |_ui|
                {};
                column(ui).absolute(150., 30.).width(240.).height(240.).radius(120.)
                    .background(Color::Literal(0x96aaa9)) + |_ui|
                {};
                column(ui).absolute(300., 160.).width(280.).height(280.).radius(140.)
                    .background(Color::Literal(0xa4a2af)) + |_ui|
                {};
                let pane = column(ui).absolute(0., 0.);
                let pane = if state.glass_style == "lens" {
                    pane.inset(24.)
                } else {
                    pane.width_percent(100.).height_percent(100.)
                };
                pane.padding(28).gap(4).radius(24.).corner_smoothing(1.).material(LiquidGlass {
                    frost: state.frost as f32,
                    refraction: state.bend as f32 / 24.,
                    rim: state.sheen as f32,
                    animated: !state.paused,
                    ..material
                }) + |ui|
                {
                    column(ui).width(360).padding(18).gap(12).radius(20.).corner_smoothing(1.)
                        .background(Color::Panel).background_alpha(0.94).elevation(Elevation::CONTROL) + |ui|
                    {
                        text(ui, "Material preview").font_size(24).weight(650);
                        text(ui, "Sharp controls over a scene-backed surface.").font_size(13)
                            .color(Color::Muted).width_percent(100.);
                        row(ui).gap(8).wrap(true) + |ui| {
                            button(ui).text("Continue").primary().radius(20.).material(material)
                                .elevation(Elevation::CONTROL);
                            button(ui).text("Secondary").radius(20.).material(material)
                                .elevation(Elevation::SUBTLE);
                            button(ui).text("Unavailable").disabled(true).radius(20.)
                                .material(material);
                        };
                    };
                };
            };
            text(ui, "LIQUID GLASS · framework material: curved-edge refraction, frost and rim lighting")
                .font_size(11).color(Color::Muted);
            row(ui).width_percent(100.).shrink(0.).gap(12) + |ui| {
                for (label, spread) in [("Soft inset", 0.), ("Recessed surface", 1.)] {
                    ui.keyed(label, |ui| {
                        column(ui).width(0.).grow(1.).height(76.).padding(16).radius(16.)
                            .background(Color::Panel).inset_shadow(plico::BoxShadow {
                            offset_x: 0.,
                            offset_y: 2.,
                            blur: 10.,
                            spread,
                            color: Color::Literal(0x152438),
                            opacity: 0.18,
                        }) + |ui|
                        {
                            text(ui, label).weight(550).font_size(13);
                            text(ui, "Inner depth · continuous corners").color(Color::Muted).font_size(11);
                        };
                    });
                }
            };
            row(ui).width_percent(100.).shrink(0.).gap(12) + |ui| {
                for (label, prefilter) in [
                    ("Sparse samples · aliases", false),
                    ("Prefiltered blur · preserved average", true),
                ] {
                    ui.keyed(label, |ui| {
                        column(ui).width(0.).grow(1.).gap(6) + |ui| {
                            text(ui, label).font_size(12).color(Color::Muted);
                            column(ui).position(Position::Relative).width_percent(100.).height(64.)
                                .radius(12.).clip(true) + |ui|
                            {
                                shader(ui, &STRIPES).absolute(0., 0.).width_percent(100.)
                                    .height_percent(100.);
                                shader(ui, &BLUR_DETAIL).absolute(0., 0.).width_percent(100.)
                                    .height_percent(100.)
                                    .prefilter_backdrop(if prefilter { 8. } else { 0. });
                            };
                        };
                    });
                }
            };
            row(ui).width_percent(100.).shrink(0.).gap(16).align(Align::Stretch) + |ui| {
                column(ui).width(0.).grow(1.).height(320.).clip(true).radius(18.)
                    .background(Color::Literal(0x171126)) + |ui|
                {
                    column(ui).padding(16).gap(4).shrink(0.) + |ui| {
                        text(ui, "Assistant orb").font_size(17).weight(650)
                            .color(Color::Literal(0xf4f1ff));
                        text(ui, "chat-glow orb.kage · glass, drifting blobs, specular rim")
                            .font_size(11).color(Color::Literal(0xb8b3d6)).ellipsis(true)
                            .width_percent(100.);
                    };
                    column(ui).width_percent(100.).height(224.).align(Align::Center)
                        .justify(Justify::Center) + |ui|
                    {
                        plico::widgets::shader(ui, &ORB).width(208.).height(208.)
                            .values(&[state.energy as f32]).animated(true).paused(state.paused);
                    };
                    row(ui).padding_x(16).gap(8).align(Align::Center).shrink(0.) + |ui| {
                        text(ui, "Energy").font_size(12).color(Color::Literal(0xb8b3d6));
                        slider(ui, "Orb energy", &mut state.energy, 0.0..=1.0).step(0.05).width(0.)
                            .grow(1.);
                    };
                };
                column(ui).width(0.).grow(1.).height(320.).clip(true).radius(18.)
                    .background(Color::Literal(0x0b1424)) + |ui|
                {
                    column(ui).padding(16).gap(4).shrink(0.) + |ui| {
                        text(ui, "Wind field").font_size(17).weight(650)
                            .color(Color::Literal(0xe6f6ff));
                        text(ui, "wind.kage · warped noise, luminous ribbons, drifting sparkles")
                            .font_size(11).color(Color::Literal(0x9db9cc)).ellipsis(true)
                            .width_percent(100.);
                    };
                    plico::widgets::shader(ui, &WIND).width_percent(100.).height(200.)
                        .values(&[state.glow as f32, state.hue as f32]).animated(true).paused(state.paused);
                    row(ui).padding_x(16).gap(8).align(Align::Center).shrink(0.) + |ui| {
                        text(ui, "Glow").font_size(12).color(Color::Literal(0x9db9cc));
                        slider(ui, "Wind glow", &mut state.glow, 0.0..=1.0).step(0.05).width(0.)
                            .grow(1.);
                        text(ui, "Hue").font_size(12).color(Color::Literal(0x9db9cc));
                        slider(ui, "Wind hue", &mut state.hue, -0.5..=0.5).step(0.05).width(0.)
                            .grow(1.);
                    };
                };
            };
            glass_controls(ui, &mut state.controls, state.paused);
            text(ui, "WGSL · injected clock · read_backdrop for the pane · MyGO liquid-glass, chat-glow and wind")
                .font_size(11).color(Color::Muted);
        }
        #[cfg(target_arch = "wasm32")]
        text(ui, "WGSL effect surfaces require the desktop wgpu host.").color(Color::Muted);
    };
}

#[cfg(test)]
mod tests {
    use plico_renderer::shader::ShaderEffectSource;

    #[test]
    fn mygo_effects_validate_under_the_plico_prelude() {
        ShaderEffectSource::with_backdrop(plico::materials::liquid_glass_shader().source(), true)
            .expect("liquid glass validates with a backdrop");
        ShaderEffectSource::new(include_str!("../assets/shaders/orb.wgsl")).expect("orb validates");
        ShaderEffectSource::new(include_str!("../assets/shaders/wind.wgsl"))
            .expect("wind validates");
    }
}
