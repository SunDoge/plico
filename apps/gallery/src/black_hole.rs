//! A small stylized cursor lens: a compact dark core and broad gravitational field.
//! Distortion stays local to the pointer; the rest of the UI is unchanged.
//! The overlay declares a backdrop-reading shader surface sized to the
//! window; the pointer flows through the declaration-time query.
use crate::heading;
use plico::prelude::*;

#[cfg(not(target_arch = "wasm32"))]
static BLACK_HOLE: plico::ShaderAsset =
    plico::shader_asset!("../assets/shaders/black_hole.wgsl").read_backdrop();

/// The overlay: call last in the root declaration, after all pages, so it
/// reads everything painted before it. `enabled` carries the toggle.
#[component]
#[rustfmt::skip]
pub fn overlay(
    ui: &mut UiContext,
    #[cfg(target_arch = "wasm32")] _enabled: &mut bool,
    #[cfg(not(target_arch = "wasm32"))] enabled: &mut bool,
) {
    #[cfg(not(target_arch = "wasm32"))]
    if *enabled {
        let (x, y) = ui.pointer_position().unwrap_or((-1., -1.));
        plico::widgets::shader(ui, &BLACK_HOLE).key("black-hole-overlay").width_percent(100.).height_percent(100.)
            .position(plico::Position::Absolute).inset(0.).values(&[x as f32, y as f32]).animated(true);
    }
}

/// The page hosting the toggle and documenting what the overlay does.
#[component]
#[rustfmt::skip]
pub fn page(
    ui: &mut UiContext,
    #[cfg(target_arch = "wasm32")] _enabled: &mut bool,
    #[cfg(not(target_arch = "wasm32"))] enabled: &mut bool,
) {
    heading(ui, "Black hole", "A small lens at your pointer. Toggle it on, then move across any page.");
    #[cfg(not(target_arch = "wasm32"))]
    {
        row(ui).gap(10).align(Align::Center) + |ui| {
            switch(ui, "Gravitational lens", enabled);
            text!(ui, "Overlay {}", if *enabled { "on" } else { "off" }).color(Color::Muted);
        };
        column(ui).padding(12).radius(10.).background(Color::Panel).border(1., Color::Border) + |ui| {
            text(ui, "How it works").font_size(15).weight(650);
            text(ui, "A 20 px dark core inside a broad gravitational field. Nearby text and edges bend toward the pointer; distortion fades smoothly within 300 px.")
                .font_size(14).color(Color::Muted);
        };
    }
    #[cfg(target_arch = "wasm32")]
    text(ui, "Backdrop-reading shaders require the desktop wgpu host.").color(Color::Muted);
}
