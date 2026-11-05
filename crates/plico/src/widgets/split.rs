//! Resizable split panes ported from MyGO's `ui/split.go`.
//!
//! The application owns the first pane's size (or percentage); the divider
//! drags it, and the second pane takes the rest. Drag state is transient and
//! never enters checkpoints.
use crate::WidgetRole;
use crate::prelude::*;

/// Dragging divider positions, keyed by declaration identity. It never enters
/// checkpoints, so a restore releases an in-flight drag.
#[derive(Clone, Debug, Default, PartialEq)]
struct SplitDrags {
    positions: std::collections::HashMap<u64, f64>,
}

/// Two panes side by side with a divider that resizes the first (MyGO
/// `Split`). `size` is the first pane's width in logical pixels, kept by the
/// divider at least the theme's `space(10)` from either edge. Returns whether
/// a drag changed `size`.
///
/// The panes meet at a one-pixel line inside a six-pixel grip, so the divider
/// stays draggable. The drag updates `size` while the primary button is down
/// on the divider. MyGO also moves it with the arrow keys once the divider has
/// focus and positions the grip over the panes; Plico has no per-widget focus
/// action wiring yet and lays the grip out between the panes.
///
/// The room shared is the window's logical width, not the split's laid-out
/// box: nested splits and padded containers clamp against the window instead.
///
/// ```text
/// split(cx, &mut sidebar, |cx| sidebar_view(cx), |cx| editor(cx));
/// ```
pub fn split<F, S>(cx: &mut UiContext, size: &mut f32, first: F, second: S) -> bool
where
    F: FnOnce(&mut UiContext),
    S: FnOnce(&mut UiContext),
{
    split_impl(cx, size, false, false, first, second)
}

/// [`split`] with the first pane above the second; `size` is its height.
pub fn split_vertical<F, S>(cx: &mut UiContext, size: &mut f32, first: F, second: S) -> bool
where
    F: FnOnce(&mut UiContext),
    S: FnOnce(&mut UiContext),
{
    split_impl(cx, size, true, false, first, second)
}

/// A proportional [`split`]: `percent` is the first pane's share of the room,
/// 0 to 100, so both panes grow and shrink with the window.
pub fn split_percent<F, S>(cx: &mut UiContext, percent: &mut f32, first: F, second: S) -> bool
where
    F: FnOnce(&mut UiContext),
    S: FnOnce(&mut UiContext),
{
    split_impl(cx, percent, false, true, first, second)
}

/// [`split_percent`] with the first pane above the second; `percent` is its
/// share of the height.
pub fn split_vertical_percent<F, S>(
    cx: &mut UiContext,
    percent: &mut f32,
    first: F,
    second: S,
) -> bool
where
    F: FnOnce(&mut UiContext),
    S: FnOnce(&mut UiContext),
{
    split_impl(cx, percent, true, true, first, second)
}

fn split_impl<F, S>(
    cx: &mut UiContext,
    size: &mut f32,
    vertical: bool,
    percent: bool,
    first: F,
    second: S,
) -> bool
where
    F: FnOnce(&mut UiContext),
    S: FnOnce(&mut UiContext),
{
    let m = cx.theme().metrics;
    let min_pane = m.space(10) as f32;
    let grip = (m.space(2) as f32).max(6.);
    let viewport = cx.viewport_size();
    let total = if vertical { viewport[1] } else { viewport[0] };
    let before = *size;
    let share_px = if percent && total > 1. {
        (*size).clamp(0., 100.) / 100. * (total - 1.)
    } else {
        0.
    };
    let drag_id = crate::macro_support::key_seed(cx.engine.auto_key(cx.parent));
    let drags = cx.transient_state("plico.split", SplitDrags::default);
    let mut container = if vertical { column(cx) } else { row(cx) };
    container = container.align(Align::Stretch);
    container.children(|cx| {
        let mut pane = column(cx)
            .clip(true)
            .shrink(1.)
            .min_width(0.)
            .min_height(0.);
        pane = if percent {
            let percent = (*size).clamp(0., 100.);
            if vertical {
                pane.height_percent(percent)
            } else {
                pane.width_percent(percent)
            }
        } else if vertical {
            pane.height(*size)
        } else {
            pane.width(*size)
        };
        let _ = pane.children(|cx| first(cx));

        let mut divider = button_base(cx, "Divider")
            .padding(0.)
            .min_width(0.)
            .min_height(0.)
            .hover_background(Color::ControlHover)
            .pressed_background(Color::ControlPressed)
            .semantic_role(WidgetRole::Separator);
        divider = if vertical {
            divider.width_percent(100.).height(grip)
        } else {
            divider.width(grip).height_percent(100.)
        };
        let _ = divider.children(|cx| {
            let active = cx.pressed() || cx.hovered();
            let line = if vertical {
                column(cx).width_percent(100.).height(1.)
            } else {
                column(cx).width(1.).height_percent(100.)
            };
            let _ = line
                .background(if active { Color::Accent } else { Color::Border })
                .children(|_cx| {});
            if let Some((x, y)) = cx.pointer_position()
                && cx.pressed()
            {
                let position = if vertical { y } else { x };
                let from = drags.read().positions.get(&drag_id).copied();
                match from {
                    Some(from) => {
                        let delta = (position - from) as f32;
                        if delta != 0. {
                            let next = (if percent { share_px } else { *size }) + delta;
                            let fitted = fit(next, total, min_pane);
                            if percent {
                                if total > 1. {
                                    *size = (fitted * 100. / (total - 1.)).clamp(0., 100.);
                                }
                            } else {
                                *size = fitted;
                            }
                            drags.update(|drags| {
                                drags.positions.insert(drag_id, position);
                            });
                        }
                    }
                    None => {
                        drags.update(|drags| {
                            drags.positions.insert(drag_id, position);
                        });
                    }
                }
            } else {
                drags.update(|drags| {
                    drags.positions.remove(&drag_id);
                });
            }
        });

        let mut rest = column(cx).clip(true).grow(1.).shrink(1.);
        rest = if vertical {
            rest.min_height(min_pane)
        } else {
            rest.min_width(min_pane)
        };
        let _ = rest.children(|cx| second(cx));
    });
    *size != before
}

/// Clamp the first pane's size so the divider keeps `min_pane` on either side
/// when `total` is known (MyGO's `fitSplit`).
fn fit(value: f32, total: f32, min_pane: f32) -> f32 {
    let mut value = value;
    if total > 0. {
        value = value.min(total - 1. - min_pane);
    }
    value.max(min_pane)
}
