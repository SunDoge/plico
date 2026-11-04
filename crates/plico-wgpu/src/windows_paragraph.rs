//! DirectWrite geometry shared by retained measurement and subsequent painting.
use crate::{Error, Renderer, Result};
use plico_native::windows_text::{Alignment, Context, Paragraph as Layout, Request};
use plico_renderer::{
    TextContext,
    paragraph::{
        NativeParagraph, ParagraphEngine, ParagraphError, ParagraphMetrics, ParagraphRequest,
    },
    style::TextAlign,
    text::FontAsset,
};
use std::{cell::RefCell, num::NonZeroUsize, rc::Rc, sync::Arc};

type RunImages = Vec<(peniko::ImageData, [i32; 2])>;
struct Raster {
    paragraph: Rc<Paragraph>,
    images: Rc<RunImages>,
    bytes: usize,
}

const BYTE_LIMIT: usize = 64 * 1024 * 1024;
fn rejected(message: impl Into<String>) -> ParagraphError {
    ParagraphError {
        message: message.into(),
    }
}
struct Paragraph {
    request: Arc<ParagraphRequest>,
    layout: Layout,
    scale: f64,
    bytes: usize,
}
impl NativeParagraph for Paragraph {
    fn metrics(&self) -> ParagraphMetrics {
        let m = self.layout.metrics();
        ParagraphMetrics {
            width: m.width,
            height: m.height,
            intrinsic: [m.min_content_width, m.max_content_width],
            baselines: m.baselines,
            first_line_height: self.layout.lines().first().map_or(0., |l| l.height),
        }
    }
    fn hit_test(&self, x: f32, y: f32) -> usize {
        self.layout.hit_test(x, y).unwrap_or_else(|error| {
            tracing::warn!(%error, "DirectWrite hit testing failed");
            0
        })
    }
    fn caret_rect(&self, byte: usize) -> Option<[f32; 4]> {
        self.layout
            .caret_rect(byte, self.scale as f32)
            .ok()
            .map(|[x, y, w, h]| [x, y, x + w, y + h])
    }
    fn selection_rects(&self, range: std::ops::Range<usize>) -> Vec<[f32; 4]> {
        match self.layout.selection_rects(range) {
            Ok(rects) => rects
                .into_iter()
                .map(|[x, y, w, h]| [x, y, x + w, y + h])
                .collect(),
            Err(error) => {
                tracing::warn!(%error, "DirectWrite selection geometry failed");
                Vec::new()
            }
        }
    }
}
pub(crate) struct Cache {
    context: Context,
    entries: lru::LruCache<(u64, u64), Rc<Paragraph>>,
    bytes: usize,
    font_error: bool,
    rasters: lru::LruCache<(u64, u64, u32, u64), Raster>,
    raster_bytes: usize,
}
impl Cache {
    fn new() -> std::result::Result<Self, ParagraphError> {
        Ok(Self {
            context: Context::new().map_err(|e| rejected(e.to_string()))?,
            entries: lru::LruCache::new(NonZeroUsize::new(512).unwrap()),
            bytes: 0,
            font_error: false,
            rasters: lru::LruCache::new(NonZeroUsize::new(512).unwrap()),
            raster_bytes: 0,
        })
    }
    fn clear(&mut self) {
        self.rasters.clear();
        self.raster_bytes = 0;
        self.entries.clear();
        self.bytes = 0;
    }
    fn raster(
        &mut self,
        request: Arc<ParagraphRequest>,
        scale: f64,
        phase: f64,
    ) -> std::result::Result<Rc<RunImages>, ParagraphError> {
        let paragraph = self.layout(request.clone(), scale)?;
        let key = (
            paragraph.request.fingerprint(),
            scale.to_bits(),
            u32::from_le_bytes(request.color),
            phase.to_bits(),
        );
        if let Some(raster) = self.rasters.get(&key)
            && Rc::ptr_eq(&raster.paragraph, &paragraph)
        {
            return Ok(raster.images.clone());
        }
        let coverage = paragraph
            .layout
            .rasterize(scale as f32, [phase as f32, 0.])
            .map_err(|e| rejected(e.to_string()))?;
        let masks = coverage
            .colorize(request.color.map(|v| v as f32 / 255.), false)
            .map_err(|e| rejected(e.to_string()))?;
        let mut bytes = paragraph.bytes;
        let images: RunImages = masks
            .into_iter()
            .map(|m| {
                bytes += m.rgba.len();
                (
                    peniko::ImageData {
                        data: peniko::Blob::new(Arc::new(m.rgba)),
                        format: peniko::ImageFormat::Rgba8,
                        alpha_type: peniko::ImageAlphaType::AlphaPremultiplied,
                        width: m.size[0],
                        height: m.size[1],
                    },
                    m.origin,
                )
            })
            .collect();
        let images = Rc::new(images);
        if bytes <= BYTE_LIMIT {
            if let Some((_, old)) = self.rasters.push(
                key,
                Raster {
                    paragraph,
                    images: images.clone(),
                    bytes,
                },
            ) {
                self.raster_bytes -= old.bytes;
            }
            self.raster_bytes += bytes;
            while self.raster_bytes > BYTE_LIMIT {
                if let Some((_, old)) = self.rasters.pop_lru() {
                    self.raster_bytes -= old.bytes;
                } else {
                    break;
                }
            }
        }
        Ok(images)
    }

    fn layout(
        &mut self,
        request: Arc<ParagraphRequest>,
        scale: f64,
    ) -> std::result::Result<Rc<Paragraph>, ParagraphError> {
        if self.font_error {
            return Err(rejected(
                "DirectWrite application font registration is not implemented",
            ));
        }
        if !scale.is_finite() || scale <= 0. || scale > 8. {
            return Err(rejected("invalid DirectWrite device scale"));
        }
        if !request.runs.is_empty()
            || request.letter_spacing != 0.
            || request.line_height.is_some()
            || request.max_lines != 0
            || request.ellipsis
        {
            return Err(rejected(
                "DirectWrite rich spans, spacing and truncation are not implemented",
            ));
        }
        if !request.font_weight.is_finite() || !(1. ..=1000.).contains(&request.font_weight) {
            return Err(rejected("invalid DirectWrite font weight"));
        }
        if request.family.len() > 4096 || request.text.len() > 1024 * 1024 {
            return Err(rejected("oversized DirectWrite paragraph request"));
        }
        let mut geometry = (*request).clone();
        geometry.color = [0; 4];
        let key = (geometry.fingerprint(), scale.to_bits());
        if let Some(p) = self.entries.get(&key)
            && *p.request == geometry
        {
            return Ok(p.clone());
        }
        // This adapter accepts an unquoted comma-separated preference list.
        // Reject CSS escaping rather than silently selecting the wrong family.
        if request.family.contains(['\'', '"', '\\']) {
            return Err(rejected(
                "quoted/escaped DirectWrite family lists are not implemented",
            ));
        }
        let candidates: Vec<_> = request.family.split(',').map(str::trim).collect();
        let family = self
            .context
            .resolve_family(&candidates)
            .map_err(|e| rejected(e.to_string()))?
            .ok_or_else(|| rejected("no requested DirectWrite font family is installed"))?;
        let mut native = Request::new(&request.text);
        native.family = &family;
        native.size = request.font_size;
        native.weight = request.font_weight as u16;
        native.italic = request.italic;
        native.alignment = match request.align {
            TextAlign::Start => Alignment::Start,
            TextAlign::Center => Alignment::Center,
            TextAlign::End => Alignment::End,
            TextAlign::Justify => Alignment::Justify,
        };
        native.wrap = request.wrap;
        native.width = request.width.filter(|w| *w != f32::INFINITY);
        let layout = self
            .context
            .layout(&native)
            .map_err(|e| rejected(e.to_string()))?;
        // Account Rust source/index/line payload conservatively; COM allocations
        // are opaque and are not included in this byte budget.
        let bytes = std::mem::size_of::<Paragraph>()
            + geometry.family.len()
            + geometry.text.len() * (2 + 2 * std::mem::size_of::<usize>())
            + std::mem::size_of_val(layout.lines());
        let p = Rc::new(Paragraph {
            request: Arc::new(geometry),
            layout,
            scale,
            bytes,
        });
        if bytes <= BYTE_LIMIT {
            if let Some((_, old)) = self.entries.push(key, p.clone()) {
                self.bytes -= old.bytes;
            }
            self.bytes += bytes;
            while self.bytes > BYTE_LIMIT {
                if let Some((_, old)) = self.entries.pop_lru() {
                    self.bytes -= old.bytes;
                } else {
                    break;
                }
            }
        }
        Ok(p)
    }
}
struct Engine {
    cache: Rc<RefCell<Cache>>,
    scale: f64,
}
impl ParagraphEngine for Engine {
    fn name(&self) -> &'static str {
        "directwrite-wgpu"
    }
    fn supports_editing(&self) -> bool {
        true
    }
    fn set_device_scale(&mut self, scale: f64) -> bool {
        if self.scale == scale || !scale.is_finite() || scale <= 0. || scale > 8. {
            return false;
        }
        self.scale = scale;
        true
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
    fn register_font(&mut self, _asset: &FontAsset) {
        let mut cache = self.cache.borrow_mut();
        cache.font_error = true;
        cache.clear();
    }
    fn clear(&mut self) {
        self.cache.borrow_mut().clear();
    }
}
impl Renderer {
    pub(crate) fn windows_paragraph_images(
        &mut self,
        request: Arc<ParagraphRequest>,
        scale: f64,
        phase: f64,
    ) -> Result<Rc<RunImages>> {
        if self.windows_paragraphs.is_none() {
            self.windows_paragraphs = Some(Rc::new(RefCell::new(
                Cache::new().map_err(|source| Error::Paragraph { source })?,
            )));
        }
        self.windows_paragraphs
            .as_ref()
            .unwrap()
            .borrow_mut()
            .raster(request, scale, phase)
            .map_err(|source| Error::Paragraph { source })
    }
    /// Drop derived DirectWrite layouts and run images; live geometry remains valid.
    pub fn trim_text_cache(&mut self) {
        if let Some(cache) = &self.windows_paragraphs {
            cache.borrow_mut().clear();
        }
        self.prepared_cache = None;
        self.invalidate_scene_damage();
    }

    /// Install shared DirectWrite measurement, editing and painting geometry.
    /// Unsupported declarations remain observable as paragraph errors.
    pub fn configure_text(&mut self, text: &mut TextContext, scale: f64) -> Result<()> {
        if !scale.is_finite() || scale <= 0. || scale > 8. {
            return Err(Error::InvalidInput {
                reason: "invalid system text device scale".into(),
            });
        }
        if self.windows_paragraphs.is_none() {
            self.windows_paragraphs = Some(Rc::new(RefCell::new(
                Cache::new().map_err(|source| Error::Paragraph { source })?,
            )));
        }
        self.prepared_cache = None;
        self.invalidate_scene_damage();
        text.set_paragraph_engine(Box::new(Engine {
            cache: self.windows_paragraphs.as_ref().unwrap().clone(),
            scale,
        }));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn request() -> Arc<ParagraphRequest> {
        Arc::new(ParagraphRequest {
            font_revision: 0,
            text: "Hello ffi".into(),
            font_size: 16.,
            font_weight: 400.,
            italic: false,
            letter_spacing: 0.,
            line_height: None,
            align: TextAlign::Start,
            color: [20, 30, 40, 255],
            runs: Arc::from([]),
            family: "system-ui".into(),
            width: None,
            wrap: true,
            max_lines: 0,
            ellipsis: false,
        })
    }
    #[test]
    fn shared_native_layout_reuses_color_independent_geometry() {
        let mut cache = Cache::new().unwrap();
        let request = request();
        let first = cache.layout(request.clone(), 1.).unwrap();
        let mut colored = (*request).clone();
        colored.color = [255, 0, 0, 255];
        assert!(Rc::ptr_eq(
            &first,
            &cache.layout(Arc::new(colored), 1.).unwrap()
        ));
        let metrics = first.metrics();
        assert!(metrics.width > 0. && metrics.intrinsic[1] >= metrics.intrinsic[0]);
        let caret = first.caret_rect(2).unwrap();
        assert_eq!(caret[2] - caret[0], 1.);
        let scaled = cache.layout(request, 2.).unwrap();
        let caret = scaled.caret_rect(2).unwrap();
        assert_eq!(caret[2] - caret[0], 0.5);
        cache.clear();
        assert_eq!(cache.bytes, 0);
        assert!(first.caret_rect(2).is_some());
    }
    #[test]
    fn alignment_is_part_of_the_shared_geometry_and_raster_request() {
        let mut cache = Cache::new().unwrap();
        let mut declaration = (*request()).clone();
        declaration.width = Some(240.);
        let start = cache.layout(Arc::new(declaration.clone()), 1.).unwrap();
        declaration.align = TextAlign::End;
        let declaration = Arc::new(declaration);
        let end = cache.layout(declaration.clone(), 1.).unwrap();
        assert!(!Rc::ptr_eq(&start, &end));
        assert!(end.caret_rect(0).unwrap()[0] > start.caret_rect(0).unwrap()[0]);
        let images = cache.raster(declaration, 1., 0.).unwrap();
        assert!(!images.is_empty());
        assert!(images[0].1[0] as f32 >= end.caret_rect(0).unwrap()[0] - 2.);
    }
    #[test]
    fn native_run_images_reuse_phase_and_reject_unsupported_requests() {
        let mut cache = Cache::new().unwrap();
        let request = request();
        let images = cache.raster(request.clone(), 1., 0.25).unwrap();
        assert!(!images.is_empty());
        assert!(Rc::ptr_eq(
            &images,
            &cache.raster(request.clone(), 1., 0.25).unwrap()
        ));
        assert!(!Rc::ptr_eq(
            &images,
            &cache.raster(request.clone(), 1., 0.5).unwrap()
        ));
        assert!(cache.raster_bytes <= BYTE_LIMIT);
        let mut unsupported = (*request).clone();
        unsupported.ellipsis = true;
        assert!(cache.layout(Arc::new(unsupported), 1.).is_err());
        cache.clear();
        assert_eq!(cache.raster_bytes, 0);
        assert!(cache.rasters.is_empty());
        assert!(!images[0].0.data.data().is_empty());
    }
}
