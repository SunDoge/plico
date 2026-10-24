//! Application components use public context-first constructors, like any widget library.
use plico_core::widgets::{button, button_base, column, icon, row, text};
use plico_core::{Align, Color, Response, SvgAsset, UiContext, component};

#[component]
#[rustfmt::skip]
pub fn project_tile(cx: &mut UiContext, name: &str, description: &str, selected: bool) -> Response {
    let m = cx.theme().metrics;
    button_base(cx, name).width(0.).grow(1.).min_width(100.).height(104.).padding(m.space(3)).radius(m.radius as f32)
        .align(Align::Start).selected(selected).background(if selected { Color::Selection } else { Color::Panel })
        .border(
        1.,
        if selected { Color::Accent } else { Color::Border },
    ).hover_background(Color::ControlHover).pressed_background(Color::ControlPressed).transition(m.interaction_ms)
        .children(|cx| {
        column(cx).gap(m.space(2)).children(|cx| {
            row(cx).gap(m.spacing) + |cx| {
                icon(cx, &crate::icons::FOLDER).size(16., 16.)
                    .color(if selected { Color::Accent } else { Color::Muted });
                text(cx, name).font_size(m.font_size).weight(650)
                    .color(if selected { Color::Accent } else { Color::Foreground });
            };
            text(cx, description).font_size(m.label_size).color(Color::Muted);
            text(cx, if selected { "Selected" } else { "Open project" }).font_size(m.label_size)
                .color(Color::Muted);
        })
    }).response
}

/// An application component; SVG inherits the button's foreground color.
#[component]
pub fn icon_button(
    cx: &mut UiContext,
    label: &str,
    image: &SvgAsset,
    enabled: bool,
    primary: bool,
) -> Response {
    let control = button(cx).icon(image).text(label).disabled(!enabled);
    let control = if primary { control.primary() } else { control };
    control.response()
}
