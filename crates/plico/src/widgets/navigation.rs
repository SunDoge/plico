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

/// A source-list sidebar (MyGO `Sidebar`): a scrolling column with the tree
/// role. Give it a width and a bounded height, then declare sections and
/// items inside it; the application owns the chosen item.
///
/// MyGO's sidebar is one Tab stop that handles Up/Down/Home/End and
/// type-selection; Plico has no per-container keyboard action wiring yet, so
/// every item and section header is its own focus stop and the pointer
/// chooses.
///
/// ```text
/// sidebar(cx).width(220.).height(320.) + |cx| {
///     sidebar_section(cx, "Mailboxes", Some(&mut open), |cx| {
///         if sidebar_item(cx, "Inbox", mailbox == "inbox", None).clicked() {
///             mailbox = "inbox".into();
///         }
///     });
/// };
/// ```
pub fn sidebar<'u, 'e>(cx: &'u mut UiContext<'e>) -> crate::Layout<'u, 'e> {
    let m = cx.theme().metrics;
    column(cx)
        .padding(m.space(2))
        .gap(m.space(1))
        .background(Color::Panel)
        .clip(true)
        .scroll_y(true)
        .semantic_role(WidgetRole::Tree)
}

/// A section of a [`sidebar`] (MyGO `SidebarSection`): a title that shows and
/// hides the items `content` declares. `open` is the application's disclosed
/// state; `None` keeps the section open and makes the title plain text.
///
/// The title carries the tree-item role and its expanded state; clicking it
/// (or pressing Enter/Space) toggles `open`.
pub fn sidebar_section(
    cx: &mut UiContext,
    title: &str,
    open: Option<&mut bool>,
    content: impl FnOnce(&mut UiContext),
) {
    let m = cx.theme().metrics;
    let mut open = open;
    column(cx).gap(m.space(1)).children(|cx| {
        match open.as_deref_mut() {
            Some(open) => {
                let response = button_base(cx, title)
                    .width_percent(100.)
                    .height(m.space(7) as f32)
                    .min_width(0.)
                    .min_height(0.)
                    .padding_x(m.space(2))
                    .gap(m.space(1))
                    .radius(m.radius as f32)
                    .hover_background(Color::ControlHover)
                    .pressed_background(Color::ControlPressed)
                    .expanded(*open)
                    .semantic_role(WidgetRole::TreeItem)
                    .children(|cx| {
                        let _ = text(cx, title)
                            .font_size(m.label_size)
                            .weight(600)
                            .color(Color::Muted)
                            .grow(1.)
                            .min_width(0.)
                            .ellipsis(true);
                        let _ = svg(
                            cx,
                            if *open {
                                &icons::CHEVRON_DOWN
                            } else {
                                &icons::CHEVRON_RIGHT
                            },
                        )
                        .size(m.label_size as f32, m.label_size as f32)
                        .color(Color::Muted);
                    })
                    .response;
                if response.clicked() {
                    *open = !*open;
                    cx.request_rebuild();
                }
            }
            None => {
                let _ = text(cx, title)
                    .font_size(m.label_size)
                    .weight(600)
                    .color(Color::Muted)
                    .padding_x(m.space(2))
                    .padding_y(m.space(1));
            }
        }
        if open.as_deref().copied().unwrap_or(true) {
            content(cx);
        }
    });
}

/// One item of a [`sidebar`] (MyGO `SidebarItem`): an icon (optional) and a
/// single-line label, selected with the theme's selection background and the
/// accent color. It carries the tree-item role and its selected state; the
/// declaration site owns the choice on [`crate::Response::clicked`].
pub fn sidebar_item(
    cx: &mut UiContext,
    label: &str,
    selected: bool,
    icon: Option<&crate::SvgAsset>,
) -> crate::Response {
    let m = cx.theme().metrics;
    button_base(cx, label)
        .width_percent(100.)
        .height(m.space(7) as f32)
        .min_width(0.)
        .min_height(0.)
        .padding_x(m.space(2))
        .gap(m.space(2))
        .radius(m.radius as f32)
        .selected(selected)
        .background(if selected {
            Color::Selection
        } else {
            Color::Panel
        })
        .hover_background(Color::ControlHover)
        .pressed_background(Color::ControlPressed)
        .color(if selected {
            Color::Accent
        } else {
            Color::Foreground
        })
        .transition(m.interaction_ms)
        .semantic_role(WidgetRole::TreeItem)
        .children(|cx| {
            if let Some(asset) = icon {
                let _ = svg(cx, asset).size(16., 16.);
            }
            let _ = text(cx, label)
                .font_size(m.font_size)
                .weight(if selected { 600 } else { 400 })
                .ellipsis(true)
                .min_width(0.)
                .grow(1.);
        })
        .response
}
