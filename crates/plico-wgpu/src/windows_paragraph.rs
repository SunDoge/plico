//! DirectWrite geometry shared by retained measurement and subsequent painting.
use crate::{Error, Renderer, Result};
use plico_native::windows_text::{
    Alignment, Context, Paragraph as Layout, Rasterized, Request, StyleSpan,
};
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
    coverage: Rc<Rasterized>,
    color: [u8; 4],
    images: Rc<RunImages>,
    bytes: usize,
}

const BYTE_LIMIT: usize = 64 * 1024 * 1024;
/// Windows text cache accounting. Excludes COM allocations, shared GPU image
/// textures and externally retained layouts/images; this is not process RSS.
#[derive(Clone, Copy, Debug, Default)]
pub struct WindowsTextCacheStats {
    /// Cached native layouts.
    pub layouts: usize,
    /// Accounted Rust layout payload.
    pub layout_bytes: usize,
    /// Cached coverage/image pairs, one current tint per geometry/phase.
    pub rasters: usize,
    /// Accounted raw coverage, RGBA and conservatively retained layout payload.
    pub raster_bytes: usize,
    /// Successful native coverage generation calls since provider creation.
    pub rasterizations: u64,
    /// Successful CPU correction/tint calls since provider creation.
    pub colorizations: u64,
}
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
    rasters: lru::LruCache<(u64, u64, u64), Raster>,
    raster_bytes: usize,
    rasterizations: u64,
    colorizations: u64,
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
            rasterizations: 0,
            colorizations: 0,
        })
    }
    fn clear(&mut self) {
        self.rasters.clear();
        self.raster_bytes = 0;
        self.entries.clear();
        self.bytes = 0;
    }
    #[cfg(test)]
    fn raster(
        &mut self,
        request: Arc<ParagraphRequest>,
        scale: f64,
        phase: f64,
    ) -> std::result::Result<Rc<RunImages>, ParagraphError> {
        let paragraph = self.layout(request.clone(), scale)?;
        self.raster_layout(paragraph, request.color, scale, phase)
    }
    fn raster_layout(
        &mut self,
        paragraph: Rc<Paragraph>,
        color: [u8; 4],
        scale: f64,
        phase: f64,
    ) -> std::result::Result<Rc<RunImages>, ParagraphError> {
        let key = (
            paragraph.request.fingerprint(),
            scale.to_bits(),
            phase.to_bits(),
        );
        if let Some(raster) = self.rasters.get(&key)
            && Rc::ptr_eq(&raster.paragraph, &paragraph)
            && raster.color == color
        {
            return Ok(raster.images.clone());
        }
        let coverage = match self.rasters.get(&key) {
            Some(raster) if Rc::ptr_eq(&raster.paragraph, &paragraph) => raster.coverage.clone(),
            _ => {
                let coverage = paragraph
                    .layout
                    .rasterize(scale as f32, [phase as f32, 0.])
                    .map_err(|e| rejected(e.to_string()))?;
                self.rasterizations += 1;
                Rc::new(coverage)
            }
        };
        let masks = coverage
            .colorize(color.map(|v| v as f32 / 255.), false)
            .map_err(|e| rejected(e.to_string()))?;
        self.colorizations += 1;
        let mut bytes = paragraph.bytes
            + std::mem::size_of::<Rasterized>()
            + coverage.masks.capacity()
                * std::mem::size_of::<plico_native::windows_text::GlyphMask>()
            + coverage
                .masks
                .iter()
                .map(|m| m.coverage.capacity())
                .sum::<usize>();
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
                    coverage,
                    color,
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
        if request.max_lines != 0 || request.ellipsis {
            return Err(rejected("DirectWrite truncation is not implemented"));
        }
        if request.runs.len() > 4096 {
            return Err(rejected("too many DirectWrite style spans"));
        }
        if request.runs.iter().any(|run| run.line_height.is_some()) {
            return Err(rejected(
                "DirectWrite range line heights are not implemented",
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
        native.letter_spacing = request.letter_spacing;
        native.rtl = plico_native::windows_text::paragraph_is_rtl(&request.text);
        native.line_height = request.line_height.map(|(value, relative)| {
            if relative {
                value * request.font_size
            } else {
                value
            }
        });
        native.alignment = match request.align {
            TextAlign::Start => Alignment::Start,
            TextAlign::Center => Alignment::Center,
            TextAlign::End => Alignment::End,
            TextAlign::Justify => Alignment::Justify,
        };
        native.wrap = request.wrap;
        native.width = request.width.filter(|w| *w != f32::INFINITY);
        let spans = request
            .runs
            .iter()
            .map(|run| {
                if !run.font_weight.is_finite() || !(1. ..=1000.).contains(&run.font_weight) {
                    return Err(rejected("invalid DirectWrite span weight"));
                }
                Ok(StyleSpan {
                    range: run.range.clone(),
                    size: run.font_size,
                    weight: run.font_weight as u16,
                    italic: run.font_italic,
                    color: Some(run.color),
                })
            })
            .collect::<std::result::Result<Vec<_>, _>>()?;
        let layout = self
            .context
            .layout_spans(&native, &spans)
            .map_err(|e| rejected(e.to_string()))?;
        // Account Rust source/index/line payload conservatively; COM allocations
        // are opaque and are not included in this byte budget.
        let bytes = std::mem::size_of::<Paragraph>()
            + geometry.family.len()
            + geometry.text.len() * (2 + 2 * std::mem::size_of::<usize>())
            + std::mem::size_of::<(usize, u32)>()
            + std::mem::size_of_val(geometry.runs.as_ref())
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
    /// Inspect DirectWrite coverage work independently of recoloring and uploads.
    pub fn windows_text_cache_stats(&self) -> WindowsTextCacheStats {
        self.windows_paragraphs
            .as_ref()
            .map_or_else(WindowsTextCacheStats::default, |cache| {
                let cache = cache.borrow();
                WindowsTextCacheStats {
                    layouts: cache.entries.len(),
                    layout_bytes: cache.bytes,
                    rasters: cache.rasters.len(),
                    raster_bytes: cache.raster_bytes,
                    rasterizations: cache.rasterizations,
                    colorizations: cache.colorizations,
                }
            })
    }
    pub(crate) fn windows_paragraph_images(
        &mut self,
        request: Arc<ParagraphRequest>,
        scale: f64,
        phase: f64,
        visible: impl FnOnce([f32; 4]) -> bool,
    ) -> Result<Option<Rc<RunImages>>> {
        if self.windows_paragraphs.is_none() {
            self.windows_paragraphs = Some(Rc::new(RefCell::new(
                Cache::new().map_err(|source| Error::Paragraph { source })?,
            )));
        }
        let mut cache = self.windows_paragraphs.as_ref().unwrap().borrow_mut();
        let paragraph = cache
            .layout(request.clone(), scale)
            .map_err(|source| Error::Paragraph { source })?;
        if !visible(paragraph.layout.ink_bounds()) {
            return Ok(None);
        }
        cache
            .raster_layout(paragraph, request.color, scale, phase)
            .map(Some)
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
    #[test]
    #[ignore = "requires a Windows GPU adapter and installed system fonts"]
    fn offscreen_paragraphs_do_not_generate_native_coverage() {
        let mut renderer = pollster::block_on(Renderer::new()).unwrap();
        let request = request();
        let mut scene = plico_renderer::Scene::new();
        scene
            .commands
            .push(plico_renderer::PaintCommand::Paragraph {
                request: request.clone(),
                transform: kurbo::Affine::translate((20., 1000.)),
            });
        let prepared = renderer.prepare_scene(&scene, [320, 180]).unwrap();
        assert!(prepared.draws().is_empty());
        assert_eq!(renderer.windows_text_cache_stats().rasterizations, 0);
        scene.commands.clear();
        scene
            .commands
            .push(plico_renderer::PaintCommand::Paragraph {
                request,
                transform: kurbo::Affine::translate((20., 20.)),
            });
        let prepared = renderer.prepare_scene(&scene, [320, 180]).unwrap();
        assert!(!prepared.draws().is_empty());
        assert_eq!(renderer.windows_text_cache_stats().rasterizations, 1);
    }
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
    fn rtl_base_direction_reaches_native_editing_and_paint_geometry() {
        let mut cache = Cache::new().unwrap();
        let mut declaration = (*request()).clone();
        declaration.text = "مرحبا".into();
        declaration.width = Some(240.);
        let declaration = Arc::new(declaration);
        let paragraph = cache.layout(declaration.clone(), 1.).unwrap();
        let first = paragraph.caret_rect(0).unwrap();
        let end = paragraph.caret_rect(declaration.text.len()).unwrap();
        assert!(first[0] > end[0]);
        assert!(end[0] > 120.);
        let images = cache.raster(declaration, 1., 0.).unwrap();
        assert!(!images.is_empty());
        assert!(images.iter().all(|(_, origin)| origin[0] > 100));
    }
    #[test]
    fn relative_and_absolute_line_heights_produce_the_same_geometry() {
        let mut cache = Cache::new().unwrap();
        let mut declaration = (*request()).clone();
        declaration.text = "Hello\nWorld".into();
        declaration.line_height = Some((2., true));
        let relative = cache.layout(Arc::new(declaration.clone()), 1.).unwrap();
        declaration.line_height = Some((32., false));
        let absolute = cache.layout(Arc::new(declaration), 1.).unwrap();
        assert_eq!(relative.metrics().height, absolute.metrics().height);
        assert_eq!(relative.metrics().baselines, absolute.metrics().baselines);
        assert_eq!(relative.metrics().first_line_height, 32.);
        assert_eq!(relative.caret_rect(6), absolute.caret_rect(6));
    }
    #[test]
    fn character_spacing_is_forwarded_to_shared_geometry_and_rasterization() {
        let mut cache = Cache::new().unwrap();
        let mut declaration = (*request()).clone();
        declaration.text = "MMMM".into();
        let natural = cache.layout(Arc::new(declaration.clone()), 1.).unwrap();
        declaration.letter_spacing = 3.;
        let declaration = Arc::new(declaration);
        let spaced = cache.layout(declaration.clone(), 1.).unwrap();
        assert!(spaced.metrics().width > natural.metrics().width);
        assert!(!Rc::ptr_eq(&natural, &spaced));
        assert!(spaced.caret_rect(2).unwrap()[0] > natural.caret_rect(2).unwrap()[0]);
        assert!(!cache.raster(declaration, 1., 0.).unwrap().is_empty());
    }
    #[test]
    fn style_spans_update_native_geometry_and_reject_unimplemented_overrides() {
        let mut cache = Cache::new().unwrap();
        let mut declaration = (*request()).clone();
        declaration.text = "Small LARGE".into();
        let natural = cache.layout(Arc::new(declaration.clone()), 1.).unwrap();
        let mut run = plico_renderer::text::TextRun {
            range: 6..11,
            font_size: 40.,
            font_weight: 700.,
            font_italic: true,
            line_height: None,
            color: declaration.color,
        };
        declaration.runs = Arc::from([run.clone()]);
        let styled = cache.layout(Arc::new(declaration.clone()), 1.).unwrap();
        assert!(styled.metrics().height > natural.metrics().height);
        assert!(styled.metrics().width > natural.metrics().width);
        assert!(
            !cache
                .raster(Arc::new(declaration.clone()), 1., 0.)
                .unwrap()
                .is_empty()
        );
        run.color = [255, 0, 0, 255];
        declaration.runs = Arc::from([run.clone()]);
        let images = cache.raster(Arc::new(declaration.clone()), 1., 0.).unwrap();
        assert!(images.iter().any(|(image, _)| {
            image
                .data
                .data()
                .as_chunks::<4>()
                .0
                .iter()
                .any(|p| p[0] > 0 && p[1] == 0 && p[2] == 0 && p[3] > 0)
        }));
        run.line_height = Some((2., true));
        declaration.runs = Arc::from([run]);
        assert!(cache.layout(Arc::new(declaration), 1.).is_err());
    }
    #[test]
    fn native_run_images_reuse_phase_and_reject_unsupported_requests() {
        let mut cache = Cache::new().unwrap();
        let request = request();
        let images = cache.raster(request.clone(), 1., 0.25).unwrap();
        assert!(!images.is_empty());
        assert_eq!((cache.rasterizations, cache.colorizations), (1, 1));
        assert!(Rc::ptr_eq(
            &images,
            &cache.raster(request.clone(), 1., 0.25).unwrap()
        ));
        assert_eq!((cache.rasterizations, cache.colorizations), (1, 1));
        let mut recolored = (*request).clone();
        recolored.color = [255, 0, 0, 255];
        let red = cache.raster(Arc::new(recolored), 1., 0.25).unwrap();
        assert_eq!((cache.rasterizations, cache.colorizations), (1, 2));
        assert!(!Rc::ptr_eq(&images, &red));
        assert_eq!(cache.rasters.len(), 1);
        assert_eq!(red[0].0.width, images[0].0.width);
        assert_eq!(red[0].0.height, images[0].0.height);
        assert_ne!(red[0].0.data.data(), images[0].0.data.data());
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
