//! MyGO-inspired GPU surfaces: a frosted liquid-glass pane, the chat-glow
//! assistant orb and the wind example's flowing field, all authored in WGSL.
use crate::heading;
use plico::prelude::*;

#[cfg(not(target_arch = "wasm32"))]
static ORB: plico::ShaderAsset = plico::shader_asset!("../assets/shaders/orb.wgsl");
#[cfg(not(target_arch = "wasm32"))]
static WIND: plico::ShaderAsset = plico::shader_asset!("../assets/shaders/wind.wgsl");

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
}

impl Default for Effects {
    fn default() -> Self {
        Self {
            paused: false,
            frost: 0.55,
            glass_style: "custom".into(),
            clarity: 0.5,
            bend: 18.,
            sheen: 0.8,
            energy: 0.6,
            glow: 0.7,
            hue: 0.15,
        }
    }
}

#[component]
#[rustfmt::skip]
pub fn page(ui: &mut UiContext, state: &mut Effects) {
    heading(ui, "WGSL effects",
        "Three MyGO-inspired GPU surfaces: frosted glass, an assistant orb and a flowing wind field.");
    column(ui).height(0.).grow(1.).scroll_y(true).gap(14) + |ui| {
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
                _ => LiquidGlass::default(),
            };
            // A busy, saturated backdrop makes the pane's blur and rim refraction
            // visible; the shader reads everything painted before it.
            column(ui).position(Position::Relative).width_percent(100.).height(300.).shrink(0.).clip(true)
                .radius(18.).corner_smoothing(1.).background(Color::Literal(0x121826)) + |ui|
            {
                column(ui).absolute(-50., -70.).width(280.).height(280.).radius(140.)
                    .background(Color::Literal(0xf97316)) + |_ui|
                {};
                column(ui).absolute(150., 30.).width(240.).height(240.).radius(120.)
                    .background(Color::Literal(0x38bdf8)) + |_ui|
                {};
                column(ui).absolute(300., 160.).width(280.).height(280.).radius(140.)
                    .background(Color::Literal(0xa855f7)) + |_ui|
                {};
                column(ui).absolute(0., 0.).width_percent(100.).height_percent(100.).padding(28).gap(4)
                    .radius(18.).corner_smoothing(1.).material(LiquidGlass {
                    frost: state.frost as f32,
                    tint_opacity: if state.glass_style == "custom" { 0.55 } else { 0. },
                    refraction: state.bend as f32 / 24.,
                    rim: state.sheen as f32,
                    animated: !state.paused,
                    ..material
                }) + |ui|
                {
                    column(ui).width(360).padding(18).gap(12).radius(16.).corner_smoothing(1.)
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
