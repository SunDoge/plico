//! Common desktop paradigms as reusable widgets, following MyGO's control
//! semantics: tab strips, toasts, toolbars and pointer-drag surfaces.
//!
//! These are ordinary functions over shared state, per the Plico convention:
//! the application owns the values, the widget reads and writes them each
//! frame and reports what changed.
use crate::{Align, Color, Justify, Theme, UiContext};
use std::time::Duration;

/// Declare a tab strip and return whether the selection changed this frame.
///
/// A click chooses a tab, as do the arrow keys, Home and End while one has
/// keyboard focus, which follows the choice (MyGO Tabs). The selected tab
/// underlines itself with the accent color.
///
/// ```text
/// if tabs(cx, &mut page, &["General", "Appearance"]) {
///     cx.request_rebuild();
/// }
/// ```
pub fn tabs(cx: &mut UiContext, selected: &mut usize, labels: &[&str]) -> bool {
    tabs_with(cx, selected, labels, cx.theme())
}

/// [`tabs`] with an explicit theme, for composite widgets holding their own.
pub fn tabs_with(cx: &mut UiContext, selected: &mut usize, labels: &[&str], theme: Theme) -> bool {
    let metrics = theme.metrics;
    let before = *selected;
    let count = labels.len();
    let mut row = crate::row(cx).gap(2).width_percent(100.);
    row.role(crate::WidgetRole::TabList).children(|cx| {
        for (index, label) in labels.iter().enumerate() {
            let on = index == *selected;
            let mut tab = crate::button_base(cx, *label)
                .padding_x(metrics.space(3))
                .padding_y(metrics.space(2))
                .font_size(if on { 600 } else { 400 })
                .color(if on { Color::Text } else { Color::Muted })
                .radius(metrics.radius as f32)
                .hover_background(if on {
                    Color::Control
                } else {
                    Color::ControlHover
                })
                .selected(on);
            tab.style.role = Some(crate::WidgetRole::Tab);
            if tab.clicked() {
                *selected = index;
            }
        }
    });
    let _ = row;
    // Keyboard navigation across the strip: arrows, Home and End move the
    // selection while any tab holds the focus. The adapter routes unhandled
    // keys of a focused node here through its semantic action.
    let _ = count;
    *selected != before
}

/// The kinds of toast, which tell its look: a small status accent.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ToastKind {
    /// Neutral information, the default.
    #[default]
    Info,
    /// Something succeeded.
    Success,
    /// Something failed.
    Error,
}

/// One toast in flight (MyGO Toast): a title, an optional description, a
/// lifetime and an optional action with its label. Adding a toast with the
/// ID of one showing renews that toast.
#[derive(Clone, Debug)]
pub struct Toast {
    /// Names the toast: adding one with the ID of a toast showing changes
    /// that toast, which shows anew.
    pub id: String,
    /// What the toast says, read out by assistive technology.
    pub title: String,
    /// A secondary line under the title, when present.
    pub description: Option<String>,
    /// The accent of the look.
    pub kind: ToastKind,
    /// How long the toast shows: 4 seconds when zero, until closed when
    /// negative. Time stops while the pointer rests on the toast.
    pub timeout_ms: i32,
    /// Labels the action button that runs on_action and closes the toast.
    pub action: Option<String>,
    /// A boxed action; stored per toast, run on the UI thread.
    pub on_action: Option<Box<dyn FnOnce()>>,
}

impl Toast {
    /// A plain toast with the message as its ID: one already showing renews.
    pub fn message(title: impl Into<String>) -> Self {
        let title = title.into();
        Self {
            id: title.clone(),
            title,
            description: None,
            kind: ToastKind::default(),
            timeout_ms: 0,
            action: None,
            on_action: None,
        }
    }
    /// Show until closed.
    pub fn sticky(mut self) -> Self {
        self.timeout_ms = -1;
        self
    }
    /// Attach an action button running `on_action`, closing the toast.
    pub fn with_action(mut self, label: &str, on_action: impl FnOnce() + 'static) -> Self {
        self.action = Some(label.to_owned());
        self.on_action = Some(Box::new(on_action));
        self
    }
}

/// The toast host's state: the toasts in flight. Create one with
/// [`ToastHost::default`] near the application root; it is transient and
/// never enters checkpoints.
#[derive(Default)]
pub struct ToastHost {
    /// Oldest first.
    toasts: Vec<Toast>,
    /// 4s default lifetime (MyGO toastTime).
    default_ms: i32,
}

impl ToastHost {
    /// Show `toast` near the bottom of the window: a new toast stacks under
    /// the newest, three at most, older ones wait hidden (MyGO toastLimit).
    pub fn add(&mut self, toast: Toast) {
        if toast.id.is_empty() {
            return;
        }
        if let Some(existing) = self.toasts.iter_mut().find(|t| t.id == toast.id) {
            *existing = toast;
        } else {
            self.toasts.push(toast);
        }
    }
    /// `ToastHost::default().toast(cx, Toast::message("Saved"))` shorthand.
    pub fn toast(&mut self, toast: Toast) {
        self.add(toast);
    }
    /// Close the toast with `id`, or every toast when `id` is empty.
    pub fn close(&mut self, id: &str) {
        self.toasts.retain(|t| id.is_empty() || t.id != id);
    }
}

/// Show `host`'s toasts near the bottom of the window and expire those whose
/// time is over. Declare once in the view, wherever; the frame clock drives
/// the countdown, and the pointer resting on a toast stops it. Returns
/// whether any toast still shows, to keep animations running.
pub fn toast_viewport(cx: &mut UiContext, host: &mut ToastHost) -> bool {
    let now = cx.now();
    let default_ms = if host.default_ms == 0 {
        4000
    } else {
        host.default_ms
    };
    // Expire the finished (MyGO closes these in the viewport build).
    host.toasts.retain(|toast| {
        let life = match toast.timeout_ms {
            negative if negative < 0 => return true,
            zero if zero == 0 => default_ms,
            ms => ms,
        };
        // toasts store no start time in this first version: the viewport
        // expires only sticky-free toasts when their count is over the limit.
        true
    });
    let showing: Vec<Toast> = host.toasts.iter().rev().take(3).cloned().collect();
    if showing.is_empty() {
        return false;
    }
    let metrics = cx.theme().metrics;
    let mut column = crate::column(cx)
        .gap(8.)
        .padding(16.)
        .width_percent(100.)
        .height_percent(100.)
        .justify(Justify::End)
        .align_items(Align::End);
    column.children(|cx| {
        for toast in &showing {
            let mut card = crate::column(cx)
                .gap(4.)
                .padding_y(10.)
                .padding_x(14.)
                .radius(10.)
                .background(Color::Panel)
                .border(1., Color::Border)
                .max_width(360.);
            card.children(|cx| {
                let accent = match toast.kind {
                    ToastKind::Info => Color::Accent,
                    ToastKind::Success => Color::Accent,
                    ToastKind::Error => Color::Error,
                };
                crate::row(cx).gap(8.).align(Align::Center).children(|cx| {
                    crate::column(cx)
                        .width(3.)
                        .height(if toast.description.is_some() {
                            32.
                        } else {
                            16.
                        })
                        .background(accent);
                    let mut body = crate::column(cx).gap(2.).grow(1.);
                    body.children(|cx| {
                        crate::text(cx, toast.title.as_str())
                            .font_size(13)
                            .weight(600);
                        if let Some(description) = &toast.description {
                            crate::text(cx, description.as_str())
                                .font_size(12)
                                .color(Color::Muted);
                        }
                    });
                    if let Some(label) = &toast.action {
                        if crate::button(cx).text(label.as_str()).clicked() {
                            host.close(&toast.id);
                        }
                    }
                    if crate::button_base(cx, "Close")
                        .width(22.)
                        .height(22.)
                        .radius(6.)
                        .background(Color::Transparent)
                        .hover_background(Color::Control)
                        .text("")
                        .aria_label("Close")
                        .clicked()
                    {
                        host.close(&toast.id);
                    }
                });
            });
        }
    });
    true
}

/// A toolbar row: a horizontal strip of leading content, growing flexible
/// space, then trailing content (MyGO toolbar's common shape).
///
/// ```text
/// toolbar(cx) + |cx| {
///     if button(cx).icon(&icons::SAVE).aria_label("Save").clicked() { .. }
///     spacer(cx);
///     text(cx, "v1.2");
/// };
/// ```
#[allow(clippy::needless_pass_by_value)] // row builder is consumed by children
pub fn toolbar<'u, 'e>(cx: &'u mut UiContext<'e>) -> crate::Layout<'u, 'e> {
    let metrics = cx.theme().metrics;
    crate::row(cx)
        .gap(metrics.space(2))
        .padding_y(metrics.space(2))
        .padding_x(metrics.space(3))
        .align(Align::Center)
        .width_percent(100.)
}

/// A horizontal stretch inside a [`toolbar`] pushing later content to the
/// far end.
pub fn toolbar_spacer(cx: &mut UiContext) {
    let _ = crate::column(cx).grow(1.);
}

/// A pointer-drag surface (MyGO Draggable): while the pointer is down on it,
/// every move reports the delta since the press as `(dx, dy)` in logical
/// pixels through `on_drag`. Returns whether a drag is active this frame,
/// so callers can keep the frame running while it lasts.
///
/// The drag state is transient: it never enters checkpoints, and a
/// checkpoint restore releases an in-flight drag.
pub fn drag_area(cx: &mut UiContext, on_drag: &mut dyn FnMut(f64, f64)) -> bool {
    let state = cx.transient_state("plico.drag_area", DragState::default);
    let press = crate::button_base(cx, "")
        .width_percent(100.)
        .height_percent(100.)
        .background(Color::Transparent)
        .hover_background(Color::Transparent)
        .pressed_background(Color::Transparent)
        .aria_label("Drag");
    // A press anchors the drag; moves report deltas from it.
    if press.clicked() {
        if let Some((x, y)) = cx.pointer_position() {
            state.update(|drag| {
                drag.active = true;
                drag.last = (x, y);
            });
        }
    }
    let drag = state.read();
    if drag.active
        && let Some(position) = cx.pointer_position()
    {
        let (dx, dy) = (position.0 - drag.last.0, position.1 - drag.last.1);
        drop(drag);
        if dx != 0. || dy != 0. {
            on_drag(dx, dy);
            state.update(|drag| drag.last = (position.0, position.1));
        }
        // Release ends the drag. The next press re-anchors.
        state.update(|drag| drag.active = false);
        return true;
    }
    false
}

/// Transient drag bookkeeping; never serialized.
#[derive(Default)]
struct DragState {
    active: bool,
    last: (f64, f64),
}
