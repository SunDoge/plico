//! Host-agnostic declaration runtime: Engine adapter, Source, HostView and motion.
//! Window and GPU backends live in plico-desktop / plico-shell; no window deps here.
use peniko::Color;
use plico::{Capabilities, Color as Token, Engine, Entity, Kind, Theme};
use plico_renderer::{
    EventResponse, KeyPhase, PointerButton, PointerPhase, Scene, TextContext, UiEvent,
    layout::{PlacedNode, compute_and_walk_with_scroll, compute_and_walk_with_scroll_into},
};
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};
use taffy::{geometry::Rect, prelude::*};

pub use plico_renderer::text::FontAsset;
pub use plico_renderer::{
    HostAction, KeyEvent, RenderBackend, WakeCallback, WindowCommand, WindowLevel, WindowOptions,
    WindowResizeEdges,
};

mod adapter;
mod button_state;
mod input;
mod layout_style;
mod modal;
mod popover;
mod presentation;
mod recipe;
pub mod regions;
mod scroll_motion;
mod scrollbar_visibility;
mod selection;
mod services;
mod shader;
mod slider;
mod source;
mod style_motion;
mod toggle;
mod tooltip;
mod transition;
mod view;

pub use adapter::{Adapter, BackendStats};
pub(crate) use adapter::{interactive, make_paint, rgb, semantic_id};
pub use services::ServiceView;
pub use source::{BatchStats, Source, SourceError};
pub use view::{HostView, NativeApp, NativeView};

#[doc(hidden)]
pub use HostView as View;
#[doc(hidden)]
pub use impl_host_view_for_app as impl_view_for_app;
#[doc(hidden)]
pub use plico as __ui_core;
#[doc(hidden)]
pub use plico_dev as __app_protocol;

#[cfg(not(target_arch = "wasm32"))]
mod surface;
#[cfg(not(target_arch = "wasm32"))]
pub use surface::SurfaceSource;

mod zoom;
pub use zoom::{ZoomController, ZoomSource};

/// Blend two packed RGB colors (`0xRRGGBB`) channel-wise.
///
/// `amount` is clamped to `0..=1`; alpha is carried by the caller.
pub(crate) fn mix_rgb(from: u32, to: u32, amount: f32) -> u32 {
    let amount = amount.clamp(0., 1.);
    let mix = |shift: u32| {
        let from = ((from >> shift) & 255) as f32;
        let to = ((to >> shift) & 255) as f32;
        (from + (to - from) * amount).round().clamp(0., 255.) as u32
    };
    (mix(16) << 16) | (mix(8) << 8) | mix(0)
}

/// Whether a theme's window surface reads as dark (MyGO `Theme.Dark`).
pub(crate) fn is_dark(theme: Theme) -> bool {
    let [r, g, b] = [16, 8, 0].map(|shift| ((theme.surface >> shift) & 255) as f32);
    0.2126 * r + 0.7152 * g + 0.0722 * b < 128.
}

/// Semantic command protocol and deterministic headless test driver.
pub mod testing;

/// Native blocking tasks and retained worker-local services.
#[cfg(not(target_arch = "wasm32"))]
pub mod background;
/// One-shot Future lifecycle and typed task services; executors remain host-owned.
pub mod tasks {
    pub use plico_dev::tasks::*;
}
