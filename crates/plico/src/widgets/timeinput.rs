//! A time-of-day field ported from MyGO's `ui/timeinput.go`, with the
//! segmented keyboard editing reduced to a validated text field.
use crate::prelude::*;
use crate::{TextBuffer, TimeOfDay};

/// The field's edit buffer and the value it last showed. Transient: an
/// in-progress edit never enters checkpoints.
#[derive(Clone)]
struct TimeEntry {
    buffer: TextBuffer,
    last: TimeOfDay,
}

// The buffer hides its contents from equality and debug output; the revision
// identifies an edit, and `last` identifies the value the field showed.
impl PartialEq for TimeEntry {
    fn eq(&self, other: &Self) -> bool {
        self.last == other.last && self.buffer.revision() == other.buffer.revision()
    }
}

impl std::fmt::Debug for TimeEntry {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("TimeEntry")
            .field("last", &self.last)
            .field("revision", &self.buffer.revision())
            .finish()
    }
}

/// A time-of-day field (MyGO `TimeInput`): a text field showing and accepting
/// `HH:MM`. Typed text applies as soon as it is a valid time of day; Enter
/// also rewrites the field in canonical `HH:MM` form, and Enter with invalid
/// text restores the current value. Returns whether the time changed.
///
/// An application change rewrites the field. MyGO edits hours, minutes and
/// AM/PM as separately focused segments with the arrows and typed digits and
/// follows the regional hour cycle; Plico has no per-segment focus or
/// formatting-on-blur hook, so the port is one text field in 24-hour form and
/// invalid text stays visible until Enter.
///
/// ```text
/// if time_input(cx, "Meeting", &mut meeting) {
///     save(meeting);
/// }
/// ```
#[track_caller]
pub fn time_input(cx: &mut UiContext, label: &str, time: &mut TimeOfDay) -> bool {
    let before = *time;
    let state = cx.state(|| {
        let buffer = TextBuffer::new();
        buffer.set(time.to_string().into());
        TimeEntry {
            buffer,
            last: *time,
        }
    });
    let mut entry = state.get();
    if entry.last != *time {
        entry.buffer.set(time.to_string().into());
        entry.last = *time;
    }
    let response = text_input(cx, label, &entry.buffer)
        .placeholder("HH:MM")
        .width(112.)
        .response();
    let text = entry.buffer.text().unwrap_or_default();
    if response.changed()
        && let Some(parsed) = parse_time(&text)
        && parsed != *time
    {
        *time = parsed;
        entry.last = parsed;
    }
    if response.submitted() {
        match parse_time(&text) {
            Some(parsed) => {
                *time = parsed;
                entry.last = parsed;
                entry.buffer.set(parsed.to_string().into());
            }
            None => entry.buffer.set(time.to_string().into()),
        }
    }
    state.set(entry);
    *time != before
}

/// Parse `H:MM`, `HH:MM`, `HMM` or `HHMM` into a time of day.
fn parse_time(text: &str) -> Option<TimeOfDay> {
    let text = text.trim();
    if let Some((hour, minute)) = text.split_once(':') {
        return TimeOfDay::new(hour.parse().ok()?, minute.parse().ok()?);
    }
    if !(3..=4).contains(&text.len()) || !text.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let split = text.len() - 2;
    TimeOfDay::new(text[..split].parse().ok()?, text[split..].parse().ok()?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_colon_and_compact_forms() {
        assert_eq!(parse_time("9:05"), TimeOfDay::new(9, 5));
        assert_eq!(parse_time(" 09:05 "), TimeOfDay::new(9, 5));
        assert_eq!(parse_time("0905"), TimeOfDay::new(9, 5));
        assert_eq!(parse_time("905"), TimeOfDay::new(9, 5));
        assert_eq!(parse_time("24:00"), None);
        assert_eq!(parse_time("9"), None);
        assert_eq!(parse_time("9:5:0"), None);
        assert_eq!(parse_time("abc"), None);
    }
}
