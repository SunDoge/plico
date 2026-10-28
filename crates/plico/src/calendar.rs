//! Date-only Gregorian calendar values: no timezone, system clock or runtime.
use serde::{Deserialize, Serialize};
use std::fmt;

/// A validated civil date in years 1..=9999. Ordering is chronological.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "DateFields")]
pub struct CalendarDate {
    year: u16,
    month: u8,
    day: u8,
}
#[derive(Deserialize)]
struct DateFields {
    year: u16,
    month: u8,
    day: u8,
}
impl TryFrom<DateFields> for CalendarDate {
    type Error = &'static str;
    fn try_from(v: DateFields) -> Result<Self, Self::Error> {
        Self::new(v.year, v.month, v.day).ok_or("invalid Gregorian calendar date")
    }
}
impl CalendarDate {
    pub const fn new(year: u16, month: u8, day: u8) -> Option<Self> {
        if year == 0
            || year > 9999
            || month == 0
            || month > 12
            || day == 0
            || day > Self::days_in_month(year, month)
        {
            None
        } else {
            Some(Self { year, month, day })
        }
    }
    pub const fn year(self) -> u16 {
        self.year
    }
    pub const fn month(self) -> u8 {
        self.month
    }
    pub const fn day(self) -> u8 {
        self.day
    }
    pub const fn first_of_month(self) -> Self {
        Self { day: 1, ..self }
    }
    pub const fn last_of_month(self) -> Self {
        Self {
            day: Self::days_in_month(self.year, self.month),
            ..self
        }
    }
    pub const fn days_in_month(year: u16, month: u8) -> u8 {
        match month {
            2 => {
                if year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400))
                {
                    29
                } else {
                    28
                }
            }
            4 | 6 | 9 | 11 => 30,
            1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
            _ => 0,
        }
    }
    /// Zero-based day number; 0001-01-01 was a Monday.
    pub fn ordinal(self) -> i32 {
        let y = i32::from(self.year) - 1;
        let mut days = y * 365 + y / 4 - y / 100 + y / 400;
        for month in 1..self.month {
            days += i32::from(Self::days_in_month(self.year, month));
        }
        days + i32::from(self.day) - 1
    }
    /// Monday=0, Sunday=6.
    pub fn weekday(self) -> u8 {
        (self.ordinal() % 7) as u8
    }
    pub fn checked_add_days(self, days: i32) -> Option<Self> {
        let target = self.ordinal().checked_add(days)?;
        if !(0..=3_652_058).contains(&target) {
            return None;
        }
        let (mut low, mut high) = (1_u16, 9999_u16);
        while low < high {
            let mid = low + (high - low).div_ceil(2);
            if (Self {
                year: mid,
                month: 1,
                day: 1,
            })
            .ordinal()
                <= target
            {
                low = mid;
            } else {
                high = mid - 1;
            }
        }
        let mut remaining = target
            - Self {
                year: low,
                month: 1,
                day: 1,
            }
            .ordinal();
        for month in 1..=12 {
            let length = i32::from(Self::days_in_month(low, month));
            if remaining < length {
                return Self::new(low, month, (remaining + 1) as u8);
            }
            remaining -= length;
        }
        None
    }
    /// Shift months, clamping the day (Jan 31 + 1 month becomes Feb 28/29).
    pub fn checked_add_months(self, months: i32) -> Option<Self> {
        let index =
            ((i32::from(self.year) - 1) * 12 + i32::from(self.month) - 1).checked_add(months)?;
        if !(0..9999 * 12).contains(&index) {
            return None;
        }
        let year = (index / 12 + 1) as u16;
        let month = (index % 12 + 1) as u8;
        Self::new(year, month, self.day.min(Self::days_in_month(year, month)))
    }
}
impl fmt::Display for CalendarDate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:04}-{:02}-{:02}", self.year, self.month, self.day)
    }
}
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CalendarMode {
    #[default]
    Single,
    Range,
}
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum WeekStart {
    #[default]
    Sunday,
    Monday,
}

/// Controlled, serializable presentation state. It contains no theme or host data.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "StateFields")]
pub struct CalendarState {
    month: CalendarDate,
    selected: Option<CalendarDate>,
    range_end: Option<CalendarDate>,
    mode: CalendarMode,
}
#[derive(Deserialize)]
struct StateFields {
    month: CalendarDate,
    #[serde(default)]
    selected: Option<CalendarDate>,
    #[serde(default)]
    range_end: Option<CalendarDate>,
    #[serde(default)]
    mode: CalendarMode,
}
impl TryFrom<StateFields> for CalendarState {
    type Error = &'static str;
    fn try_from(v: StateFields) -> Result<Self, Self::Error> {
        if let Some(end) = v.range_end
            && (v.mode != CalendarMode::Range || v.selected.is_none_or(|start| start > end))
        {
            return Err("invalid calendar range");
        }
        Ok(Self {
            month: v.month.first_of_month(),
            selected: v.selected,
            range_end: v.range_end,
            mode: v.mode,
        })
    }
}
impl CalendarState {
    pub const fn new(month: CalendarDate) -> Self {
        Self {
            month: month.first_of_month(),
            selected: None,
            range_end: None,
            mode: CalendarMode::Single,
        }
    }
    pub const fn range(month: CalendarDate) -> Self {
        Self {
            mode: CalendarMode::Range,
            ..Self::new(month)
        }
    }
    pub fn with_selected(mut self, date: CalendarDate) -> Self {
        self.select(date);
        self
    }
    pub const fn month(&self) -> CalendarDate {
        self.month
    }
    pub const fn mode(&self) -> CalendarMode {
        self.mode
    }
    pub const fn selected(&self) -> Option<CalendarDate> {
        self.selected
    }
    pub fn selected_range(&self) -> Option<(CalendarDate, Option<CalendarDate>)> {
        (self.mode == CalendarMode::Range).then_some((self.selected?, self.range_end))
    }
    pub fn set_month(&mut self, month: CalendarDate) {
        self.month = month.first_of_month();
    }
    pub fn clear(&mut self) {
        self.selected = None;
        self.range_end = None;
    }
    pub fn select(&mut self, date: CalendarDate) {
        match (self.mode, self.selected, self.range_end) {
            (CalendarMode::Range, Some(start), None) => {
                self.selected = Some(start.min(date));
                self.range_end = Some(start.max(date));
            }
            _ => {
                self.selected = Some(date);
                self.range_end = None;
            }
        }
    }
    pub(crate) fn selected_day(&self, date: CalendarDate) -> bool {
        self.selected == Some(date) || self.range_end == Some(date)
    }
    pub(crate) fn in_range(&self, date: CalendarDate) -> bool {
        self.selected
            .zip(self.range_end)
            .is_some_and(|(start, end)| start <= date && date <= end)
    }
}
