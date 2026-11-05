//! Shared measurement/drawing cache. Geometry and paint retain the same native layout.
use crate::{Error, Renderer, Result};
use plico_native::text::{
    TextRequest,
    atlas::{GlyphLayout, LayoutContext},
};
use plico_renderer::{
    TextContext,
    paragraph::{
        NativeParagraph, ParagraphEngine, ParagraphError, ParagraphMetrics, ParagraphRequest,
    },
    text::FontAsset,
};
use std::{cell::RefCell, num::NonZeroUsize, rc::Rc, sync::Arc};
const PARAGRAPH_BYTES: usize = 64 * 1024 * 1024;
/// Accounted cache payload, not native-library allocation size or process RSS.
#[derive(Clone, Copy, Debug, Default)]
pub struct ParagraphCacheStats {
    /// Native layouts owned by the cache (external retained references excluded).
    pub layouts: usize,
    /// Rust-owned glyph/source/request payload retained by these entries.
    pub rust_payload_bytes: usize,
    /// Registered application font/alias pairs.
    pub fonts: usize,
    /// Font bytes retained by registered assets, conservatively counted per alias.
    pub font_bytes: usize,
    /// Successful full-request cache hits.
    pub hits: u64,
    /// Layout requests requiring native shaping, including rejected requests.
    pub misses: u64,
    /// Entries evicted by count, bytes or replacement (explicit trim excluded).
    pub evictions: u64,
}
pub(crate) struct Cache {
    context: LayoutContext,
    entries: lru::LruCache<(u64, u64), Rc<Paragraph>>,
    asset_error: Option<String>,
    fonts: Vec<FontAsset>,
    stats: ParagraphCacheStats,
    byte_limit: usize,
}
struct Paragraph {
    request: Arc<ParagraphRequest>,
    layout: Rc<GlyphLayout>,
    metrics: ParagraphMetrics,
    payload_bytes: usize,
}
impl NativeParagraph for Paragraph {
    fn metrics(&self) -> ParagraphMetrics {
        self.metrics
    }
    fn hit_test(&self, x: f32, y: f32) -> usize {
        self.layout.hit_test(x as f64, y as f64).unwrap_or(0)
    }
    fn caret_rect(&self, byte: usize) -> Option<[f32; 4]> {
        self.layout
            .caret_rect(byte)
            .map(|[x, y, w, h]| [x as f32, y as f32, (x + w) as f32, (y + h) as f32])
    }
    fn selection_rects(&self, range: std::ops::Range<usize>) -> Vec<[f32; 4]> {
        self.layout
            .selection_rects(range)
            .unwrap_or_default()
            .into_iter()
            .map(|[x, y, w, h]| [x as f32, y as f32, (x + w) as f32, (y + h) as f32])
            .collect()
    }
}
fn rejected(message: impl Into<String>) -> ParagraphError {
    ParagraphError {
        message: message.into(),
    }
}
impl Cache {
    fn new() -> std::result::Result<Self, ParagraphError> {
        Ok(Self {
            context: LayoutContext::new().map_err(|e| rejected(e.to_string()))?,
            entries: lru::LruCache::new(NonZeroUsize::new(512).unwrap()),
            asset_error: None,
            fonts: Vec::new(),
            stats: ParagraphCacheStats::default(),
            byte_limit: PARAGRAPH_BYTES,
        })
    }
    fn clear_entries(&mut self) {
        self.entries.clear();
        self.stats.rust_payload_bytes = 0;
    }
    fn layout(
        &mut self,
        request: Arc<ParagraphRequest>,
        scale: f64,
    ) -> std::result::Result<Rc<Paragraph>, ParagraphError> {
        if let Some(error) = &self.asset_error {
            self.stats.misses += 1;
            return Err(rejected(error.clone()));
        }
        // Base tint is independent of coverage. Range colors remain in the key:
        // native itemization must preserve their shaping/ligature boundaries.
        let mut geometry = (*request).clone();
        geometry.color = [0; 4];
        let key = (geometry.fingerprint(), scale.to_bits());
        if let Some(p) = self.entries.get(&key)
            && *p.request == geometry
        {
            self.stats.hits += 1;
            return Ok(p.clone());
        }
        self.stats.misses += 1;
        if request.max_lines != 0 && request.ellipsis && request.text.contains('\n') {
            return Err(rejected(format!(
                "unsupported paragraph: runs={} spacing={} line_height={:?} max_lines={} ellipsis={} align={:?}",
                request.runs.len(),
                request.letter_spacing,
                request.line_height,
                request.max_lines,
                request.ellipsis,
                request.align
            )));
        }
        let mut native = TextRequest::new(&request.text);
        native.family = &request.family;
        native.size = request.font_size as f64;
        native.weight = request.font_weight as u16;
        native.italic = request.italic;
        native.scale = scale;
        native.width = if request.wrap || request.ellipsis {
            request.width.filter(|v| *v != f32::INFINITY).map(f64::from)
        } else {
            None
        };
        let options = plico_native::text::atlas::LayoutOptions {
            alignment: match request.align {
                plico_renderer::style::TextAlign::Start => {
                    plico_native::text::atlas::Alignment::Start
                }
                plico_renderer::style::TextAlign::Center => {
                    plico_native::text::atlas::Alignment::Center
                }
                plico_renderer::style::TextAlign::End => plico_native::text::atlas::Alignment::End,
                plico_renderer::style::TextAlign::Justify => {
                    plico_native::text::atlas::Alignment::Justify
                }
            },
            ellipsis: request.ellipsis && (!request.wrap || request.max_lines > 0),
            max_lines: request.max_lines,
            letter_spacing: request.letter_spacing as f64,
            line_height: request
                .line_height
                .map(|(v, multiplier)| if multiplier { v * request.font_size } else { v })
                .map(f64::from),
        };
        let spans: Vec<_> = request
            .runs
            .iter()
            .map(|r| {
                if !r.font_weight.is_finite() || !(1. ..=1000.).contains(&r.font_weight) {
                    return Err(rejected("invalid span font weight"));
                }
                Ok(plico_native::text::atlas::LayoutSpan {
                    range: r.range.clone(),
                    size: f64::from(r.font_size),
                    weight: r.font_weight as u16,
                    italic: r.font_italic,
                    line_height: r
                        .line_height
                        .map(|(v, relative)| f64::from(if relative { v * r.font_size } else { v })),
                    color: r.color,
                })
            })
            .collect::<std::result::Result<_, _>>()?;
        let layout = Rc::new(
            self.context
                .layout_rich(&native, options, &spans)
                .map_err(|e| rejected(e.to_string()))?,
        );
        let lines = layout.line_metrics();
        let payload_bytes = std::mem::size_of::<Paragraph>()
            + layout.rust_payload_bytes()
            + geometry.text.len()
            + geometry.family.len()
            + geometry.runs.len() * std::mem::size_of::<plico_renderer::text::TextRun>();
        let p = Rc::new(Paragraph {
            payload_bytes,
            request: Arc::new(geometry),
            metrics: ParagraphMetrics {
                width: layout.logical_size[0] as f32,
                height: layout.logical_size[1] as f32,
                intrinsic: layout.intrinsic_widths().map(|v| v as f32),
                baselines: [
                    lines.first().map(|l| l[4] as f32),
                    lines.last().map(|l| l[4] as f32),
                ],
                first_line_height: lines.first().map_or(0., |l| l[3] as f32),
            },
            layout,
        });
        if payload_bytes <= self.byte_limit {
            while self.stats.rust_payload_bytes + payload_bytes > self.byte_limit
                || self.entries.len() >= self.entries.cap().get()
            {
                if let Some((_, old)) = self.entries.pop_lru() {
                    self.stats.rust_payload_bytes -= old.payload_bytes;
                    self.stats.evictions += 1;
                } else {
                    break;
                }
            }
            if let Some(old) = self.entries.put(key, p.clone()) {
                self.stats.rust_payload_bytes -= old.payload_bytes;
                self.stats.evictions += 1;
            }
            self.stats.rust_payload_bytes += payload_bytes;
        }
        Ok(p)
    }
}
struct Engine {
    cache: Rc<RefCell<Cache>>,
    scale: f64,
}
impl ParagraphEngine for Engine {
    fn set_device_scale(&mut self, scale: f64) -> bool {
        if self.scale == scale || !scale.is_finite() || scale <= 0. || scale > 8. {
            return false;
        }
        self.scale = scale;
        true
    }
    fn supports_editing(&self) -> bool {
        true
    }
    fn name(&self) -> &'static str {
        "pango-wgpu"
    }
    fn layout(
        &mut self,
        request: Arc<ParagraphRequest>,
    ) -> std::result::Result<Rc<dyn NativeParagraph>, ParagraphError> {
        self.cache
            .borrow_mut()
            .layout(request, self.scale)
            .map(|p| p as Rc<dyn NativeParagraph>)
    }
    fn register_font(&mut self, asset: &FontAsset) {
        let mut cache = self.cache.borrow_mut();
        if cache
            .fonts
            .iter()
            .any(|font| font.family() == asset.family() && font.bytes() == asset.bytes())
        {
            return;
        }
        match cache.context.register_font(asset.bytes(), asset.family()) {
            Ok(()) => cache.fonts.push(asset.clone()),
            Err(error) => cache.asset_error = Some(error.to_string()),
        }
        cache.clear_entries();
    }
    fn clear(&mut self) {
        self.cache.borrow_mut().clear_entries();
    }
}
impl Renderer {
    /// Inspect paragraph payload separately from the raster atlas and positioned glyphs.
    pub fn paragraph_cache_stats(&self) -> ParagraphCacheStats {
        self.paragraphs
            .as_ref()
            .map_or(ParagraphCacheStats::default(), |cache| {
                let cache = cache.borrow();
                ParagraphCacheStats {
                    layouts: cache.entries.len(),
                    fonts: cache.fonts.len(),
                    font_bytes: cache.fonts.iter().map(|f| f.bytes().len()).sum(),
                    ..cache.stats
                }
            })
    }
    /// Release derived native paragraph/atlas storage. Registered fonts and live
    /// layout references remain valid; the next scene redraw repopulates glyphs.
    pub fn trim_text_cache(&mut self) {
        if let Some(cache) = &self.paragraphs {
            cache.borrow_mut().clear_entries();
        }
        self.trim_native_atlas();
        self.prepared_cache = None;
        self.invalidate_scene_damage();
    }

    fn paragraph_cache(&mut self) -> Result<Rc<RefCell<Cache>>> {
        if self.paragraphs.is_none() {
            self.paragraphs = Some(Rc::new(RefCell::new(
                Cache::new().map_err(|source| Error::Paragraph { source })?,
            )));
        }
        Ok(self.paragraphs.as_ref().unwrap().clone())
    }
    /// Install the system provider before Source layout. Call again after DPI changes.
    /// Layout failures remain observable via TextContext::take_native_paragraph_error.
    pub fn configure_text(&mut self, text: &mut TextContext, scale: f64) -> Result<()> {
        if !scale.is_finite() || scale <= 0. || scale > 8. {
            return Err(Error::InvalidInput {
                reason: "invalid system text device scale".into(),
            });
        }
        #[cfg(feature = "paint-scene")]
        {
            self.prepared_cache = None;
            self.invalidate_scene_damage();
        }
        text.set_paragraph_engine(Box::new(Engine {
            cache: self.paragraph_cache()?,
            scale,
        }));
        Ok(())
    }
    pub(crate) fn paragraph_layout(
        &mut self,
        request: Arc<ParagraphRequest>,
        scale: f64,
    ) -> Result<Rc<GlyphLayout>> {
        let p = self
            .paragraph_cache()?
            .borrow_mut()
            .layout(request, scale)
            .map_err(|source| Error::Paragraph { source })?;
        Ok(p.layout.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn request(value: &str) -> Arc<ParagraphRequest> {
        Arc::new(
            ParagraphRequest::from_paint(
                &plico_renderer::style::Paint {
                    text: Some(value.into()),
                    font_size: 16.,
                    ..Default::default()
                },
                None,
            )
            .unwrap(),
        )
    }
    #[test]
    #[ignore = "requires system fonts"]
    fn paragraph_payload_budget_evicts_without_invalidating_live_layouts() {
        let mut cache = Cache::new().unwrap();
        let first = cache.layout(request("AAA"), 1.).unwrap();
        cache.byte_limit = first.payload_bytes * 2;
        cache.layout(request("BBB"), 1.).unwrap();
        cache.layout(request("CCC"), 1.).unwrap();
        assert!(cache.entries.len() <= 2);
        assert!(cache.stats.rust_payload_bytes <= cache.byte_limit);
        assert!(cache.stats.evictions > 0);
        assert!(first.caret_rect(1).is_some());
        let hits = cache.stats.hits;
        cache.layout(request("CCC"), 1.).unwrap();
        assert_eq!(cache.stats.hits, hits + 1);
        cache.clear_entries();
        assert_eq!(cache.stats.rust_payload_bytes, 0);
        assert!(first.caret_rect(1).is_some());
        cache.byte_limit = 1;
        let uncached = cache
            .layout(request("Large uncached paragraph"), 1.)
            .unwrap();
        assert_eq!(cache.entries.len(), 0);
        assert_eq!(cache.stats.rust_payload_bytes, 0);
        assert!(uncached.metrics.width > 0.);
    }
}
