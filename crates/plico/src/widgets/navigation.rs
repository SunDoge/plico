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
/// The selected row is the sidebar's one Tab stop. Up/Down/Home/End and
/// buffered prefix typing move to and activate another item; section headers
/// remain pointer/assistive-technology disclosures rather than list choices.
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
        .padding(m.spacing as f32 * 2.5)
        .gap(m.spacing as f32 * 0.5)
        .background(Color::Control)
        .clip(true)
        .scroll_y(true)
        .semantic_role(WidgetRole::Tree)
        .selection_follows_focus(true)
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
                    .tab_stop(false)
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
/// single-line label. The chosen row is gray outside the sidebar's focus scope
/// and uses the accent face while its scope holds focus, following MyGO.
/// It carries the tree-item role and its selected state; the
/// declaration site owns the choice on [`crate::Response::clicked`].
pub fn sidebar_item(
    cx: &mut UiContext,
    label: &str,
    selected: bool,
    icon: Option<&crate::SvgAsset>,
) -> crate::Response {
    let m = cx.theme().metrics;
    super::nav_entry(cx, label, selected)
        .min_width(0.)
        .semantic_role(WidgetRole::TreeItem)
        .children(|cx| {
            if let Some(asset) = icon {
                let glyph = svg(cx, asset).size(16., 16.);
                if !selected {
                    glyph.color(Color::Accent);
                }
            }
            let _ = text(cx, label)
                .font_size(m.font_size)
                .weight(400)
                .ellipsis(true)
                .min_width(0.)
                .grow(1.);
        })
        .response
}

/// A text link (MyGO `Link`): accent-colored text with the link role that
/// reports clicks and keyboard activation. The caller decides what the click
/// does; Plico core cannot open URLs, so no `url` parameter is taken.
///
/// MyGO underlines the link while hovered; Plico's text style has no
/// underline, so the accent color and hover background carry the affordance.
pub fn link(cx: &mut UiContext, label: &str) -> crate::Response {
    let m = cx.theme().metrics;
    button_base(cx, label)
        .padding_x(m.space(1))
        .padding_y(m.space(1))
        .radius(m.radius as f32)
        .color(Color::Accent)
        .hover_background(Color::ControlHover)
        .pressed_background(Color::ControlPressed)
        .semantic_role(WidgetRole::Link)
        .children(|cx| {
            let _ = text(cx, label).font_size(m.font_size).color(Color::Accent);
        })
        .response
}

/// A "Back" button for a [`crate::Router`] (MyGO `BackButton`), disabled at
/// the oldest history entry. Clicking it goes back.
pub fn back_button<R: Copy + Eq>(
    cx: &mut UiContext,
    router: &mut crate::Router<R>,
) -> crate::Response {
    let response = button(cx)
        .icon(&icons::CHEVRON_LEFT)
        .aria_label("Back")
        .ghost()
        .disabled(!router.can_back())
        .response();
    if response.clicked() {
        router.back();
    }
    response
}

/// A "Forward" button for a [`crate::Router`] (MyGO `ForwardButton`), disabled
/// at the newest history entry. Clicking it goes forward.
pub fn forward_button<R: Copy + Eq>(
    cx: &mut UiContext,
    router: &mut crate::Router<R>,
) -> crate::Response {
    let response = button(cx)
        .icon(&icons::CHEVRON_RIGHT)
        .aria_label("Forward")
        .ghost()
        .disabled(!router.can_forward())
        .response();
    if response.clicked() {
        router.forward();
    }
    response
}
