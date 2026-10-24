//! Immediate API over a generational retained arena. Renderer-free foundation.
#![forbid(unsafe_code)]
extern crate self as plico_core;
mod arena;
mod asset;
mod clock;
mod engine;
pub mod icons;
mod input;
mod layout;
mod modal;
mod model;
mod router;
pub use input::{InputEdit, InputNavigation, InputPresentation, TextBuffer};
pub use layout::*;
pub use modal::{Modal, ModalResponse, ModalState, Popover, PopoverState};
mod virtual_list;
pub use virtual_list::{VirtualList, VirtualListResponse};
mod numeric;
pub use numeric::{NumericRange, SliderResponse};
mod shader;
pub use shader::{ShaderAsset, ShaderParams};
mod snapshot;
mod theme;
mod ui;
pub mod widgets;
pub use arena::{Arena, Entity};
pub use asset::{ImageAsset, SvgAsset};
pub use clock::{Clock, ClockError, FrameTime, SystemClock, VirtualClock};
pub use engine::Engine;
pub use model::{
    Align, Changes, Color, FontFamily, FrameDelta, FrameError, FrameStats, ImplicitKey, Justify,
    Key, Kind, KindFamily, Motion, NodeChange, NodeRef, Size, SliderSpec, Style, StyleDiff,
    StyleProperties, TextSpan, UnmatchedPaths, WidgetRole,
};
pub use plico_macros::{InvalidateChannels, KindClass, component, text, view};
pub use router::{DEFAULT_HISTORY_LIMIT, Router};
pub use snapshot::{PathKey, ViewNode};
pub use theme::{Metrics, Theme};
pub use widgets::{SelectOption, SelectResponse};
/// A widget builds immediately. Its data and callbacks never enter the retained node arena.
pub trait Widget {
    type Response;
    fn show(self, ui: &mut UiContext<'_>) -> Self::Response;
}
impl<F, R> Widget for F
where
    F: FnOnce(&mut UiContext<'_>) -> R,
{
    type Response = R;
    fn show(self, ui: &mut UiContext<'_>) -> R {
        self(ui)
    }
}
pub use engine::DeclareView;
pub use ui::{
    Button, ButtonBase, Image, InnerResponse, Input, InputResponse, Layout, Response, Shader,
    Slider, Svg, Text, Toggle, ToggleResponse, UiContext,
};
/// Implementation surface that `plico-macros` generated code links against.
///
/// These are as public and stable as any other API: the macros compile into
/// downstream code that references these items by path. `site_id` and the
/// FNV-1a hasher back deterministic identity across builds; `bitflags` is
/// re-exported because generated invalidation flags need it in scope.
pub mod macro_support {
    pub use bitflags::bitflags;
    /// Deterministic namespace hash, evaluated at compile time by macros.
    pub const fn site_id(namespace: &str, site: u32) -> u64 {
        let bytes = namespace.as_bytes();
        let mut hash = 0xcbf29ce484222325u64;
        let mut i = 0;
        while i < bytes.len() {
            hash = (hash ^ bytes[i] as u64).wrapping_mul(0x100000001b3);
            i += 1;
        }
        (hash ^ site as u64).wrapping_mul(0x100000001b3)
    }
    /// FNV-1a hasher for runtime scope identity. Unlike `DefaultHasher`, its
    /// output is stable across compiler versions; reload identity matching
    /// depends on this, so it shares the algorithm with `site_id`.
    pub struct StableHasher(u64);
    impl Default for StableHasher {
        fn default() -> Self {
            Self::new()
        }
    }
    impl StableHasher {
        pub const fn new() -> Self {
            Self(0xcbf29ce484222325)
        }
        pub fn write(&mut self, bytes: &[u8]) {
            for &b in bytes {
                self.0 = (self.0 ^ b as u64).wrapping_mul(0x100000001b3);
            }
        }
        pub fn write_u64(&mut self, value: u64) {
            self.write(&value.to_le_bytes());
        }
        pub fn finish(&self) -> u64 {
            self.0
        }
    }
    /// Stable seed for one logical key: equal keys hash equally across builds,
    /// and the discriminant byte keeps different variants from colliding.
    pub fn key_seed(key: crate::Key) -> u64 {
        let mut hash = StableHasher::new();
        match key {
            crate::Key::Static(s) => {
                hash.write(&[0]);
                hash.write(s.as_bytes());
            }
            crate::Key::Number(n) => {
                hash.write(&[1]);
                hash.write_u64(n);
            }
            crate::Key::Auto(n) => {
                hash.write(&[2]);
                hash.write_u64(n as u64);
            }
            crate::Key::Generated(n) => {
                hash.write(&[3]);
                hash.write_u64(n);
            }
        }
        hash.finish()
    }
}

/// Recommended imports for application and custom widget modules.
pub mod prelude {
    pub use crate::widgets::{
        Button, Checkbox, Panel, Switch, badge, button, button_base, checkbox, choice, collapsible,
        column, grid, icon, image, menu, menu_item, modal, nav_item, panel, popover, progress, row,
        segmented, select, separator, shader, slider, stat, svg, svg_source, switch, text,
        text_input, tooltip, virtual_list,
    };
    pub use crate::{
        AUTO, Align, BoxSizing, Color, Direction, Display, Edges, FlexDirection, FlexWrap,
        GridFlow, GridLine, GridTemplate, ImplicitKey, InnerResponse, InputNavigation,
        InputResponse, Justify, Length, Metrics, ModalState, Motion, PopoverState, Position,
        Response, Router, SelectOption, SelectResponse, ShaderAsset, SliderResponse, SvgAsset,
        TextBuffer, Theme, ToggleResponse, Track, TrackSize, UiContext, VirtualList,
        VirtualListResponse, Widget, component, fr, minmax, percent, px, text, view,
    };
}

mod capabilities;
pub use capabilities::Capabilities;
