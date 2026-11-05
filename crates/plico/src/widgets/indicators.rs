//! Indicators ported from MyGO's `ui` package: level meters, unknown-length
//! spinners and star ratings.
//!
//! They keep MyGO's split between the value the application owns and the
//! drawing the widget derives from it: the meter and rating read and write
//! caller state, and the spinner samples only the injected frame clock.
use crate::WidgetRole;
use crate::prelude::*;
use crate::text;

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
/// The container carries the meter semantic role; the numeric value is not
/// exposed to assistive technology yet.
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
        .semantic_role(WidgetRole::Meter)
        .children(|cx| {
            row(cx).gap(m.space(2)).wrap(true).children(|cx| {
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
/// already-set star clears it to zero, as MyGO does. Each star is a button
/// named "N of MAX stars", so assistive technology and the keyboard can
/// choose one directly. Values above `max` are clamped; zero means no
/// rating. Returns whether the value changed this frame.
///
/// MyGO's hover preview and its arrow/Home/End handling on one slider are
/// not ported yet: Plico has no per-widget focus action wiring, and every
/// star is its own focus stop.
///
/// ```text
/// if rating(cx, &mut stars, 5) { save(); }
/// ```
pub fn rating(cx: &mut UiContext, value: &mut u8, max: u8) -> bool {
    let max = max.max(1);
    let before = *value;
    *value = before.min(max);
    let m = cx.theme().metrics;
    let star = m.font_size as f32 * 1.3;
    row(cx).gap(m.space(1)).align(Align::Center).children(|cx| {
        for index in 0..max {
            let number = index + 1;
            let filled = index < *value;
            let response = button_base(cx, format!("{number} of {max} stars"))
                .width(star)
                .height(star)
                .padding(0.)
                .min_height(0.)
                .radius(m.radius as f32)
                .color(if filled { Color::Warning } else { Color::Muted })
                .hover_background(Color::ControlHover)
                .pressed_background(Color::ControlPressed)
                .children(|cx| {
                    let _ =
                        svg(cx, if filled { &STAR_FILLED } else { &STAR_OUTLINE }).size(star, star);
                })
                .response;
            if response.clicked() {
                *value = if *value == number { 0 } else { number };
            }
        }
    });
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
    let m = cx.theme().metrics;
    let size = m.font_size as f32 * 1.25;
    let dot = (size * 0.16).max(2.);
    let radius = size * 0.38;
    let turn_ms = 900_u128;
    let elapsed = cx.now().as_millis() % turn_ms;
    let lead = (elapsed * 12 / turn_ms) as u32;
    column(cx)
        .width(size)
        .height(size)
        .semantic_role(WidgetRole::ProgressBar)
        .children(|cx| {
            for index in 0..12 {
                let angle = index as f32 * std::f32::consts::TAU / 12.;
                let x = size / 2. + angle.sin() * radius - dot / 2.;
                let y = size / 2. - angle.cos() * radius - dot / 2.;
                // The dot just behind the lead is brightest; the trail fades.
                let age = (lead + 12 - index) % 12;
                let opacity = 1. - age as f32 / 12. * 0.85;
                let _ = column(cx)
                    .absolute(x, y)
                    .width(dot)
                    .height(dot)
                    .radius(dot / 2.)
                    .background(Color::Muted)
                    .opacity(opacity)
                    .children(|_cx| {});
            }
        });
    cx.request_animation_frame();
}
