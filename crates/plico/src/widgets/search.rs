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
    navigation: Option<crate::InputNavigation>,
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
    pub(crate) const fn navigation(self) -> Option<crate::InputNavigation> {
        self.navigation
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
    search_field_with(cx, label, placeholder, value, false, false)
}

/// [`search_field`] with a one-shot focus request when the field mounts, for
/// bars that open on demand (MyGO's find bar focuses its field).
pub(crate) fn search_field_with(
    cx: &mut UiContext,
    label: &str,
    placeholder: &str,
    value: &mut String,
    autofocus: bool,
    find_navigation: bool,
) -> SearchResponse {
    let m = cx.theme().metrics;
    let mut changed = false;
    let mut submitted = false;
    let mut navigation = None;
    cx.input_field()
        .align(Align::Center)
        .gap(m.spacing as f32 * 1.5)
        .children(|cx| {
            let _ = svg(cx, &SEARCH)
                .size(m.font_size as f32, m.font_size as f32)
                .color(Color::Muted);
            let response = text_input_string(cx, label, value)
                .placeholder(placeholder)
                .autofocus(autofocus)
                .select_all_on_autofocus(autofocus)
                .find_navigation(find_navigation)
                .unstyled()
                .width(0.)
                .min_width(0.)
                .grow(1.)
                .height(m.font_size as f32 * 1.4)
                .min_height(0.)
                .response();
            changed |= response.changed;
            submitted |= response.submitted;
            navigation = response.navigation;
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
    SearchResponse {
        changed,
        submitted,
        navigation,
    }
}
