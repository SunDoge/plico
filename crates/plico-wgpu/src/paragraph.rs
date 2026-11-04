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
pub(crate) struct Cache {
    context: LayoutContext,
    entries: lru::LruCache<(u64, u64), Rc<Paragraph>>,
    asset_error: bool,
}
struct Paragraph {
    request: Arc<ParagraphRequest>,
    layout: Rc<GlyphLayout>,
    metrics: ParagraphMetrics,
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
            .map(|r| r.map(|v| v as f32))
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
            asset_error: false,
        })
    }
    fn layout(
        &mut self,
        request: Arc<ParagraphRequest>,
        scale: f64,
    ) -> std::result::Result<Rc<Paragraph>, ParagraphError> {
        if self.asset_error {
            return Err(rejected(
                "application-font registration is not implemented by the Pango provider",
            ));
        }
        // Base tint is independent of coverage. Range colors remain in the key:
        // native itemization must preserve their shaping/ligature boundaries.
        let mut geometry = (*request).clone();
        geometry.color = [0; 4];
        let key = (geometry.fingerprint(), scale.to_bits());
        if let Some(p) = self.entries.get(&key)
            && *p.request == geometry
        {
            return Ok(p.clone());
        }
        if request.max_lines != 0 && (!request.ellipsis || request.text.contains('\n')) {
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
        let p = Rc::new(Paragraph {
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
        self.entries.put(key, p.clone());
        Ok(p)
    }
}
struct Engine {
    cache: Rc<RefCell<Cache>>,
    scale: f64,
}
impl ParagraphEngine for Engine {
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
    fn register_font(&mut self, _: &FontAsset) {
        let mut cache = self.cache.borrow_mut();
        cache.asset_error = true;
        cache.entries.clear();
    }
    fn clear(&mut self) {
        self.cache.borrow_mut().entries.clear();
    }
}
impl Renderer {
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
        if !text.registered_font_assets().is_empty() {
            return Err(Error::Unsupported {
                command: 0,
                reason: "application font registration not implemented".into(),
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
