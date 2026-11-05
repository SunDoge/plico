//! Experimental retained CoreText geometry and bounded per-line RGBA cache.
use crate::{Error, Renderer, Result};
use plico_native::macos_text::{
    Alignment, FontStyle, FontWeight, Line, Paragraph as Layout, Request,
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

struct Paragraph {
    request: Arc<ParagraphRequest>,
    layout: Layout,
    ink: [f64; 4],
    line_inks: Vec<Option<[f64; 4]>>,
    bytes: usize,
    intrinsic: [f32; 2],
}
impl NativeParagraph for Paragraph {
    fn metrics(&self) -> ParagraphMetrics {
        let size = self.layout.size();
        let lines = self.layout.lines();
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
        self.layout.hit_test([x as f64, y as f64]).unwrap_or(0)
    }
    fn caret_rect(&self, byte: usize) -> Option<[f32; 4]> {
        self.layout
            .caret_rects(byte)
            .ok()
            .map(|r| r[0].map(|v| v as f32))
    }
    fn selection_rects(&self, range: std::ops::Range<usize>) -> Vec<[f32; 4]> {
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
/// Accounted Rust caches only; excludes CoreText objects, GPU textures and live
/// externally retained paragraphs/images. This is not a process RSS estimate.
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
    font_error: bool,
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
            font_error: false,
            intrinsic: lru::LruCache::new(NonZeroUsize::new(128).unwrap()),
            intrinsic_bytes: 0,
            intrinsic_measurements: 0,
        }
    }
    fn clear(&mut self) {
        self.layouts.clear();
        self.rasters.clear();
        self.layout_bytes = 0;
        self.raster_bytes = 0;
        self.intrinsic.clear();
        self.intrinsic_bytes = 0;
    }
    #[tracing::instrument(name = "plico_wgpu.coretext_layout", skip_all, fields(text_bytes = request.text.len()))]
    fn layout(
        &mut self,
        request: Arc<ParagraphRequest>,
    ) -> std::result::Result<Rc<Paragraph>, ParagraphError> {
        if self.font_error {
            return Err(rejected(
                "CoreText application font registration is not implemented",
            ));
        }
        if !request.runs.is_empty() || request.ellipsis || request.max_lines != 0 {
            return Err(rejected(
                "CoreText rich spans, ellipsis and line limits are not implemented",
            ));
        }
        if request.family.len() > 4096 || request.family.contains(['\'', '"', '\\']) {
            return Err(rejected("invalid/unsupported CoreText family list"));
        }
        let mut geometry = (*request).clone();
        geometry.color = [0; 4];
        let key = geometry.fingerprint();
        if let Some(p) = self.layouts.get(&key)
            && *p.request == geometry
        {
            return Ok(p.clone());
        }
        let mut family = None;
        for candidate in request.family.split(',').map(str::trim) {
            let mapped = match candidate {
                "" | "sans-serif" | "system-ui" => "system-ui",
                "serif" => "Times",
                "monospace" => "Menlo",
                other => other,
            };
            let mut probe = Request::new("");
            probe.family = mapped;
            let face = Line::new(&probe).map_err(|e| rejected(e.to_string()))?;
            if mapped == "system-ui"
                || face.font_family().eq_ignore_ascii_case(mapped)
                || face.font_postscript_name().eq_ignore_ascii_case(mapped)
            {
                family = Some(mapped);
                break;
            }
        }
        let family = family.ok_or_else(|| rejected("no requested CoreText family is installed"))?;
        let mut native = Request::new(&request.text);
        native.family = family;
        native.size = request.font_size as f64;
        native.letter_spacing = request.letter_spacing as f64;
        native.style = Some(FontStyle {
            weight: FontWeight::from_css(request.font_weight as f64)
                .map_err(|e| rejected(e.to_string()))?,
            italic: request.italic,
        });
        let height = request.line_height.map(|(h, relative)| {
            if relative {
                h as f64 * native.size
            } else {
                h as f64
            }
        });
        let width = request.width.filter(|w| *w != f32::INFINITY).map(f64::from);
        let align = match request.align {
            TextAlign::Start => Alignment::Start,
            TextAlign::Center => Alignment::Center,
            TextAlign::End => Alignment::End,
            TextAlign::Justify => Alignment::Justify,
        };
        let layout = if request.wrap {
            Layout::with_alignment(&native, width, height, align)
        } else {
            Layout::without_wrapping(&native, width, height, align)
        }
        .map_err(|e| rejected(e.to_string()))?;
        let mut source = geometry.clone();
        source.width = None;
        source.wrap = false;
        source.align = TextAlign::Start;
        source.line_height = None;
        let source_key = source.fingerprint();
        let intrinsic = if let Some(cached) = self.intrinsic.get(&source_key)
            && *cached.request == source
        {
            cached.widths
        } else {
            let widths = Layout::measure_intrinsic(&native)
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
        let mut ink: Option<[f64; 4]> = None;
        let mut line_inks = Vec::with_capacity(layout.lines().len());
        for line in layout.lines() {
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
        clip: [f64; 4],
    ) -> std::result::Result<Rc<Images>, ParagraphError> {
        let mut images = Vec::new();
        let mut frame_bytes = 0usize;
        let fingerprint = paragraph.request.fingerprint();
        for (index, line) in paragraph.layout.lines().iter().enumerate() {
            let Some(ink) = paragraph.line_inks[index] else {
                continue;
            };
            let position = [line.baseline[0] * scale + phase, line.baseline[1] * scale];
            let base = position.map(f64::floor);
            let phases = [position[0] - base[0], position[1] - base[1]];
            let bounds = [
                (ink[0] * scale + position[0]).floor() - 2.,
                (ink[1] * scale + position[1]).floor() - 2.,
                (ink[2] * scale + position[0]).ceil() + 2.,
                (ink[3] * scale + position[1]).ceil() + 2.,
            ];
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
        self.prepared_cache = None;
        self.invalidate_scene_damage();
        Ok(())
    }
    pub(crate) fn macos_paragraph_images(
        &mut self,
        request: Arc<ParagraphRequest>,
        scale: f64,
        phase: f64,
        clip: [f64; 4],
    ) -> Result<(Rc<Images>, [f64; 4])> {
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
        let images = cache
            .images(p, request.color, scale, phase, clip)
            .map_err(|source| Error::Paragraph { source })?;
        Ok((images, ink))
    }
    /// Clear derived native text caches; live externally retained layouts stay valid.
    pub fn trim_text_cache(&mut self) {
        if let Some(c) = &self.macos_paragraphs {
            c.borrow_mut().clear();
        }
        self.prepared_cache = None;
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
    fn clipped_native_images_stay_bounded_and_unsupported_declarations_reject() {
        let mut c = Cache::new();
        let mut declaration = (*request()).clone();
        declaration.text = "W".repeat(10000).into();
        declaration.wrap = false;
        let p = c.layout(Arc::new(declaration.clone())).unwrap();
        let images = c
            .images(p, declaration.color, 1., 0., [0., 0., 120., 80.])
            .unwrap();
        assert!(!images.is_empty());
        for (image, _) in images.iter() {
            assert!(image.width <= 120 && image.height <= 80);
        }
        declaration.ellipsis = true;
        assert!(c.layout(Arc::new(declaration)).is_err());
        let mut bad = (*request()).clone();
        bad.font_weight = f32::NAN;
        assert!(c.layout(Arc::new(bad)).is_err());
        let mut bad = (*request()).clone();
        bad.family = "Definitely Not A Font, sans-serif".into();
        assert!(c.layout(Arc::new(bad)).is_ok());
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
    #[test]
    #[ignore = "requires a macOS GPU adapter and installed system fonts"]
    fn coretext_scene_pixels_survive_reuse_damage_and_cache_trim() {
        let mut renderer = pollster::block_on(Renderer::new()).unwrap();
        let mut text = TextContext::new();
        renderer.configure_text(&mut text, 1.25).unwrap();
        assert_eq!(text.raster_backend_name(), "coretext-wgpu");
        let request = Arc::new(ParagraphRequest {
            font_revision: 0,
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
