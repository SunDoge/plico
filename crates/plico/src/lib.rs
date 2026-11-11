//! Immediate API over a generational retained arena. Renderer-free foundation.
#![forbid(unsafe_code)]
extern crate self as plico;
mod arena;
mod asset;
mod clock;
mod engine;
pub mod icons;
mod input;
mod keyboard;
pub use keyboard::{
    KeyEvent, KeyLocation, KeyPhase, Modifiers, Shortcut, ShortcutConflict, ShortcutMap,
    StandardShortcut,
};
mod layout;
mod local;
mod localization;
pub use localization::Localizer;
mod storage;
pub use calendar::{CalendarDate, CalendarMode, CalendarState, WeekStart};
pub use local::LocalState;
pub use storage::{StateStorageSnapshot, StorageError};
mod calendar;
pub mod effects;
pub mod materials;
mod modal;
mod model;
pub use materials::{BackdropBlur, Elevation, GlassStyle, LiquidGlass, ScrollEdgeSide};
mod router;
pub use input::{InputEdit, InputNavigation, InputPresentation, TextBuffer};
pub use layout::*;
pub use modal::{Modal, ModalResponse, ModalState, Popover, PopoverState};
mod visibility;
pub use visibility::VisibleRegion;
mod virtual_list;
pub use virtual_list::{VirtualList, VirtualListResponse};
mod numeric;
pub use numeric::{NumericRange, SliderResponse, ValueControlResponse};
mod shader;
pub use shader::{ShaderAsset, ShaderParams, ShaderTransition, ShaderValue};
mod snapshot;
mod text_style;
mod time;
pub use time::TimeOfDay;
mod preferences;
pub use preferences::DisplayPreferences;
mod theme;
mod ui;
pub mod widgets;
pub use arena::{Arena, Entity};
pub use asset::{ImageAsset, SvgAsset};
pub use clock::{Clock, ClockError, FrameTime, SystemClock, VirtualClock};
pub use engine::Engine;
pub use model::{
    Align, BoxShadow, Changes, Color, Easing, FocusAxis, FontFamily, FrameDelta, FrameError,
    FrameStats, ImplicitKey, Justify, Key, Kind, KindFamily, Motion, NodeChange, NodeRef,
    Presentation, Size, SliderSpec, Style, StyleDiff, StyleProperties, TextSpan, UnmatchedPaths,
    WidgetRole,
};
pub use plico_macros::{InvalidateChannels, KindClass, Route, component, text};
pub use router::{DEFAULT_HISTORY_LIMIT, Route, Router};
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
#[doc(hidden)]
pub use View as DeclareView;
pub use engine::View;
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
    /// Shared setter definitions used by framework builders and property editors.
    #[doc(hidden)]
    pub use crate::__plico_text_style_methods as text_style_methods;
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
    pub use crate::materials::{backdrop_blur, scroll_edge};
    pub use crate::widgets::{
        ALERT_DISMISSED, Avatar, Button, CalendarResponse, Checkbox, Field, FindResponse,
        MeterLevels, Panel, RubyText, SearchResponse, Switch, TableColumn, Toast, ToastHost,
        ToastKind, Tone, accordion, alert, alert_dialog, avatar, back_button, badge, badge_tone,
        breadcrumbs, button, button_base, calendar, calendar_content, calendar_with, checkbox,
        chip, choice, collapsible, color_picker, color_well, column, date_input, date_input_with,
        drag_area, empty_state, field, fieldset, find_bar, form, forward_button, grid, icon, image,
        link, menu, menu_item, meter, modal, nav_entry, nav_item, nav_menu, notification,
        number_input, pagination, panel, popover, progress, progress_ring, progress_ring_sized,
        property_field, radio, radio_group, range_slider, rating, rating_labeled, row, ruby_text,
        search_field, segmented, select, separator, settings_row, settings_section, shader,
        sidebar, sidebar_item, sidebar_section, skeleton, slider, spinner, split, split_percent,
        split_vertical, split_vertical_percent, stat, stepper, stepper_labeled, svg, svg_source,
        switch, table_header, table_row, tabs, tag, text, text_empty, text_input,
        text_input_string, time_input, toast_viewport, toolbar, toolbar_spacer, tooltip, tree,
        tree_item, virtual_list,
    };
    pub use crate::{
        AUTO, Align, BackdropBlur, BoxSizing, CalendarDate, Color, Direction, Display,
        DisplayPreferences, Edges, Elevation, FlexDirection, FlexWrap, FocusAxis, FontFamily,
        GlassStyle, GridFlow, GridLine, GridTemplate, ImplicitKey, InnerResponse, InputNavigation,
        InputResponse, Justify, KeyEvent, KeyLocation, KeyPhase, Length, LiquidGlass, Metrics,
        ModalState, Modifiers, Motion, PopoverState, Position, Presentation, Response, Route,
        Router, ScrollEdgeSide, SelectOption, SelectResponse, ShaderAsset, Shortcut, ShortcutMap,
        SliderResponse, StandardShortcut, SvgAsset, TextBuffer, Theme, TimeOfDay, ToggleResponse,
        Track, TrackSize, UiContext, ValueControlResponse, VirtualList, VirtualListResponse,
        VisibleRegion, Widget, component, fr, minmax, percent, px, text,
    };
}

mod capabilities;
pub use capabilities::Capabilities;

/// Portable style translation shared by native-widget backends.
#[cfg(feature = "taffy")]
pub mod taffy_layout;

#[cfg(feature = "experiment-patch")]
#[doc(hidden)]
pub use engine::{ExperimentButtonStyle, ExperimentContext, ExperimentTextStyle};
