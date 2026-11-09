//! Experimental retained CoreText geometry and bounded per-line RGBA cache.
use crate::{Error, Renderer, Result};
use plico_native::macos_text::{
    Alignment, FontCollection, FontStyle, FontWeight, Line, Paragraph as Layout, Request, StyleSpan,
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
const LIMIT: usize = 64 * 1024 * 1024;
const INTRINSIC_LIMIT: usize = 8 * 1024 * 1024;
type Images = Vec<(peniko::ImageData, [i32; 2])>;
fn rejected(message: impl Into<String>) -> ParagraphError {
    ParagraphError {
        message: message.into(),
    }
}

pub(crate) struct PreparedParagraph {
    pub images: Rc<Images>,
    pub ink: [f64; 4],
    /// Exact physical line-limit bottom; CPU bitmap clips round outward.
    pub clip_height: Option<f64>,
}

struct Paragraph {
    request: Arc<ParagraphRequest>,
    layout: Layout,
    ink: [f64; 4],
    line_inks: Vec<Option<[f64; 4]>>,
    bytes: usize,
    intrinsic: [f32; 2],
    visible_lines: usize,
    visible_end: usize,
    visible_size: [f64; 2],
}
impl NativeParagraph for Paragraph {
    fn metrics(&self) -> ParagraphMetrics {
        let size = self.visible_size;
        let lines = &self.layout.lines()[..self.visible_lines];
        ParagraphMetrics {
            width: size[0] as f32,
            height: size[1] as f32,
            intrinsic: self.intrinsic,
            baselines: [
                lines.first().map(|l| l.baseline[1] as f32),
                lines.last().map(|l| l.baseline[1] as f32),
            ],
            first_line_height: lines
                .first()
                .map_or(0., |l| (l.vertical_bounds[1] - l.vertical_bounds[0]) as f32),
        }
    }
    fn hit_test(&self, x: f32, y: f32) -> usize {
        if !x.is_finite() || !y.is_finite() {
            return 0;
        }
        let last = &self.layout.lines()[self.visible_lines - 1];
        let y = if f64::from(y) >= self.visible_size[1] {
            (last.vertical_bounds[0] + last.vertical_bounds[1]) * 0.5
        } else {
            f64::from(y)
        };
        self.layout
            .hit_test([f64::from(x), y])
            .unwrap_or(0)
            .min(self.visible_end)
    }
    fn line_range_at(&self, y: f32) -> Option<std::ops::Range<usize>> {
        if !y.is_finite() {
            return None;
        }
        let lines = &self.layout.lines()[..self.visible_lines];
        let line = lines
            .iter()
            .find(|line| f64::from(y) < line.vertical_bounds[1])
            .or_else(|| lines.last())?;
        Some(line.range.start.min(self.visible_end)..line.range.end.min(self.visible_end))
    }
    fn caret_rect(&self, byte: usize) -> Option<[f32; 4]> {
        if !self.request.text.is_char_boundary(byte) {
            return None;
        }
        if self.visible_end < self.request.text.len() && byte >= self.visible_end {
            // Native paragraph carets prefer the next soft line at a boundary;
            // use the final visible line directly to avoid entering hidden text.
            let last = &self.layout.lines()[self.visible_lines - 1];
            let x = last.baseline[0]
                + last
                    .line
                    .caret_offsets(last.line.text().len())
                    .ok()?
                    .primary;
            return Some([
                x as f32,
                last.vertical_bounds[0] as f32,
                (x + 1.) as f32,
                last.vertical_bounds[1] as f32,
            ]);
        }
        self.layout
            .caret_rects(byte)
            .ok()
            .map(|r| r[0].map(|v| v as f32))
    }
    fn selection_rects(&self, range: std::ops::Range<usize>) -> Vec<[f32; 4]> {
        if range.start > range.end
            || !self.request.text.is_char_boundary(range.start)
            || !self.request.text.is_char_boundary(range.end)
        {
            return Vec::new();
        }
        let range = range.start.min(self.visible_end)..range.end.min(self.visible_end);
        match self.layout.selection_rects(range) {
            Ok(rects) => rects.into_iter().map(|r| r.map(|v| v as f32)).collect(),
            Err(error) => {
                tracing::warn!(%error, "CoreText selection geometry failed");
                Vec::new()
            }
        }
    }
}
#[derive(Clone, PartialEq, Eq, Hash)]
struct RasterKey {
    request: u64,
    line: usize,
    scale: u64,
    phase: [u64; 2],
    clip: [i32; 4],
    color: [u8; 4],
}
struct Intrinsic {
    request: Arc<ParagraphRequest>,
    widths: [f32; 2],
    bytes: usize,
}
struct Raster {
    paragraph: Rc<Paragraph>,
    image: peniko::ImageData,
    origin: [i32; 2],
    bytes: usize,
}
/// Accounted layout/raster/intrinsic payload only; excludes native objects,
/// GPU textures, bounded family names and externally retained paragraphs/images.
/// This is not a process RSS estimate.
#[derive(Clone, Copy, Debug, Default)]
pub struct MacosTextCacheStats {
    /// Retained native paragraph entries.
    pub layouts: usize,
    /// Conservatively accounted Rust layout payload.
    pub layout_bytes: usize,
    /// Retained clipped native line images.
    pub rasters: usize,
    /// Accounted RGBA plus retained paragraph payload.
    pub raster_bytes: usize,
    /// Native bitmap generation calls since provider creation.
    pub rasterizations: u64,
    /// Width-only measurement cache entries, shared across wrapping constraints.
    pub intrinsic_entries: usize,
    /// Accounted retained width-measurement source payload.
    pub intrinsic_bytes: usize,
    /// Native intrinsic measurement calls since provider creation.
    pub intrinsic_measurements: u64,
}
pub(crate) struct Cache {
    layouts: lru::LruCache<u64, Rc<Paragraph>>,
    layout_bytes: usize,
    rasters: lru::LruCache<RasterKey, Raster>,
    raster_bytes: usize,
    rasterizations: u64,
    fonts: FontCollection,
    families: lru::LruCache<Arc<str>, Arc<str>>,
    #[cfg(test)]
    family_probes: u64,
    font_assets: Vec<FontAsset>,
    intrinsic: lru::LruCache<u64, Intrinsic>,
    intrinsic_bytes: usize,
    intrinsic_measurements: u64,
}
impl Cache {
    fn new() -> Self {
        Self {
            layouts: lru::LruCache::new(NonZeroUsize::new(512).unwrap()),
            layout_bytes: 0,
            rasters: lru::LruCache::new(NonZeroUsize::new(1024).unwrap()),
            raster_bytes: 0,
            rasterizations: 0,
            fonts: FontCollection::default(),
            families: lru::LruCache::new(NonZeroUsize::new(128).unwrap()),
            #[cfg(test)]
            family_probes: 0,
            font_assets: Vec::new(),
            intrinsic: lru::LruCache::new(NonZeroUsize::new(128).unwrap()),
            intrinsic_bytes: 0,
            intrinsic_measurements: 0,
        }
    }
    fn clear(&mut self) {
        self.families.clear();
        self.layouts.clear();
        self.rasters.clear();
        self.layout_bytes = 0;
        self.raster_bytes = 0;
        self.intrinsic.clear();
        self.intrinsic_bytes = 0;
    }
    fn resolve_family(
        &mut self,
        requested: &Arc<str>,
    ) -> std::result::Result<Arc<str>, ParagraphError> {
        if let Some(family) = self.families.get(requested.as_ref()) {
            return Ok(family.clone());
        }
        for candidate in requested.split(',').map(str::trim) {
            let mapped = match candidate {
                "" | "sans-serif" | "system-ui" => "system-ui",
                "serif" => "Times",
                "monospace" | "ui-monospace" => "ui-monospace",
                other => other,
            };
            let known = self.fonts.contains_family(mapped)
                || matches!(mapped, "system-ui" | "ui-monospace");
            let installed = if known {
                true
            } else {
                let mut probe = Request::new("");
                probe.family = mapped;
                probe.fonts = Some(&self.fonts);
                #[cfg(test)]
                {
                    self.family_probes += 1;
                }
                let face = Line::new(&probe).map_err(|e| rejected(e.to_string()))?;
                face.font_family().eq_ignore_ascii_case(mapped)
                    || face.font_postscript_name().eq_ignore_ascii_case(mapped)
            };
            if installed {
                let family: Arc<str> = mapped.into();
                self.families.put(requested.clone(), family.clone());
                return Ok(family);
            }
        }
        Err(rejected("no requested CoreText family is installed"))
    }
    #[tracing::instrument(name = "plico_wgpu.coretext_layout", skip_all, fields(text_bytes = request.text.len()))]
    fn layout(
        &mut self,
        request: Arc<ParagraphRequest>,
    ) -> std::result::Result<Rc<Paragraph>, ParagraphError> {
        if request.runs.len() > 4096 {
            return Err(rejected("too many CoreText rich spans"));
        }
        if request.runs.iter().any(|run| {
            run.range.start > run.range.end
                || run.range.end > request.text.len()
                || !request.text.is_char_boundary(run.range.start)
                || !request.text.is_char_boundary(run.range.end)
                || !run.font_size.is_finite()
                || run.font_size <= 0.
                || run.font_size > 4096.
                || run.line_height.is_some()
        }) {
            return Err(rejected(
                "invalid CoreText rich span or per-span line height",
            ));
        }
        if request.ellipsis
            && (request.wrap
                || request.max_lines > 0
                || request
                    .text
                    .contains(['\n', '\r', '\u{85}', '\u{2028}', '\u{2029}']))
        {
            return Err(rejected("CoreText multiline ellipsis is not implemented"));
        }
        if request.family.len() > 4096 || request.family.contains(['\'', '"', '\\']) {
            return Err(rejected("invalid/unsupported CoreText family list"));
        }
        if request
            .width
            .is_some_and(|width| width.is_nan() || width < 0.)
        {
            return Err(rejected("invalid CoreText paragraph width"));
        }
        let mut geometry = (*request).clone();
        geometry.color = [0; 4];
        let key = geometry.fingerprint();
        if let Some(p) = self.layouts.get(&key)
            && *p.request == geometry
        {
            return Ok(p.clone());
        }
        let family = self.resolve_family(&request.family)?;
        let mut source_native = Request::new(&request.text);
        source_native.family = &family;
        source_native.fonts = Some(&self.fonts);
        source_native.size = request.font_size as f64;
        source_native.letter_spacing = request.letter_spacing as f64;
        source_native.style = Some(FontStyle {
            weight: FontWeight::from_css(request.font_weight as f64)
                .map_err(|e| rejected(e.to_string()))?,
            italic: request.italic,
        });
        let source_spans = request
            .runs
            .iter()
            .map(|run| {
                Ok(StyleSpan {
                    range: run.range.clone(),
                    size: f64::from(run.font_size),
                    style: FontStyle {
                        weight: FontWeight::from_css(f64::from(run.font_weight))?,
                        italic: run.font_italic,
                    },
                    color: run.color,
                })
            })
            .collect::<plico_native::macos_text::Result<Vec<_>>>()
            .map_err(|error| rejected(error.to_string()))?;
        let height = request.line_height.map(|(h, relative)| {
            if relative {
                h as f64 * source_native.size
            } else {
                h as f64
            }
        });
        let requested_width = request.width.map(f64::from);
        let ellipsized = match (request.ellipsis, requested_width) {
            // Intrinsic measurement has no constraint. Preserve the full text;
            // the final layout pass supplies the resolved content-box width.
            (true, None) | (true, Some(f64::INFINITY)) => None,
            // A collapsed flex item has no visible text area. Keep the request
            // valid and clamp its editing geometry to the source start.
            (true, Some(0.)) => Some((String::new(), 0)),
            (true, Some(width)) => {
                Line::ellipsized_text_styled(&source_native, &source_spans, width)
                    .map_err(|e| rejected(e.to_string()))?
            }
            (false, _) => None,
        };
        let width = requested_width.filter(|width| width.is_finite() && *width > 0.);
        let visible_end_override = ellipsized.as_ref().map(|(_, end)| *end);
        let native = Request {
            text: ellipsized
                .as_ref()
                .map_or(source_native.text, |(text, _)| text.as_str()),
            ..source_native
        };
        let native_spans = if let Some((_, end)) = &ellipsized {
            source_spans
                .iter()
                .filter(|span| span.range.start < *end)
                .map(|span| StyleSpan {
                    range: span.range.start..span.range.end.min(*end),
                    ..span.clone()
                })
                .collect::<Vec<_>>()
        } else {
            source_spans.clone()
        };
        let align = match request.align {
            TextAlign::Start => Alignment::Start,
            TextAlign::Center => Alignment::Center,
            TextAlign::End => Alignment::End,
            TextAlign::Justify => Alignment::Justify,
        };
        let layout = if request.wrap {
            Layout::with_alignment_styled(&native, &native_spans, width, height, align)
        } else {
            Layout::without_wrapping_styled(&native, &native_spans, width, height, align)
        }
        .map_err(|e| rejected(e.to_string()))?;
        let mut source = geometry.clone();
        source.width = None;
        source.wrap = false;
        source.align = TextAlign::Start;
        source.line_height = None;
        source.max_lines = 0;
        let source_key = source.fingerprint();
        let intrinsic = if let Some(cached) = self.intrinsic.get(&source_key)
            && *cached.request == source
        {
            cached.widths
        } else {
            let widths = Layout::measure_intrinsic_styled(&source_native, &source_spans)
                .map_err(|e| rejected(e.to_string()))?
                .map(|v| v as f32);
            self.intrinsic_measurements += 1;
            let bytes = std::mem::size_of::<Intrinsic>()
                + std::mem::size_of::<ParagraphRequest>()
                + source.text.len()
                + source.family.len()
                + 64;
            if bytes <= INTRINSIC_LIMIT {
                if let Some((_, old)) = self.intrinsic.push(
                    source_key,
                    Intrinsic {
                        request: Arc::new(source),
                        widths,
                        bytes,
                    },
                ) {
                    self.intrinsic_bytes -= old.bytes;
                }
                self.intrinsic_bytes += bytes;
                while self.intrinsic_bytes > INTRINSIC_LIMIT {
                    if let Some((_, old)) = self.intrinsic.pop_lru() {
                        self.intrinsic_bytes -= old.bytes;
                    } else {
                        break;
                    }
                }
            }
            widths
        };
        let visible_lines = if request.max_lines == 0 {
            layout.lines().len()
        } else {
            layout.lines().len().min(request.max_lines)
        };
        let visible_end =
            visible_end_override.unwrap_or_else(|| layout.lines()[visible_lines - 1].range.end);
        let visible_size = [
            layout.lines()[..visible_lines]
                .iter()
                .map(|line| line.line.metrics().width)
                .fold(0., f64::max),
            layout.lines()[visible_lines - 1].vertical_bounds[1],
        ];
        let mut ink: Option<[f64; 4]> = None;
        let mut line_inks = Vec::with_capacity(visible_lines);
        for line in &layout.lines()[..visible_lines] {
            let bounds = line
                .line
                .ink_bounds()
                .map_err(|e| rejected(e.to_string()))?;
            line_inks.push(bounds);
            if let Some(b) = bounds {
                let b = [
                    b[0] + line.baseline[0],
                    b[1] + line.baseline[1],
                    b[2] + line.baseline[0],
                    b[3] + line.baseline[1],
                ];
                ink = Some(ink.map_or(b, |a| {
                    [
                        a[0].min(b[0]),
                        a[1].min(b[1]),
                        a[2].max(b[2]),
                        a[3].max(b[3]),
                    ]
                }));
            }
        }
        if visible_lines < layout.lines().len() {
            ink = ink.and_then(|mut bounds| {
                bounds[3] = bounds[3].min(visible_size[1]);
                (bounds[2] > bounds[0] && bounds[3] > bounds[1]).then_some(bounds)
            });
        }
        let bytes = std::mem::size_of::<Paragraph>()
            + geometry.family.len()
            + geometry.text.len() * 32
            + std::mem::size_of_val(layout.lines())
            + layout.lines().len() * 128
            + std::mem::size_of_val(line_inks.as_slice());
        let p = Rc::new(Paragraph {
            request: Arc::new(geometry),
            layout,
            ink: ink.unwrap_or([0.; 4]),
            line_inks,
            bytes,
            intrinsic,
            visible_lines,
            visible_end,
            visible_size,
        });
        if bytes <= LIMIT {
            if let Some((_, old)) = self.layouts.push(key, p.clone()) {
                self.layout_bytes -= old.bytes;
            }
            self.layout_bytes += bytes;
            while self.layout_bytes > LIMIT {
                if let Some((_, old)) = self.layouts.pop_lru() {
                    self.layout_bytes -= old.bytes;
                } else {
                    break;
                }
            }
        }
        Ok(p)
    }
    #[tracing::instrument(name = "plico_wgpu.coretext_images", skip_all)]
    fn images(
        &mut self,
        paragraph: Rc<Paragraph>,
        color: [u8; 4],
        scale: f64,
        phase: f64,
        mut clip: [f64; 4],
    ) -> std::result::Result<Rc<Images>, ParagraphError> {
        if paragraph.visible_lines < paragraph.layout.lines().len() {
            clip[3] = clip[3].min(paragraph.visible_size[1] * scale);
        }
        let mut images = Vec::new();
        let mut frame_bytes = 0usize;
        let fingerprint = paragraph.request.fingerprint();
        for (index, line) in paragraph.layout.lines()[..paragraph.visible_lines]
            .iter()
            .enumerate()
        {
            let Some(ink) = paragraph.line_inks[index] else {
                continue;
            };
            let position = [line.baseline[0] * scale + phase, line.baseline[1] * scale];
            let base = position.map(f64::floor);
            let phases = [position[0] - base[0], position[1] - base[1]];
            let mut bounds = [
                (ink[0] * scale + position[0]).floor() - 2.,
                (ink[1] * scale + position[1]).floor() - 2.,
                (ink[2] * scale + position[0]).ceil() + 2.,
                (ink[3] * scale + position[1]).ceil() + 2.,
            ];
            // A max-lines boundary is part of the paragraph, unlike a moving
            // viewport. Preserve that hard raster bound even for reusable lines.
            if paragraph.visible_lines < paragraph.layout.lines().len() {
                bounds[3] = bounds[3].min((paragraph.visible_size[1] * scale).ceil());
            }
            let clipped = [
                bounds[0].max(clip[0].floor()),
                bounds[1].max(clip[1].floor()),
                bounds[2].min(clip[2].ceil()),
                bounds[3].min(clip[3].ceil()),
            ];
            if clipped[0] >= clipped[2] || clipped[1] >= clipped[3] {
                continue;
            }
            if clipped
                .iter()
                .chain(base.iter())
                .any(|v| !v.is_finite() || v.abs() > 1e9)
            {
                return Err(rejected("CoreText raster bounds overflow"));
            }
            let local = [
                clipped[0] - base[0],
                clipped[1] - base[1],
                clipped[2] - base[0],
                clipped[3] - base[1],
            ];
            let local = crate::native_raster_window::cache_window(
                [
                    bounds[0] - base[0],
                    bounds[1] - base[1],
                    bounds[2] - base[0],
                    bounds[3] - base[1],
                ],
                local,
            );
            let key = RasterKey {
                request: fingerprint,
                line: index,
                scale: scale.to_bits(),
                phase: phases.map(f64::to_bits),
                clip: local.map(|v| v as i32),
                color,
            };
            if let Some(raster) = self.rasters.get(&key)
                && Rc::ptr_eq(&raster.paragraph, &paragraph)
            {
                frame_bytes += raster.image.data.data().len();
                if frame_bytes > LIMIT {
                    return Err(rejected("CoreText frame raster budget exceeded"));
                }
                images.push((
                    raster.image.clone(),
                    [
                        raster.origin[0] + base[0] as i32,
                        raster.origin[1] + base[1] as i32,
                    ],
                ));
                continue;
            }
            let Some(raster) = line
                .line
                .rasterize_clipped(scale, phases, color, local)
                .map_err(|e| rejected(e.to_string()))?
            else {
                continue;
            };
            self.rasterizations += 1;
            frame_bytes += raster.pixels.len();
            if frame_bytes > LIMIT {
                return Err(rejected("CoreText frame raster budget exceeded"));
            }
            let bytes = raster.pixels.len() + paragraph.bytes;
            let image = peniko::ImageData {
                data: peniko::Blob::new(Arc::new(raster.pixels)),
                format: peniko::ImageFormat::Rgba8,
                alpha_type: peniko::ImageAlphaType::AlphaPremultiplied,
                width: raster.size[0],
                height: raster.size[1],
            };
            images.push((
                image.clone(),
                [
                    raster.origin[0] + base[0] as i32,
                    raster.origin[1] + base[1] as i32,
                ],
            ));
            if bytes <= LIMIT {
                if let Some((_, old)) = self.rasters.push(
                    key,
                    Raster {
                        paragraph: paragraph.clone(),
                        image,
                        origin: raster.origin,
                        bytes,
                    },
                ) {
                    self.raster_bytes -= old.bytes;
                }
                self.raster_bytes += bytes;
                while self.raster_bytes > LIMIT {
                    if let Some((_, old)) = self.rasters.pop_lru() {
                        self.raster_bytes -= old.bytes;
                    } else {
                        break;
                    }
                }
            }
        }
        Ok(Rc::new(images))
    }
}
struct Engine {
    cache: Rc<RefCell<Cache>>,
    scale: f64,
}
impl ParagraphEngine for Engine {
    fn name(&self) -> &'static str {
        "coretext-wgpu"
    }
    fn supports_editing(&self) -> bool {
        true
    }
    fn set_device_scale(&mut self, scale: f64) -> bool {
        if !scale.is_finite() || scale <= 0. || scale > 8. || self.scale == scale {
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
            .layout(request)
            .map(|p| p as Rc<dyn NativeParagraph>)
    }
    fn register_font(&mut self, asset: &FontAsset) {
        let mut cache = self.cache.borrow_mut();
        if cache.font_assets.iter().any(|registered| {
            registered.family() == asset.family() && std::ptr::eq(registered.bytes(), asset.bytes())
        }) {
            return;
        }
        match cache.fonts.register(asset.bytes(), asset.family()) {
            Ok(()) => {
                cache.font_assets.push(asset.clone());
                cache.clear();
            }
            Err(error) => tracing::warn!(%error, "CoreText rejected application font"),
        }
    }
    fn clear(&mut self) {
        self.cache.borrow_mut().clear();
    }
}
impl Renderer {
    /// Install shared CoreText measurement/editing geometry (experimental).
    pub fn configure_text(&mut self, text: &mut TextContext, scale: f64) -> Result<()> {
        if !scale.is_finite() || scale <= 0. || scale > 8. {
            return Err(Error::InvalidInput {
                reason: "invalid CoreText device scale".into(),
            });
        }
        let cache = self
            .macos_paragraphs
            .get_or_insert_with(|| Rc::new(RefCell::new(Cache::new())))
            .clone();
        text.set_paragraph_engine(Box::new(Engine { cache, scale }));
        self.prepared_cache.clear();
        self.invalidate_scene_damage();
        Ok(())
    }
    pub(crate) fn macos_paragraph_images(
        &mut self,
        request: Arc<ParagraphRequest>,
        scale: f64,
        phase: f64,
        clip: [f64; 4],
    ) -> Result<PreparedParagraph> {
        if !scale.is_finite()
            || scale <= 0.
            || scale > 8.
            || !phase.is_finite()
            || !(0. ..1.).contains(&phase)
            || clip.iter().any(|v| !v.is_finite() || v.abs() > 1e9)
            || clip[2] < clip[0]
            || clip[3] < clip[1]
        {
            return Err(Error::InvalidInput {
                reason: "invalid CoreText sampling/clip".into(),
            });
        }
        let cache = self
            .macos_paragraphs
            .get_or_insert_with(|| Rc::new(RefCell::new(Cache::new())));
        let mut cache = cache.borrow_mut();
        let p = cache
            .layout(request.clone())
            .map_err(|source| Error::Paragraph { source })?;
        let ink = p.ink;
        let clip_height =
            (p.visible_lines < p.layout.lines().len()).then_some(p.visible_size[1] * scale);
        let images = cache
            .images(p, request.color, scale, phase, clip)
            .map_err(|source| Error::Paragraph { source })?;
        Ok(PreparedParagraph {
            images,
            ink,
            clip_height,
        })
    }
    /// Clear derived native text caches; live externally retained layouts stay valid.
    pub fn trim_text_cache(&mut self) {
        if let Some(c) = &self.macos_paragraphs {
            c.borrow_mut().clear();
        }
        self.prepared_cache.clear();
        self.invalidate_scene_damage();
    }
    /// Inspect bounded Rust-owned CoreText caches, excluding native/GPU allocations.
    pub fn macos_text_cache_stats(&self) -> MacosTextCacheStats {
        self.macos_paragraphs
            .as_ref()
            .map_or_else(MacosTextCacheStats::default, |c| {
                let c = c.borrow();
                MacosTextCacheStats {
                    layouts: c.layouts.len(),
                    layout_bytes: c.layout_bytes,
                    rasters: c.rasters.len(),
                    raster_bytes: c.raster_bytes,
                    rasterizations: c.rasterizations,
                    intrinsic_entries: c.intrinsic.len(),
                    intrinsic_bytes: c.intrinsic_bytes,
                    intrinsic_measurements: c.intrinsic_measurements,
                }
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn request() -> Arc<ParagraphRequest> {
        Arc::new(ParagraphRequest {
            font_revision: 0,
            raster_scale_hint: None,
            text: "Hello ffi 中文🙂".into(),
            font_size: 16.,
            font_weight: 500.,
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
    fn text_and_width_changes_reuse_family_resolution_without_empty_line_probes() {
        let mut cache = Cache::new();
        let mut declaration = (*request()).clone();
        cache.layout(Arc::new(declaration.clone())).unwrap();
        assert_eq!(
            cache.family_probes, 0,
            "system fonts need no availability probe"
        );
        declaration.family = "Plico Definitely Missing Fixture,system-ui".into();
        cache.layout(Arc::new(declaration.clone())).unwrap();
        let probes = cache.family_probes;
        assert_eq!(probes, 1);
        for index in 0..16 {
            declaration.text = format!("edited value {index}").into();
            declaration.width = Some(40. + index as f32);
            cache.layout(Arc::new(declaration.clone())).unwrap();
        }
        assert_eq!(
            cache.family_probes, probes,
            "new native paragraphs must not repeat family availability shaping"
        );
        for index in 0..160 {
            cache
                .resolve_family(&format!("system-ui,Unused{index}").into())
                .unwrap();
        }
        assert_eq!(cache.families.len(), 128);
        cache.clear();
        assert!(cache.families.is_empty());
    }

    #[test]
    fn generic_monospace_selects_the_native_system_monospaced_face() {
        let mut cache = Cache::new();
        let mut requested = (*request()).clone();
        requested.family = "monospace".into();
        requested.text = "iiii WWWW 0123".into();
        let generic = cache.layout(Arc::new(requested.clone())).unwrap();
        requested.family = "ui-monospace".into();
        let explicit = cache.layout(Arc::new(requested)).unwrap();
        assert_eq!(generic.metrics().width, explicit.metrics().width);
        assert!(generic.caret_rect(4).is_some());
    }

    #[test]
    fn application_font_alias_survives_engine_reinstallation_and_cache_trim() {
        let asset = FontAsset::from_static(include_bytes!(
            "../../../backend/examples/web-preview/public/DejaVuSans.ttf"
        ))
        .with_family("Private CoreText Fixture");
        let cache = Rc::new(RefCell::new(Cache::new()));
        let mut engine = Engine {
            cache: cache.clone(),
            scale: 1.,
        };
        let mut request = (*request()).clone();
        request.family = "Private CoreText Fixture".into();
        assert!(engine.layout(Arc::new(request.clone())).is_err());
        request.family = "Private CoreText Fixture,system-ui".into();
        engine.layout(Arc::new(request.clone())).unwrap();
        assert_eq!(
            cache
                .borrow_mut()
                .resolve_family(&request.family)
                .unwrap()
                .as_ref(),
            "system-ui"
        );
        engine.register_font(&asset);
        assert!(
            cache.borrow().families.is_empty(),
            "registration invalidates cached fallback choices"
        );
        let first = engine.layout(Arc::new(request.clone())).unwrap();
        assert!(first.metrics().width > 0.);
        assert_eq!(
            cache
                .borrow_mut()
                .resolve_family(&request.family)
                .unwrap()
                .as_ref(),
            "Private CoreText Fixture"
        );
        for scale in [1.25, 2., 1.] {
            // configure_text replays assets when reinstalling the provider.
            let mut next = Engine {
                cache: cache.clone(),
                scale,
            };
            next.register_font(&asset.clone());
            assert_eq!(cache.borrow().font_assets.len(), 1);
            next.clear();
            let restored = next.layout(Arc::new(request.clone())).unwrap();
            assert_eq!(restored.metrics().width, first.metrics().width);
            assert!(restored.caret_rect(1).is_some());
            assert!(
                next.layout(self::request()).is_ok(),
                "system text must remain usable"
            );
        }
    }
    #[test]
    fn geometry_is_color_independent_and_per_line_rasters_are_reused() {
        let mut c = Cache::new();
        let request = request();
        let p = c.layout(request.clone()).unwrap();
        let mut changed = (*request).clone();
        changed.color = [250, 0, 0, 255];
        assert!(Rc::ptr_eq(&p, &c.layout(Arc::new(changed)).unwrap()));
        let clip = [-1000., -1000., 1000., 1000.];
        let first = c
            .images(p.clone(), request.color, 1.25, 0.25, clip)
            .unwrap();
        let calls = c.rasterizations;
        let second = c
            .images(p.clone(), request.color, 1.25, 0.25, clip)
            .unwrap();
        assert!(!first.is_empty());
        assert_eq!(calls, c.rasterizations);
        assert_eq!(first[0].0.data.id(), second[0].0.data.id());
        let offscreen = c
            .images(
                p.clone(),
                request.color,
                1.25,
                0.25,
                [10000., 10000., 10100., 10100.],
            )
            .unwrap();
        assert!(offscreen.is_empty());
        assert_eq!(calls, c.rasterizations);
        c.clear();
        assert!(c.layouts.is_empty() && c.rasters.is_empty());
        assert_eq!((c.layout_bytes, c.raster_bytes), (0, 0));
        assert!(p.caret_rect(1).is_some());
        assert!(
            !c.images(p, request.color, 1.25, 0.25, clip)
                .unwrap()
                .is_empty()
        );
        assert!(c.rasterizations > calls);
    }
    #[test]
    fn scrolling_clip_reuses_native_pixels_and_matches_an_exact_raster_crop() {
        let mut cache = Cache::new();
        let mut declaration = (*request()).clone();
        declaration.wrap = false;
        declaration.text = "AV office 日本語".into();
        let paragraph = cache.layout(Arc::new(declaration.clone())).unwrap();
        for scale in [1., 1.25, 2.] {
            let first = cache
                .images(
                    paragraph.clone(),
                    declaration.color,
                    scale,
                    0.25,
                    [0., 0., 80., 40.],
                )
                .unwrap();
            assert!(!first.is_empty());
            let calls = cache.rasterizations;
            for y in 1..12 {
                let next = cache
                    .images(
                        paragraph.clone(),
                        declaration.color,
                        scale,
                        0.25,
                        [1., y as f64, 79., 40.],
                    )
                    .unwrap();
                assert_eq!(cache.rasterizations, calls);
                assert_eq!(next[0].0.data.id(), first[0].0.data.id());
            }
            let line = &paragraph.layout.lines()[0];
            let position = [line.baseline[0] * scale + 0.25, line.baseline[1] * scale];
            let base = position.map(f64::floor);
            let phase = [position[0] - base[0], position[1] - base[1]];
            let exact = line
                .line
                .rasterize_clipped(
                    scale,
                    phase,
                    declaration.color,
                    [2. - base[0], 2. - base[1], 78. - base[0], 39. - base[1]],
                )
                .unwrap()
                .unwrap();
            let (image, origin) = &first[0];
            for y in 0..exact.size[1] {
                for x in 0..exact.size[0] {
                    let world_x = exact.origin[0] + base[0] as i32 + x as i32;
                    let world_y = exact.origin[1] + base[1] as i32 + y as i32;
                    let source_x = (world_x - origin[0]) as usize;
                    let source_y = (world_y - origin[1]) as usize;
                    let source = (source_y * image.width as usize + source_x) * 4;
                    let expected = (y as usize * exact.size[0] as usize + x as usize) * 4;
                    assert_eq!(
                        &image.data.data()[source..source + 4],
                        &exact.pixels[expected..expected + 4]
                    );
                }
            }
        }
    }

    #[test]
    fn clipped_native_images_stay_bounded_and_rich_spans_render() {
        let mut c = Cache::new();
        let mut declaration = (*request()).clone();
        declaration.text = "W".repeat(10000).into();
        declaration.wrap = false;
        let p = c.layout(Arc::new(declaration.clone())).unwrap();
        let natural_height = p.metrics().height;
        let images = c
            .images(p, declaration.color, 1., 0., [0., 0., 120., 80.])
            .unwrap();
        assert!(!images.is_empty());
        for (image, _) in images.iter() {
            assert!(image.width <= 120 + 256 && image.height <= 80 + 64);
        }
        let mut run = plico_renderer::text::TextRun {
            range: 0..1,
            font_size: declaration.font_size * 1.5,
            font_weight: 700.,
            font_italic: false,
            line_height: None,
            color: [255, 0, 0, 255],
        };
        declaration.runs = Arc::from([run.clone()]);
        let styled = c.layout(Arc::new(declaration.clone())).unwrap();
        assert!(styled.metrics().height > natural_height);
        let rich_images = c
            .images(styled, declaration.color, 1., 0., [0., 0., 120., 80.])
            .unwrap();
        assert!(!rich_images.is_empty());
        assert!(rich_images.iter().any(|(image, _)| {
            image
                .data
                .data()
                .as_chunks::<4>()
                .0
                .iter()
                .any(|pixel| pixel[0] > 0 && pixel[1] == 0 && pixel[2] == 0 && pixel[3] > 0)
        }));
        run.line_height = Some((1.5, true));
        declaration.runs = Arc::from([run]);
        assert!(c.layout(Arc::new(declaration)).is_err());
        let mut bad = (*request()).clone();
        bad.font_weight = f32::NAN;
        assert!(c.layout(Arc::new(bad)).is_err());
        let mut bad = (*request()).clone();
        bad.family = "Definitely Not A Font, sans-serif".into();
        assert!(c.layout(Arc::new(bad)).is_ok());
    }
    #[test]
    fn single_line_ellipsis_keeps_source_intrinsics_and_clamps_editing() {
        let mut c = Cache::new();
        let mut r = (*request()).clone();
        r.text = "A long native label that must be shortened".into();
        r.width = Some(90.);
        r.wrap = false;
        r.ellipsis = true;
        let p = c.layout(Arc::new(r.clone())).unwrap();
        assert_eq!(p.layout.lines().len(), 1);
        assert!(p.layout.lines()[0].line.text().ends_with('…'));
        assert!(p.visible_end < r.text.len());
        assert!(p.metrics().width <= r.width.unwrap() + 0.01);
        assert!(p.metrics().intrinsic[1] > p.metrics().width);
        let edge = p.caret_rect(p.visible_end).unwrap();
        assert_eq!(p.caret_rect(r.text.len()), Some(edge));
        assert_eq!(p.hit_test(1000., 0.), p.visible_end);
        assert!(p.selection_rects(p.visible_end..r.text.len()).is_empty());

        r.width = None;
        let unconstrained = c.layout(Arc::new(r.clone())).unwrap();
        assert_eq!(unconstrained.visible_end, r.text.len());
        assert!(!unconstrained.layout.lines()[0].line.text().ends_with('…'));

        r.width = Some(0.);
        let collapsed = c.layout(Arc::new(r)).unwrap();
        assert_eq!(collapsed.visible_end, 0);
        assert_eq!(collapsed.metrics().width, 0.);
    }
    #[test]
    fn native_intrinsic_measurements_survive_constraint_and_color_changes() {
        let mut c = Cache::new();
        let mut r = (*request()).clone();
        r.text = "Hello world".into();
        r.width = Some(100.);
        let first = c.layout(Arc::new(r.clone())).unwrap();
        let widths = first.metrics().intrinsic;
        assert!(widths[0] > 0. && widths[1] > widths[0]);
        assert_eq!(c.intrinsic_measurements, 1);
        r.width = Some(180.);
        r.align = TextAlign::End;
        r.color = [200, 100, 0, 255];
        r.line_height = Some((2., true));
        let resized = c.layout(Arc::new(r.clone())).unwrap();
        assert_eq!(resized.metrics().intrinsic, widths);
        assert_eq!(c.intrinsic_measurements, 1);
        assert_eq!(c.intrinsic.len(), 1);
        r.letter_spacing = 1.;
        let spaced = c.layout(Arc::new(r)).unwrap();
        assert!(spaced.metrics().intrinsic[1] > widths[1]);
        assert_eq!(c.intrinsic_measurements, 2);
        c.clear();
        assert!(c.intrinsic.is_empty());
        assert_eq!(c.intrinsic_bytes, 0);
    }
    #[test]
    fn line_limits_preserve_intrinsics_and_clamp_hidden_editing_and_rasters() {
        let mut c = Cache::new();
        let mut r = (*request()).clone();
        r.text = "first\nsecond\n中文".into();
        let full = c.layout(Arc::new(r.clone())).unwrap();
        r.max_lines = 1;
        let clipped = c.layout(Arc::new(r.clone())).unwrap();
        assert_eq!(clipped.visible_lines, 1);
        assert_eq!(clipped.visible_end, "first".len());
        assert_eq!(clipped.metrics().intrinsic, full.metrics().intrinsic);
        assert_eq!(c.intrinsic_measurements, 1);
        assert!(clipped.metrics().height < full.metrics().height);
        let edge = clipped.caret_rect(clipped.visible_end).unwrap();
        assert_eq!(clipped.caret_rect(r.text.len()), Some(edge));
        assert_eq!(clipped.hit_test(edge[0], 1000.), clipped.visible_end);
        assert!(clipped.selection_rects(6..r.text.len()).is_empty());
        assert!(
            clipped
                .selection_rects(0..r.text.len())
                .iter()
                .all(|rect| rect[3] <= clipped.metrics().height)
        );
        assert!(clipped.caret_rect(r.text.len() + 1).is_none());
        let images = c
            .images(
                clipped.clone(),
                r.color,
                1.25,
                0.25,
                [-1000., -1000., 1000., 1000.],
            )
            .unwrap();
        assert!(!images.is_empty());
        assert_eq!(c.rasterizations, 1);
        for (image, origin) in images.iter() {
            assert!(
                f64::from(origin[1]) + f64::from(image.height)
                    <= (clipped.visible_size[1] * 1.25).ceil()
            );
        }
    }
    #[test]
    fn clipped_soft_boundary_uses_visible_line_affinity_and_empty_lines_stay_empty() {
        let mut c = Cache::new();
        let mut r = (*request()).clone();
        r.text = "NNN NNN NNN NNN".into();
        r.width = Some(60.);
        r.max_lines = 1;
        let p = c.layout(Arc::new(r.clone())).unwrap();
        assert!(p.layout.lines().len() > 1);
        let edge = p.caret_rect(p.visible_end).unwrap();
        assert_eq!(edge[1], 0.);
        assert_eq!(edge[3], p.metrics().height);
        r.text = "\nsecret".into();
        let p = c.layout(Arc::new(r.clone())).unwrap();
        assert_eq!(p.metrics().width, 0.);
        assert_eq!(p.visible_end, 0);
        assert_eq!(p.hit_test(100., 1000.), 0);
        assert!(p.caret_rect(r.text.len()).is_some());
        assert!(
            c.images(p, r.color, 1.25, 0., [-1000., -1000., 1000., 1000.])
                .unwrap()
                .is_empty()
        );
    }
    #[test]
    fn no_wrap_keeps_alignment_without_introducing_soft_lines() {
        let mut c = Cache::new();
        let mut r = (*request()).clone();
        r.width = Some(20.);
        r.wrap = false;
        r.align = TextAlign::End;
        let p = c.layout(Arc::new(r)).unwrap();
        assert_eq!(p.layout.lines().len(), 1);
        assert!(p.layout.size()[0] > 20.);
        assert!(p.layout.lines()[0].baseline[0] < 0.);
    }
}

#[cfg(test)]
mod gpu_tests {
    use super::*;
    fn metal_renderer() -> Renderer {
        let renderer = pollster::block_on(Renderer::new()).unwrap();
        assert_eq!(renderer.adapter_info().backend, wgpu::Backend::Metal);
        eprintln!("native Metal adapter: {:?}", renderer.adapter_info());
        renderer
    }
    #[test]
    #[ignore = "requires a macOS GPU adapter and installed system fonts"]
    fn scrolling_native_line_reuses_cpu_raster_and_gpu_upload() {
        let mut renderer = metal_renderer();
        let request = Arc::new(ParagraphRequest {
            font_revision: 0,
            raster_scale_hint: None,
            text: "Scrolling UI".into(),
            font_size: 20.,
            font_weight: 500.,
            italic: false,
            letter_spacing: 0.,
            line_height: None,
            align: TextAlign::Start,
            color: [20, 30, 40, 255],
            runs: Arc::from([]),
            family: "system-ui".into(),
            width: None,
            wrap: false,
            max_lines: 0,
            ellipsis: false,
        });
        let mut calls = None;
        for offset in 0..8 {
            use plico_renderer::{PaintCommand, PaintShape, Scene};
            let mut scene = Scene::new();
            scene.commands.push(PaintCommand::PushClip {
                transform: kurbo::Affine::IDENTITY,
                clip: PaintShape::record(&kurbo::Rect::new(10., 10., 90., 40.), 0.1),
            });
            scene.commands.push(PaintCommand::Paragraph {
                request: request.clone(),
                transform: kurbo::Affine::translate((12.25, 12. - f64::from(offset))),
            });
            scene.commands.push(PaintCommand::PopLayer);
            let pixels = renderer
                .render_scene(&scene, [128, 64], [1.; 4], false)
                .unwrap();
            assert!(pixels.as_chunks::<4>().0.iter().any(|p| p[0] < 200));
            let current = (
                renderer.macos_text_cache_stats().rasterizations,
                renderer.image_cache_stats().2,
            );
            if let Some(first) = calls {
                assert_eq!(
                    current, first,
                    "scrolling should only change draw geometry/clip"
                );
            } else {
                assert!(current.0 > 0 && current.1 > 0);
                calls = Some(current);
            }
        }
    }

    #[test]
    #[ignore = "requires a macOS GPU adapter and installed system fonts"]
    fn fractional_line_limit_matches_an_explicit_clip_of_only_visible_content() {
        let mut renderer = metal_renderer();
        let make = |text: &str, max_lines| {
            Arc::new(ParagraphRequest {
                font_revision: 0,
                raster_scale_hint: None,
                text: text.into(),
                font_size: 20.,
                font_weight: 500.,
                italic: false,
                letter_spacing: 0.,
                line_height: Some((7.3, false)),
                align: TextAlign::Start,
                color: [20, 30, 40, 255],
                runs: Arc::from([]),
                family: "system-ui".into(),
                width: Some(240.),
                wrap: true,
                max_lines,
                ellipsis: false,
            })
        };
        let transform = kurbo::Affine::new([1.25, 0., 0., 1.25, 20.25, 10.]);
        let mut limited = plico_renderer::Scene::new();
        limited
            .commands
            .push(plico_renderer::PaintCommand::Paragraph {
                request: make("SSSS\nhidden", 1),
                transform,
            });
        let pixels = renderer
            .render_scene(&limited, [360, 100], [1.; 4], false)
            .unwrap();
        let mut reference = plico_renderer::Scene::new();
        reference
            .commands
            .push(plico_renderer::PaintCommand::PushClip {
                transform: kurbo::Affine::IDENTITY,
                clip: plico_renderer::PaintShape::record(
                    &kurbo::Rect::new(0., 0., 360., 10. + 7.3 * 1.25),
                    0.1,
                ),
            });
        reference
            .commands
            .push(plico_renderer::PaintCommand::Paragraph {
                request: make("SSSS", 0),
                transform,
            });
        reference
            .commands
            .push(plico_renderer::PaintCommand::PopLayer);
        assert_eq!(
            pixels,
            renderer
                .render_scene(&reference, [360, 100], [1.; 4], false)
                .unwrap()
        );
    }
    #[test]
    #[ignore = "requires a macOS GPU adapter and installed system fonts"]
    fn coretext_scene_pixels_survive_reuse_damage_and_cache_trim() {
        let mut renderer = metal_renderer();
        let mut text = TextContext::new();
        renderer.configure_text(&mut text, 1.25).unwrap();
        assert_eq!(text.raster_backend_name(), "coretext-wgpu");
        let request = Arc::new(ParagraphRequest {
            font_revision: 0,
            raster_scale_hint: None,
            text: "CoreText S 中文 日本語🙂".into(),
            font_size: 20.,
            font_weight: 500.,
            italic: false,
            letter_spacing: 0.25,
            line_height: None,
            align: TextAlign::Start,
            color: [20, 30, 40, 255],
            runs: Arc::from([]),
            family: "system-ui".into(),
            width: Some(240.),
            wrap: true,
            max_lines: 0,
            ellipsis: false,
        });
        let mut scene = plico_renderer::Scene::new();
        scene
            .commands
            .push(plico_renderer::PaintCommand::Paragraph {
                request,
                transform: kurbo::Affine::new([1.25, 0., 0., 1.25, 20.25, 15.]),
            });
        let first = renderer
            .render_scene(&scene, [360, 150], [1.; 4], false)
            .unwrap();
        assert!(
            first
                .as_chunks::<4>()
                .0
                .iter()
                .any(|p| p[0] < 200 && p[1] < 200 && p[2] < 200)
        );
        let calls = renderer.macos_text_cache_stats().rasterizations;
        assert!(calls > 0);
        assert_eq!(
            renderer
                .render_scene(&scene, [360, 150], [1.; 4], true)
                .unwrap(),
            first
        );
        assert_eq!(renderer.macos_text_cache_stats().rasterizations, calls);
        renderer.trim_text_cache();
        assert_eq!(renderer.macos_text_cache_stats().rasters, 0);
        assert_eq!(
            renderer
                .render_scene(&scene, [360, 150], [1.; 4], true)
                .unwrap(),
            first
        );
        assert!(renderer.macos_text_cache_stats().rasterizations > calls);
    }
}
