//! Composed controls use the same public API as application components.
use crate::{icons, prelude::*};

/// A noninteractive status label. Color follows the current theme.
#[component]
pub fn badge(cx: &mut UiContext, label: &str) {
    let m = cx.theme().metrics;
    text(cx, label)
        .font_size(m.label_size)
        .weight(600)
        .padding(m.space(1))
        .radius(m.radius as f32)
        .background(Color::Selection)
        .color(Color::Accent);
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
    row(cx).gap(m.space(1)).wrap(true).children(|cx| {
        for (index, label) in labels.iter().enumerate() {
            cx.keyed(index as u64, |cx| {
                choice(cx, label, selected, index);
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
    column(cx)
        .gap(m.space(2))
        .children(|cx| {
            let response = button_base(cx, label)
                .expanded(*open)
                .min_height(m.control_height as f32)
                .padding_x(m.space(2))
                .padding_y(m.space(1))
                .gap(m.space(2))
                .radius(m.radius as f32)
                .background(Color::Control)
                .align(Align::Center)
                .motion(Motion::Quick)
                .children(|cx| {
                    icon(
                        cx,
                        if *open {
                            &icons::CHEVRON_DOWN
                        } else {
                            &icons::CHEVRON_RIGHT
                        },
                    );
                    text(cx, label).weight(600);
                })
                .response;
            if response.clicked() {
                *open = !*open;
            }
            if *open {
                Some(
                    column(cx)
                        .padding(m.space(2))
                        .gap(m.space(2))
                        .children(|cx| content(cx))
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
            .height(m.spacing.max(1) as f32 * 2.)
            .width_percent(100.)
            .radius(m.radius as f32)
            .background(Color::Control)
            .children(|cx| {
                let _ = column(cx)
                    .width_percent(fraction * 100.)
                    .height_percent(100.)
                    .radius(m.radius as f32)
                    .background(Color::Accent)
                    .children(|_cx| {});
            })
    });
}
