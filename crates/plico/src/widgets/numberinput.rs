//! A bounded number field ported from MyGO's `ui/numberinput.go`.
use crate::TextBuffer;
use crate::prelude::*;

#[derive(Clone)]
struct NumberEntry {
    buffer: TextBuffer,
    last: f64,
}

impl PartialEq for NumberEntry {
    fn eq(&self, other: &Self) -> bool {
        self.last.to_bits() == other.last.to_bits()
            && self.buffer.revision() == other.buffer.revision()
    }
}

impl std::fmt::Debug for NumberEntry {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("NumberEntry")
            .field("last", &self.last)
            .field("revision", &self.buffer.revision())
            .finish()
    }
}

/// A bounded number field (MyGO `NumberInput`): a text field between two
/// bounds with − and + buttons changing it by `step`. Typed text applies as
/// soon as it parses to a number inside the bounds; Enter rewrites it in
/// canonical form, and Enter with invalid text restores the current value.
/// Returns whether the value changed.
/// Non-finite bounds skip the control without changing the value. Non-finite
/// values reset to the lower bound; reversed finite bounds are accepted.
///
/// Values are formatted with the decimals implied by `step` (at most six
/// decimals); a zero step leaves the buttons at one whole unit. MyGO moves the
/// value with Up/Down and formats on blur; Plico has no blur hook here, so the
/// buttons are the stepping path and the field formats on Enter.
///
/// ```text
/// if number_input(cx, "Copies", &mut copies, 1., 99., 1.) {
///     update(copies);
/// }
/// ```
#[track_caller]
pub fn number_input(
    cx: &mut UiContext,
    label: &str,
    value: &mut f64,
    min: f64,
    max: f64,
    step: f64,
) -> bool {
    if !min.is_finite() || !max.is_finite() {
        return false;
    }
    let before = *value;
    let (min, max) = if min <= max { (min, max) } else { (max, min) };
    let step = if step.is_finite() && step > 0. {
        step
    } else {
        1.
    };
    let decimals = decimals(step);
    *value = if value.is_finite() {
        value.clamp(min, max)
    } else {
        min
    };
    let state = cx.state(|| {
        let buffer = TextBuffer::new();
        buffer.set(format_number(*value, decimals).into());
        NumberEntry {
            buffer,
            last: *value,
        }
    });
    let mut entry = state.get();
    if entry.last != *value {
        entry.buffer.set(format_number(*value, decimals).into());
        entry.last = *value;
    }
    let response = row(cx).gap(4).align(Align::Center).children(|cx| {
        let response = text_input(cx, label, &entry.buffer)
            .width(0.)
            .grow(1.)
            .min_width(64.)
            .response();
        if button(cx)
            .text("−")
            .aria_label(format!("Decrease {label}"))
            .ghost()
            .clicked()
        {
            *value = (*value - step).clamp(min, max);
            entry.buffer.set(format_number(*value, decimals).into());
            entry.last = *value;
        }
        if button(cx)
            .text("+")
            .aria_label(format!("Increase {label}"))
            .ghost()
            .clicked()
        {
            *value = (*value + step).clamp(min, max);
            entry.buffer.set(format_number(*value, decimals).into());
            entry.last = *value;
        }
        response
    });
    let response = response.inner;
    let text = entry.buffer.text().unwrap_or_default();
    if response.changed
        && let Some(parsed) = parse_number(&text)
        && (min..=max).contains(&parsed)
        && parsed != *value
    {
        *value = parsed;
        entry.last = parsed;
    }
    if response.submitted {
        match parse_number(&text).filter(|parsed| (min..=max).contains(parsed)) {
            Some(parsed) => {
                *value = parsed;
                entry.last = parsed;
                entry.buffer.set(format_number(parsed, decimals).into());
            }
            None => entry.buffer.set(format_number(*value, decimals).into()),
        }
    }
    state.set(entry);
    *value != before
}

fn parse_number(text: &str) -> Option<f64> {
    text.trim()
        .parse::<f64>()
        .ok()
        .filter(|value| value.is_finite())
}

fn decimals(step: f64) -> usize {
    let mut decimals = 0;
    let mut scaled = step;
    while decimals < 6 && (scaled - scaled.round()).abs() > 1e-9 {
        scaled *= 10.;
        decimals += 1;
    }
    decimals
}

fn format_number(value: f64, decimals: usize) -> String {
    if decimals == 0 {
        format!("{value:.0}")
    } else {
        let text = format!("{value:.decimals$}");
        text.trim_end_matches('0').trim_end_matches('.').to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_decimals_from_the_step() {
        assert_eq!(format_number(3., 0), "3");
        assert_eq!(format_number(2.5, 1), "2.5");
        assert_eq!(format_number(2.50, 2), "2.5");
        assert_eq!(decimals(1.), 0);
        assert_eq!(decimals(0.05), 2);
        assert_eq!(decimals(0.1), 1);
    }
}
