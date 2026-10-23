//! Application components use public context-first constructors, like any widget library.
use plico_core::widgets::{button, button_base, column, icon, row, text};
use plico_core::{Align, Color, InnerResponse, Layout, Response, SvgAsset, UiContext, component};

#[component]
pub fn nav_item(cx: &mut UiContext, label: &str, selected: bool, image: &SvgAsset) -> Response {
    let m = cx.theme().metrics;
    button_base(cx, label)
        .width(140.)
        .padding_x(m.space(2))
        .padding_y(m.space(1))
        .height(m.control_height as f32 + 4.)
        .radius(m.radius as f32)
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
            Color::Muted
        })
        .selected(selected)
        .transition(m.interaction_ms)
        .gap(m.spacing)
        .children(|cx| {
            icon(cx, image).size(14., 14.);
            text(cx, label)
                .weight(600)
                .font_size(m.font_size.saturating_sub(2));
        })
        .response
}

pub struct Panel<'u, 'e> {
    layout: Layout<'u, 'e>,
    title: &'u str,
    font_size: u16,
}
/// A custom container is a function returning its own configurable builder.
pub fn panel<'u, 'e>(cx: &'u mut UiContext<'e>, title: &'u str) -> Panel<'u, 'e> {
    let m = cx.theme().metrics;
    Panel {
        layout: column(cx)
            .padding(m.space(4))
            .gap(m.space(3))
            .radius(m.radius as f32 + 4.)
            .background(Color::Panel)
            .border(1., Color::Border),
        title,
        font_size: m.font_size,
    }
}
impl Panel<'_, '_> {
    #[doc(hidden)]
    pub fn __implicit_key(mut self, site: u64) -> Self {
        self.layout = self.layout.__implicit_key(site);
        self
    }
    pub fn padding(mut self, padding: u16) -> Self {
        self.layout = self.layout.padding(padding);
        self
    }
    pub fn children<R>(self, content: impl FnOnce(&mut UiContext) -> R) -> InnerResponse<R> {
        self.layout.children(|cx| {
            text(cx, self.title).font_size(self.font_size).weight(650);
            content(cx)
        })
    }
}

impl<F, R> std::ops::Add<F> for Panel<'_, '_>
where
    F: FnOnce(&mut UiContext) -> R,
{
    type Output = InnerResponse<R>;
    fn add(self, content: F) -> Self::Output {
        self.children(content)
    }
}

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
