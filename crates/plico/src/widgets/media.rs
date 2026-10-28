//! Reusable media presentation; loading and account state belong to the host.
use super::{column, image, row, text};
use crate::{Align, Color, ImageAsset, ImplicitKey, Justify, Key, Layout, TextSpan, UiContext};

/// A circular image with an explicit text fallback. Loading stays outside the widget.
pub struct Avatar<'u, 'e> {
    layout: Option<Layout<'u, 'e>>,
    image: Option<&'u ImageAsset>,
    fallback: &'u str,
    size: f32,
    font_size: u16,
    color: Color,
}
/// Start a decorative avatar; callers provide initials or other fallback text.
pub fn avatar<'u, 'e>(cx: &'u mut UiContext<'e>) -> Avatar<'u, 'e> {
    let font_size = cx.theme().metrics.font_size;
    Avatar {
        layout: Some(
            column(cx)
                .clip(true)
                .shrink(0.)
                .align(Align::Center)
                .justify(Justify::Center)
                .background(Color::Control),
        ),
        image: None,
        fallback: "",
        size: 32.,
        font_size,
        color: Color::Muted,
    }
}
impl<'u> Avatar<'u, '_> {
    pub fn image(mut self, image: impl Into<Option<&'u ImageAsset>>) -> Self {
        self.image = image.into();
        self
    }
    pub fn fallback(mut self, value: &'u str) -> Self {
        self.fallback = value;
        self
    }
    pub fn size(mut self, size: f32) -> Self {
        self.size = size;
        self
    }
    pub fn font_size(mut self, size: u16) -> Self {
        self.font_size = size;
        self
    }
    pub fn color(mut self, color: Color) -> Self {
        self.color = color;
        self
    }
    pub fn background(mut self, color: Color) -> Self {
        self.layout = self.layout.take().map(|layout| layout.background(color));
        self
    }
}
impl ImplicitKey for Avatar<'_, '_> {
    fn key_of(&self) -> Key {
        self.layout.as_ref().unwrap().key_of()
    }
    fn set_key(mut self, key: Key) -> Self {
        self.layout = self.layout.take().map(|layout| layout.set_key(key));
        self
    }
}
impl Drop for Avatar<'_, '_> {
    fn drop(&mut self) {
        self.layout
            .take()
            .unwrap()
            .width(self.size)
            .height(self.size)
            .radius(self.size / 2.)
            .children(|cx| {
                if let Some(asset) = self.image {
                    image(cx, asset).size(self.size, self.size);
                } else {
                    text(cx, self.fallback)
                        .font_size(self.font_size)
                        .color(self.color);
                }
            });
    }
}

/// Annotation above a text fragment. The reserved annotation slot keeps adjacent
/// fragments aligned even when some have no reading. Span offsets are local UTF-8 bytes.
pub struct RubyText<'u, 'e> {
    layout: Option<Layout<'u, 'e>>,
    body: &'u str,
    annotation: Option<&'u str>,
    spans: &'u [TextSpan],
    font_size: u16,
    weight: u16,
    color: Color,
    annotation_size: u16,
    annotation_height: Option<f32>,
    annotation_weight: u16,
    annotation_color: Color,
}
/// Start an inline text fragment with an optional phonetic reading.
pub fn ruby_text<'u, 'e>(cx: &'u mut UiContext<'e>, body: &'u str) -> RubyText<'u, 'e> {
    let m = cx.theme().metrics;
    let (font_size, weight, color) = (
        cx.inherited.font_size,
        cx.inherited.font_weight,
        cx.inherited.foreground,
    );
    RubyText {
        layout: Some(column(cx).gap(0).align(Align::Center).shrink(0.)),
        body,
        annotation: None,
        spans: &[],
        font_size,
        weight,
        color,
        annotation_size: m.label_size,
        annotation_height: None,
        annotation_weight: 400,
        annotation_color: Color::Muted,
    }
}
impl<'u> RubyText<'u, '_> {
    pub fn annotation(mut self, reading: impl Into<Option<&'u str>>) -> Self {
        self.annotation = reading.into();
        self
    }
    pub fn spans(mut self, spans: &'u [TextSpan]) -> Self {
        self.spans = spans;
        self
    }
    pub fn font_size(mut self, size: u16) -> Self {
        self.font_size = size;
        self
    }
    pub fn weight(mut self, weight: u16) -> Self {
        self.weight = weight;
        self
    }
    pub fn color(mut self, color: Color) -> Self {
        self.color = color;
        self
    }
    pub fn annotation_font_size(mut self, size: u16) -> Self {
        self.annotation_size = size;
        self
    }
    pub fn annotation_height(mut self, height: f32) -> Self {
        self.annotation_height = Some(height);
        self
    }
    pub fn annotation_weight(mut self, weight: u16) -> Self {
        self.annotation_weight = weight;
        self
    }
    pub fn annotation_color(mut self, color: Color) -> Self {
        self.annotation_color = color;
        self
    }
}
impl ImplicitKey for RubyText<'_, '_> {
    fn key_of(&self) -> Key {
        self.layout.as_ref().unwrap().key_of()
    }
    fn set_key(mut self, key: Key) -> Self {
        self.layout = self.layout.take().map(|layout| layout.set_key(key));
        self
    }
}
impl Drop for RubyText<'_, '_> {
    fn drop(&mut self) {
        self.layout.take().unwrap().children(|cx| {
            row(cx)
                .height(
                    self.annotation_height
                        .unwrap_or(self.annotation_size as f32 * 1.5),
                )
                .shrink(0.)
                .align(Align::Center)
                .children(|cx| {
                    if let Some(reading) = self.annotation {
                        text(cx, reading)
                            .font_size(self.annotation_size)
                            .weight(self.annotation_weight)
                            .color(self.annotation_color);
                    }
                });
            text(cx, self.body)
                .font_size(self.font_size)
                .weight(self.weight)
                .color(self.color)
                .spans(self.spans);
        });
    }
}
