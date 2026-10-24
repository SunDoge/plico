//! Context-first widget constructors. Ordinary functions compose application widgets.
//!
//! Builders borrow the context, apply configuration, then submit when consumed or
//! dropped. No `.show(cx)` or widget trait implementation is needed.
//!
//! ```
//! use plico_core::{Engine, prelude::*};
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
use crate::{Color, InnerResponse, Motion, Response, SvgAsset, Ui, Widget};
use std::borrow::Cow;

#[derive(Default, Clone, Copy)]
enum Variant {
    #[default]
    Default,
    Primary,
    Ghost,
}
/// A standalone widget value. Constructing it does not borrow Ui or retain callbacks.
pub struct Button<'a> {
    label: Cow<'a, str>,
    variant: Variant,
    disabled: bool,
    icon: Option<&'a SvgAsset>,
    width: Option<f32>,
    motion: Option<Motion>,
}
impl<'a> Button<'a> {
    pub fn new(label: impl Into<Cow<'a, str>>) -> Self {
        Self {
            label: label.into(),
            variant: Variant::Default,
            disabled: false,
            icon: None,
            width: None,
            motion: None,
        }
    }
    pub fn primary(mut self) -> Self {
        self.variant = Variant::Primary;
        self
    }
    pub fn ghost(mut self) -> Self {
        self.variant = Variant::Ghost;
        self
    }
    pub fn disabled(mut self, value: bool) -> Self {
        self.disabled = value;
        self
    }
    pub fn icon(mut self, icon: &'a SvgAsset) -> Self {
        self.icon = Some(icon);
        self
    }
    pub fn width(mut self, width: f32) -> Self {
        self.width = Some(width);
        self
    }
    pub fn motion(mut self, motion: Motion) -> Self {
        self.motion = Some(motion);
        self
    }
    fn bind<'u, 'e>(&self, ui: &'u mut Ui<'e>) -> crate::Button<'u, 'e, '_> {
        let mut button = ui.button(self.label.as_ref()).disabled(self.disabled);
        match self.variant {
            Variant::Primary => button = button.primary(),
            Variant::Ghost => button = button.background(Color::Surface),
            Variant::Default => {}
        }
        if let Some(width) = self.width {
            button = button.width(width);
        }
        if let Some(motion) = self.motion {
            button = button.motion(motion);
        }
        button
    }
    pub fn children<F, R>(self, content: F) -> ContentButton<'a, F>
    where
        F: FnOnce(&mut Ui<'_>) -> R,
    {
        ContentButton {
            button: self,
            content,
        }
    }
}
impl Widget for Button<'_> {
    type Response = Response;
    fn show(self, ui: &mut Ui<'_>) -> Response {
        let button = self.bind(ui);
        if let Some(icon) = self.icon {
            button.icon(icon).text(self.label.as_ref()).response()
        } else {
            button.response()
        }
    }
}
/// Composable default button; the closure is invoked once and dropped during show.
pub struct ContentButton<'a, F> {
    button: Button<'a>,
    content: F,
}
impl<F, R> Widget for ContentButton<'_, F>
where
    F: FnOnce(&mut Ui<'_>) -> R,
{
    type Response = InnerResponse<R>;
    fn show(self, ui: &mut Ui<'_>) -> Self::Response {
        self.button.bind(ui).children(self.content)
    }
}

macro_rules! toggle {
    ($name:ident, $method:ident) => {
        pub struct $name<'a> {
            label: Cow<'a, str>,
            value: &'a mut bool,
        }
        impl<'a> $name<'a> {
            pub fn new(label: impl Into<Cow<'a, str>>, value: &'a mut bool) -> Self {
                Self {
                    label: label.into(),
                    value,
                }
            }
        }
        impl Widget for $name<'_> {
            type Response = bool;
            fn show(self, ui: &mut Ui<'_>) -> bool {
                ui.$method(self.label.as_ref(), self.value)
            }
        }
    };
}
toggle!(Checkbox, checkbox);
toggle!(Switch, switch);

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
    cx.button("").composable()
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

/// A theme-colored SVG icon with a default size matching the inherited text size.
pub fn icon<'u, 'e>(cx: &'u mut crate::UiContext<'e>, asset: &SvgAsset) -> crate::Svg<'u, 'e> {
    let size = cx.inherited.font_size.saturating_add(2) as f32;
    cx.svg(asset).size(size, size)
}

mod composite;
pub use composite::{
    Panel, badge, choice, collapsible, nav_item, panel, progress, segmented, separator, stat,
};

/// A native single-line editor. Secret buffers expose only masked presentation.
pub fn text_input<'u, 'e, 't>(
    cx: &'u mut crate::UiContext<'e>,
    label: impl Into<Cow<'t, str>>,
    buffer: &crate::TextBuffer,
) -> crate::Input<'u, 'e, 't> {
    cx.text_input(label, buffer)
}

/// A window-level modal portal. Its content closure executes immediately.
pub fn modal<'u, 'e, 's>(
    cx: &'u mut Ui<'e>,
    state: &'s mut crate::ModalState,
) -> crate::Modal<'u, 'e, 's> {
    cx.modal(state)
}

pub fn shader<'u, 'e>(
    cx: &'u mut Ui<'e>,
    asset: &crate::ShaderAsset,
) -> crate::Shader<'u, 'e, 'static> {
    cx.shader(asset)
}

pub fn slider<'u, 'e, 't>(
    cx: &'u mut Ui<'e>,
    label: impl Into<Cow<'t, str>>,
    value: &'t mut f64,
    range: std::ops::RangeInclusive<f64>,
) -> crate::Slider<'u, 'e, 't> {
    cx.slider(label, value, range)
}

/// Declare a trigger and a portable anchored popup.
pub fn popover<'u, 'e, 's>(
    cx: &'u mut Ui<'e>,
    state: &'s mut crate::PopoverState,
    label: impl Into<Cow<'s, str>>,
) -> crate::Popover<'u, 'e, 's> {
    cx.popover(state, label)
}

mod popup;
pub use popup::{MenuItem, Select, SelectOption, SelectResponse, menu, menu_item, select};

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
