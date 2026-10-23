use crate::{InnerResponse, Key, Kind, Style, Ui};
use std::borrow::Cow;
/// Describes its contained controls without retaining a callback or tooltip state.
pub struct Tooltip<'u, 'e, 't> {
    cx: &'u mut Ui<'e>,
    key: Key,
    description: Cow<'t, str>,
}
pub fn tooltip<'u, 'e, 't>(
    cx: &'u mut Ui<'e>,
    description: impl Into<Cow<'t, str>>,
) -> Tooltip<'u, 'e, 't> {
    let key = cx.engine.auto_key(cx.parent);
    Tooltip {
        cx,
        key,
        description: description.into(),
    }
}
impl Tooltip<'_, '_, '_> {
    #[doc(hidden)]
    pub fn __implicit_key(mut self, site: u64) -> Self {
        if matches!(self.key, Key::Auto(_)) {
            self.key = Key::Generated(site)
        }
        self
    }
    pub fn key(mut self, key: impl Into<Key>) -> Self {
        self.key = key.into();
        self
    }
    pub fn children<R>(self, build: impl FnOnce(&mut Ui<'_>) -> R) -> InnerResponse<R> {
        let style = Style {
            font_size: self.cx.inherited.font_size,
            foreground: self.cx.inherited.foreground,
            font_weight: self.cx.inherited.font_weight,
            disabled: self.cx.inherited.disabled,
            ..Default::default()
        };
        let (inner, clicked) = self.cx.element(
            self.key,
            Kind::TooltipAnchor,
            style,
            self.description.as_ref(),
            build,
        );
        InnerResponse {
            inner,
            response: crate::Response::new(clicked, !style.disabled),
        }
    }
}

/// Native addition spelling for the consuming children operation.
impl<F, R> std::ops::Add<F> for Tooltip<'_, '_, '_>
where
    F: FnOnce(&mut Ui<'_>) -> R,
{
    type Output = InnerResponse<R>;
    fn add(self, build: F) -> Self::Output {
        self.children(build)
    }
}
