//! SkParagraph owns ordinary text shaping, geometry, hit testing and painting.
use plico_renderer::{
    paragraph::{NativeParagraph, ParagraphEngine, ParagraphMetrics, ParagraphRequest},
    text::FontAsset,
};
use skia_safe::{
    FontMgr,
    textlayout::{
        self, FontCollection, Paragraph, ParagraphBuilder, ParagraphStyle, TextStyle,
        TypefaceFontProvider,
    },
};
use std::{cell::RefCell, num::NonZeroUsize, rc::Rc, sync::Arc};

pub(crate) struct ParagraphCache {
    #[cfg(feature = "profiling")]
    stats: crate::diagnostics::CacheCounters,
    fonts: FontCollection,
    assets: TypefaceFontProvider,
    layouts: lru::LruCache<u64, Rc<SkParagraph>>,
}
impl Default for ParagraphCache {
    fn default() -> Self {
        let mut fonts = FontCollection::new();
        fonts.set_default_font_manager(FontMgr::new(), None);
        let assets = TypefaceFontProvider::new();
        fonts.set_asset_font_manager(skia_safe::FontMgr::from(assets.clone()));
        Self {
            #[cfg(feature = "profiling")]
            stats: Default::default(),
            fonts,
            assets,
            layouts: lru::LruCache::new(NonZeroUsize::new(512).unwrap()),
        }
    }
}
pub(crate) struct SkParagraph {
    request: Arc<ParagraphRequest>,
    paragraph: Paragraph,
    metrics: ParagraphMetrics,
    ink: kurbo::Rect,
}
fn byte_to_utf16(text: &str, byte: usize) -> usize {
    text.char_indices()
        .take_while(|(i, _)| *i < byte)
        .map(|(_, c)| c.len_utf16())
        .sum()
}
fn utf16_to_byte(text: &str, index: usize) -> usize {
    let mut units = 0;
    for (byte, c) in text.char_indices() {
        if units + c.len_utf16() > index {
            return byte;
        }
        units += c.len_utf16();
    }
    text.len()
}
impl NativeParagraph for SkParagraph {
    fn metrics(&self) -> ParagraphMetrics {
        self.metrics
    }
    fn hit_test(&self, x: f32, y: f32) -> usize {
        let index = self
            .paragraph
            .get_glyph_position_at_coordinate((x, y))
            .position
            .max(0) as usize;
        utf16_to_byte(&self.request.text, index)
    }
    fn selection_rects(&self, range: std::ops::Range<usize>) -> Vec<[f32; 4]> {
        self.paragraph
            .get_rects_for_range(
                byte_to_utf16(&self.request.text, range.start)
                    ..byte_to_utf16(&self.request.text, range.end),
                textlayout::RectHeightStyle::Tight,
                textlayout::RectWidthStyle::Tight,
            )
            .into_iter()
            .map(|b| [b.rect.left, b.rect.top, b.rect.right, b.rect.bottom])
            .collect()
    }
}
impl SkParagraph {
    pub(crate) fn paint(&self, canvas: &skia_safe::Canvas, origin: (f32, f32)) {
        self.paragraph.paint(canvas, origin);
    }
    pub(crate) fn ink_bounds(&self) -> kurbo::Rect {
        self.ink
    }
}
fn style(request: &ParagraphRequest, run: Option<&plico_renderer::text::TextRun>) -> TextStyle {
    let size = run.map_or(request.font_size, |r| r.font_size);
    let weight = run.map_or(request.font_weight, |r| r.font_weight);
    let italic = run.map_or(request.italic, |r| r.font_italic);
    let color = run.map_or(request.color, |r| r.color);
    let families: Vec<_> = request
        .family
        .split(',')
        .map(|f| f.trim().trim_matches(['\'', '"']))
        .filter(|f| !f.is_empty())
        .collect();
    let mut s = TextStyle::new();
    s.set_font_size(size)
        .set_font_families(&families)
        .set_color(skia_safe::Color::from_argb(
            color[3], color[0], color[1], color[2],
        ))
        .set_font_style(skia_safe::FontStyle::new(
            skia_safe::font_style::Weight::from(weight.round() as i32),
            skia_safe::font_style::Width::NORMAL,
            if italic {
                skia_safe::font_style::Slant::Italic
            } else {
                skia_safe::font_style::Slant::Upright
            },
        ))
        .set_letter_spacing(request.letter_spacing)
        .set_font_edging(skia_safe::font::Edging::AntiAlias)
        .set_subpixel(true)
        .set_font_hinting(skia_safe::FontHinting::Normal);
    if let Some((height, relative)) = run.and_then(|r| r.line_height).or(request.line_height) {
        s.set_height(if relative {
            height
        } else {
            height / size.max(f32::EPSILON)
        })
        .set_height_override(true);
    }
    s
}
impl ParagraphCache {
    pub(crate) fn layout(&mut self, request: Arc<ParagraphRequest>) -> Rc<SkParagraph> {
        let key = request.fingerprint();
        if let Some(cached) = self.layouts.get(&key).filter(|p| p.request == request) {
            #[cfg(feature = "profiling")]
            {
                self.stats.hits += 1;
            }
            return cached.clone();
        }
        #[cfg(feature = "profiling")]
        {
            self.stats.misses += 1;
        }
        #[cfg(feature = "profiling")]
        let _span = tracing::trace_span!(target: "plico::skia", "skia.paragraph_miss", bytes = request.text.len(), runs = request.runs.len(),
            request_key = key, font_size = request.font_size, width = ?request.width,
            font_revision = request.font_revision).entered();
        let mut ps = ParagraphStyle::new();
        ps.set_text_style(&style(&request, None));
        ps.set_text_align(match request.align {
            plico_renderer::style::TextAlign::Start => textlayout::TextAlign::Start,
            plico_renderer::style::TextAlign::Center => textlayout::TextAlign::Center,
            plico_renderer::style::TextAlign::End => textlayout::TextAlign::End,
            plico_renderer::style::TextAlign::Justify => textlayout::TextAlign::Justify,
        });
        if request.max_lines > 0 {
            ps.set_max_lines(request.max_lines);
        } else if request.ellipsis && !request.wrap && request.width.is_some() {
            ps.set_max_lines(1);
        }
        if request.ellipsis {
            ps.set_ellipsis("…");
        }
        let mut builder = ParagraphBuilder::new(&ps, self.fonts.clone());
        let mut boundaries = vec![0, request.text.len()];
        for run in request.runs.iter() {
            if run.range.start <= run.range.end
                && run.range.end <= request.text.len()
                && request.text.is_char_boundary(run.range.start)
                && request.text.is_char_boundary(run.range.end)
            {
                boundaries.extend([run.range.start, run.range.end]);
            }
        }
        boundaries.sort_unstable();
        boundaries.dedup();
        for range in boundaries.windows(2) {
            let run = request
                .runs
                .iter()
                .rev()
                .find(|r| r.range.start <= range[0] && r.range.end >= range[1]);
            builder.push_style(&style(&request, run));
            builder.add_text(&request.text[range[0]..range[1]]);
            builder.pop();
        }
        let mut paragraph = builder.build();
        paragraph.layout(request.width.unwrap_or(f32::INFINITY));
        if request.width.is_none() {
            paragraph.layout(paragraph.max_intrinsic_width().ceil().max(1.));
        }
        let lines = paragraph.get_line_metrics();
        let metrics = ParagraphMetrics {
            width: paragraph.longest_line(),
            height: paragraph.height(),
            intrinsic: [
                paragraph.min_intrinsic_width(),
                paragraph.max_intrinsic_width(),
            ],
            baselines: [
                lines.first().map(|l| l.baseline as f32),
                lines.last().map(|l| l.baseline as f32),
            ],
            first_line_height: lines.first().map_or(0., |l| l.height as f32),
        };
        let mut ink: Option<kurbo::Rect> = None;
        paragraph.visit(|_, info| {
            let Some(info) = info else {
                return;
            };
            let mut bounds = vec![skia_safe::Rect::default(); info.glyphs().len()];
            info.font().get_bounds(info.glyphs(), &mut bounds, None);
            let origin = info.origin();
            for (r, p) in bounds.iter().zip(info.positions()) {
                if r.is_empty() {
                    continue;
                }
                let r = kurbo::Rect::new(
                    (origin.x + p.x + r.left) as f64,
                    (origin.y + p.y + r.top) as f64,
                    (origin.x + p.x + r.right) as f64,
                    (origin.y + p.y + r.bottom) as f64,
                );
                ink = Some(ink.map_or(r, |old| old.union(r)));
            }
        });
        let p = Rc::new(SkParagraph {
            request,
            paragraph,
            metrics,
            ink: ink.unwrap_or(kurbo::Rect::ZERO),
        });
        #[cfg(feature = "profiling")]
        if self.layouts.len() == self.layouts.cap().get() && !self.layouts.contains(&key) {
            self.stats.evictions += 1;
        }
        self.layouts.put(key, p.clone());
        p
    }
    fn clear(&mut self) {
        #[cfg(feature = "profiling")]
        {
            self.stats.invalidations += 1;
        }
        self.layouts.clear();
    }
    #[cfg(feature = "profiling")]
    pub(crate) fn trace_cache(&self) {
        if !tracing::enabled!(target: "plico::cache", tracing::Level::TRACE) {
            return;
        }
        // Logical request payload, not SkParagraph/font/GPU allocations. Shared
        // Arc payloads can be counted more than once across cached requests.
        let request_bytes: usize = self
            .layouts
            .iter()
            .map(|(_, p)| {
                p.request.text.len()
                    + p.request.family.len()
                    + std::mem::size_of_val(p.request.runs.as_ref())
            })
            .sum();
        tracing::trace!(target: "plico::cache", cache = %"skia.paragraph", cache_id = self.stats.id,
            entries = self.layouts.len(), capacity = self.layouts.cap().get(),
            hits_total = self.stats.hits, misses_total = self.stats.misses,
            evictions_total = self.stats.evictions, invalidations_total = self.stats.invalidations,
            request_payload_bytes_estimate = request_bytes, "cache.snapshot");
    }
    fn register(&mut self, asset: &FontAsset) {
        let data = skia_safe::Data::new_copy(asset.bytes());
        let count = if asset.bytes().starts_with(b"ttcf") && asset.bytes().len() >= 12 {
            u32::from_be_bytes(asset.bytes()[8..12].try_into().unwrap()).min(64)
        } else {
            1
        };
        let manager = FontMgr::new();
        for index in 0..count {
            if let Some(face) = manager.new_from_data(data.clone(), index) {
                self.assets.register_typeface(face, asset.family());
            }
        }
        self.fonts.clear_caches();
        self.clear();
    }
}
pub(crate) struct Engine(pub Rc<RefCell<ParagraphCache>>);
impl ParagraphEngine for Engine {
    fn name(&self) -> &'static str {
        "skparagraph"
    }
    fn layout(&mut self, request: Arc<ParagraphRequest>) -> Rc<dyn NativeParagraph> {
        self.0.borrow_mut().layout(request)
    }
    fn register_font(&mut self, asset: &FontAsset) {
        self.0.borrow_mut().register(asset);
    }
    fn clear(&mut self) {
        self.0.borrow_mut().clear();
    }
}

#[cfg(all(test, feature = "profiling"))]
mod profile_tests {
    use super::*;
    #[test]
    fn cache_counters_distinguish_hits_evictions_and_invalidations() {
        let mut cache = ParagraphCache {
            layouts: lru::LruCache::new(NonZeroUsize::new(2).unwrap()),
            ..Default::default()
        };
        let request = |text: &str| {
            Arc::new(
                ParagraphRequest::from_paint(
                    &plico_renderer::style::Paint {
                        text: Some(text.into()),
                        ..Default::default()
                    },
                    None,
                )
                .unwrap(),
            )
        };
        let first = cache.layout(request("one"));
        assert!(Rc::ptr_eq(&first, &cache.layout(request("one"))));
        cache.layout(request("two"));
        cache.layout(request("three"));
        assert_eq!(
            (cache.stats.hits, cache.stats.misses, cache.stats.evictions),
            (1, 3, 1)
        );
        assert_eq!(cache.layouts.len(), 2);
        cache.clear();
        assert_eq!(cache.stats.invalidations, 1);
        assert_eq!(cache.stats.evictions, 1, "invalidation is not LRU eviction");
        assert!(cache.layouts.is_empty());
        assert!(
            first.metrics().width > 0.,
            "eviction does not count external owners as freed"
        );
    }
}
