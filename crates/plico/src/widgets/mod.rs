//! Context-first widget constructors. Ordinary functions compose application widgets.
//!
//! Builders borrow the context, apply configuration, then submit when consumed or
//! dropped. No `.show(cx)` or widget trait implementation is needed.
//!
//! ```
//! use plico::{Engine, prelude::*};
//! #[component]
//! #[rustfmt::skip]
//! fn settings(cx: &mut UiContext) {
//!     column(cx).gap(12) + |cx| {
//!         text(cx, "Settings").font_size(24);
//!         button(cx).text("Save").primary();
//!     };
//! }
//! Engine::default().frame_declarations(settings).unwrap();
//! ```
use crate::{SvgAsset, UiContext};
use std::borrow::Cow;

/// Declare a vertical container; consume it with `+ |cx| { ... }` or `.children(...)`.
pub fn column<'u, 'e>(cx: &'u mut crate::UiContext<'e>) -> crate::Layout<'u, 'e> {
    cx.column()
}
/// Declare a grid container.
pub fn grid<'u, 'e>(cx: &'u mut crate::UiContext<'e>) -> crate::Layout<'u, 'e> {
    cx.grid()
}
/// Declare a horizontal container.
pub fn row<'u, 'e>(cx: &'u mut crate::UiContext<'e>) -> crate::Layout<'u, 'e> {
    cx.row()
}
/// Declare text; borrowed strings remain borrowed during the declaration.
pub fn text<'u, 'e, 't>(
    cx: &'u mut crate::UiContext<'e>,
    label: impl Into<Cow<'t, str>>,
) -> crate::Text<'u, 'e, 't> {
    cx.text(label)
}
/// Declare a themed button, then configure it before reading its response.
pub fn button<'u, 'e, 't>(cx: &'u mut crate::UiContext<'e>) -> crate::Button<'u, 'e, 't> {
    cx.themed_button()
}
/// Declare button behavior without appearance; the label names it for accessibility.
pub fn button_base<'u, 'e, 't>(
    cx: &'u mut crate::UiContext<'e>,
    label: impl Into<Cow<'t, str>>,
) -> crate::ButtonBase<'u, 'e, 't> {
    cx.button_base(label)
}
/// Declare a compile-time embedded SVG asset.
pub fn svg<'u, 'e>(cx: &'u mut crate::UiContext<'e>, asset: &SvgAsset) -> crate::Svg<'u, 'e> {
    cx.svg(asset)
}
/// Declare generated SVG markup (for example a cached data visualization).
pub fn svg_source<'u, 'e>(cx: &'u mut crate::UiContext<'e>, source: String) -> crate::Svg<'u, 'e> {
    cx.svg_source(source)
}
/// Declare an embedded raster image; the backend decodes and caches it.
pub fn image<'u, 'e>(
    cx: &'u mut crate::UiContext<'e>,
    asset: &'u crate::ImageAsset,
) -> crate::Image<'u, 'e> {
    cx.image(asset)
}
/// Declare and configure a controlled switch. Dropping it submits the declaration.
pub fn switch<'u, 'e, 't>(
    cx: &'u mut crate::UiContext<'e>,
    label: impl Into<Cow<'t, str>>,
    value: &'t mut bool,
) -> crate::Toggle<'u, 'e, 't> {
    cx.toggle_control(crate::Kind::Switch, label, value)
}
/// Declare and configure a controlled checkbox; `.changed()` reads its response.
pub fn checkbox<'u, 'e, 't>(
    cx: &'u mut crate::UiContext<'e>,
    label: impl Into<Cow<'t, str>>,
    value: &'t mut bool,
) -> crate::Toggle<'u, 'e, 't> {
    cx.toggle_control(crate::Kind::Checkbox, label, value)
}

/// Format text synchronously without allocating a fresh temporary String each frame.
pub fn text_fmt<'u, 'e>(
    cx: &'u mut crate::UiContext<'e>,
    args: std::fmt::Arguments<'_>,
) -> crate::Text<'u, 'e, 'static> {
    cx.text_fmt(args)
}

/// A theme-colored SVG icon sized for the active control density.
pub fn icon<'u, 'e>(cx: &'u mut crate::UiContext<'e>, asset: &SvgAsset) -> crate::Svg<'u, 'e> {
    let size = cx.theme().metrics.control_icon() as f32;
    cx.svg(asset).size(size, size)
}

mod alert;
mod calendar;
mod colorpicker;
mod composite;
mod dateinput;
mod desktop;
mod find;
mod form;
mod indicators;
mod media;
mod navigation;
mod numberinput;
mod search;
mod split;
mod timeinput;
mod tree;
pub use alert::{ALERT_DISMISSED, alert_dialog};
pub use calendar::{CalendarResponse, calendar, calendar_content, calendar_with};
pub use colorpicker::{color_picker, color_well};
pub use composite::{
    NavEntryAppearance, Panel, accordion, badge, choice, collapsible, nav_entry,
    nav_entry_with_appearance, nav_item, nav_menu, panel, progress, segmented, separator, stat,
};
pub use dateinput::{date_input, date_input_with};
pub use desktop::{
    Toast, ToastHost, ToastKind, drag_area, drag_area_content, tabs, toast_viewport, toolbar,
    toolbar_spacer,
};
pub use find::{FindResponse, find_bar};
pub use form::{Field, field, fieldset, form};
pub use indicators::{
    MeterLevels, meter, progress_ring, progress_ring_sized, rating, rating_labeled, spinner,
    spinner_sized, stepper, stepper_labeled,
};
pub use media::{Avatar, RubyText, avatar, ruby_text};
pub use navigation::{
    back_button, breadcrumbs, forward_button, link, link_base, sidebar, sidebar_entry,
    sidebar_item, sidebar_section,
};
pub use numberinput::number_input;
pub use search::{SearchResponse, search_field};
pub use split::{split, split_percent, split_vertical, split_vertical_percent};
pub use timeinput::time_input;
pub use tree::{tree, tree_item};

/// A native editor. `TextBuffer::multiline()` enables wrapped, top-aligned text;
/// ordinary and secret buffers remain single-line.
pub fn text_input<'u, 'e, 't>(
    cx: &'u mut crate::UiContext<'e>,
    label: impl Into<Cow<'t, str>>,
    buffer: &crate::TextBuffer,
) -> crate::Input<'u, 'e, 't> {
    cx.text_input(label, buffer)
}

/// Edit an ordinary string with persistent caret, selection, and IME state.
/// Use [`text_input`] with a secret [`crate::TextBuffer`] for credentials.
#[track_caller]
pub fn text_input_string<'u, 'e, 't>(
    cx: &'u mut crate::UiContext<'e>,
    label: impl Into<Cow<'t, str>>,
    value: &'t mut String,
) -> crate::Input<'u, 'e, 't> {
    cx.text_input_string(label, value)
}

/// A window-level modal portal. Its content closure executes immediately.
pub fn modal<'u, 'e, 's>(
    cx: &'u mut UiContext<'e>,
    state: &'s mut crate::ModalState,
) -> crate::Modal<'u, 'e, 's> {
    cx.modal(state)
}

pub fn shader<'u, 'e>(
    cx: &'u mut UiContext<'e>,
    asset: &crate::ShaderAsset,
) -> crate::Shader<'u, 'e, 'static> {
    cx.shader(asset)
}

pub fn slider<'u, 'e, 't>(
    cx: &'u mut UiContext<'e>,
    label: impl Into<Cow<'t, str>>,
    value: &'t mut f64,
    range: std::ops::RangeInclusive<f64>,
) -> crate::Slider<'u, 'e, 't> {
    cx.slider(label, value, range)
}

/// A two-thumb slider with the active fill between the ends. Each thumb
/// has its own keyboard focus and accessible numeric value.
pub fn range_slider<'u, 'e, 't>(
    cx: &'u mut UiContext<'e>,
    label: impl Into<Cow<'t, str>>,
    low: &'t mut f64,
    high: &'t mut f64,
    range: std::ops::RangeInclusive<f64>,
) -> crate::Slider<'u, 'e, 't> {
    cx.range_slider(label, low, high, range)
}

/// Declare a trigger and a portable anchored popup.
pub fn popover<'u, 'e, 's>(
    cx: &'u mut UiContext<'e>,
    state: &'s mut crate::PopoverState,
    label: impl Into<Cow<'s, str>>,
) -> crate::Popover<'u, 'e, 's> {
    cx.popover(state, label)
}

mod popup;
mod radio;
pub use popup::{MenuItem, Select, SelectOption, SelectResponse, menu, menu_item, select};
pub use radio::{radio, radio_group};

mod tooltip;
pub use tooltip::{Tooltip, tooltip};

/// Fixed-height virtual scrolling list. Only visible rows and overscan are declared.
pub fn virtual_list<'u, 'e>(
    cx: &'u mut crate::UiContext<'e>,
    count: usize,
    row_height: f32,
) -> crate::VirtualList<'u, 'e> {
    crate::VirtualList::new(cx, count, row_height)
}

/// Variable-height virtual scrolling list. Visible rows are measured by the
/// host; unseen rows use the running measured average.
pub fn variable_virtual_list<'u, 'e>(
    cx: &'u mut crate::UiContext<'e>,
    count: usize,
    estimated_row_height: f32,
) -> crate::VariableVirtualList<'u, 'e> {
    crate::VariableVirtualList::new(cx, count, estimated_row_height)
}

mod inspection;
pub use inspection::{TableColumn, property_field, table_header, table_row};

mod settings;
pub use settings::{SettingsRow, SettingsSection, settings_row, settings_section};

mod feedback;
pub use feedback::{
    Tone, alert, badge_tone, chip, empty_state, notification, pagination, skeleton, tag,
};
pub mod liquid;
