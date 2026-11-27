//! Immediate command menus and controlled selection on the shared portal layer.
use crate::{Key, WidgetRole, prelude::*};
use std::borrow::Cow;

/// Menu commands and select options compose the same base row. Only semantic
/// role, chosen face and MyGO's horizontal inset differ.
fn popup_entry<'u, 'e, 't>(
    cx: &'u mut UiContext<'e>,
    key: Key,
    label: impl Into<Cow<'t, str>>,
    selected: Option<bool>,
) -> (crate::ButtonBase<'u, 'e, 't>, bool, bool) {
    let m = cx.theme().metrics;
    let highlighted = cx.engine.focus_within(crate::model::Identity {
        parent: cx.parent,
        key,
        scope: cx.scope,
    });
    let mut parent = cx.parent;
    let mut glass = false;
    while let Some(node) = cx.engine.nodes.get(parent) {
        if node.kind == crate::Kind::Overlay {
            glass = node.style.liquid_popup;
            break;
        }
        if parent == cx.engine.root {
            break;
        }
        parent = node.identity.parent;
    }
    let control = button_base(cx, label)
        .key(key)
        .semantic_role(if selected.is_some() {
            WidgetRole::Option
        } else {
            WidgetRole::MenuItem
        })
        .width_percent(100.)
        .min_height(m.control_height as f32)
        .padding_x(m.spacing as f32 * if selected.is_some() { 2.5 } else { 2. })
        .padding_y(m.control_padding_y())
        .gap(m.space(2))
        .align(Align::Center)
        .radius(m.radius as f32)
        .hover_background(Color::Accent)
        .pressed_background(Color::Accent);
    let highlight = Color::ControlHover;
    let control = if glass {
        let item = control
            .height(40.)
            .min_height(40.)
            .padding_x(14.)
            .padding_y(0.)
            .gap(10)
            .radius(16.)
            .corner_smoothing(0.)
            .font_size(15)
            .weight(if selected == Some(true) { 600 } else { 500 })
            .hover_background(highlight)
            .pressed_background(highlight);
        if highlighted {
            item.background(highlight)
        } else {
            item
        }
    } else if highlighted {
        control.background(Color::Accent)
    } else if selected == Some(true) {
        control.background(Color::Control)
    } else {
        control
    };
    let control = if let Some(on) = selected {
        control.selected(on)
    } else {
        control
    };
    (control, highlighted, glass)
}

pub fn menu<'u, 'e, 's>(
    cx: &'u mut UiContext<'e>,
    state: &'s mut PopoverState,
    label: impl Into<Cow<'s, str>>,
) -> crate::Popover<'u, 'e, 's> {
    cx.popover(state, label)
        .role(WidgetRole::Menu)
        .max_height(320.)
}
/// Activating a command also requests closure of its containing popover.
pub struct MenuItem<'u, 'e, 's> {
    cx: Option<&'u mut UiContext<'e>>,
    label: Cow<'s, str>,
    key: Key,
    disabled: bool,
    icon: Option<&'s crate::SvgAsset>,
    destructive: bool,
}
pub fn menu_item<'u, 'e, 's>(
    cx: &'u mut UiContext<'e>,
    label: impl Into<Cow<'s, str>>,
) -> MenuItem<'u, 'e, 's> {
    let key = cx.engine.auto_key(cx.parent);
    MenuItem {
        cx: Some(cx),
        label: label.into(),
        key,
        disabled: false,
        icon: None,
        destructive: false,
    }
}
impl<'s> crate::ImplicitKey for MenuItem<'_, '_, 's> {
    fn key_of(&self) -> Key {
        self.key
    }
    fn set_key(mut self, key: Key) -> Self {
        self.key = key;
        self
    }
}
impl<'s> MenuItem<'_, '_, 's> {
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
    pub fn destructive(mut self, enabled: bool) -> Self {
        self.destructive = enabled;
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
        let disabled = self.disabled || cx.inherited.disabled;
        let (control, highlighted, glass) = popup_entry(cx, self.key, self.label.as_ref(), None);
        let response = control
            .disabled(disabled)
            .opacity(if disabled { 0.45 } else { 1. })
            .children(|cx| {
                let color = if !glass && (highlighted || cx.hovered()) {
                    Color::OnAccent
                } else if self.destructive {
                    Color::Danger
                } else {
                    Color::Foreground
                };
                if let Some(icon) = self.icon {
                    crate::widgets::icon(cx, icon).color(color);
                }
                text(cx, self.label.as_ref())
                    .color(color)
                    .grow(1.)
                    .ellipsis(true);
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
/// MyGO combobox.matching keeps equal-rank entries in source order.
fn visit_matches<'a, T>(
    options: &'a [SelectOption<'_, T>],
    query: &str,
    mut visit: impl FnMut(&'a SelectOption<'a, T>),
) -> bool {
    let query = query.trim();
    if query.is_empty() {
        options.iter().for_each(visit);
        return !options.is_empty();
    }
    let query = query.to_lowercase();
    let mut contains = Vec::new();
    let mut found = false;
    for option in options {
        let label = option.label;
        let position = if label.is_ascii() {
            label
                .as_bytes()
                .windows(query.len())
                .position(|part| part.eq_ignore_ascii_case(query.as_bytes()))
        } else {
            label.to_lowercase().find(&query)
        };
        match position {
            Some(0) => {
                visit(option);
                found = true;
            }
            Some(_) => {
                contains.push(option);
                found = true;
            }
            None => {}
        }
    }
    contains.into_iter().for_each(visit);
    found
}

#[derive(Default, Debug, Clone, Copy, PartialEq, Eq)]
pub struct SelectResponse {
    changed: bool,
}
impl SelectResponse {
    pub const fn changed(self) -> bool {
        self.changed
    }
}
/// Controlled value and option borrows last only until the declaration is consumed.
pub struct Select<'u, 'e, 's, T: Clone + PartialEq> {
    cx: Option<&'u mut UiContext<'e>>,
    state: &'s mut PopoverState,
    label: &'s str,
    selected: &'s mut T,
    options: &'s [SelectOption<'s, T>],
    key: Key,
    width: f32,
    max_height: f32,
    disabled: bool,
    motion: Motion,
    material: Option<crate::LiquidGlass>,
    placeholder: Option<&'s str>,
    search: Option<&'s crate::TextBuffer>,
}
pub fn select<'u, 'e, 's, T: Clone + PartialEq>(
    cx: &'u mut UiContext<'e>,
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
        placeholder: None,
        search: None,
        material: None,
    }
}
impl<T: Clone + PartialEq> crate::ImplicitKey for Select<'_, '_, '_, T> {
    fn key_of(&self) -> Key {
        self.key
    }
    fn set_key(mut self, key: Key) -> Self {
        self.key = key;
        self
    }
}
impl<'s, T: Clone + PartialEq> Select<'_, '_, 's, T> {
    /// Text shown while the controlled value has no matching option.
    pub fn placeholder(mut self, text: &'s str) -> Self {
        self.placeholder = Some(text);
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
    /// Add a filter field inside the listbox. The caller retains the query.
    /// Case-insensitive prefix matches appear before other substring matches.
    /// Within each group, options keep their source order and business keys.
    pub fn searchable(mut self, query: &'s crate::TextBuffer) -> Self {
        self.search = Some(query);
        self
    }
    pub fn material(mut self, glass: crate::LiquidGlass) -> Self {
        self.material = Some(glass);
        self
    }
    pub fn response(mut self) -> SelectResponse {
        self.finish()
    }
    pub fn changed(self) -> bool {
        self.response().changed()
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
            .map_or(self.placeholder.unwrap_or(label), |option| option.label);
        let max_height = self.max_height;
        let search = self.search;
        let mut changed = false;
        let popup = cx.popover(state, label);
        let popup = if let Some(glass) = self.material {
            popup.material(glass)
        } else {
            popup
        };
        popup
            .display_text(shown)
            .key(key)
            .width(width)
            .max_height(max_height)
            .disabled(disabled || options.is_empty())
            .motion(motion)
            .role(WidgetRole::ListBox)
            .children(|cx| {
                let mut submit = search.is_some_and(|query| {
                    let label = cx.tr("select-search", "Search options");
                    let placeholder = cx.tr("select-search-placeholder", "Search…");
                    text_input(cx, label, query)
                        .placeholder(placeholder)
                        .width_percent(100.)
                        .autofocus(true)
                        .navigation(true)
                        .response()
                        .submitted()
                });
                let query = search.and_then(|buffer| buffer.text()).unwrap_or_default();
                let mut ancestor = cx.parent;
                let mut animate_rows = false;
                while let Some(node) = cx.engine.nodes.get(ancestor) {
                    if node.kind == crate::Kind::Overlay {
                        animate_rows = node.style.popup_motion;
                        break;
                    }
                    if ancestor == cx.engine.root {
                        break;
                    }
                    ancestor = node.identity.parent;
                }
                let found = visit_matches(options, &query, |option| {
                    let on = option.value == *selected;
                    let (control, highlighted, glass) =
                        popup_entry(cx, option.key, option.label, Some(on));
                    let response = control
                        .disabled(option.disabled)
                        .opacity(if option.disabled {
                            if glass { 0.4 } else { 0.45 }
                        } else {
                            1.
                        })
                        .motion(if animate_rows { Motion::None } else { motion })
                        .children(|cx| {
                            let color = if !glass && (highlighted || cx.hovered()) {
                                Color::OnAccent
                            } else {
                                Color::Foreground
                            };
                            text(cx, option.label).color(color).grow(1.).ellipsis(true);
                            let side = cx.theme().metrics.control_icon() as f32;
                            if glass {
                                return;
                            }
                            if on {
                                svg(cx, &crate::icons::CHECK).size(side, side).color(color);
                            } else {
                                let _ = column(cx)
                                    .width(side)
                                    .height(side)
                                    .shrink(0.)
                                    .children(|_cx| {});
                            }
                        })
                        .response;
                    if response.clicked() || (!option.disabled && std::mem::take(&mut submit)) {
                        changed |= !on;
                        *selected = option.value.clone();
                        cx.close_popover();
                    }
                });
                if !found {
                    let label = cx.tr("select-no-results", "No matching options");
                    text(cx, label).color(Color::Muted);
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
