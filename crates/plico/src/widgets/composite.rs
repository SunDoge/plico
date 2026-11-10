//! Composed controls use the same public API as application components.
use crate::{Key, Layout, icons, prelude::*};

/// A noninteractive status label. Color follows the current theme.
#[component]
pub fn badge(cx: &mut UiContext, label: &str) {
    let m = cx.theme().metrics;
    text(cx, label)
        .font_size(m.font_size.saturating_sub(2))
        .padding_x(m.spacing as f32 * 1.5)
        .padding_y(0)
        .radius(999.)
        .corner_smoothing(0.)
        .shrink(0.)
        .background(Color::Foreground)
        .background_alpha(0.1);
}

/// A titled content panel: the card container repeated across applications
/// (sidebar sections, metric groups, inspector cards). Padding, gap, radius
/// and colors derive from the current theme metrics.
pub struct Panel<'u, 'e> {
    layout: Layout<'u, 'e>,
    title: Option<&'u str>,
    description: Option<&'u str>,
    font_size: u16,
}
/// Start a panel with an optional title row above the content.
pub fn panel<'u, 'e>(
    cx: &'u mut UiContext<'e>,
    title: impl Into<Option<&'u str>>,
) -> Panel<'u, 'e> {
    let m = cx.theme().metrics;
    Panel {
        layout: column(cx)
            .padding(m.space(4))
            .gap(m.space(3))
            .radius(m.radius as f32 + 4.)
            .background(Color::Panel)
            .border(1., Color::Border),
        title: title.into(),
        description: None,
        font_size: m.font_size,
    }
}
impl<'u> Panel<'u, '_> {
    /// Supporting text below the title, using the current theme's body size.
    pub fn description(mut self, description: &'u str) -> Self {
        self.description = Some(description);
        self
    }
}
impl crate::ImplicitKey for Panel<'_, '_> {
    fn key_of(&self) -> Key {
        self.layout.key_of()
    }
    fn set_key(mut self, key: Key) -> Self {
        self.layout = self.layout.set_key(key);
        self
    }
}
impl Panel<'_, '_> {
    /// Override the derived inner padding, in theme space steps.
    pub fn padding(mut self, padding: u16) -> Self {
        self.layout = self.layout.padding(padding);
        self
    }
    /// Panel content under the optional title. The closure's return value is
    /// preserved.
    pub fn children<R>(self, content: impl FnOnce(&mut UiContext) -> R) -> InnerResponse<R> {
        let title = self.title;
        let font_size = self.font_size;
        let description = self.description;
        self.layout.children(|cx| {
            if let Some(title) = title {
                text(cx, title).font_size(font_size).weight(650);
            }
            if let Some(description) = description {
                text(cx, description)
                    .font_size(font_size)
                    .color(Color::Muted);
            }
            content(cx)
        })
    }
}
/// Native `+ |cx| { ... }` children spelling for panels.
impl<F, R> std::ops::Add<F> for Panel<'_, '_>
where
    F: FnOnce(&mut UiContext) -> R,
{
    type Output = InnerResponse<R>;
    fn add(self, build: F) -> Self::Output {
        self.children(build)
    }
}

/// A labeled statistic: small muted label, large value, optional muted
/// caption. The card shape repeated across dashboards.
#[component]
pub fn stat(cx: &mut UiContext, label: &str, value: &str, caption: Option<&str>) {
    let m = cx.theme().metrics;
    let _ = panel(cx, None).padding(m.space(3)).children(|cx| {
        text(cx, label)
            .font_size(m.label_size)
            .weight(600)
            .color(Color::Muted);
        text(cx, value).font_size(m.font_size + 12).weight(700);
        if let Some(caption) = caption {
            text(cx, caption)
                .font_size(m.label_size)
                .color(Color::Muted);
        }
    });
}

/// A navigation entry with framework-owned selection, hover and press motion.
/// Add custom children (icons, counts or badges) and disable it like a button.
pub fn nav_entry<'u, 'e, 't>(
    cx: &'u mut UiContext<'e>,
    label: impl Into<std::borrow::Cow<'t, str>>,
    selected: bool,
) -> crate::ButtonBase<'u, 'e, 't> {
    let m = cx.theme().metrics;
    // Sections are composition containers, not separate sidebar focus scopes.
    let mut owner = cx.parent;
    let mut cursor = cx.parent;
    while let Some(node) = cx.engine.nodes.get(cursor) {
        if node.style.role == Some(crate::WidgetRole::Tree) {
            owner = cursor;
            break;
        }
        if cursor == cx.engine.root {
            break;
        }
        cursor = node.identity.parent;
    }
    let focused = cx
        .engine
        .nodes
        .get(owner)
        .is_some_and(|node| cx.engine.focus_within(node.identity));
    let control = button_base(cx, label)
        .width_percent(100.)
        .padding_x(m.space(2))
        .padding_y(m.spacing as f32 * 1.5)
        .min_height(m.control_height as f32)
        .radius(m.radius as f32)
        .selected(selected)
        .transition(m.interaction_ms)
        .gap(m.space(2));
    if selected {
        let face = if focused {
            Color::Accent
        } else {
            Color::ControlPressed
        };
        control
            .background(face)
            .hover_background(face)
            .pressed_background(face)
            .color(if focused {
                Color::OnAccent
            } else {
                Color::Foreground
            })
    } else {
        control
            .hover_background(Color::ControlHover)
            .pressed_background(Color::ControlPressed)
            .color(Color::Foreground)
    }
}

/// A sidebar navigation entry bound to a [`crate::Router`]-owned route.
/// Selected state colors follow the theme; the icon is optional. The
/// declaration site owns the navigation call on [`Response::clicked`].
#[component]
pub fn nav_item(
    cx: &mut UiContext,
    label: &str,
    selected: bool,
    icon: Option<&crate::SvgAsset>,
) -> Response {
    let m = cx.theme().metrics;
    nav_entry(cx, label, selected)
        .children(|cx| {
            if let Some(icon) = icon {
                crate::widgets::icon(cx, icon);
            }
            text(cx, label)
                .width(0.)
                .grow(1.)
                .min_width(0.)
                .weight(if selected { 600 } else { 400 })
                .font_size(m.font_size)
                .ellipsis(true);
        })
        .response
}

/// Render one sidebar entry per route, in declaration order. Each entry is
/// keyed by the route's stable ID, shows its label and optional icon, and
/// navigates on click; the current route renders selected. Routes enumerate
/// themselves via `#[derive(Route)]`'s `ALL`, so adding a variant updates the
/// menu, the IDs and the labels in one place.
///
/// ```ignore
/// #[derive(Clone, Copy, Route)]
/// enum Page { Workspace, Settings }
/// nav_menu(ui, &mut self.router);
/// ```
#[component]
pub fn nav_menu<Route: crate::Route + Copy + Eq>(
    cx: &mut UiContext,
    router: &mut crate::Router<Route>,
) {
    let routes = router.targets();
    for &route in routes {
        cx.keyed(route.id(), |cx| {
            let label = cx.tr(route.id(), route.label());
            if nav_item(cx, &label, router.show(route), route.icon()).clicked() {
                router.go(route);
            }
        });
    }
}

/// A bordered box of collapsible sections (MyGO `Accordion`): each item opens
/// and closes on its own, and `collapsible` is the item (MyGO `AccordionItem`).
///
/// Headers retain ordinary Tab/Enter/Space behavior. While one is focused,
/// Up/Down/Home/End traverse the accordion's visible headers and clamp at its
/// ends, matching MyGO's group keyboard handling.
///
/// ```text
/// accordion(cx, |cx| {
///     collapsible(cx, "General", general_open, |cx| general(cx));
///     collapsible(cx, "Privacy", privacy_open, |cx| privacy(cx));
/// });
/// ```
pub fn accordion(cx: &mut UiContext, content: impl FnOnce(&mut UiContext)) {
    let m = cx.theme().metrics;
    column(cx)
        .width_percent(100.)
        .focus_group(FocusAxis::Vertical)
        .accordion_items()
        .radius(m.radius as f32)
        .clip(true)
        .border(1., Color::Border)
        .children(|cx| content(cx));
}

/// A horizontal divider that stretches to the available width.
#[component]
pub fn separator(cx: &mut UiContext) {
    let _ = column(cx)
        .height(1.)
        .width_percent(100.)
        .shrink(0.)
        .background(Color::Border)
        .children(|_cx| {});
}

/// A selectable button bound to application-owned state. Repeat in a keyed list
/// for dynamic choices. This is a button with selected state, not a radio role.
#[component]
pub fn choice<T: PartialEq>(
    cx: &mut UiContext,
    label: &str,
    selected: &mut T,
    value: T,
) -> Response {
    let m = cx.theme().metrics;
    let on = *selected == value;
    let response = button_base(cx, label)
        .selected(on)
        .min_height(m.control_height as f32)
        .padding_x(m.space(3))
        .padding_y(m.space(1))
        .radius(m.radius as f32)
        .background(if on { Color::Selection } else { Color::Control })
        .color(if on { Color::Accent } else { Color::Foreground })
        .motion(Motion::Quick)
        .align(Align::Center)
        .justify(Justify::Center)
        .children(|cx| {
            text(cx, label).weight(if on { 600 } else { 400 });
        })
        .response;
    if response.clicked() {
        *selected = value;
    }
    response
}

/// A wrapping group of selectable buttons. Labels and their order must stay
/// fixed; for dynamic data use keyed `choice` calls. Empty lists preserve state,
/// and an out-of-range index shows no selection until a button is activated.
/// Every button is a Tab stop; Enter/Space select it. This is not a tabs widget.
#[component]
pub fn segmented(cx: &mut UiContext, selected: &mut usize, labels: &[&str]) -> bool {
    let m = cx.theme().metrics;
    let before = *selected;
    row(cx)
        .padding(m.spacing as f32 * 0.5)
        .gap(m.spacing as f32 * 0.5)
        .radius(m.radius as f32)
        .background(Color::Control)
        .border(1., Color::Border)
        .children(|cx| {
            for (index, label) in labels.iter().enumerate() {
                cx.keyed(index as u64, |cx| {
                    if button_base(cx, *label)
                        .segment(*selected == index)
                        .children(|cx| {
                            text(cx, *label);
                        })
                        .response
                        .clicked()
                    {
                        *selected = index;
                    }
                });
            }
        });
    before != *selected
}

/// A disclosure with application-owned open state. The callback runs only while
/// open; its local application state should live outside the conditional panel.
/// Content insertion/removal is immediate; height reveal animation is not implied.
#[component]
pub fn collapsible<R>(
    cx: &mut UiContext,
    label: &str,
    open: &mut bool,
    content: impl FnOnce(&mut UiContext) -> R,
) -> Option<R> {
    let m = cx.theme().metrics;
    let accordion = cx.accordion_items;
    let separated = accordion && cx.accordion_index > 0;
    if accordion {
        cx.accordion_index += 1;
    }
    column(cx)
        .children(|cx| {
            if separated {
                column(cx)
                    .width_percent(100.)
                    .height(1.)
                    .background(Color::Border)
                    .children(|_cx| {});
            }
            let trigger = button_base(cx, label)
                .expanded(*open)
                .min_height(0.)
                .gap(m.space(if accordion { 2 } else { 1 }))
                .padding_y(m.spacing as f32 * if accordion { 2.5 } else { 1. })
                .padding_left(m.spacing as f32 * if accordion { 3. } else { 0.5 })
                .padding_right(m.space(if accordion { 3 } else { 2 }))
                .align_self(if accordion {
                    Align::Stretch
                } else {
                    Align::Start
                })
                .radius(if accordion { 0. } else { m.radius as f32 })
                .hover_background(Color::ControlHover)
                .pressed_background(Color::ControlPressed)
                .align(Align::Center)
                .motion(Motion::Quick);
            let response = trigger
                .children(|cx| {
                    if !accordion {
                        icon(
                            cx,
                            if *open {
                                &icons::CHEVRON_DOWN
                            } else {
                                &icons::CHEVRON_RIGHT
                            },
                        )
                        .size(14., 14.);
                    }
                    let title = text(cx, label).weight(if accordion { 500 } else { 400 });
                    if accordion {
                        title.grow(1.);
                        icon(
                            cx,
                            if *open {
                                &super::indicators::CHEVRON_UP
                            } else {
                                &icons::CHEVRON_DOWN
                            },
                        )
                        .size(14., 14.);
                    }
                })
                .response;
            if response.clicked() {
                *open = !*open;
            }
            if *open {
                Some(
                    column(cx)
                        .padding_top(if accordion { 0. } else { m.space(1) as f32 })
                        .padding_bottom(m.space(if accordion { 3 } else { 1 }))
                        .padding_left(m.spacing as f32 * if accordion { 3. } else { 5.5 })
                        .padding_right(if accordion { m.space(3) as f32 } else { 0. })
                        .gap(m.space(2))
                        .children(content)
                        .inner,
                )
            } else {
                None
            }
        })
        .inner
}

/// A labeled determinate meter. Values are fractions in 0..=1; non-finite
/// values become zero and finite values are clamped. The percentage is visible
/// text; this composed version does not yet expose a native progress-bar role.
#[component]
pub fn progress(cx: &mut UiContext, label: &str, fraction: f32) {
    let fraction = if fraction.is_finite() {
        fraction.clamp(0., 1.)
    } else {
        0.
    };
    let m = cx.theme().metrics;
    let _ = column(cx).gap(m.space(1)).children(|cx| {
        row(cx).gap(m.space(2)).wrap(true).children(|cx| {
            text(cx, label).font_size(m.label_size).grow(1.);
            text!(cx, "{:.0}%", fraction * 100.)
                .font_size(m.label_size)
                .color(Color::Muted);
        });
        row(cx)
            .height(m.spacing.max(1) as f32 * 1.5)
            .width_percent(100.)
            .radius(m.spacing as f32 * 0.75)
            .corner_smoothing(0.)
            .background(Color::Border)
            .children(|cx| {
                let _ = column(cx)
                    .width_percent(fraction * 100.)
                    .height_percent(100.)
                    .radius(m.spacing as f32 * 0.75)
                    .corner_smoothing(0.)
                    .background(Color::Accent)
                    .children(|_cx| {});
            })
    });
}
