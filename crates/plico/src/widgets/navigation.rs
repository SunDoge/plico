//! Navigation paths ported from MyGO's `ui` package.
use crate::WidgetRole;
use crate::icons;
use crate::prelude::*;

/// A path of items separated by chevrons (MyGO `Breadcrumbs`): every item
/// but the last is a link, and a click sets `chosen` to its index. The last
/// item is where the user is, so it is plain text rather than a link.
/// Returns whether the choice changed this frame.
///
/// The row carries the navigation semantic role, and links carry the link
/// role, so assistive technology can jump to the trail. MyGO also marks the
/// last item as the current page and shortens long items with an ellipsis;
/// Plico has no current-page state yet and lets long items take their room.
///
/// ```text
/// let path = ["Macintosh HD", "Users", "ada", "Documents"];
/// if breadcrumbs(cx, &path, &mut chosen) {
///     open(&path[..=chosen]);
/// }
/// ```
pub fn breadcrumbs(cx: &mut UiContext, items: &[&str], chosen: &mut usize) -> bool {
    if items.is_empty() {
        return false;
    }
    let before = *chosen;
    *chosen = before.min(items.len() - 1);
    let m = cx.theme().metrics;
    row(cx)
        .gap(m.space(1))
        .align(Align::Center)
        .semantic_role(WidgetRole::Navigation)
        .children(|cx| {
            for (index, item) in items.iter().enumerate() {
                if index > 0 {
                    let _ = svg(cx, &icons::CHEVRON_RIGHT)
                        .size(m.space(2) as f32, m.space(2) as f32)
                        .color(Color::Muted);
                }
                if index + 1 == items.len() {
                    let _ = text(cx, *item)
                        .font_size(m.font_size)
                        .weight(500)
                        .color(Color::Foreground)
                        .ellipsis(true);
                    continue;
                }
                let response = button_base(cx, *item)
                    .padding_x(m.space(2))
                    .padding_y(m.space(1))
                    .radius(m.radius as f32)
                    .color(Color::Muted)
                    .hover_background(Color::ControlHover)
                    .pressed_background(Color::ControlPressed)
                    .semantic_role(WidgetRole::Link)
                    .max_width(200.)
                    .children(|cx| {
                        let _ = text(cx, *item).font_size(m.font_size).ellipsis(true);
                    })
                    .response;
                if response.clicked() {
                    *chosen = index;
                }
            }
        });
    *chosen != before
}
