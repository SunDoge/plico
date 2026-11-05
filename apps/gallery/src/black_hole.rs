//! A gravitational lens over the whole window: with the toggle on, every
//! page renders behind a black hole that follows the pointer - horizon
//! silhouette, photon ring and Einstein-arc distortion of the content.
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
pub fn overlay(ui: &mut UiContext, enabled: &mut bool) {
    #[cfg(not(target_arch = "wasm32"))]
    if *enabled {
        let (x, y) = ui.pointer_position().unwrap_or((-1., -1.));
        plico::widgets::shader(ui, &BLACK_HOLE).key("black-hole-overlay").width_percent(100.).height_percent(100.)
            .position(plico::Position::Absolute).values(&[x as f32, y as f32]).animated(true);
    }
}

/// The page hosting the toggle and documenting what the overlay does.
#[component]
#[rustfmt::skip]
pub fn page(ui: &mut UiContext, enabled: &mut bool) {
    heading(ui, "Black hole",
        "A gravitational lens at the pointer. Toggle it on and every page is distorted behind the horizon.");
    #[cfg(not(target_arch = "wasm32"))]
    {
        row(ui).gap(10).align(Align::Center) + |ui| {
            switch(ui, "Gravitational lens", enabled);
            text!(ui, "Overlay {}", if *enabled { "on" } else { "off" }).color(Color::Muted);
        };
        column(ui).padding(12).radius(10.).background(Color::Panel).border(1., Color::Border) + |ui| {
            text(ui, "How it works").font_size(15).weight(650);
            text(ui, "The overlay is a backdrop-reading WGSL surface: it samples what each page paints behind it through plico.backdrop_at, displaces the sample toward the pointer (first-order lens deflection), paints the event horizon and the photon ring, and follows the pointer across pages. Backdrop readers get full-damage semantics - like MyGO's scene.Effect.Backdrop.")
                .font_size(12).color(Color::Muted);
        };
    }
    #[cfg(target_arch = "wasm32")]
    text(ui, "Backdrop-reading shaders require the desktop wgpu host.").color(Color::Muted);
}
