//! Time-of-day values: no timezone, no date and no runtime clock.
use serde::{Deserialize, Serialize};
use std::fmt;

/// A validated time of day in hours 0..=23 and minutes 0..=59. Ordering is
/// chronological within a day.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(try_from = "TimeFields")]
pub struct TimeOfDay {
    hour: u8,
    minute: u8,
}

#[derive(Deserialize)]
struct TimeFields {
    hour: u8,
    minute: u8,
}

impl TryFrom<TimeFields> for TimeOfDay {
    type Error = &'static str;

    fn try_from(fields: TimeFields) -> Result<Self, Self::Error> {
        Self::new(fields.hour, fields.minute).ok_or("invalid time of day")
    }
}

impl TimeOfDay {
    /// Midnight, the default time of day.
    pub const MIDNIGHT: Self = Self { hour: 0, minute: 0 };

    /// A time of day, or `None` when the hour is past 23 or the minute past 59.
    pub const fn new(hour: u8, minute: u8) -> Option<Self> {
        if hour > 23 || minute > 59 {
            None
        } else {
            Some(Self { hour, minute })
        }
    }

    /// A time of day from minutes since midnight, or `None` past 1439.
    pub const fn from_minutes(minutes: u16) -> Option<Self> {
        if minutes >= 24 * 60 {
            return None;
        }
        Some(Self {
            hour: (minutes / 60) as u8,
            minute: (minutes % 60) as u8,
        })
    }

    /// Hours since midnight, 0..=23.
    pub const fn hour(self) -> u8 {
        self.hour
    }

    /// Minutes past the hour, 0..=59.
    pub const fn minute(self) -> u8 {
        self.minute
    }

    /// Minutes since midnight, 0..=1439.
    pub const fn minutes(self) -> u16 {
        self.hour as u16 * 60 + self.minute as u16
    }
}

impl fmt::Display for TimeOfDay {
    /// `HH:MM`, zero-padded.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{:02}:{:02}", self.hour, self.minute)
    }
}
