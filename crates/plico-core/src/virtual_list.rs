use crate::{Key, Motion, UiContext, widgets::column};
use std::ops::Range;

/// Fixed-height list. Rows outside the window are unmounted; keep their state in
/// the application, keyed by business identity. Heights are logical pixels.
pub struct VirtualList<'u, 'e> {
    layout: crate::Layout<'u, 'e>,
    count: usize,
    row_height: f32,
    height: f32,
    fill: bool,
    overscan: usize,
    reveal: Option<usize>,
}
#[derive(Debug, Clone)]
pub struct VirtualListResponse {
    pub rendered: Range<usize>,
    pub offset: f32,
}
impl<'u, 'e> crate::ImplicitKey for VirtualList<'u, 'e> {
    fn key(mut self, key: impl Into<Key>) -> Self {
        self.layout = self.layout.key(key);
        self
    }
    fn site_id(mut self, site: u64) -> Self {
        self.layout = self.layout.site_id(site);
        self
    }
}
impl<'u, 'e> VirtualList<'u, 'e> {
    pub(crate) fn new(cx: &'u mut UiContext<'e>, count: usize, row_height: f32) -> Self {
        assert!(
            row_height.is_finite() && row_height > 0.,
            "row height must be finite and positive"
        );
        assert!(
            (count as f64 * row_height as f64) <= f32::MAX as f64,
            "list extent exceeds layout coordinates"
        );
        Self {
            layout: column(cx),
            count,
            row_height,
            height: 240.,
            fill: false,
            overscan: 2,
            reveal: None,
        }
    }
    pub fn height(mut self, height: f32) -> Self {
        assert!(
            height.is_finite() && height > 0.,
            "list height must be finite and positive"
        );
        self.height = height;
        self
    }
    /// Grow to fill the parent's remaining height instead of a fixed viewport.
    /// Windowing uses the viewport height observed on the previous frame; the
    /// `height` value remains the fallback until the host reports one.
    pub fn fill(mut self) -> Self {
        self.fill = true;
        self
    }
    /// Reveal a row without moving keyboard focus. Pass Some only on selection
    /// changes; None leaves wheel/trackpad scrolling under the user's control.
    pub fn reveal(mut self, index: Option<usize>) -> Self {
        self.reveal = index;
        self
    }
    pub fn smooth_scroll(mut self, enabled: bool) -> Self {
        self.layout = self.layout.smooth_scroll(enabled);
        self
    }
    pub fn overscan(mut self, rows: usize) -> Self {
        self.overscan = rows;
        self
    }
    pub fn width(mut self, width: f32) -> Self {
        self.layout = self.layout.width(width);
        self
    }
    /// Business keys must be unique among siblings. The row callback runs only
    /// for the rendered range; each row has a clipped, non-shrinking fixed box.
    pub fn rows<K: Into<Key>>(
        self,
        mut key: impl FnMut(usize) -> K,
        mut build: impl FnMut(&mut UiContext<'_>, usize),
    ) -> VirtualListResponse {
        let Self {
            layout,
            count,
            row_height,
            height,
            fill,
            overscan,
            reveal,
        } = self;
        layout
            .height(if fill { 0. } else { height })
            .grow(if fill { 1. } else { 0. })
            .shrink(0.)
            .scroll_y(true)
            .motion(Motion::None)
            .children(|cx| {
                let height = if fill {
                    cx.engine
                        .scroll_viewport(cx.parent)
                        .map_or(height, |v| v[1].max(1.))
                } else {
                    height
                };
                let mut offset = cx
                    .engine
                    .scroll_viewport(cx.parent)
                    .map_or(0., |v| v[0])
                    .clamp(0., (count as f32 * row_height - height).max(0.));
                if let Some(index) = reveal.filter(|i| *i < count) {
                    let top = index as f32 * row_height;
                    let bottom = top + row_height;
                    let target = if top < offset {
                        top
                    } else if bottom > offset + height {
                        bottom - height
                    } else {
                        offset
                    };
                    if target != offset {
                        offset = target.clamp(0., (count as f32 * row_height - height).max(0.));
                        cx.engine.scroll_requests.insert(cx.parent, offset);
                        cx.engine.set_scroll_viewport(cx.parent, offset, height);
                    }
                }
                let start = ((offset / row_height).floor() as usize)
                    .saturating_sub(overscan)
                    .min(count);
                let end = (((offset + height) / row_height).ceil() as usize)
                    .saturating_add(overscan)
                    .min(count);
                column(cx)
                    .key("vlist-before")
                    .height(start as f32 * row_height)
                    .shrink(0.)
                    .children(|_| {});
                // Separate business keys from the two internal spacer identities.
                cx.keyed("vlist-rows", |cx| {
                    for index in start..end {
                        column(cx)
                            .key(key(index))
                            .height(row_height)
                            .shrink(0.)
                            .clip(true)
                            .motion(Motion::None)
                            .children(|cx| build(cx, index));
                    }
                });
                column(cx)
                    .key("vlist-after")
                    .height((count - end) as f32 * row_height)
                    .shrink(0.)
                    .children(|_| {});
                VirtualListResponse {
                    rendered: start..end,
                    offset,
                }
            })
            .inner
    }
}
