//! A date field with a calendar popover, ported from MyGO's `ui/dateinput.go`
//! and the date half of its date/time pair.
use crate::prelude::*;
use crate::{CalendarDate, CalendarState, PopoverState, WeekStart};

/// The field's remembered calendar state per declaration. It is transient:
/// an open month or a highlighted day never enters checkpoints.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct DateEntry {
    calendar: CalendarState,
    last: CalendarDate,
}

/// A date field (MyGO `DateInput`): a pop-up button showing the date as
/// `YYYY-MM-DD`, whose calendar below it chooses a day. A click, Enter or
/// Space opens the calendar; choosing a day sets `date` and closes it.
/// Returns whether the date changed this frame.
///
/// The trigger is named by `label`; the panel opens below it and the calendar
/// keeps its month while open. [`date_input_with`] takes the week start; this
/// entry point uses ISO Monday.
///
/// MyGO writes the user's regional numeric format and keeps the time of day;
/// Plico dates carry no time and no regional numeric ordering data, so the
/// field shows ISO and the caller composes a time input separately.
///
/// ```text
/// if date_input(cx, "Birthday", &mut birthday, &mut calendar) {
///     save(birthday);
/// }
/// ```
#[track_caller]
pub fn date_input<'s>(
    cx: &mut UiContext,
    label: &'s str,
    date: &mut CalendarDate,
    state: &'s mut PopoverState,
) -> bool {
    date_input_with(cx, label, WeekStart::Monday, date, state)
}

/// [`date_input`] with an explicit first day of the week.
#[track_caller]
pub fn date_input_with<'s>(
    cx: &mut UiContext,
    label: &'s str,
    week_start: WeekStart,
    date: &mut CalendarDate,
    state: &'s mut PopoverState,
) -> bool {
    let before = *date;
    let local = cx.state(|| DateEntry {
        calendar: CalendarState::new(*date).with_selected(*date),
        last: *date,
    });
    let entry = local.get();
    let mut calendar_state = if entry.last == *date {
        entry.calendar
    } else {
        CalendarState::new(*date).with_selected(*date)
    };
    let shown = format!("{:04}-{:02}-{:02}", date.year(), date.month(), date.day());
    let mut chosen = None;
    let _ = cx
        .popover(state, label)
        .display_text(shown)
        .width(320.)
        .children(|cx| {
            let response = calendar_content(cx, &mut calendar_state, week_start, None);
            if response.day_selected() {
                chosen = calendar_state.selected();
                cx.close_popover();
            }
        });
    if let Some(chosen) = chosen {
        *date = chosen;
    }
    local.set(DateEntry {
        calendar: calendar_state,
        last: *date,
    });
    *date != before
}
