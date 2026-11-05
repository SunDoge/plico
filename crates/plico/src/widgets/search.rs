//! Search field ported from MyGO's `ui/search.go`.
use crate::prelude::*;

/// The magnifier that opens a search field (Lucide `search`).
const SEARCH: SvgAsset = SvgAsset::new(
    r#"<svg
  xmlns="http://www.w3.org/2000/svg"
  width="24"
  height="24"
  viewBox="0 0 24 24"
  fill="none"
  stroke="currentColor"
  stroke-width="2"
  stroke-linecap="round"
  stroke-linejoin="round"
>
  <path d="m21 21-4.34-4.34" />
  <circle cx="11" cy="11" r="8" />
</svg>
"#,
);

/// What a [`search_field`] reported this frame.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SearchResponse {
    changed: bool,
    submitted: bool,
}

impl SearchResponse {
    /// The text changed, whether typed, composed or cleared.
    pub const fn changed(self) -> bool {
        self.changed
    }
    /// Enter was pressed in the field.
    pub const fn submitted(self) -> bool {
        self.submitted
    }
}

/// A search field (MyGO `SearchField`): a magnifier, the editable text and a
/// clear button while the text is not empty. The label names the field for
/// assistive technology. `placeholder` shows while the field is empty.
///
/// Clearing reports `changed`, so a filter can run from the same frame; Escape
/// does not clear yet, and the field is a normal text input to assistive
/// technology rather than a dedicated search-box role.
///
/// ```text
/// if search_field(cx, "Search mail", "Search", &mut query).changed() {
///     filter(&query);
/// }
/// ```
pub fn search_field(
    cx: &mut UiContext,
    label: &str,
    placeholder: &str,
    value: &mut String,
) -> SearchResponse {
    let m = cx.theme().metrics;
    let mut changed = false;
    let mut submitted = false;
    row(cx)
        .height(m.control_height as f32)
        .align(Align::Center)
        .gap(m.space(1))
        .padding_x(m.space(2))
        .radius(m.radius as f32)
        .background(Color::Control)
        .border(1., Color::Border)
        .children(|cx| {
            let _ = svg(cx, &SEARCH)
                .size(m.font_size as f32, m.font_size as f32)
                .color(Color::Muted);
            let response = text_input_string(cx, label, value)
                .placeholder(placeholder)
                .background(Color::Control)
                .border(0., Color::Control)
                .width(0.)
                .min_width(0.)
                .grow(1.)
                .height(m.control_height as f32 - 2.)
                .min_height(0.)
                .response();
            changed |= response.changed;
            submitted |= response.submitted;
            if !value.is_empty()
                && button_base(cx, "Clear search")
                    .width(m.space(5) as f32)
                    .height(m.space(5) as f32)
                    .min_height(0.)
                    .padding(0.)
                    .radius(m.radius as f32)
                    .hover_background(Color::ControlHover)
                    .pressed_background(Color::ControlPressed)
                    .children(|cx| {
                        let _ = svg(cx, &crate::icons::X)
                            .size(m.label_size as f32, m.label_size as f32)
                            .color(Color::Muted);
                    })
                    .response
                    .clicked()
            {
                value.clear();
                changed = true;
            }
        });
    SearchResponse { changed, submitted }
}
