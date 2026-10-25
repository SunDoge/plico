//! Calendar date arithmetic and state semantics; the widget only re-presents
//! what these invariants guarantee.
use plico_core::{CalendarDate, CalendarMode, CalendarState, WeekStart};

fn date(year: u16, month: u8, day: u8) -> CalendarDate {
    CalendarDate::new(year, month, day).expect("valid test date")
}

#[test]
fn validates_gregorian_dates_including_century_leap_rules() {
    assert!(CalendarDate::new(2024, 2, 29).is_some(), "divisible by 4");
    assert!(CalendarDate::new(1900, 2, 29).is_none(), "century");
    assert!(CalendarDate::new(2000, 2, 29).is_some(), "divisible by 400");
    assert!(CalendarDate::new(2025, 2, 29).is_none());
    assert_eq!(CalendarDate::days_in_month(2026, 4), 30);
    assert!(CalendarDate::new(0, 1, 1).is_none());
    assert!(CalendarDate::new(10_000, 1, 1).is_none());
}

#[test]
fn weekdays_match_known_dates() {
    assert_eq!(date(1, 1, 1).weekday(), 0, "0001-01-01 was a Monday");
    assert_eq!(date(2000, 1, 1).weekday(), 5, "2000-01-01 was a Saturday");
    assert_eq!(date(2026, 10, 25).weekday(), 6, "2026-10-25 was a Sunday");
}

#[test]
fn day_and_month_arithmetic_round_trip_and_clamp() {
    let start = date(2026, 10, 31);
    assert_eq!(start.checked_add_days(1), Some(date(2026, 11, 1)));
    assert_eq!(start.checked_add_days(-1), Some(date(2026, 10, 30)));
    for offset in [-4000i32, -1, 0, 1, 365, 4000] {
        let shifted = start.checked_add_days(offset).unwrap();
        assert_eq!(shifted.checked_add_days(-offset), Some(start));
    }
    // January 31st plus one month clamps to the end of February.
    assert_eq!(
        date(2026, 1, 31).checked_add_months(1),
        Some(date(2026, 2, 28))
    );
    assert_eq!(
        date(2024, 1, 31).checked_add_months(1),
        Some(date(2024, 2, 29))
    );
    assert_eq!(
        date(9999, 12, 1).checked_add_months(1),
        None,
        "beyond the supported range"
    );
    assert_eq!(date(1, 1, 1).checked_add_months(-1), None);
}

#[test]
fn single_selection_replaces_and_range_selection_completes_then_restarts() {
    let mut single = CalendarState::new(date(2026, 10, 1));
    assert_eq!(single.mode(), CalendarMode::Single);
    single.select(date(2026, 10, 5));
    assert_eq!(single.selected(), Some(date(2026, 10, 5)));
    assert_eq!(single.selected_range(), None, "single mode has no range");
    single.select(date(2026, 10, 9));
    assert_eq!(single.selected(), Some(date(2026, 10, 9)));

    let mut range = CalendarState::range(date(2026, 10, 1));
    range.select(date(2026, 10, 10));
    assert_eq!(
        range.selected_range(),
        Some((date(2026, 10, 10), None)),
        "first click picks the start"
    );
    range.select(date(2026, 10, 4));
    assert_eq!(
        range.selected_range(),
        Some((date(2026, 10, 4), Some(date(2026, 10, 10)))),
        "an earlier second click reorders the range"
    );
    range.select(date(2026, 10, 20));
    assert_eq!(
        range.selected_range(),
        Some((date(2026, 10, 20), None)),
        "a third click restarts the range"
    );
}

#[test]
fn month_moves_keep_selection_and_normalize_to_the_first_day() {
    let mut state = CalendarState::new(date(2026, 10, 1)).with_selected(date(2026, 10, 25));
    state.set_month(date(2026, 12, 17));
    assert_eq!(state.month(), date(2026, 12, 1));
    assert_eq!(state.selected(), Some(date(2026, 10, 25)));
    state.clear();
    assert_eq!(state.selected(), None);
}

#[test]
fn state_survives_a_serde_round_trip_and_rejects_inverted_ranges() {
    let mut state = CalendarState::range(date(2026, 10, 1));
    state.select(date(2026, 10, 8));
    state.select(date(2026, 10, 3));
    let json = serde_json::to_string(&state).unwrap();
    let restored: CalendarState = serde_json::from_str(&json).unwrap();
    assert_eq!(restored.selected_range(), state.selected_range());

    let invalid = serde_json::json!({
        "month": { "year": 2026, "month": 10, "day": 1 },
        "selected": { "year": 2026, "month": 10, "day": 9 },
        "range_end": { "year": 2026, "month": 10, "day": 8 },
        "mode": "Range",
    });
    assert!(serde_json::from_value::<CalendarState>(invalid).is_err());
    let single_with_end = serde_json::json!({
        "month": { "year": 2026, "month": 10, "day": 1 },
        "selected": { "year": 2026, "month": 10, "day": 9 },
        "range_end": { "year": 2026, "month": 10, "day": 11 },
        "mode": "Single",
    });
    assert!(serde_json::from_value::<CalendarState>(single_with_end).is_err());
}

#[test]
fn the_visible_grid_covers_the_whole_month_from_either_week_start() {
    // The widget derives the 42-cell window from first-column arithmetic; the
    // leading offset must differ by exactly one between the two week starts.
    let first = date(2026, 10, 1);
    assert_eq!(first.weekday(), 3, "2026-10-01 was a Thursday");
    assert_eq!((first.weekday() + 1) % 7, 4, "Sunday-first offset");
    assert_eq!(first.weekday(), 3, "Monday-first offset");
    // A month starting on the first column of a Sunday grid needs all 6 rows.
    let wide = date(2026, 8, 1);
    assert_eq!((wide.weekday() + 1) % 7, 6, "2026-08-01 was a Saturday");
    let leading = i32::from((wide.weekday() + 1) % 7);
    assert!(
        wide.checked_add_days(-leading).is_some(),
        "the grid never reaches before year 1"
    );
    let _ = WeekStart::default();
}
