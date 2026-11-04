//! Compact month pickers: a selected date and a continuous date range.
use crate::heading;
use plico::prelude::*;
use plico::{CalendarDate, CalendarState, WeekStart};

fn date(day: u8) -> CalendarDate {
    CalendarDate::new(2026, 10, day).expect("valid demo date")
}

#[component]
#[rustfmt::skip]
pub fn page(ui: &mut UiContext) {
    let l = ui.localizer();
    let catalog = crate::i18n::catalog(if l.locale().starts_with("zh") {
        crate::i18n::Language::Chinese
    } else {
        crate::i18n::Language::English
    });
    heading(
        ui,
        &l.text("calendar", "Calendar"),
        &l.text("single-dates-and-date-ranges-with-a-quieter-month-view",
            "Single dates and date ranges, with a quieter month view."),
    );
    let single = ui.persisted_state("single", || {
        CalendarState::new(date(1)).with_selected(date(25))
    });
    let range = ui.persisted_state("range", || {
        let mut state = CalendarState::range(date(1));
        state.select(date(12));
        state.select(date(18));
        state
    });
    row(ui).gap(28).wrap(true).align(Align::Start) + |ui| {
        column(ui).width(292.).gap(12) + |ui| {
            column(ui).gap(4) + |ui| {
                text(ui, l.text("single-date", "Single date")).font_size(15).weight(600);
                text(ui, l.text("pick-a-day-for-your-next-event", "Pick a day for your next event.")).font_size(13).color(Color::Muted);
            };
            let mut state = single.get();
            calendar_with(ui, &mut state, WeekStart::Sunday, Some(date(25)));
            single.set(state);
            column(ui).gap(4).padding_x(2) + |ui| {
                text(ui, l.text("selected-date", "Selected date")).font_size(12).color(Color::Muted);
                if let Some(selected) = state.selected() {
                    text(ui, catalog.date(selected)).font_size(14).weight(500);
                } else {
                    text(ui, l.text("choose-a-date", "Choose a date")).font_size(14).color(Color::Muted);
                }
            };
        };
        column(ui).width(292.).gap(12) + |ui| {
            column(ui).gap(4) + |ui| {
                text(ui, l.text("date-range", "Date range")).font_size(15).weight(600);
                text(ui, l.text("choose-a-start-and-an-end-date", "Choose a start and an end date.")).font_size(13).color(Color::Muted);
            };
            let mut state = range.get();
            calendar_with(ui, &mut state, WeekStart::Monday, Some(date(25)));
            range.set(state);
            column(ui).gap(4).padding_x(2) + |ui| {
                text(ui, l.text("selected-range", "Selected range")).font_size(12).color(Color::Muted);
                match state.selected_range() {
                    Some((start, Some(end))) => {
                        text!(ui, "{} — {}", catalog.date(start), catalog.date(end)).font_size(14)
                            .weight(500);
                    }
                    Some((start, None)) => {
                        text!(
                            ui,
                            "{} — {}",
                            catalog.date(start),
                            l.text("pick-an-end", "Pick an end date")
                        ).font_size(14).weight(500);
                    }
                    None => {
                        text(ui, l.text("choose-two-dates", "Choose two dates")).font_size(14)
                            .color(Color::Muted);
                    }
                }
            };
        };
    };
    date_picker(ui);
}

#[component]
#[rustfmt::skip]
fn date_picker(ui: &mut UiContext) {
    let l = ui.localizer();
    let catalog = crate::i18n::catalog(if l.locale().starts_with("zh") {
        crate::i18n::Language::Chinese
    } else {
        crate::i18n::Language::English
    });
    let selected = ui.persisted_state("date-picker", || CalendarState::new(date(1)));
    let popup = ui.state(PopoverState::default);
    let mut state = selected.get();
    let mut open = popup.get();
    let label = state.selected().map_or_else(|| l.text("pick-a-date", "Pick a date").into_owned(),
        |day| catalog.date(day));
    column(ui).width(284.).gap(8).padding_top(12) + |ui| {
        text(ui, l.text("date-picker", "Date picker")).font_size(15).weight(600);
        popover(ui, &mut open, label.as_str()).width(284.) + |ui| {
            if calendar_content(ui, &mut state, WeekStart::Sunday, Some(date(25))).day_selected() {
                ui.close_popover();
            }
        };
    };
    selected.set(state);
    popup.set(open);
}
