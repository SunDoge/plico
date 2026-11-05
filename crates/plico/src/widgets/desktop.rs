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
    counted_at: Option<Duration>,
    key: u64,
    motion: Option<ToastMotion>,
}

// A small component-local presence tween, sampled only from the injected clock.
// Retargeting starts at the displayed value, so closing/reopening never jumps.
struct ToastMotion {
    from: f32,
    to: f32,
    start: Option<Duration>,
    duration: Duration,
}
impl ToastMotion {
    fn value(&self, now: Duration) -> f32 {
        let t = if self.duration.is_zero() {
            1.
        } else {
            (now.saturating_sub(self.start.unwrap_or(now)).as_secs_f32()
                / self.duration.as_secs_f32())
            .min(1.)
        };
        self.from + (self.to - self.from) * (1. - (1. - t).powi(3))
    }
    fn active(&self, now: Duration) -> bool {
        self.from != self.to
            && !self.duration.is_zero()
            && self
                .start
                .is_none_or(|start| now.saturating_sub(start) < self.duration)
    }
}

/// The toast host's state: the toasts in flight, oldest first. Own it near
/// the application root; it is host-local by construction (the action is a
/// boxed callback and the countdown runs on the frame clock) and never
/// enters checkpoints.
pub struct ToastHost {
    entries: Vec<ToastEntry>,
    /// Frame time of the last viewport pass; the gap advances the countdown.
    last_tick: Option<Duration>,
    /// Whether the pointer rested on the toasts last frame (MyGO pauses
    /// while hovered): the current pass holds the time still meanwhile.
    held: bool,
    exiting: Vec<ToastEntry>,
    next_key: u64,
    motion_duration: Duration,
}
impl Default for ToastHost {
    fn default() -> Self {
        Self {
            entries: Vec::new(),
            exiting: Vec::new(),
            last_tick: None,
            held: false,
            next_key: 0,
            motion_duration: Duration::from_millis(180),
        }
    }
}

impl ToastHost {
    /// Show `toast` near the bottom of the window: a toast with the ID of
    /// one showing renews it; new ones stack under the newest, three show
    /// at most, older ones waiting hidden (MyGO toastLimit).
    pub fn add(&mut self, toast: Toast) {
        if toast.id.is_empty() {
            return;
        }
        if let Some(entry) = self
            .entries
            .iter_mut()
            .find(|entry| entry.toast.id == toast.id)
        {
            entry.toast = toast;
            entry.shown = Duration::ZERO;
            entry.counted_at = None;
            return;
        }
        if let Some(index) = self
            .exiting
            .iter()
            .position(|entry| entry.toast.id == toast.id)
        {
            let mut entry = self.exiting.remove(index);
            let now = self.last_tick.unwrap_or_default();
            let from = entry.motion.as_ref().map_or(0., |motion| motion.value(now));
            entry.motion = Some(ToastMotion {
                from,
                to: 1.,
                start: None,
                duration: self.motion_duration,
            });
            entry.toast = toast;
            entry.shown = Duration::ZERO;
            entry.counted_at = None;
            self.entries.push(entry);
            return;
        }
        let key = self.next_key;
        self.next_key = self
            .next_key
            .checked_add(1)
            .expect("toast identity exhausted");
        self.entries.push(ToastEntry {
            toast,
            shown: Duration::ZERO,
            counted_at: None,
            key,
            motion: None,
        });
    }
    /// `host.toast(Toast::message("Saved"))` shorthand.
    pub fn toast(&mut self, toast: Toast) {
        self.add(toast);
    }
    /// Close the toast with `id`; an empty `id` closes every toast
    /// (MyGO CloseToast).
    pub fn close(&mut self, id: &str) {
        self.close_at(id, None);
    }
    fn close_at(&mut self, id: &str, start: Option<Duration>) {
        let now = start.or(self.last_tick).unwrap_or_default();
        let mut index = 0;
        while index < self.entries.len() {
            if !id.is_empty() && self.entries[index].toast.id != id {
                index += 1;
                continue;
            }
            let mut entry = self.entries.remove(index);
            // A queued toast that has never appeared has no pixels to retire.
            if let Some(motion) = &entry.motion {
                let from = motion.value(now);
                entry.motion = Some(ToastMotion {
                    from,
                    to: 0.,
                    start,
                    duration: self.motion_duration,
                });
                entry.toast.on_action = None;
                self.exiting.push(entry);
            }
        }
    }
    /// The number of active toasts, excluding retained exit animations.
    pub fn len(&self) -> usize {
        self.entries.len()
    }
    /// Whether there are no active toasts; exit animations may still be mounted.
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
/// frames keep coming while a lifetime or presence animation is active.
/// Closed toasts stop accepting input immediately; their pixels remain until
/// fade/slide-out completes. Theme `interaction_ms = 0` disables the tween.
pub fn toast_viewport(cx: &mut UiContext, host: &mut ToastHost) -> bool {
    // Like modals, a toast viewport is a window portal, not a percentage-height
    // child of whichever (possibly auto-sized or clipped) page called it.
    let key = cx.engine.auto_key(cx.parent);
    let mut hash = crate::macro_support::StableHasher::new();
    hash.write(b"plico.toast.viewport");
    hash.write_u64(cx.scope);
    hash.write_u64(crate::macro_support::key_seed(key));
    let mut ancestor = cx.parent;
    while ancestor != cx.engine.root {
        let identity = cx.engine.nodes.get(ancestor).unwrap().identity;
        hash.write_u64(crate::macro_support::key_seed(identity.key));
        hash.write_u64(identity.scope);
        ancestor = identity.parent;
    }
    let mut portal = UiContext {
        parent: cx.engine.root,
        scope: hash.finish(),
        inherited: cx.inherited,
        localizer: cx.localizer,
        in_button: false,
        engine: &mut *cx.engine,
    };
    let cx = &mut portal;
    let now = cx.now();
    host.last_tick = Some(now);
    let metrics = cx.theme().metrics;
    host.motion_duration = Duration::from_millis(u64::from(metrics.interaction_ms));
    for entry in host.entries.iter_mut().chain(&mut host.exiting) {
        if let Some(motion) = &mut entry.motion {
            motion.start.get_or_insert(now);
            if host.motion_duration.is_zero() {
                motion.duration = Duration::ZERO;
            }
        }
    }
    host.exiting.retain(|entry| {
        entry
            .motion
            .as_ref()
            .is_some_and(|motion| motion.active(now))
    });
    let hidden = host.entries.len().saturating_sub(3);
    for entry in &mut host.entries[..hidden] {
        entry.counted_at = None;
    }
    for entry in &mut host.entries[hidden..] {
        entry.motion.get_or_insert(ToastMotion {
            from: 0.,
            to: 1.,
            start: Some(now),
            duration: host.motion_duration,
        });
    }
    if host.entries.is_empty() && host.exiting.is_empty() {
        host.held = false;
        return false;
    }
    let mut close_ids = Vec::new();
    let mut action_ids = Vec::new();
    let mut held_now = false;
    column(cx)
        .position(crate::Position::Absolute)
        .inset(0.)
        .gap(8.)
        .padding(16.)
        .width_percent(100.)
        .height_percent(100.)
        .justify(Justify::End)
        .align(Align::End)
        .children(|cx| {
            let mut showing: Vec<_> = host.entries[hidden..]
                .iter()
                .map(|entry| (entry, false))
                .chain(host.exiting.iter().map(|entry| (entry, true)))
                .collect();
            showing.sort_by_key(|(entry, _)| entry.key);
            for (entry, exiting) in showing {
                let toast = &entry.toast;
                let value = entry.motion.as_ref().map_or(1., |motion| motion.value(now));
                column(cx)
                    .key(entry.key)
                    .opacity(value)
                    .translate(0., 16. * (1. - value))
                    .disabled(exiting || value <= 0.)
                    .gap(4.)
                    .padding_y(10.)
                    .padding_x(14.)
                    .radius(10.)
                    .background(Color::Panel)
                    .border(1., Color::Border)
                    .width_percent(100.)
                    .max_width(360.)
                    .children(|cx| {
                        if !exiting {
                            held_now |= cx.hovered_within();
                        }
                        row(cx).gap(8.).align(Align::Center).children(|cx| {
                            column(cx)
                                .width(3.)
                                .height(if toast.description.is_some() {
                                    32.
                                } else {
                                    16.
                                })
                                .background(toast.kind.accent());
                            column(cx).gap(2.).grow(1.).children(|cx| {
                                text(cx, toast.title.as_str())
                                    .font_size(metrics.font_size)
                                    .weight(600);
                                if let Some(description) = &toast.description {
                                    text(cx, description.as_str())
                                        .font_size(metrics.font_size - 1)
                                        .color(Color::Muted);
                                }
                            });
                            if let Some(label) = &toast.action
                                && button(cx).text(label.as_str()).clicked()
                            {
                                action_ids.push(toast.id.clone());
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
                                close_ids.push(toast.id.clone());
                            }
                        });
                    });
            }
        });
    let was_held = host.held;
    host.held = held_now;
    for entry in &mut host.entries[hidden..] {
        let gap = entry
            .counted_at
            .replace(now)
            .map_or(Duration::ZERO, |before| now.saturating_sub(before));
        if !was_held {
            entry.shown = entry.shown.saturating_add(gap);
            if entry.toast.life().is_some_and(|life| entry.shown >= life) {
                close_ids.push(entry.toast.id.clone());
            }
        }
    }
    if !action_ids.is_empty() || !close_ids.is_empty() {
        cx.request_rebuild();
    }
    for id in action_ids {
        if let Some(entry) = host.entries.iter_mut().find(|entry| entry.toast.id == id)
            && let Some(action) = entry.toast.on_action.take()
        {
            action();
        }
        host.close_at(&id, Some(now));
    }
    for id in close_ids {
        host.close_at(&id, Some(now));
    }
    if host.entries[host.entries.len().saturating_sub(3)..]
        .iter()
        .any(|entry| {
            (!host.held && entry.toast.life().is_some())
                || entry
                    .motion
                    .as_ref()
                    .is_some_and(|motion| motion.active(now))
        })
        || !host.exiting.is_empty()
    {
        cx.request_animation_frame();
    }
    !host.entries.is_empty() || !host.exiting.is_empty()
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
