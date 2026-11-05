//! Form layout ported from MyGO's `ui/form.go`: a column of fields, each with
//! a label above its control and optional description/error texts below.
//!
//! MyGO's two-column `Form` aligns labels to the widest one; Plico's immediate
//! API has no intrinsic measuring pass for a widget, so the port stacks labels
//! above controls (MyGO's standalone `Field`) and leaves the label-anchored
//! macOS form out.
use crate::prelude::*;

/// A column of fields (MyGO `Field` outside a `Form`): the label, the control
/// the content callback builds, and optional description/error texts below it.
///
/// The error text uses the theme's [`Color::Danger`]; the control itself is not
/// marked invalid, since Plico cannot reach a sibling control's style from the
/// field. A click on the label does not focus the control (no focus action
/// wiring yet).
///
/// ```text
/// field(cx, "Email").description("For receipts.") + |cx| {
///     text_input_string(cx, "Email", &mut email);
/// };
/// ```
pub struct Field<'u, 'e, 't> {
    layout: crate::Layout<'u, 'e>,
    label: &'t str,
    description: Option<&'t str>,
    error: Option<&'t str>,
    font_size: u16,
    label_size: u16,
}

/// Start a field labelled `label`, with the control declared in its content.
pub fn field<'u, 'e, 't>(cx: &'u mut UiContext<'e>, label: &'t str) -> Field<'u, 'e, 't> {
    let m = cx.theme().metrics;
    Field {
        layout: column(cx).gap(m.space(1)).width_percent(100.),
        label,
        description: None,
        error: None,
        font_size: m.font_size,
        label_size: m.label_size,
    }
}

impl<'t> Field<'_, '_, 't> {
    /// Supporting text below the control, as help text.
    pub fn description(mut self, description: &'t str) -> Self {
        self.description = (!description.is_empty()).then_some(description);
        self
    }
    /// The control's error, shown below it in the danger color (MyGO's empty
    /// error leaves the control valid).
    pub fn error(mut self, error: &'t str) -> Self {
        self.error = (!error.is_empty()).then_some(error);
        self
    }
}

impl crate::ImplicitKey for Field<'_, '_, '_> {
    fn key_of(&self) -> crate::Key {
        self.layout.key_of()
    }
    fn set_key(mut self, key: crate::Key) -> Self {
        self.layout = self.layout.set_key(key);
        self
    }
}

impl<'t> Field<'_, '_, 't> {
    /// Declare the field's label, then the control the callback builds, then
    /// the description and error. The callback's result is preserved.
    pub fn children<R>(self, control: impl FnOnce(&mut UiContext) -> R) -> InnerResponse<R> {
        let label = self.label;
        let font_size = self.font_size;
        let label_size = self.label_size;
        let description = self.description;
        let error = self.error;
        self.layout.children(|cx| {
            let _ = text(cx, label).font_size(font_size).weight(500);
            let result = control(cx);
            if let Some(description) = description {
                let _ = text(cx, description)
                    .font_size(label_size)
                    .color(Color::Muted);
            }
            if let Some(error) = error {
                let _ = text(cx, error).font_size(label_size).color(Color::Danger);
            }
            result
        })
    }
}

/// Native `+ |cx| { ... }` spelling for a field's control.
impl<F, R> std::ops::Add<F> for Field<'_, '_, '_>
where
    F: FnOnce(&mut UiContext) -> R,
{
    type Output = InnerResponse<R>;
    fn add(self, build: F) -> Self::Output {
        self.children(build)
    }
}

/// A column of fields (MyGO `Form`). Layout, not state: the fields, their
/// values and their validation stay with the application. MyGO's label column
/// beside the controls is not ported (see the module docs).
pub fn form(cx: &mut UiContext, content: impl FnOnce(&mut UiContext)) {
    let m = cx.theme().metrics;
    column(cx)
        .gap(m.space(3))
        .width_percent(100.)
        .children(|cx| content(cx));
}

/// A group of fields under a legend (MyGO `Fieldset`): the legend in bold,
/// then the fields, spaced as a [`form`] would.
pub fn fieldset(cx: &mut UiContext, legend: &str, content: impl FnOnce(&mut UiContext)) {
    let m = cx.theme().metrics;
    column(cx).gap(m.space(3)).children(|cx| {
        let _ = text(cx, legend).font_size(m.font_size).weight(600);
        content(cx);
    });
}
