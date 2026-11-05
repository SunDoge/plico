//! Theme-scaled settings composition. Controls keep their own accessible names.
use crate::{ImplicitKey, InnerResponse, Key, Layout, prelude::*};

/// A heading and description above a bordered group of related settings.
/// Content executes synchronously and may return an application result.
///
/// ```
/// use plico::{Engine, prelude::*};
/// Engine::default().frame_declarations(|cx| {
///     let _ = settings_section(cx, "Notifications").description("Choose what reaches you.") + |cx| {
///         let _ = settings_row(cx, "Updates").description("New releases and improvements.") + |cx| {
///             button(cx).text("Configure");
///         };
///     };
/// }).unwrap();
/// ```
pub fn settings_section<'u, 'e>(
    cx: &'u mut UiContext<'e>,
    title: &'u str,
) -> SettingsSection<'u, 'e> {
    let m = cx.theme().metrics;
    SettingsSection {
        layout: column(cx).gap(m.space(3)).min_width(0.),
        title,
        description: None,
        metrics: m,
    }
}
pub struct SettingsSection<'u, 'e> {
    layout: Layout<'u, 'e>,
    title: &'u str,
    description: Option<&'u str>,
    metrics: Metrics,
}
impl SettingsSection<'_, '_> {
    pub fn children<R>(self, content: impl FnOnce(&mut UiContext) -> R) -> InnerResponse<R> {
        let Self {
            layout,
            title,
            description,
            metrics: m,
        } = self;
        layout.children(|cx| {
            let _ = column(cx).gap(m.space(1))
                + |cx| {
                    text(cx, title).font_size(m.font_size + 2).weight(650);
                    if let Some(description) = description {
                        text(cx, description)
                            .font_size(m.font_size)
                            .color(Color::Muted);
                    }
                };
            column(cx)
                .padding_x(m.space(5))
                .radius(m.radius as f32 + 4.)
                .background(Color::Panel)
                .border(1., Color::Border)
                .min_width(0.)
                .children(content)
                .inner
        })
    }
}
impl<'u> SettingsSection<'u, '_> {
    pub fn description(mut self, description: &'u str) -> Self {
        self.description = Some(description);
        self
    }
}

/// A label and supporting text beside a control. Below 700 logical viewport
/// pixels the control moves below the label. The control must still provide its
/// own accessible name; this helper does not create label/focus associations.
pub fn settings_row<'u, 'e>(cx: &'u mut UiContext<'e>, label: &'u str) -> SettingsRow<'u, 'e> {
    let m = cx.theme().metrics;
    let stacked = cx.viewport_width() < 700.;
    let layout = if stacked {
        column(cx)
    } else {
        row(cx).align(Align::Center)
    };
    SettingsRow {
        layout: layout.padding_y(m.space(4)).gap(m.space(3)).min_width(0.),
        label,
        description: None,
        metrics: m,
        stacked,
    }
}
pub struct SettingsRow<'u, 'e> {
    layout: Layout<'u, 'e>,
    label: &'u str,
    description: Option<&'u str>,
    metrics: Metrics,
    stacked: bool,
}
impl SettingsRow<'_, '_> {
    pub fn children<R>(self, content: impl FnOnce(&mut UiContext) -> R) -> InnerResponse<R> {
        let Self {
            layout,
            label,
            description,
            metrics: m,
            stacked,
        } = self;
        layout.children(|cx| {
            let labels = column(cx).gap(m.space(1)).min_width(0.);
            let labels = if stacked {
                labels
            } else {
                labels.width(0.).grow(1.)
            };
            let _ = labels
                + |cx| {
                    text(cx, label).font_size(m.font_size).weight(550);
                    if let Some(description) = description {
                        text(cx, description)
                            .font_size(m.font_size)
                            .color(Color::Muted);
                    }
                };
            row(cx)
                .gap(m.space(2))
                .wrap(true)
                .shrink(0.)
                .children(content)
                .inner
        })
    }
}
impl<'u> SettingsRow<'u, '_> {
    pub fn description(mut self, description: &'u str) -> Self {
        self.description = Some(description);
        self
    }
}
macro_rules! settings_builder {
    ($ty:ident) => {
        impl ImplicitKey for $ty<'_, '_> {
            fn key_of(&self) -> Key {
                self.layout.key_of()
            }
            fn set_key(mut self, key: Key) -> Self {
                self.layout = self.layout.set_key(key);
                self
            }
        }
        impl<F, R> std::ops::Add<F> for $ty<'_, '_>
        where
            F: FnOnce(&mut UiContext) -> R,
        {
            type Output = InnerResponse<R>;
            fn add(self, content: F) -> Self::Output {
                self.children(content)
            }
        }
    };
}
settings_builder!(SettingsSection);
settings_builder!(SettingsRow);
