//! Common desktop paradigms as reusable widgets, following MyGO's control
//! semantics: tab strips, toasts, toolbars and pointer-drag surfaces.
//!
//! These are ordinary functions over shared state, per the Plico convention:
//! the application owns the values, the widget reads and writes them each
//! frame and reports what changed.
use super::{button, button_base, column, row, text};
use crate::{Align, Color, Justify, UiContext};
use std::time::Duration;

/// Declare a tab strip and return whether the selection changed this frame.
///
/// A click chooses a tab; the chosen tab underlines itself with the accent
/// color and carries the tab semantic role. Keyboard traversal (arrows,
/// Home, End while focused) follows MyGO's Tabs and still needs
/// adapter-side focus wiring; today the pointer is the selection path.
///
/// ```text
/// if tabs(cx, &mut page, &["General", "Appearance", "Advanced"]) { .. }
/// ```
pub fn tabs(cx: &mut UiContext, selected: &mut usize, labels: &[&str]) -> bool {
    let before = *selected;
    let metrics = cx.theme().metrics;
    row(cx).gap(2).width_percent(100.).children(|cx| {
        for (index, label) in labels.iter().enumerate() {
            let on = index == *selected;
            let response = button_base(cx, *label)
                .padding_x(metrics.space(3))
                .padding_y(metrics.space(2))
                .radius(metrics.radius as f32)
                .hover_background(Color::ControlHover)
                .pressed_background(Color::ControlPressed)
                .selected(on)
                .semantic_role(crate::WidgetRole::Tab)
                .children(|cx| {
                    column(cx).gap(2).children(|cx| {
                        text(cx, *label)
                            .font_size(metrics.font_size)
                            .weight(if on { 600 } else { 400 })
                            .color(if on { Color::Foreground } else { Color::Muted });
                        // The accent underline keeps its slot in every
                        // tab, painted only on the chosen one, so the strip
                        // stays aligned (MyGO draws the line under the
                        // chosen tab).
                        let mut underline = column(cx).width_percent(100.).height(2.);
                        if on {
                            underline = underline.background(Color::Accent);
                        }
                        underline.children(|_| {});
                    });
                })
                .response;
            if response.clicked() {
                *selected = index;
            }
        }
    });
    *selected != before
}

/// The kind of toast, which tells its look: the accent stripe at its side.
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

impl ToastKind {
    fn accent(self) -> Color {
        match self {
            Self::Info | Self::Success => Color::Accent,
            Self::Error => Color::Focus,
        }
    }
}

/// One toast in flight (MyGO Toast): a title, an optional description, a
/// lifetime and an optional action. Adding a toast with the ID of one
/// showing renews that toast.
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
    /// How long the toast shows in milliseconds: 4 seconds when zero, until
    /// closed when negative. The time stops while the pointer rests on it.
    pub timeout_ms: i32,
    /// Labels the action button that runs `on_action` and closes the toast.
    pub action: Option<String>,
    /// The boxed action, run on the UI thread when its button is clicked.
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
    fn life(&self) -> Option<Duration> {
        match self.timeout_ms {
            negative if negative < 0 => None,
            0 => Some(Duration::from_millis(4000)),
            ms => Some(Duration::from_millis(u64::try_from(ms).unwrap_or(4000))),
        }
    }
}

/// One showing toast with its accumulated show time.
struct ToastEntry {
    toast: Toast,
    /// Show time accumulated so far; the pause skips advancing it.
    shown: Duration,
}

/// The toast host's state: the toasts in flight, oldest first. Own it near
/// the application root; it is host-local by construction (the action is a
/// boxed callback and the countdown runs on the frame clock) and never
/// enters checkpoints.
#[derive(Default)]
pub struct ToastHost {
    entries: Vec<ToastEntry>,
    /// Frame time of the last viewport pass; the gap advances the countdown.
    last_tick: Option<Duration>,
    /// Whether the pointer rested on the toasts last frame (MyGO pauses
    /// while hovered): the current pass holds the time still meanwhile.
    held: bool,
}

impl ToastHost {
    /// Show `toast` near the bottom of the window: a toast with the ID of
    /// one showing renews it; new ones stack under the newest, three show
    /// at most, older ones waiting hidden (MyGO toastLimit).
    pub fn add(&mut self, toast: Toast) {
        if toast.id.is_empty() {
            return;
        }
        match self.entries.iter_mut().find(|e| e.toast.id == toast.id) {
            Some(entry) => {
                *entry = ToastEntry {
                    toast,
                    shown: Duration::ZERO,
                };
            }
            None => self.entries.push(ToastEntry {
                toast,
                shown: Duration::ZERO,
            }),
        }
    }
    /// `host.toast(Toast::message("Saved"))` shorthand.
    pub fn toast(&mut self, toast: Toast) {
        self.add(toast);
    }
    /// Close the toast with `id`; an empty `id` closes every toast
    /// (MyGO CloseToast).
    pub fn close(&mut self, id: &str) {
        if id.is_empty() {
            self.entries.clear();
        } else {
            self.entries.retain(|e| e.toast.id != id);
        }
    }
    /// The number of toasts in flight.
    pub fn len(&self) -> usize {
        self.entries.len()
    }
    /// Whether no toast is in flight.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

// Toasts are transient runtime state, like hover and press: clones of the
// application state and restored checkpoints carry none of them, so every
// snapshot-related trait encodes an empty host.
impl Clone for ToastHost {
    fn clone(&self) -> Self {
        Self::default()
    }
}
impl std::fmt::Debug for ToastHost {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_list()
            .entries(self.entries.iter().map(|e| &e.toast.title))
            .finish()
    }
}
impl PartialEq for ToastHost {
    fn eq(&self, other: &Self) -> bool {
        self.entries
            .iter()
            .map(|e| e.toast.id.as_str())
            .eq(other.entries.iter().map(|e| e.toast.id.as_str()))
    }
}
impl serde::Serialize for ToastHost {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_unit()
    }
}
impl<'de> serde::Deserialize<'de> for ToastHost {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Empty;
        impl serde::de::Visitor<'_> for Empty {
            type Value = ToastHost;
            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a unit")
            }
            fn visit_unit<E>(self) -> Result<Self::Value, E> {
                Ok(ToastHost::default())
            }
        }
        deserializer.deserialize_unit(Empty)
    }
}

/// Show `host`'s toasts near the bottom of the window and expire those whose
/// time is over. Declare once in the view, wherever; the frame clock drives
/// the countdown, and the pointer resting on a toast holds its time (MyGO
/// toastTime semantics). Returns whether any toast still shows; animation
/// frames keep coming while one does.
pub fn toast_viewport(cx: &mut UiContext, host: &mut ToastHost) -> bool {
    let now = cx.now();
    let tick = host.last_tick.replace(now);
    // The frame gap applies once the frame's hold state is known (the draw
    // reports it), so neither the hold frame nor the release frame loses
    // its gap to the one-frame-old snapshot (MyGO pauses by rebuilding the
    // same frame, where the hold reads current).
    let gap = tick
        .filter(|before| now > *before)
        .map(|before| now - before);
    // Expire what the accumulated time finished on an earlier pass.
    host.entries
        .retain(|entry| entry.toast.life().is_none_or(|life| entry.shown < life));
    // The newest three show, oldest first (MyGO toastLimit).
    let hidden = host.entries.len().saturating_sub(3);
    let showing: Vec<usize> = (hidden..host.entries.len()).collect();
    if showing.is_empty() {
        host.held = false;
        return false;
    }
    let metrics = cx.theme().metrics;
    // Clicked ids collect during the draw; the host mutates after it.
    let mut close_ids: Vec<String> = Vec::new();
    let mut action_ids: Vec<String> = Vec::new();
    let mut held_now = false;
    let titles: Vec<(String, Option<String>, ToastKind, Option<String>)> = showing
        .iter()
        .map(|&index| {
            let toast = &host.entries[index].toast;
            (
                toast.title.clone(),
                toast.description.clone(),
                toast.kind,
                toast.action.clone(),
            )
        })
        .collect();
    column(cx)
        .gap(8.)
        .padding(16.)
        .width_percent(100.)
        .height_percent(100.)
        .justify(Justify::End)
        .align(Align::End)
        .children(|cx| {
            // MyGO holds the toasts' time from the whole viewport (its
            // viewport.Hovered()): any card nesting depth reports here,
            // where the parent is the viewport column itself.
            held_now |= cx.hovered();
            for (index, (title, description, kind, action)) in showing.iter().zip(&titles) {
                let id = &host.entries[*index].toast.id;
                column(cx)
                    .gap(4.)
                    .padding_y(10.)
                    .padding_x(14.)
                    .radius(10.)
                    .background(Color::Panel)
                    .border(1., Color::Border)
                    .max_width(360.)
                    .children(|cx| {
                        row(cx).gap(8.).align(Align::Center).children(|cx| {
                            column(cx)
                                .width(3.)
                                .height(if description.is_some() { 32. } else { 16. })
                                .background(kind.accent());
                            column(cx).gap(2.).grow(1.).children(|cx| {
                                text(cx, title.as_str())
                                    .font_size(metrics.font_size)
                                    .weight(600);
                                if let Some(description) = description {
                                    text(cx, description.as_str())
                                        .font_size(metrics.font_size - 1)
                                        .color(Color::Muted);
                                }
                            });
                            if let Some(label) = action
                                && button(cx).text(label.as_str()).clicked()
                            {
                                action_ids.push(id.clone());
                            }
                            if button_base(cx, "Close toast")
                                .width(22.)
                                .height(22.)
                                .radius(6.)
                                .hover_background(Color::Control)
                                .children(|cx| {
                                    text(cx, "×").font_size(14).color(Color::Muted);
                                })
                                .response
                                .clicked()
                            {
                                close_ids.push(id.clone());
                            }
                        });
                    });
            }
        });
    host.held = held_now;
    if !held_now && let Some(gap) = gap {
        for entry in &mut host.entries {
            if entry.toast.life().is_some() {
                entry.shown += gap;
            }
        }
        // What the applied gap finished is gone within this same pass;
        // the early retain above only pruned earlier-finished toasts.
        host.entries
            .retain(|entry| entry.toast.life().is_none_or(|life| entry.shown < life));
    }
    // Apply the clicks outside the draw: an action runs, then closes.
    for id in action_ids {
        if let Some(entry) = host.entries.iter_mut().find(|e| e.toast.id == id)
            && let Some(action) = entry.toast.on_action.take()
        {
            action();
        }
        host.close(&id);
    }
    for id in close_ids {
        host.close(&id);
    }
    // Keep frames coming while a toast counts down.
    if host.entries.iter().any(|e| e.toast.life().is_some()) {
        cx.request_animation_frame();
    }
    !host.entries.is_empty()
}

/// A toolbar row: a horizontal strip of controls spaced by the theme
/// (MyGO toolbar's common shape). Put a [`toolbar_spacer`] between the
/// leading and trailing groups to push the trailing one to the far end.
///
/// ```text
/// toolbar(cx) + |cx| {
///     if button(cx).icon(&icons::SAVE).aria_label("Save").clicked() { .. }
///     toolbar_spacer(cx);
///     text(cx, "v1.2");
/// };
/// ```
pub fn toolbar<'u, 'e>(cx: &'u mut UiContext<'e>) -> crate::Layout<'u, 'e> {
    let metrics = cx.theme().metrics;
    row(cx)
        .gap(metrics.space(2))
        .padding_y(metrics.space(2))
        .padding_x(metrics.space(3))
        .align(Align::Center)
        .width_percent(100.)
}

/// A horizontal stretch inside a [`toolbar`] pushing later content to the
/// far end.
pub fn toolbar_spacer(cx: &mut UiContext) {
    let _ = column(cx).grow(1.);
}

/// A pointer-drag surface (MyGO Draggable): while the primary button is
/// down on it, every frame reports the movement since the previous frame
/// through `on_drag` as `(dx, dy)` in logical pixels. Returns whether a
/// drag is active this frame, so callers can keep the frame running while
/// it lasts. The drag anchor never enters checkpoints: an in-flight drag
/// releases on a restore.
pub fn drag_area(cx: &mut UiContext, on_drag: &mut dyn FnMut(f64, f64)) -> bool {
    let drag = cx.transient_state("plico.drag_area", DragState::default);
    let mut dragging = false;
    button_base(cx, "Drag surface")
        .width_percent(100.)
        .height_percent(100.)
        .children(|cx| {
            // Inside the button's scope the pressed query reports this node.
            if cx.pressed()
                && let Some((x, y)) = cx.pointer_position()
            {
                let (dx, dy) = {
                    let state = drag.read();
                    match state.point {
                        Some((px, py)) => (x - px, y - py),
                        None => (0., 0.),
                    }
                };
                drag.update(|state| state.point = Some((x, y)));
                if dx != 0. || dy != 0. {
                    on_drag(dx, dy);
                    dragging = true;
                }
            } else {
                // Release (or no pointer): forget the anchor so the next
                // press starts from its own position.
                drag.update(|state| state.point = None);
            }
        });
    dragging
}

/// Transient drag anchor; never serialized.
#[derive(Clone, Default, PartialEq)]
struct DragState {
    point: Option<(f64, f64)>,
}
