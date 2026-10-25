//! A shadcn-style month calendar card. State is controlled and serializable;
//! the widget never reads a clock, so "today" is whatever the caller injects.
use super::{button, column, grid, row, text};
use crate::icons;
use crate::{Align, CalendarDate, CalendarState, Color, Justify, UiContext, WeekStart};

const MONTHS: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

/// Whether the user picked a day this frame. Month navigation mutates
/// [`CalendarState::month`] directly and is not reported here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CalendarResponse {
    day_selected: bool,
}
impl CalendarResponse {
    pub const fn day_selected(self) -> bool {
        self.day_selected
    }
}

/// Show a month grid bound to `state`. Clicking a day selects it; in
/// [`crate::CalendarMode::Range`] a second click completes the range and the next
/// click starts over. Days outside the focused month stay clickable but render
/// muted, matching the shadcn calendar.
pub fn calendar(
    cx: &mut UiContext,
    state: &mut CalendarState,
    week_start: WeekStart,
) -> CalendarResponse {
    calendar_with(cx, state, week_start, None)
}

/// Like [`calendar`], with a caller-provided "today" highlighted with a subtle
/// background. Hosts pass the current date from their `Clock`; the widget itself
/// never reads system time.
pub fn calendar_with(
    cx: &mut UiContext,
    state: &mut CalendarState,
    week_start: WeekStart,
    today: Option<CalendarDate>,
) -> CalendarResponse {
    let mut response = CalendarResponse::default();
    column(cx)
        .width(292.)
        .padding(12.)
        .radius(10.)
        .background(Color::Panel)
        .border(1., Color::Border)
        .children(|cx| {
            response = calendar_content(cx, state, week_start, today);
        });
    response
}

/// The month picker without an outer card, for popovers and custom surfaces.
/// The parent supplies width, padding, border and background; 266 logical pixels
/// gives each day a 38-pixel column with the default typography.
pub fn calendar_content(
    cx: &mut UiContext,
    state: &mut CalendarState,
    week_start: WeekStart,
    today: Option<CalendarDate>,
) -> CalendarResponse {
    let mut day_selected = false;
    column(cx).gap(8.).children(|cx| {
        header(cx, state);
        weekday_row(cx, week_start);
        day_selected = day_grid(cx, state, week_start, today);
    });
    CalendarResponse { day_selected }
}

fn header(cx: &mut UiContext, state: &mut CalendarState) {
    let month = state.month();
    row(cx).align(Align::Center).children(|cx| {
        nav_button(
            cx,
            "Previous month",
            &icons::CHEVRON_LEFT,
            month.checked_add_months(-1),
            state,
        );
        column(cx)
            .grow(1.)
            .align(Align::Center)
            .justify(Justify::Center)
            .children(|cx| {
                crate::text!(
                    cx,
                    "{} {}",
                    MONTHS[usize::from(month.month() - 1)],
                    month.year()
                )
                .font_size(14)
                .weight(600);
            });
        nav_button(
            cx,
            "Next month",
            &icons::CHEVRON_RIGHT,
            month.checked_add_months(1),
            state,
        );
    });
}

fn nav_button(
    cx: &mut UiContext,
    label: &str,
    icon: &'static crate::SvgAsset,
    target: Option<CalendarDate>,
    state: &mut CalendarState,
) {
    let disabled = target.is_none();
    if button(cx)
        .width(28.)
        .height(28.)
        .padding(0)
        .radius(6.)
        .background(Color::Panel)
        .border(1., Color::Border)
        .hover_background(Color::Control)
        .pressed_background(Color::ControlHover)
        .icon(icon)
        .text("")
        .aria_label(label)
        .disabled(disabled)
        .clicked()
        && let Some(target) = target
    {
        state.set_month(target);
    }
}

fn weekday_row(cx: &mut UiContext, week_start: WeekStart) {
    const SUNDAY_FIRST: [&str; 7] = ["Su", "Mo", "Tu", "We", "Th", "Fr", "Sa"];
    let mut labels = SUNDAY_FIRST;
    if week_start == WeekStart::Monday {
        labels.rotate_left(1);
    }
    let row = grid(cx).grid_columns([crate::fr(1.); 7]);
    row.children(|cx| {
        for label in labels {
            text(cx, label)
                .font_size(12)
                .weight(400)
                .color(Color::Muted)
                .height(24.)
                .align_self(Align::Center)
                .justify_self(Align::Center);
        }
    });
}

fn day_grid(
    cx: &mut UiContext,
    state: &mut CalendarState,
    week_start: WeekStart,
    today: Option<CalendarDate>,
) -> bool {
    let first = state.month().first_of_month();
    // 0001-01-01 was a Monday: weekday() numbers Monday=0, Sunday=6.
    let first_column = match week_start {
        WeekStart::Sunday => (first.weekday() + 1) % 7,
        WeekStart::Monday => first.weekday(),
    };
    let grid = grid(cx).grid_columns([crate::fr(1.); 7]).row_gap(4.);
    let mut picked = false;
    grid.children(|cx| {
        for index in 0..42i32 {
            match first.checked_add_days(index - i32::from(first_column)) {
                Some(date) => picked |= day_cell(cx, state, date, today),
                // Grid cells before year 1 or past year 9999 have no date;
                // an inert placeholder keeps the columns aligned.
                None => {
                    let _ = column(cx).height(36.);
                }
            }
        }
    });
    picked
}

fn day_cell(
    cx: &mut UiContext,
    state: &mut CalendarState,
    date: CalendarDate,
    today: Option<CalendarDate>,
) -> bool {
    let outside = date.month() != state.month().month();
    let selected = state.selected_day(date);
    let in_range = !selected && state.in_range(date);
    let mut cell = button(cx)
        .text(date.day().to_string())
        .height(36.)
        .padding(0)
        .font_size(14)
        .weight(400)
        .border(0., Color::Border)
        .background(Color::Panel)
        .hover_background(Color::ControlHover)
        .pressed_background(Color::ControlPressed)
        .radius(6.);
    if selected {
        cell = cell
            .background(Color::Foreground)
            .color(Color::Panel)
            .hover_background(Color::Foreground)
            .pressed_background(Color::Foreground)
            .weight(500);
    } else if in_range {
        cell = cell.background(Color::Control).radius(0.);
    } else if today == Some(date) {
        cell = cell.background(Color::Control).weight(600);
    } else if outside {
        cell = cell.color(Color::Muted);
    }
    let picked = cell.clicked();
    if picked {
        state.select(date);
    }
    picked
}
