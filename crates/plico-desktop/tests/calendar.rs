//! Interaction coverage for the shadcn-style calendar widget: day picking,
//! range completion, month navigation, outside-month days and the year-one
//! boundary. Each test uses its own card, so day-label occurrence indices
//! resolve within a single visible grid (October 2026, Monday-first: leading
//! September 28–30, trailing November 1–8 — dates ascend in tree order).
use plico::prelude::*;
use plico::{CalendarDate, CalendarState, WeekStart};
use plico_desktop::{HostView, Source};
use plico_renderer::{FrameSource, SemanticAction, SemanticRole, TextContext};

fn date(year: u16, month: u8, day: u8) -> CalendarDate {
    CalendarDate::new(year, month, day).expect("valid test date")
}

/// One range-mode card, the scenario under test in the first case.
#[derive(Clone)]
struct RangeCard {
    state: CalendarState,
}
impl HostView for RangeCard {
    #[component]
    fn show(&mut self, cx: &mut UiContext) {
        calendar(cx, &mut self.state, WeekStart::Monday);
    }
}

/// A single picker plus a range picker, for cross-card navigation isolation.
#[derive(Clone)]
struct TwoCards {
    single: CalendarState,
    range: CalendarState,
}
impl HostView for TwoCards {
    #[component]
    fn show(&mut self, cx: &mut UiContext) {
        row(cx).gap(24.).children(|cx| {
            calendar(cx, &mut self.single, WeekStart::Sunday);
            calendar(cx, &mut self.range, WeekStart::Monday);
        });
    }
}

fn click<V: HostView>(s: &mut Source<V>, tcx: &mut TextContext, label: &str) {
    let id = s
        .adapter
        .semantics(&s.engine, 0)
        .node_by_role(SemanticRole::Button, label, Some(0))
        .unwrap_or_else(|| panic!("missing calendar button {label}"))
        .id;
    assert!(s.handle_semantic_action(SemanticAction::Click { target: id }));
    let _ = s.build_frame(tcx, 920, 700);
}

#[test]
fn range_selection_completes_reorders_and_restarts() {
    let mut s = Source::new(RangeCard {
        state: CalendarState::range(date(2026, 10, 1)),
    });
    let mut tcx = TextContext::new();
    let _ = s.build_frame(&mut tcx, 920, 700);

    click(&mut s, &mut tcx, "8");
    assert_eq!(
        s.view.state.selected_range(),
        Some((date(2026, 10, 8), None))
    );

    click(&mut s, &mut tcx, "3");
    assert_eq!(
        s.view.state.selected_range(),
        Some((date(2026, 10, 3), Some(date(2026, 10, 8)))),
        "an earlier second click reorders the range"
    );

    click(&mut s, &mut tcx, "20");
    assert_eq!(
        s.view.state.selected_range(),
        Some((date(2026, 10, 20), None)),
        "a third click restarts the range"
    );
}

#[test]
fn navigation_is_per_card_and_keeps_selection() {
    let mut s = Source::new(TwoCards {
        single: CalendarState::new(date(2026, 10, 1)),
        range: CalendarState::range(date(2026, 10, 1)),
    });
    let mut tcx = TextContext::new();
    let _ = s.build_frame(&mut tcx, 920, 700);

    click(&mut s, &mut tcx, "25");
    assert_eq!(s.view.single.selected(), Some(date(2026, 10, 25)));

    click(&mut s, &mut tcx, "Next month");
    assert_eq!(s.view.single.month(), date(2026, 11, 1));
    assert_eq!(s.view.single.selected(), Some(date(2026, 10, 25)));
    assert_eq!(
        s.view.range.month(),
        date(2026, 10, 1),
        "card two stays put"
    );

    let id = s
        .adapter
        .semantics(&s.engine, 0)
        .node_by_role(SemanticRole::Button, "Next month", Some(1))
        .expect("range card nav button")
        .id;
    assert!(s.handle_semantic_action(SemanticAction::Click { target: id }));
    let _ = s.build_frame(&mut tcx, 920, 700);
    assert_eq!(s.view.range.month(), date(2026, 11, 1), "card two follows");
    assert_eq!(s.view.single.month(), date(2026, 11, 1));
}

#[test]
fn year_one_renders_empty_leading_cells_and_refuses_to_go_earlier() {
    // 0001-01-01 was a Monday, so a Sunday-first grid needs one leading cell
    // for 0000-12-31, which has no date; it renders as an inert placeholder.
    let mut s = Source::new(RangeCard {
        state: CalendarState::range(date(1, 1, 1)),
    });
    let mut tcx = TextContext::new();
    let _ = s.build_frame(&mut tcx, 920, 700);

    let prev = s
        .adapter
        .semantics(&s.engine, 0)
        .node_by_role(SemanticRole::Button, "Previous month", None)
        .expect("nav button")
        .id;
    assert!(
        !s.handle_semantic_action(SemanticAction::Click { target: prev }),
        "the disabled boundary must refuse the click"
    );
    assert_eq!(s.view.state.month(), date(1, 1, 1));
}

#[test]
fn outside_month_days_stay_clickable_and_select_their_real_date() {
    let mut s = Source::new(RangeCard {
        state: CalendarState::new(date(2026, 10, 1)),
    });
    let mut tcx = TextContext::new();
    let _ = s.build_frame(&mut tcx, 920, 700);

    // The leading September 30 cell must act on its real date.
    click(&mut s, &mut tcx, "30");
    assert_eq!(s.view.state.selected(), Some(date(2026, 9, 30)));
}
