//! Radio buttons and their group, ported from MyGO's `ui/base.go` and
//! `ui/widgets.go`.
use crate::WidgetRole;
use crate::prelude::*;

/// A radio button choosing the caller's value (MyGO `Radio`): a dot and a
/// label. The declaration site owns the choice on [`crate::Response::clicked`]
/// and passes whether this radio is the chosen one.
///
/// The button carries the radio-button role and its selected state. MyGO's
/// group is one Tab stop with arrow/Home/End selection; Plico has no
/// per-container focus action wiring yet, so every radio is its own focus
/// stop and Enter/Space activates it.
///
/// ```text
/// radio_group(cx, |cx| {
///     for size in ["Small", "Medium", "Large"] {
///         if radio(cx, size, choice == size).clicked() {
///             choice = size;
///         }
///     }
/// });
/// ```
pub fn radio(cx: &mut UiContext, label: &str, selected: bool) -> crate::Response {
    let m = cx.theme().metrics;
    button_base(cx, label)
        .padding_x(m.space(1))
        .padding_y(m.space(1))
        .gap(m.space(2))
        .radius(m.radius as f32)
        .selected(selected)
        .color(Color::Foreground)
        .hover_background(Color::ControlHover)
        .pressed_background(Color::ControlPressed)
        .semantic_role(WidgetRole::RadioButton)
        .children(|cx| {
            let mut dot = column(cx)
                .width(m.space(4) as f32)
                .height(m.space(4) as f32)
                .radius(m.space(2) as f32)
                .shrink(0.)
                .align(Align::Center)
                .justify(Justify::Center);
            dot = if selected {
                dot.background(Color::Accent)
            } else {
                dot.background(Color::Panel).border(1., Color::Border)
            };
            let _ = dot.children(|cx| {
                if selected {
                    let inner = m.space(2) as f32 * 0.75;
                    let _ = column(cx)
                        .width(inner)
                        .height(inner)
                        .radius(inner / 2.)
                        .background(Color::OnAccent)
                        .children(|_cx| {});
                }
            });
            let _ = text(cx, label).font_size(m.font_size);
        })
        .response
}

/// A column of [`radio`] buttons (MyGO `RadioGroup`) carrying the radio-group
/// role. MyGO makes the group one Tab stop whose arrows choose; Plico cannot
/// wire that yet, so the group is only a semantic container.
pub fn radio_group(cx: &mut UiContext, content: impl FnOnce(&mut UiContext)) {
    let m = cx.theme().metrics;
    column(cx)
        .gap(m.space(1))
        .semantic_role(WidgetRole::RadioGroup)
        .children(|cx| content(cx));
}
