//! A find bar ported from MyGO's `ui/feedback.go` (`FindBar`).
use super::search::search_field_with;
use crate::WidgetRole;
use crate::icons;
use crate::prelude::*;

/// What a [`find_bar`] did this frame.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FindResponse {
    changed: bool,
}

impl FindResponse {
    /// The current match moved by a button or by Enter in the field.
    pub const fn changed(self) -> bool {
        self.changed
    }
}

/// A find bar (MyGO `FindBar`) while `open`: a search field over the query, the
/// "N of M matches" status, previous/next buttons and Done. The application
/// owns the query and counts the matches; `current` is clamped to them and
/// moves round the ends.
///
/// Enter in the field goes to the next match, and opening the bar focuses the
/// field. Escape in the field does not close the bar yet (Plico does not route
/// it out of the text input), and Cmd+G / F3 and closing from the keyboard are
/// not wired; Done closes.
///
/// ```text
/// if find_bar(cx, &mut finding, &mut query, matches.len(), &mut current).changed() {
///     show(matches[current]);
/// }
/// ```
pub fn find_bar(
    cx: &mut UiContext,
    open: &mut bool,
    query: &mut String,
    matches: usize,
    current: &mut usize,
) -> FindResponse {
    if !*open {
        return FindResponse::default();
    }
    let m = cx.theme().metrics;
    *current = if matches == 0 {
        0
    } else {
        (*current).min(matches - 1)
    };
    let mut changed = false;
    let mut closed = false;
    row(cx)
        .padding_x(m.space(3))
        .padding_y(m.space(1))
        .gap(m.space(2))
        .align(Align::Center)
        .width_percent(100.)
        .background(Color::Surface)
        .border(1., Color::Border)
        .semantic_role(WidgetRole::Toolbar)
        .children(|cx| {
            let field = column(cx).grow(1.).max_width(360.);
            let response = field
                .children(|cx| search_field_with(cx, "Find", "Find", query, true))
                .inner;
            if response.submitted() && matches > 0 {
                *current = (*current + 1) % matches;
                changed = true;
            }
            let status = if query.is_empty() {
                None
            } else if matches == 0 {
                Some(cx.tr("find-no-matches", "No matches").into_owned())
            } else {
                Some(format!("{} of {matches}", *current + 1))
            };
            if let Some(status) = status {
                let _ = text(cx, status).font_size(m.label_size).color(Color::Muted);
            }
            let can = matches > 0;
            if button(cx)
                .icon(&icons::CHEVRON_LEFT)
                .aria_label("Previous match")
                .ghost()
                .disabled(!can)
                .clicked()
                && can
            {
                *current = (*current + matches - 1) % matches;
                changed = true;
            }
            if button(cx)
                .icon(&icons::CHEVRON_RIGHT)
                .aria_label("Next match")
                .ghost()
                .disabled(!can)
                .clicked()
                && can
            {
                *current = (*current + 1) % matches;
                changed = true;
            }
            let done = cx.tr("find-done", "Done").into_owned();
            if button(cx).text(done).ghost().clicked() {
                closed = true;
            }
        });
    if closed {
        *open = false;
    }
    FindResponse { changed }
}
