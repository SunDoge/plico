//! Month calendar composed from ordinary controls, styled after MyGO ui/date.go.
//! the widget never reads a clock, so "today" is whatever the caller injects.
use super::{button, column, grid, row, text};
// MyGO date.go monthChevron, expressed with the shared SVG primitive.
const PREVIOUS: crate::SvgAsset = crate::SvgAsset::new(
    r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 6 10" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M5.1 1L.9 5l4.2 4"/></svg>"#,
);
const NEXT: crate::SvgAsset = crate::SvgAsset::new(
    r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 6 10" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M.9 1L5.1 5 .9 9"/></svg>"#,
);
use crate::{Align, CalendarDate, CalendarState, Color, Justify, UiContext, WeekStart};

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
/// muted, following MyGO.
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
    calendar_content(cx, state, week_start, today)
}

/// The month picker for popovers and custom surfaces.
/// The parent supplies the surface; MyGO uses seven 32-pixel day columns.
pub fn calendar_content(
    cx: &mut UiContext,
    state: &mut CalendarState,
    week_start: WeekStart,
    today: Option<CalendarDate>,
) -> CalendarResponse {
    let mut day_selected = false;
    let m = cx.theme().metrics;
    column(cx).gap(m.spacing as f32 * 0.5).children(|cx| {
        header(cx, state);
        weekday_row(cx, week_start);
        day_selected = day_grid(cx, state, week_start, today);
    });
    CalendarResponse { day_selected }
}

fn header(cx: &mut UiContext, state: &mut CalendarState) {
    let month = state.month();
    let heading = cx.localizer().month_year(month);
    let previous = cx.tr("calendar-previous-month", "Previous month");
    let next = cx.tr("calendar-next-month", "Next month");
    let m = cx.theme().metrics;
    row(cx).gap(m.spacing).align(Align::Center).children(|cx| {
        nav_button(
            cx,
            &previous,
            &PREVIOUS,
            month.checked_add_months(-1),
            state,
        );
        column(cx)
            .grow(1.)
            .align(Align::Center)
            .justify(Justify::Center)
            .children(|cx| {
                text(cx, heading.as_str())
                    .font_size(m.font_size)
                    .weight(700);
            });
        nav_button(cx, &next, &NEXT, month.checked_add_months(1), state);
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
    let m = cx.theme().metrics;
    if button(cx)
        .min_height(0.)
        .padding_x(m.spacing as f32 * 2.5)
        .padding_y(m.space(1))
        .aria_label(label)
        .disabled(disabled)
        .children(|cx| {
            super::svg(cx, icon).size(m.spacing as f32 * 1.5, m.spacing as f32 * 2.5);
        })
        .response
        .clicked()
        && let Some(target) = target
    {
        state.set_month(target);
    }
}

fn weekday_row(cx: &mut UiContext, week_start: WeekStart) {
    let m = cx.theme().metrics;
    let mut labels = std::array::from_fn::<_, 7, _>(|day| cx.localizer().weekday(day));
    if week_start == WeekStart::Monday {
        labels.rotate_left(1);
    }
    let row = grid(cx)
        .width(m.space(56) as f32)
        .grid_columns([crate::fr(1.); 7]);
    row.children(|cx| {
        for label in labels {
            text(cx, label)
                .font_size(m.font_size.saturating_sub(2))
                .weight(400)
                .color(Color::Muted)
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
    let m = cx.theme().metrics;
    let grid = grid(cx)
        .width(m.space(56) as f32)
        .grid_columns([crate::fr(1.); 7])
        .row_gap(m.spacing as f32 * 0.5);
    let mut picked = false;
    grid.children(|cx| {
        for index in 0..42i32 {
            match first.checked_add_days(index - i32::from(first_column)) {
                Some(date) => picked |= day_cell(cx, state, date, today),
                // Grid cells before year 1 or past year 9999 have no date;
                // an inert placeholder keeps the columns aligned.
                None => {
                    let _ = column(cx).height(m.space(7) as f32);
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
    let m = cx.theme().metrics;
    let outside = date.month() != state.month().month();
    let selected = state.selected_day(date);
    let in_range = !selected && state.in_range(date);
    let mut cell = button(cx)
        .text(date.day().to_string())
        .height(m.space(7) as f32)
        .min_height(0.)
        .padding(0)
        .font_size(m.font_size)
        .weight(400)
        .border(0., Color::Border)
        .ghost()
        .hover_background(Color::ControlHover)
        .pressed_background(Color::ControlPressed)
        .radius(m.radius as f32);
    if selected {
        cell = cell
            .background(Color::Accent)
            .color(Color::OnAccent)
            .hover_background(Color::Accent)
            .pressed_background(Color::Accent);
    } else if in_range {
        cell = cell.background(Color::Selection).radius(0.);
    } else if today == Some(date) {
        cell = cell.border(1., Color::Accent);
    } else if outside {
        cell = cell.color(Color::Muted);
    }
    let picked = cell.clicked();
    if picked {
        state.select(date);
    }
    picked
}
