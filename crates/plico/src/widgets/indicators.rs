//! Indicators ported from MyGO's `ui` package: level meters, unknown-length
//! spinners, star ratings and numeric steppers.
//!
//! They keep MyGO's split between the value the application owns and the
//! drawing the widget derives from it: the meter, rating and stepper read and
//! write caller state, and the spinner samples only the injected frame clock.
use crate::prelude::*;
use crate::text;
use crate::{LocalState, ShaderAsset, WidgetRole, icons};
use std::time::Duration;

static PROGRESS_RING: ShaderAsset = ShaderAsset::new(
    r#"fn ring_coverage(distance: f32, half_width: f32, aa: f32) -> f32 {
        return 1.0 - smoothstep(half_width - aa, half_width + aa, abs(distance));
    }

    fn circle_coverage(distance: f32, radius: f32, aa: f32) -> f32 {
        return 1.0 - smoothstep(radius - aa, radius + aa, distance);
    }

    fn plico_effect(uv: vec2<f32>) -> vec4<f32> {
        let resolution = max(plico.resolution_time_scale.xy, vec2<f32>(1.0));
        let scale = max(plico.resolution_time_scale.w, 0.001);
        let fraction = clamp(plico.values[0].x, 0.0, 1.0);
        let stroke = max(plico.values[0].y * scale, 1.0);
        let active_color = vec4<f32>(plico.values[0].zw, plico.values[1].xy);
        let track_color = vec4<f32>(plico.values[1].zw, plico.values[2].xy);
        let center = resolution * 0.5;
        let point = uv * resolution - center;
        let half_width = stroke * 0.5;
        let radius = max(min(resolution.x, resolution.y) * 0.5 - half_width - scale, half_width);
        let distance = length(point);
        let aa = max(fwidth(distance), 0.75);
        let track_alpha = ring_coverage(distance - radius, half_width, aa);

        var angle = atan2(point.y, point.x) + 0.5 * 3.141592653589793;
        if angle < 0.0 { angle += 2.0 * 3.141592653589793; }
        let sweep = fraction * 2.0 * 3.141592653589793;
        var active_alpha = select(0.0, track_alpha, angle <= sweep || fraction >= 0.9999);

        if fraction > 0.0 && fraction < 0.9999 {
            let start = vec2<f32>(0.0, -radius);
            let end = vec2<f32>(sin(sweep) * radius, -cos(sweep) * radius);
            active_alpha = max(active_alpha, circle_coverage(length(point - start), half_width, aa));
            active_alpha = max(active_alpha, circle_coverage(length(point - end), half_width, aa));
        }

        let track_premultiplied = vec4<f32>(track_color.rgb * track_color.a, track_color.a) * track_alpha;
        let active_premultiplied = vec4<f32>(active_color.rgb * active_color.a, active_color.a) * active_alpha;
        return active_premultiplied + track_premultiplied * (1.0 - active_premultiplied.a);
    }"#,
);

// AppKit/MyGO-style twelve-spoke activity indicator. Keeping the complete
// trail in one analytic surface avoids twelve tiny absolute nodes and gives
// every spoke equal rounded coverage at fractional backing scales.
static SPINNER: ShaderAsset = ShaderAsset::new(
    r#"fn capsule_coverage(point: vec2<f32>, direction: vec2<f32>, inner: f32,
        outer: f32, half_width: f32) -> f32 {
        let along = clamp(dot(point, direction), inner, outer);
        let distance = length(point - direction * along) - half_width;
        return 1.0 - smoothstep(-0.75, 0.75, distance);
    }

    fn plico_effect(uv: vec2<f32>) -> vec4<f32> {
        let resolution = max(plico.resolution_time_scale.xy, vec2<f32>(1.0));
        let point = uv * resolution - resolution * 0.5;
        let side = min(resolution.x, resolution.y);
        let inner = side * 0.22;
        let outer = side * 0.46;
        let half_width = max(side * 0.045, 0.75);
        let phase = u32(floor(fract(plico.resolution_time_scale.z / 0.9) * 12.0)) % 12u;
        let color = vec4<f32>(plico.values[0]);
        var alpha = 0.0;
        for (var index = 0u; index < 12u; index += 1u) {
            let angle = f32(index) * 6.283185307179586 / 12.0;
            let direction = vec2<f32>(sin(angle), -cos(angle));
            let age = (phase + 12u - index) % 12u;
            let trail = 1.0 - f32(age) / 12.0 * 0.85;
            alpha = max(alpha, capsule_coverage(point, direction, inner, outer, half_width) * trail);
        }
        let out_alpha = color.a * alpha;
        return vec4<f32>(color.rgb * out_alpha, out_alpha);
    }"#,
);

fn shader_color(theme: Theme, color: Color) -> [f32; 4] {
    let value = theme.resolve(color);
    [
        ((value >> 16) & 0xff) as f32 / 255.,
        ((value >> 8) & 0xff) as f32 / 255.,
        (value & 0xff) as f32 / 255.,
        1.,
    ]
}

/// A determinate circular progress track rendered as one analytic GPU surface.
///
/// Unlike a ring assembled from rotated rectangles or dots, this keeps even
/// coverage at fractional DPI and gives the active arc round caps. The value
/// is clamped to `0..=1`; non-finite values show an empty track.
pub fn progress_ring(cx: &mut UiContext, value: f32) {
    let size = cx.theme().metrics.font_size as f32 * 2.5;
    progress_ring_sized(cx, value, size, 2.5, Color::Accent, Color::Border);
}

/// [`progress_ring`] with explicit geometry and theme-aware colors.
pub fn progress_ring_sized(
    cx: &mut UiContext,
    value: f32,
    size: f32,
    stroke: f32,
    color: Color,
    track: Color,
) {
    let value = if value.is_finite() {
        value.clamp(0., 1.)
    } else {
        0.
    };
    let size = if size.is_finite() {
        size.clamp(4., 512.)
    } else {
        36.
    };
    let stroke = if stroke.is_finite() {
        stroke.clamp(1., size * 0.5)
    } else {
        2.5
    };
    let active = shader_color(cx.theme(), color);
    let track = shader_color(cx.theme(), track);
    shader(cx, &PROGRESS_RING)
        .width(size)
        .height(size)
        .shrink(0.)
        .radius(size * 0.5)
        .values(&[
            value, stroke, active[0], active[1], active[2], active[3], track[0], track[1],
            track[2], track[3],
        ])
        .paused(true);
}

/// The levels from which a [`meter`] shows that its value is worse, in the
/// meter's own units. The bar is `Success` colored below them, `Warning`
/// from [`warning`](Self::warning) and `Danger` from
/// [`critical`](Self::critical); a critical level below the warning level
/// makes low values the bad ones, as a battery's.
///
/// `None` leaves that level unused, so `MeterLevels::default()` shows the
/// success color throughout (MyGO passes a nil `*MeterLevels` for none).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct MeterLevels {
    /// The value from which the meter turns warning colored.
    pub warning: Option<f64>,
    /// The value from which the meter turns danger colored.
    pub critical: Option<f64>,
}

impl MeterLevels {
    fn color(self, value: f64) -> Color {
        match (self.warning, self.critical) {
            // A critical level below the warning level: low values are bad.
            (Some(warning), Some(critical)) if critical < warning => {
                if value <= critical {
                    Color::Danger
                } else if value <= warning {
                    Color::Warning
                } else {
                    Color::Success
                }
            }
            (Some(warning), Some(critical)) => {
                if value >= critical {
                    Color::Danger
                } else if value >= warning {
                    Color::Warning
                } else {
                    Color::Success
                }
            }
            (Some(warning), None) => {
                if value >= warning {
                    Color::Warning
                } else {
                    Color::Success
                }
            }
            (None, Some(critical)) => {
                if value >= critical {
                    Color::Danger
                } else {
                    Color::Success
                }
            }
            (None, None) => Color::Success,
        }
    }
}

/// A bar showing `value` between the bounds of `range` (MyGO `Meter`): the
/// label on the left, the percentage of the range on the right, and a fill
/// colored by `levels`. Non-finite values show an empty bar.
///
/// MyGO draws only the bar and takes its name from `.Label(...)`; Plico's
/// `progress` shows the label and the percentage, and this meter follows
/// that convention so the value stays readable without assistive technology.
/// The container carries the meter role, accessible name and bounded value.
///
/// ```text
/// meter(cx, "Disk", used, 0.0..=size, MeterLevels {
///     warning: Some(0.8 * size),
///     critical: Some(0.95 * size),
/// });
/// ```
pub fn meter(
    cx: &mut UiContext,
    label: &str,
    value: f64,
    range: std::ops::RangeInclusive<f64>,
    levels: MeterLevels,
) {
    let (low, high) = (*range.start(), *range.end());
    let fraction = if value.is_finite() && high > low {
        ((value - low) / (high - low)).clamp(0., 1.)
    } else {
        0.
    };
    let color = levels.color(value);
    let m = cx.theme().metrics;
    column(cx)
        .gap(m.space(1))
        .semantic_range(WidgetRole::Meter, label, low, high, value)
        .children(|cx| {
            row(cx)
                .gap(m.space(2))
                .wrap(true)
                .semantic_hidden(true)
                .children(|cx| {
                    text(cx, label).font_size(m.label_size).grow(1.);
                    text!(cx, "{:.0}%", fraction * 100.)
                        .font_size(m.label_size)
                        .color(Color::Muted);
                });
            row(cx)
                .height(m.spacing.max(1) as f32 * 2.)
                .width_percent(100.)
                .radius(m.radius as f32)
                .background(Color::Control)
                .semantic_hidden(true)
                .children(|cx| {
                    let _ = column(cx)
                        .width_percent(fraction as f32 * 100.)
                        .height_percent(100.)
                        .radius(m.radius as f32)
                        .background(color)
                        .children(|_cx| {});
                });
        });
}

/// A star outline for [`rating`]'s unfilled stars (Lucide `star`).
const STAR_OUTLINE: SvgAsset = SvgAsset::new(
    r#"<svg
  xmlns="http://www.w3.org/2000/svg"
  width="24"
  height="24"
  viewBox="0 0 24 24"
  fill="none"
  stroke="currentColor"
  stroke-width="1.6"
  stroke-linecap="round"
  stroke-linejoin="round"
>
  <path d="M11.525 2.295a.53.53 0 0 1 .95 0l2.31 4.679a2.123 2.123 0 0 0 1.595 1.16l5.166.756a.53.53 0 0 1 .294.904l-3.736 3.638a2.123 2.123 0 0 0-.611 1.878l.882 5.14a.53.53 0 0 1-.771.56l-4.618-2.428a2.122 2.122 0 0 0-1.973 0L6.396 21.01a.53.53 0 0 1-.77-.56l.881-5.139a2.122 2.122 0 0 0-.611-1.879L2.16 9.795a.53.53 0 0 1 .294-.906l5.165-.755a2.122 2.122 0 0 0 1.597-1.16z" />
</svg>
"#,
);

/// The same star filled, for the stars a [`rating`] shows.
const STAR_FILLED: SvgAsset = SvgAsset::new(
    r#"<svg
  xmlns="http://www.w3.org/2000/svg"
  width="24"
  height="24"
  viewBox="0 0 24 24"
  fill="currentColor"
  stroke="currentColor"
  stroke-width="1.6"
  stroke-linecap="round"
  stroke-linejoin="round"
>
  <path d="M11.525 2.295a.53.53 0 0 1 .95 0l2.31 4.679a2.123 2.123 0 0 0 1.595 1.16l5.166.756a.53.53 0 0 1 .294.904l-3.736 3.638a2.123 2.123 0 0 0-.611 1.878l.882 5.14a.53.53 0 0 1-.771.56l-4.618-2.428a2.122 2.122 0 0 0-1.973 0L6.396 21.01a.53.53 0 0 1-.77-.56l.881-5.139a2.122 2.122 0 0 0-.611-1.879L2.16 9.795a.53.53 0 0 1 .294-.906l5.165-.755a2.122 2.122 0 0 0 1.597-1.16z" />
</svg>
"#,
);

/// A row of `max` stars, `*value` of them filled (MyGO `Rating`).
///
/// A click on a star sets the rating to that star, and a click on the
/// already-set star clears it to zero, as MyGO does. The whole rating is one
/// slider focus stop: arrows change it by one, Home clears it and End chooses
/// the maximum. Hovering previews the stars that a click would choose. Values
/// above `max` are clamped; zero means no rating. Returns whether the value
/// changed this frame.
///
/// ```text
/// if rating(cx, &mut stars, 5) { save(); }
/// ```
pub fn rating(cx: &mut UiContext, value: &mut u8, max: u8) -> bool {
    rating_labeled(cx, "Rating", value, max)
}

/// [`rating`] with an application-specific accessible name.
pub fn rating_labeled(cx: &mut UiContext, label: &str, value: &mut u8, max: u8) -> bool {
    let max = max.max(1);
    let before = *value;
    *value = before.min(max);
    let m = cx.theme().metrics;
    let star = m.font_size as f32 * 1.3;
    let mut numeric = f64::from(*value);
    cx.value_control(label, &mut numeric, 0. ..=f64::from(max), |cx, current| {
        let preview = cx.pointer_fraction().and_then(|fraction| {
            cx.hovered_within()
                .then(|| ((fraction[0].clamp(0., 0.999_999) * f32::from(max)).floor() as u8) + 1)
        });
        for index in 0..max {
            let number = index + 1;
            let shown = preview.unwrap_or(current.round().clamp(0., f64::from(max)) as u8);
            let filled = index < shown;
            let response = button_base(cx, format!("{number} of {max} stars"))
                .width(star)
                .height(star)
                .padding(0.)
                .min_height(0.)
                .radius(m.radius as f32)
                .tab_stop(false)
                .semantic_hidden(true)
                .color(if filled { Color::Warning } else { Color::Muted })
                .hover_background(Color::ControlHover)
                .pressed_background(Color::ControlPressed)
                .opacity(if preview.is_some() { 0.72 } else { 1. })
                .children(|cx| {
                    let _ =
                        svg(cx, if filled { &STAR_FILLED } else { &STAR_OUTLINE }).size(star, star);
                })
                .response;
            if response.clicked() {
                *current = if current.round() as u8 == number {
                    0.
                } else {
                    f64::from(number)
                };
                cx.request_focus();
            }
        }
    });
    *value = numeric.round().clamp(0., f64::from(max)) as u8;
    *value != before
}

/// An unknown-length work indicator (MyGO `Spinner`): twelve muted dots
/// around a circle, each fading in turn every 900 ms. The size follows the
/// theme's font size, as MyGO's does. It keeps requesting animation frames
/// while it is declared, so the host repaints it like MyGO's painter-only
/// spin.
///
/// Place the explanatory text beside the spinner; the dots are decorative
/// and carry the progress role. MyGO strokes rotated spokes and names the
/// control with `.Label(...)`; Plico has no per-container accessible-name
/// API yet, so no label is attached here.
///
/// ```text
/// row(cx).gap(8) + |cx| {
///     spinner(cx);
///     text(cx, "Loading messages…").color(Color::Muted);
/// };
/// ```
pub fn spinner(cx: &mut UiContext) {
    spinner_sized(cx, cx.theme().metrics.font_size as f32 * 1.25);
}

/// A spinner with an explicit logical-pixel diameter.
pub fn spinner_sized(cx: &mut UiContext, size: f32) {
    let size = if size.is_finite() {
        size.clamp(4., 256.)
    } else {
        18.
    };
    let color = shader_color(cx.theme(), Color::Muted);
    let reduce_motion = cx.display_preferences().reduce_motion;
    column(cx)
        .width(size)
        .height(size)
        .shrink(0.)
        .semantic_role(WidgetRole::ProgressBar)
        .children(|cx| {
            shader(cx, &SPINNER)
                .width(size)
                .height(size)
                .radius(size * 0.5)
                .values(&color)
                .animated(!reduce_motion)
                .paused(reduce_motion);
        });
}

/// The chevron-up for [`stepper`]'s increment arrow (Lucide `chevron-up`).
pub(super) const CHEVRON_UP: SvgAsset = SvgAsset::new(
    r#"<svg
  xmlns="http://www.w3.org/2000/svg"
  width="24"
  height="24"
  viewBox="0 0 24 24"
  fill="none"
  stroke="currentColor"
  stroke-width="2"
  stroke-linecap="round"
  stroke-linejoin="round"
>
  <path d="m18 15-6-6-6 6" />
</svg>
"#,
);

/// How long an arrow is held before it steps repeatedly (MyGO 400 ms).
const STEP_HOLD_DELAY: Duration = Duration::from_millis(400);
/// The repeat interval after the hold delay (MyGO 80 ms).
const STEP_REPEAT_MS: u128 = 80;

/// One arrow's hold bookkeeping: when the press started and how many steps it
/// has applied, so repeats do not run twice in a frame.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct StepHold {
    started: Option<Duration>,
    applied: u32,
}

/// The stepper's transient hold state, keyed by declaration identity. It never
/// enters checkpoints, and a restore releases an in-flight hold.
#[derive(Clone, Debug, Default, PartialEq)]
struct StepperHolds {
    arrows: std::collections::HashMap<u64, StepHold>,
}

/// A pair of arrows stepping `value` by `step` between the bounds of `range`
/// (MyGO `Stepper`): the up arrow increases and the down arrow decreases, a
/// press steps at once and holding keeps stepping after 400 ms, every 80 ms,
/// as MyGO's does. Values are snapped to the step grid anchored at the low
/// bound and clamped to the range. Returns whether the value changed.
///
/// The whole control is one spin-button focus stop. Up/Right increment,
/// Down/Left decrement, and Home/End choose the bounds. Its pointer-only
/// arrows are hidden from the accessibility projection so they do not
/// duplicate the parent value semantics.
///
/// ```text
/// row(cx).gap(6) + |cx| {
///     text!(cx, "{} copies", copies);
///     stepper(cx, &mut copies, 1.0..=99.0, 1.0);
/// };
/// ```
#[track_caller]
pub fn stepper(
    cx: &mut UiContext,
    value: &mut f64,
    range: std::ops::RangeInclusive<f64>,
    step: f64,
) -> bool {
    stepper_labeled(cx, "Stepper", value, range, step)
}

/// [`stepper`] with an application-specific accessible name.
#[track_caller]
pub fn stepper_labeled(
    cx: &mut UiContext,
    label: &str,
    value: &mut f64,
    range: std::ops::RangeInclusive<f64>,
    step: f64,
) -> bool {
    let before = *value;
    // Non-finite or reversed bounds fall back to a single-point range rather
    // than panicking in `clamp`.
    let start = if range.start().is_finite() {
        *range.start()
    } else {
        0.
    };
    let end = if range.end().is_finite() {
        *range.end()
    } else {
        start
    };
    let (low, high) = (start.min(end), start.max(end));
    *value = snap_value(*value, low, high, step);
    let m = cx.theme().metrics;
    // Two arrow slots inside this stepper's component-local state.
    let up_id = 0;
    let down_id = 1;
    let height = m.space(4) as f32;
    cx.value_control_with(
        label,
        value,
        low..=high,
        step.abs(),
        WidgetRole::SpinButton,
        crate::FlexDirection::Column,
        |mut style| {
            style.width = Some(m.space(5) as f32);
            style.radius = m.radius as f32;
            style.border_width = 1.;
            style.border_color = Color::Border;
            style.background = Some(Color::Surface);
            style.clip = true;
            style
        },
        |cx, value| {
            let holds = cx.state(StepperHolds::default);
            stepper_arrow(cx, value, low, high, step, true, height, up_id, &holds);
            stepper_arrow(cx, value, low, high, step, false, height, down_id, &holds);
        },
    );
    *value != before
}

#[allow(clippy::too_many_arguments)]
fn stepper_arrow(
    cx: &mut UiContext,
    value: &mut f64,
    low: f64,
    high: f64,
    step: f64,
    up: bool,
    height: f32,
    id: u64,
    holds: &LocalState<StepperHolds>,
) {
    let can = if up { *value < high } else { *value > low };
    let mut pressing = false;
    let response = button_base(cx, if up { "Increase" } else { "Decrease" })
        .width_percent(100.)
        .height(height)
        .padding(0.)
        .min_height(0.)
        .tab_stop(false)
        .semantic_hidden(true)
        .align(Align::Center)
        .justify(Justify::Center)
        .hover_background(Color::ControlHover)
        .pressed_background(Color::ControlPressed)
        .color(if can { Color::Foreground } else { Color::Muted })
        .disabled(!can)
        .semantic_role(WidgetRole::SpinButton)
        .children(|cx| {
            let _ = svg(
                cx,
                if up {
                    &CHEVRON_UP
                } else {
                    &icons::CHEVRON_DOWN
                },
            )
            .size(height * 0.55, height * 0.55);
            pressing = can && cx.pressed();
            if pressing {
                let now = cx.now();
                let mut hold = holds.read().arrows.get(&id).copied().unwrap_or_default();
                let started = hold.started.unwrap_or(now);
                let elapsed = now.saturating_sub(started);
                let due = if elapsed > STEP_HOLD_DELAY {
                    1 + u32::try_from((elapsed - STEP_HOLD_DELAY).as_millis() / STEP_REPEAT_MS)
                        .unwrap_or(u32::MAX)
                } else {
                    1
                };
                if due > hold.applied {
                    let direction = if up { step } else { -step };
                    for _ in hold.applied..due {
                        *value = snap_value(*value + direction, low, high, step);
                    }
                    hold.applied = due;
                }
                hold.started = Some(started);
                holds.update(|holds| {
                    holds.arrows.insert(id, hold);
                });
                // The repeats run on the frame clock, not on pointer moves.
                cx.request_animation_frame();
            }
        })
        .response;
    if pressing {
        cx.request_focus();
        return;
    }
    let was_holding = holds.read().arrows.contains_key(&id);
    if was_holding {
        // The release frame also reports a click; the press already stepped,
        // so only forget the hold.
        holds.update(|holds| {
            holds.arrows.remove(&id);
        });
        return;
    }
    // Keyboard/assistive activation arrives as a click without a press.
    if response.clicked() && can {
        let direction = if up { step } else { -step };
        *value = snap_value(*value + direction, low, high, step);
        cx.request_focus();
    }
}

/// Snap `value` to the grid anchored at `low`, clamped to `[low, high]`, and
/// rounded away from accumulated floating-point noise as MyGO's `snap` does.
fn snap_value(value: f64, low: f64, high: f64, step: f64) -> f64 {
    let value = if value.is_finite() {
        value.clamp(low, high)
    } else {
        low
    };
    if !step.is_finite() || step <= 0. || high <= low {
        return value;
    }
    let snapped = low + ((value - low) / step).round() * step;
    let snapped = if snapped > high {
        snapped - step
    } else {
        snapped
    };
    ((snapped * 1e9).round() / 1e9).clamp(low, high)
}
