//! Borrowed localization scopes. The core does not depend on a translation engine.
use crate::CalendarDate;
use std::borrow::Cow;

/// App-owned messages and calendar labels, inherited by ordinary child builders
/// and portals. Implementations may use compiled messages or an external library.
/// Formatting must not read a system clock or change the selected locale.
pub trait Localizer {
    fn locale(&self) -> &str;
    /// The fallback is the widget's English label, not its identity.
    fn text(&self, id: &str, fallback: &'static str) -> Cow<'_, str>;
    fn month_year(&self, date: CalendarDate) -> String;
    /// Sunday is zero; callers pass a value in 0..7.
    fn weekday(&self, sunday_index: usize) -> Cow<'_, str>;
}

pub(crate) struct English;
pub(crate) static ENGLISH: English = English;
impl Localizer for English {
    fn locale(&self) -> &str {
        "en"
    }
    fn text(&self, _: &str, fallback: &'static str) -> Cow<'_, str> {
        Cow::Borrowed(fallback)
    }
    fn month_year(&self, date: CalendarDate) -> String {
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
        format!("{} {}", MONTHS[usize::from(date.month() - 1)], date.year())
    }
    fn weekday(&self, sunday_index: usize) -> Cow<'_, str> {
        Cow::Borrowed(["Su", "Mo", "Tu", "We", "Th", "Fr", "Sa"][sunday_index % 7])
    }
}
