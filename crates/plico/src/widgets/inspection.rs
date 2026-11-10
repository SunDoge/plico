//! Renderer-independent inspection controls. Rows are buttons, so keyboard/focus and
//! disabled behavior use the normal control contract; this is not an ARIA grid.
use crate::{Response, prelude::*};

#[derive(Clone, Copy, Debug)]
pub struct TableColumn<'a> {
    pub label: &'a str,
    pub weight: f32,
}
impl<'a> TableColumn<'a> {
    pub const fn new(label: &'a str, weight: f32) -> Self {
        Self { label, weight }
    }
    fn flex(self) -> f32 {
        if self.weight.is_finite() && self.weight > 0. {
            self.weight.clamp(0.1, 100.)
        } else {
            1.
        }
    }
}
/// Matching header/cell geometry for a bounded, application-owned list of rows.
#[component]
pub fn table_header(cx: &mut UiContext, columns: &[TableColumn<'_>]) {
    let m = cx.theme().metrics;
    row(cx)
        .min_height(m.space(8) as f32)
        .width_percent(100.)
        .gap(0)
        .padding_x(m.spacing as f32 * 2.5)
        .padding_y(m.spacing as f32 * 1.5)
        .background(Color::Control)
        .children(|cx| {
            for (i, c) in columns.iter().enumerate() {
                cx.keyed(i as u64, |cx| {
                    text(cx, c.label)
                        .width(0.)
                        .grow(c.flex())
                        .min_width(0.)
                        .font_size(m.font_size)
                        .weight(600)
                        .color(Color::Muted)
                        .ellipsis(true);
                });
            }
        });
}
/// Cells borrow their text; row identity belongs to the caller's business-key scope.
/// Missing cells remain empty; extra cells are ignored, preserving header geometry.
#[component]
pub fn table_row(
    cx: &mut UiContext,
    label: &str,
    selected: bool,
    columns: &[TableColumn<'_>],
    cells: &[&str],
) -> Response {
    let m = cx.theme().metrics;
    button_base(cx, label)
        .width_percent(100.)
        .min_height(m.space(8) as f32)
        .padding_x(m.spacing as f32 * 2.5)
        .padding_y(m.spacing as f32 * 1.5)
        .gap(0)
        .selected(selected)
        .color(if selected {
            Color::OnAccent
        } else {
            Color::Foreground
        })
        .background(if selected {
            Color::Accent
        } else {
            Color::Panel
        })
        .hover_background(if selected {
            Color::Accent
        } else {
            Color::ControlHover
        })
        .pressed_background(if selected {
            Color::Accent
        } else {
            Color::ControlPressed
        })
        .children(|cx| {
            for (i, c) in columns.iter().enumerate() {
                cx.keyed(i as u64, |cx| {
                    text(cx, cells.get(i).copied().unwrap_or(""))
                        .width(0.)
                        .grow(c.flex())
                        .min_width(0.)
                        .font_size(m.font_size)
                        .ellipsis(true);
                });
            }
        })
        .response
}
/// Vertical label/value pair for inspectors. Values wrap and remain selectable;
/// long identifiers are never shortened by the widget.
#[component]
pub fn property_field(cx: &mut UiContext, label: &str, value: &str) {
    column(cx)
        .selectable(true)
        .min_width(0.)
        .width_percent(100.)
        .gap(5)
        .children(|cx| {
            text(cx, label)
                .font_size(12)
                .weight(600)
                .color(Color::Muted);
            text(cx, if value.is_empty() { "—" } else { value })
                .font_size(13)
                .width_percent(100.);
        });
}
