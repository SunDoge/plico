//! A two-thumb range slider over the shared slider core.
use crate::prelude::*;

/// A range slider (MyGO `RangeSlider`): a low and a high thumb on one track,
/// with the active filled between them. Tabbing to it focuses the control;
/// the arrows, Home and End move the thumb last touched, and a press picks the
/// nearer thumb. The thumbs cannot cross. Returns whether either value changed.
///
/// MyGO gives each knob its own focus stop and exposes two named sliders to
/// assistive technology. Plico's slider core currently has one focusable
/// entity per slider, so the pair shares one focus and one semantic value
/// (the low bound); the high bound is drawn but not yet separately exposed.
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
