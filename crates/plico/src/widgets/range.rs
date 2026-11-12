//! A two-thumb range slider over the shared slider core.
use crate::prelude::*;

/// A range slider (MyGO `RangeSlider`): a low and a high thumb on one track,
/// with the active filled between them. Each thumb is a separate Tab stop;
/// arrows, Home and End move the focused thumb, and a press picks and focuses
/// the nearer thumb. The thumbs cannot cross. Assistive technology sees a
/// named group with "minimum" and "maximum" sliders. Returns whether either
/// value changed.
///
/// ```text
/// if range_slider(cx, "Price", &mut low, &mut high, 0.0..=1000.0) {
///     filter(low, high);
/// }
/// ```
pub fn range_slider(
    cx: &mut UiContext,
    label: &str,
    low: &mut f64,
    high: &mut f64,
    range: std::ops::RangeInclusive<f64>,
) -> bool {
    let before = (*low, *high);
    let _ = cx.range_slider(label, low, high, range).step(0.);
    *low != before.0 || *high != before.1
}
