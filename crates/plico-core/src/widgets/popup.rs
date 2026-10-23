//! Immediate command menus and controlled selection on the shared portal layer.
use crate::{Key, WidgetRole, prelude::*};
use std::borrow::Cow;

pub fn menu<'u, 'e, 's>(
    cx: &'u mut Ui<'e>,
    state: &'s mut PopoverState,
    label: impl Into<Cow<'s, str>>,
) -> crate::Popover<'u, 'e, 's> {
    cx.popover(state, label)
        .role(WidgetRole::Menu)
        .max_height(320.)
}
/// Activating a command also requests closure of its containing popover.
pub struct MenuItem<'u, 'e, 's> {
    cx: Option<&'u mut Ui<'e>>,
    label: Cow<'s, str>,
    key: Key,
    disabled: bool,
    icon: Option<&'s crate::SvgAsset>,
}
pub fn menu_item<'u, 'e, 's>(
    cx: &'u mut Ui<'e>,
    label: impl Into<Cow<'s, str>>,
) -> MenuItem<'u, 'e, 's> {
    let key = cx.engine.auto_key(cx.parent);
    MenuItem {
        cx: Some(cx),
        label: label.into(),
        key,
        disabled: false,
        icon: None,
    }
}
impl<'s> MenuItem<'_, '_, 's> {
    #[doc(hidden)]
    pub fn __implicit_key(mut self, site: u64) -> Self {
        if matches!(self.key, Key::Auto(_)) {
            self.key = Key::Generated(site)
        }
        self
    }
    pub fn key(mut self, key: impl Into<Key>) -> Self {
        self.key = key.into();
        self
    }
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
    pub fn icon(mut self, icon: &'s crate::SvgAsset) -> Self {
        self.icon = Some(icon);
        self
    }
    pub fn response(mut self) -> Response {
        self.finish()
    }
    pub fn clicked(self) -> bool {
        self.response().clicked()
    }
    fn finish(&mut self) -> Response {
        let cx = self.cx.take().expect("one declaration");
        let m = cx.theme().metrics;
        let disabled = self.disabled || cx.inherited.disabled;
        let response = button_base(cx, self.label.as_ref())
            .key(self.key)
            .semantic_role(WidgetRole::MenuItem)
            .disabled(disabled)
            .opacity(if disabled { 0.45 } else { 1. })
            .width_percent(100.)
            .min_height(m.control_height as f32)
            .padding_x(m.space(2))
            .padding_y(m.space(1))
            .gap(m.space(2))
            .align(Align::Center)
            .radius(m.radius as f32)
            .background(Color::Panel)
            .hover_background(Color::ControlHover)
            .children(|cx| {
                if let Some(icon) = self.icon {
                    svg(cx, icon).size(16., 16.);
                }
                text(cx, self.label.as_ref()).grow(1.);
            })
            .response;
        if response.clicked() {
            cx.close_popover();
        }
        response
    }
}
impl Drop for MenuItem<'_, '_, '_> {
    fn drop(&mut self) {
        if self.cx.is_some() {
            self.finish();
        }
    }
}

/// Business-keyed option. Labels can borrow application data without allocation.
pub struct SelectOption<'a, T> {
    key: Key,
    value: T,
    label: &'a str,
    disabled: bool,
}
impl<'a, T> SelectOption<'a, T> {
    pub fn new(key: impl Into<Key>, value: T, label: &'a str) -> Self {
        Self {
            key: key.into(),
            value,
            label,
            disabled: false,
        }
    }
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}
#[derive(Default, Debug, Clone, Copy, PartialEq, Eq)]
pub struct SelectResponse {
    pub changed: bool,
}
/// Controlled value and option borrows last only until the declaration is consumed.
pub struct Select<'u, 'e, 's, T: Clone + PartialEq> {
    cx: Option<&'u mut Ui<'e>>,
    state: &'s mut PopoverState,
    label: &'s str,
    selected: &'s mut T,
    options: &'s [SelectOption<'s, T>],
    key: Key,
    width: f32,
    max_height: f32,
    disabled: bool,
    motion: Motion,
}
pub fn select<'u, 'e, 's, T: Clone + PartialEq>(
    cx: &'u mut Ui<'e>,
    state: &'s mut PopoverState,
    label: &'s str,
    selected: &'s mut T,
    options: &'s [SelectOption<'s, T>],
) -> Select<'u, 'e, 's, T> {
    let key = cx.engine.auto_key(cx.parent);
    Select {
        cx: Some(cx),
        state,
        label,
        selected,
        options,
        key,
        width: 240.,
        max_height: 280.,
        disabled: false,
        motion: Motion::Quick,
    }
}
impl<T: Clone + PartialEq> Select<'_, '_, '_, T> {
    #[doc(hidden)]
    pub fn __implicit_key(mut self, site: u64) -> Self {
        if matches!(self.key, Key::Auto(_)) {
            self.key = Key::Generated(site)
        }
        self
    }
    pub fn key(mut self, key: impl Into<Key>) -> Self {
        self.key = key.into();
        self
    }
    pub fn width(mut self, width: f32) -> Self {
        self.width = width;
        self
    }
    pub fn max_height(mut self, height: f32) -> Self {
        self.max_height = height;
        self
    }
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
    pub fn motion(mut self, motion: Motion) -> Self {
        self.motion = motion;
        self
    }
    pub fn response(mut self) -> SelectResponse {
        self.finish()
    }
    fn finish(&mut self) -> SelectResponse {
        let cx = self.cx.take().expect("one declaration");
        let state = &mut *self.state;
        let selected = &mut *self.selected;
        let (label, options, key, width, disabled, motion) = (
            self.label,
            self.options,
            self.key,
            self.width,
            self.disabled,
            self.motion,
        );
        // Preserve an unknown value, rather than silently selecting the first item.
        let shown = options
            .iter()
            .find(|option| option.value == *selected)
            .map_or(label, |option| option.label);
        let max_height = self.max_height;
        let mut changed = false;
        cx.popover(state, label)
            .display_text(shown)
            .key(key)
            .width(width)
            .max_height(max_height)
            .disabled(disabled || options.is_empty())
            .motion(motion)
            .role(WidgetRole::ListBox)
            .children(|cx| {
                for option in options {
                    let on = option.value == *selected;
                    let m = cx.theme().metrics;
                    let response = button_base(cx, option.label)
                        .key(option.key)
                        .semantic_role(WidgetRole::Option)
                        .selected(on)
                        .disabled(option.disabled)
                        .opacity(if option.disabled { 0.45 } else { 1. })
                        .width_percent(100.)
                        .min_height(m.control_height as f32)
                        .padding_x(m.space(2))
                        .padding_y(m.space(1))
                        .gap(m.space(2))
                        .align(Align::Center)
                        .radius(m.radius as f32)
                        .background(if on { Color::Selection } else { Color::Panel })
                        .children(|cx| {
                            text(cx, option.label).grow(1.);
                            if on {
                                svg(cx, &crate::icons::CHECK).size(16., 16.);
                            } else {
                                let _ = column(cx).width(16.).height(16.).children(|_cx| {});
                            }
                        })
                        .response;
                    if response.clicked() {
                        changed |= !on;
                        *selected = option.value.clone();
                        cx.close_popover();
                    }
                }
            });
        SelectResponse { changed }
    }
}

impl<T: Clone + PartialEq> Drop for Select<'_, '_, '_, T> {
    fn drop(&mut self) {
        if self.cx.is_some() {
            self.finish();
        }
    }
}
