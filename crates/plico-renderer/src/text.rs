//! Shared text context with host-selected ordinary paragraphs and Parley layouts.
//!
//! The default Parley path handles font selection (system fonts via fontique), shaping, and
//! positioning. We build a `Layout` from a node's text string + font size,
//! use it for both taffy measurement (width/height) and vello rasterisation
//! (positioned glyph runs). Layouts are cached by width constraint so
//! measurement and painting use identical line breaks.

#![warn(missing_docs)]

use std::num::NonZeroUsize;
use std::sync::Arc;

use crate::{Glyph, PaintScene, Scene};
use lru::LruCache;
use parley::{
    Alignment, AlignmentOptions, FontContext, Layout, LayoutContext, PositionedLayoutItem,
    StyleProperty,
};
use unicode_segmentation::UnicodeSegmentation;
use vello_common::kurbo::{Affine, Vec2};
use vello_common::peniko::Fill;

use crate::style::TextAlign;
use vello_common::peniko::Color;

/// Geometry shared by ordinary text nodes and text-backed native widgets.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SingleLineTextMetrics {
    /// Local line box `[x, y, width, height]` inside the owning content box.
    pub line_box: [f32; 4],
    /// Local baseline offset from the content-box top.
    pub baseline: f32,
}

/// Center a Parley single-line layout inside a content box.
///
/// This is the canonical vertical-positioning rule for both normal text and
/// native editors. Components must not add tag-specific optical offsets.
pub fn single_line_text_metrics<B: parley::Brush>(
    layout: &Layout<B>,
    container_height: f32,
) -> Option<SingleLineTextMetrics> {
    let line = layout.lines().next()?;
    let origin_y = ((container_height - layout.height()).max(0.0)) * 0.5;
    Some(SingleLineTextMetrics {
        line_box: [0.0, origin_y, layout.width(), layout.height()],
        baseline: origin_y + line.metrics().baseline,
    })
}

/// Convert a Peniko color into Parley's RGBA brush representation.
pub fn brush_for_color(color: Color) -> [u8; 4] {
    color.to_rgba8().to_u8_array()
}

#[derive(Clone, Debug, PartialEq)]
/// Styled UTF-8 byte range layered over a text node's base style.
pub struct TextRun {
    /// UTF-8 byte range in the owning text string.
    pub range: std::ops::Range<usize>,
    /// Font size in logical pixels.
    pub font_size: f32,
    /// Numeric CSS font weight.
    pub font_weight: f32,
    /// Whether this range uses an italic face or synthetic skew.
    pub font_italic: bool,
    /// Line height and whether it is relative to font size.
    pub line_height: Option<(f32, bool)>,
    /// RGBA text brush.
    pub color: [u8; 4],
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
/// Synthetic styles requested by Parley after resolving the available fonts.
pub struct TextSynthesis {
    /// At least one glyph run requires geometric emboldening.
    pub embolden: bool,
    /// At least one glyph run requires a synthetic skew.
    pub skew: bool,
}

/// Inspect the resolved glyph runs without reshaping or rasterizing them.
pub fn text_synthesis(layout: &Layout<[u8; 4]>) -> TextSynthesis {
    let mut result = TextSynthesis::default();
    for line in layout.lines() {
        for item in line.items() {
            let PositionedLayoutItem::GlyphRun(run) = item else {
                continue;
            };
            result.embolden |= run.run().synthesis().embolden();
            result.skew |= run.run().synthesis().skew().is_some();
        }
    }
    result
}

/// Synthetic styles that the active painter will actually apply.
pub fn applied_text_synthesis(layout: &Layout<[u8; 4]>) -> TextSynthesis {
    let mut synthesis = text_synthesis(layout);
    synthesis.embolden = false;
    synthesis
}

#[derive(Clone, Hash, PartialEq, Eq)]
struct TextRunKey {
    start: usize,
    end: usize,
    font_size: u32,
    font_weight: u32,
    font_italic: bool,
    line_height: Option<(u32, bool)>,
    color: [u8; 4],
}

#[derive(Clone, Hash, PartialEq, Eq)]
struct TextLayoutKey {
    text: Arc<str>,
    font_size: u32,
    font_weight: u32,
    font_italic: bool,
    letter_spacing: u32,
    line_height: Option<(u32, bool)>,
    max_width: Option<u32>,
    alignment: TextAlign,
    color: [u8; 4],
    runs: Arc<[TextRunKey]>,
    font_family: Option<Arc<str>>,
}

/// Shared application font data registered once per text context.
/// Clones retain the same font identity and backing storage.
#[derive(Clone, Debug)]
pub struct FontAsset {
    data: parley::fontique::Blob<u8>,
    family: Option<Arc<str>>,
}
impl FontAsset {
    /// Original font bytes, shared with the registered collection.
    pub fn bytes(&self) -> &[u8] {
        self.data.as_ref()
    }
    /// Optional private family alias.
    pub fn family(&self) -> Option<&str> {
        self.family.as_deref()
    }
    /// Own an in-memory TTF/OTF file.
    pub fn new(bytes: Vec<u8>) -> Self {
        Self {
            data: bytes.into(),
            family: None,
        }
    }
    /// Reference an embedded font without copying its bytes.
    pub fn from_static(bytes: &'static [u8]) -> Self {
        Self {
            data: parley::fontique::Blob::new(Arc::new(bytes)),
            family: None,
        }
    }
    /// Register under a private family alias, leaving the original font untouched.
    pub fn with_family(mut self, family: impl Into<Arc<str>>) -> Self {
        self.family = Some(family.into());
        self
    }
}

/// The lifetime of a returned [`Layout`] is tied to a borrow of this context.
pub struct TextContext {
    native: Option<Box<dyn crate::paragraph::ParagraphEngine>>,
    font_assets: Vec<FontAsset>,
    paragraph_revision: u64,
    /// Font discovery and fallback database shared by layouts.
    pub font_cx: FontContext,
    /// Scratch state used by Parley while constructing layouts.
    pub layout_cx: LayoutContext,
    loaded_fonts: std::collections::HashSet<(u64, Option<Arc<str>>)>,
    cache: LruCache<TextLayoutKey, Arc<Layout<[u8; 4]>>>,
    intrinsic_width_cache: LruCache<(TextLayoutKey, bool), [f32; 2]>,
    ellipsis_cache: LruCache<TextLayoutKey, Arc<Layout<[u8; 4]>>>,
    clamp_cache: LruCache<TextClampCacheKey, SharedTextLayout>,
    glyph_cache: LruCache<(usize, u64), GlyphSceneEntry>,
}

type SharedTextLayout = Arc<Layout<[u8; 4]>>;
type TextClampCacheKey = (TextLayoutKey, u32, bool);
type GlyphSceneEntry = (std::sync::Weak<Layout<[u8; 4]>>, Arc<Scene>);

/// Default UI font stack. Explicit application stacks always take precedence.
/// Linux prefers matching Noto Latin/CJK faces, including native Medium weights,
/// before the platform's generic fallback list. Missing families remain optional.
/// Applications targeting other CJK regions can override the family stack.
pub const DEFAULT_UI_FONT_FAMILY: &str = if cfg!(target_os = "linux") {
    "Noto Sans, Noto Sans CJK SC, sans-serif"
} else {
    "sans-serif"
};

/// Optional geometric small-text weight tuning, in milli-em.
///
/// This expands outlines; it is not gamma correction. Controlled mask tests
/// show that Vello image alpha compositing closely agrees with Cairo, while
/// text appearance also depends on hinting, pixel phase, and text-specific
/// coverage transfer (see docs/pango-text-comparison.md). Do not infer a
/// universal blending defect from a perceived difference in text weight.
/// Disabled by default: expanding a hinted outline pushes its edges off the
/// pixel grid and introduces faint fringe pixels. Prefer real font weights.
/// `PLICO_TEXT_STEM_BOOST` is an explicit override in units of 1/1000 em.
fn stem_boost_milli_em(value: Option<&str>) -> f64 {
    value
        .and_then(|value| value.parse::<f64>().ok())
        .filter(|value| value.is_finite())
        .unwrap_or(0.)
}

fn stem_boost(font_size: f32, device_scale: f64) -> f64 {
    let value = std::env::var("PLICO_TEXT_STEM_BOOST").ok();
    let milli_em = stem_boost_milli_em(value.as_deref());
    f64::from(font_size) * device_scale * milli_em / 1000.0
}
fn synthetic_embolden(requested: bool, allowed: bool, font_size: f32, device_scale: f64) -> Vec2 {
    let boost = stem_boost(font_size, device_scale);
    embolden_amount(requested, allowed, font_size, device_scale, boost)
}

fn embolden_amount(
    requested: bool,
    allowed: bool,
    font_size: f32,
    device_scale: f64,
    boost: f64,
) -> Vec2 {
    if requested && allowed {
        // Match the conventional FreeType synthetic-bold strength of roughly
        // one twenty-fourth of an em on raster-oriented platforms.
        let amount = f64::from(font_size) * device_scale / 24.0;
        Vec2::new(amount + boost, amount + boost)
    } else {
        Vec2::new(boost, boost)
    }
}

impl Default for TextContext {
    fn default() -> Self {
        Self::new()
    }
}

impl TextContext {
    /// Discover system fonts (parley default = "system" feature → fontique).
    pub fn new() -> Self {
        let mut font_cx = FontContext::new();
        // Parley prunes SourceCache as new layouts are built. The secondary
        // weak cache reuses a Blob still owned by older layouts instead of
        // reloading the same font file under a fresh identity after pruning.
        font_cx.source_cache.make_shared();
        Self {
            native: None,
            font_assets: Vec::new(),
            paragraph_revision: 0,
            font_cx,
            layout_cx: LayoutContext::new(),
            loaded_fonts: Default::default(),
            cache: LruCache::new(NonZeroUsize::new(2048).unwrap()),
            intrinsic_width_cache: LruCache::new(NonZeroUsize::new(2048).unwrap()),
            ellipsis_cache: LruCache::new(NonZeroUsize::new(1024).unwrap()),
            clamp_cache: LruCache::new(NonZeroUsize::new(1024).unwrap()),
            glyph_cache: LruCache::new(NonZeroUsize::new(2048).unwrap()),
        }
    }

    /// Stable diagnostic name for the active ordinary-text raster backend.
    pub fn raster_backend_name(&self) -> &'static str {
        self.native
            .as_ref()
            .map_or("vello-outline", |engine| engine.name())
    }

    /// Install the host's ordinary paragraph engine before layout.
    pub fn set_paragraph_engine(&mut self, mut engine: Box<dyn crate::paragraph::ParagraphEngine>) {
        for asset in &self.font_assets {
            engine.register_font(asset);
        }
        self.clear_caches();
        self.native = Some(engine);
        self.paragraph_revision = self.paragraph_revision.wrapping_add(1);
    }
    /// Declare ordinary text with the current font collection's identity.
    pub fn paragraph_request(
        &self,
        paint: &crate::style::Paint,
        width: Option<f32>,
    ) -> Option<crate::paragraph::ParagraphRequest> {
        let mut request = crate::paragraph::ParagraphRequest::from_paint(paint, width)?;
        request.font_revision = self.paragraph_revision;
        Some(request)
    }
    /// Whether ordinary text uses a host-native paragraph implementation.
    pub fn has_native_paragraphs(&self) -> bool {
        self.native.is_some()
    }
    /// Lay out an ordinary text request with the selected host engine.
    pub fn native_paragraph(
        &mut self,
        request: Arc<crate::paragraph::ParagraphRequest>,
    ) -> Option<std::rc::Rc<dyn crate::paragraph::NativeParagraph>> {
        Some(self.native.as_mut()?.layout(request))
    }

    /// Stable diagnostic name for the platform outline fallback policy.
    pub fn outline_fallback_name(&self) -> &'static str {
        // Fontique recommends geometric emboldening whenever a fallback face
        // does not expose the exact requested weight. Many CJK system families
        // provide 400 and 600 but not 500, making ordinary `font-medium`
        // controls look double-bold. Prefer the nearest native face on every
        // platform; real bold faces and variable weight axes are unaffected.
        "direct-native-weight"
    }

    /// Register a raw font file (TTF/OTF) from its bytes so it becomes
    /// available to parley's font matching. WOFF/WOFF2 are not decoded here —
    /// decode upstream if needed. The text layout cache is cleared because
    /// newly-registered fonts change how existing text resolves.
    pub fn load_font(&mut self, bytes: Vec<u8>) {
        self.register_font(&FontAsset::new(bytes));
    }

    /// Register an application font once. Returns true when new fonts were added.
    /// Reusing the same asset is cheap and preserves all shaped-layout caches.
    pub fn register_font(&mut self, asset: &FontAsset) -> bool {
        let key = (asset.data.id(), asset.family.clone());
        if self.loaded_fonts.contains(&key) {
            return false;
        }
        let registered = self.font_cx.collection.register_fonts(
            asset.data.clone(),
            Some(parley::fontique::FontInfoOverride {
                family_name: asset.family.as_deref(),
                ..Default::default()
            }),
        );
        if registered.is_empty() {
            return false;
        }
        self.loaded_fonts.insert(key);
        self.font_assets.push(asset.clone());
        self.paragraph_revision = self.paragraph_revision.wrapping_add(1);
        if let Some(engine) = &mut self.native {
            engine.register_font(asset);
        }
        // Browsers provide no system-font enumeration to Fontique. Register
        // explicit defaults so a loaded font can resolve generic text styles.
        #[cfg(target_arch = "wasm32")]
        for family in [
            parley::fontique::GenericFamily::SansSerif,
            parley::fontique::GenericFamily::Serif,
            parley::fontique::GenericFamily::Monospace,
        ] {
            self.font_cx
                .collection
                .append_generic_families(family, registered.iter().map(|(id, _)| *id));
        }
        #[cfg(not(target_arch = "wasm32"))]
        drop(registered);
        self.clear_caches();
        true
    }

    /// Diagnostic counts of cached shaped layouts and encoded glyph scenes.
    pub fn cache_entries(&self) -> (usize, usize) {
        (self.cache.len(), self.glyph_cache.len())
    }

    /// Release derived text layouts and encoded glyph scenes while retaining
    /// the discovered and application-provided font collection.
    pub fn clear_caches(&mut self) {
        if let Some(engine) = &mut self.native {
            engine.clear();
        }
        self.cache.clear();
        self.intrinsic_width_cache.clear();
        self.ellipsis_cache.clear();
        self.clamp_cache.clear();
        self.glyph_cache.clear();
    }

    /// Encode a positioned Parley layout once and reuse the retained Wabou paint
    /// fragment while only its node transform changes. Animated text commonly
    /// keeps identical shaping for thousands of frames; rebuilding every glyph
    /// run into the scene each frame is pure encoding overhead.
    pub fn glyph_scene(&mut self, layout: &Arc<Layout<[u8; 4]>>) -> Arc<Scene> {
        self.glyph_scene_scaled(layout, 1.0)
    }

    /// Encode glyphs at the target device scale.
    ///
    /// Outline glyphs can be scaled after encoding without losing detail, but
    /// bitmap emoji cannot: the renderer selects an embedded bitmap strike from the
    /// font while `draw_glyphs` is encoded. Encoding physical font sizes and
    /// positions here lets Apple Color Emoji select a Retina-sized `sbix`
    /// strike instead of enlarging a logical-pixel bitmap later.
    pub fn glyph_scene_scaled(
        &mut self,
        layout: &Arc<Layout<[u8; 4]>>,
        device_scale: f64,
    ) -> Arc<Scene> {
        let device_scale = device_scale.max(f64::EPSILON);
        let id = Arc::as_ptr(layout) as usize;
        let key = (id, device_scale.to_bits());
        if let Some((cached_layout, scene)) = self.glyph_cache.get(&key)
            && cached_layout
                .upgrade()
                .is_some_and(|cached| Arc::ptr_eq(&cached, layout))
        {
            return scene.clone();
        }

        #[cfg(feature = "profiling")]
        let span = tracing::trace_span!(
            target: "wabou::perf",
            "text.glyph_encode.cache_miss",
            lines = layout.len() as u64,
            device_scale,
        );
        #[cfg(feature = "profiling")]
        let _guard = span.enter();

        let mut scene = Scene::new();
        Self::draw_layout_into(
            &mut scene,
            layout,
            device_scale,
            Affine::IDENTITY,
            true,
            true,
        );
        let scene = Arc::new(scene);
        self.glyph_cache
            .put(key, (Arc::downgrade(layout), scene.clone()));
        scene
    }

    /// Draw a synthesis-free hinted glyph layout directly into the destination scene.
    ///
    /// This follows Xilem/Masonry's Parley-to-Vello path. Direct encoding
    /// avoids retained bitmap resampling while hinting keeps small UI text
    /// aligned to the device rasterizer. Geometric weight synthesis remains
    /// disabled so fallback CJK faces do not become artificially heavy.
    pub(crate) fn draw_native_weight_layout_into(
        &self,
        scene: &mut Scene,
        layout: &Layout<[u8; 4]>,
        transform: Affine,
        device_scale: f64,
    ) {
        Self::draw_layout_into(scene, layout, device_scale, transform, true, false);
    }

    fn draw_layout_into(
        scene: &mut Scene,
        layout: &Layout<[u8; 4]>,
        device_scale: f64,
        transform: Affine,
        hint: bool,
        allow_synthetic_embolden: bool,
    ) {
        let device_scale = device_scale.max(f64::EPSILON);
        for line in layout.lines() {
            for item in line.items() {
                if let PositionedLayoutItem::GlyphRun(gr) = item {
                    let run = gr.run();
                    let font_data = run.font().clone();
                    let glyph_transform = run
                        .synthesis()
                        .skew()
                        .map(|angle| Affine::skew(angle.to_radians().tan() as f64, 0.0));
                    let synthesis = run.synthesis();
                    let embolden = synthetic_embolden(
                        synthesis.embolden(),
                        allow_synthetic_embolden,
                        run.font_size(),
                        device_scale,
                    );
                    let glyphs: Vec<Glyph> = gr
                        .positioned_glyphs()
                        .map(|g| Glyph {
                            id: g.id,
                            x: g.x * device_scale as f32,
                            y: g.y * device_scale as f32,
                        })
                        .collect();
                    if !glyphs.is_empty() {
                        // The buffer moves straight into the command; the
                        // iterator-taking entry point would collect a second
                        // copy per run.
                        scene.draw_glyphs_vec(
                            &font_data,
                            run.font_size() * device_scale as f32,
                            hint,
                            bytemuck::cast_slice(run.normalized_coords()),
                            embolden,
                            Fill::NonZero,
                            Color::from_rgba8(
                                gr.style().brush[0],
                                gr.style().brush[1],
                                gr.style().brush[2],
                                gr.style().brush[3],
                            ),
                            1.0,
                            transform,
                            glyph_transform,
                            glyphs,
                        );
                    }
                }
            }
        }
    }
}

/// Build a single-line layout for `text` at `font_size`. The returned layout
/// borrows `tcx` for its lifetime.
pub fn layout_text(tcx: &mut TextContext, text: &str, font_size: f32) -> Arc<Layout<[u8; 4]>> {
    layout_text_styled(
        tcx,
        Arc::from(text),
        font_size,
        400.0,
        false,
        None,
        TextAlign::Start,
        [0, 0, 0, 255],
        Arc::from([]),
        None,
        None,
    )
}

#[allow(clippy::too_many_arguments)]
fn text_layout_key(
    text: Arc<str>,
    font_size: f32,
    font_weight: f32,
    font_italic: bool,
    letter_spacing: f32,
    line_height: Option<(f32, bool)>,
    alignment: TextAlign,
    color: [u8; 4],
    runs: &[TextRun],
    font_family: Option<&Arc<str>>,
    max_width: Option<f32>,
) -> TextLayoutKey {
    TextLayoutKey {
        text,
        font_size: font_size.to_bits(),
        font_weight: font_weight.to_bits(),
        font_italic,
        letter_spacing: letter_spacing.to_bits(),
        line_height: line_height.map(|(value, relative)| (value.to_bits(), relative)),
        max_width: max_width.map(f32::to_bits),
        alignment,
        color,
        runs: if runs.is_empty() {
            // An empty `Arc<[TextRunKey]>` still allocates its header; key
            // construction runs per text lookup, so share one static instead.
            empty_runs_key()
        } else {
            runs.iter()
                .map(|run| TextRunKey {
                    start: run.range.start,
                    end: run.range.end,
                    font_size: run.font_size.to_bits(),
                    font_weight: run.font_weight.to_bits(),
                    font_italic: run.font_italic,
                    line_height: run
                        .line_height
                        .map(|(value, relative)| (value.to_bits(), relative)),
                    color: run.color,
                })
                .collect::<Vec<_>>()
                .into()
        },
        font_family: font_family.cloned(),
    }
}

/// Shared empty run key: allocating a fresh `Arc<[TextRunKey]>` header per
/// lookup dominates steady-state text cost with plain (runless) paragraphs.
fn empty_runs_key() -> Arc<[TextRunKey]> {
    static EMPTY: std::sync::OnceLock<Arc<[TextRunKey]>> = std::sync::OnceLock::new();
    EMPTY.get_or_init(|| Vec::new().into()).clone()
}

#[allow(clippy::too_many_arguments)]
/// Shape and cache styled text with an optional logical width constraint.
pub fn layout_text_styled_with_spacing(
    tcx: &mut TextContext,
    text: Arc<str>,
    font_size: f32,
    font_weight: f32,
    font_italic: bool,
    letter_spacing: f32,
    line_height: Option<(f32, bool)>,
    alignment: TextAlign,
    color: [u8; 4],
    runs: Arc<[TextRun]>,
    font_family: Option<&Arc<str>>,
    max_width: Option<f32>,
) -> Arc<Layout<[u8; 4]>> {
    let key = text_layout_key(
        text.clone(),
        font_size,
        font_weight,
        font_italic,
        letter_spacing,
        line_height,
        alignment,
        color,
        &runs,
        font_family,
        max_width,
    );
    if let Some(layout) = tcx.cache.get(&key) {
        return layout.clone();
    }
    #[cfg(feature = "profiling")]
    let span = tracing::trace_span!(
        target: "wabou::perf",
        "text.shape.cache_miss",
        bytes = text.len() as u64,
        styled_runs = runs.len() as u64,
        constrained = max_width.is_some(),
    );
    #[cfg(feature = "profiling")]
    let _guard = span.enter();
    let mut builder = tcx
        .layout_cx
        .ranged_builder(&mut tcx.font_cx, &text, 1.0, false);
    builder.push_default(StyleProperty::FontSize(font_size));
    builder.push_default(StyleProperty::Brush(color));
    builder.push_default(StyleProperty::FontWeight(parley::FontWeight::new(
        font_weight,
    )));
    builder.push_default(StyleProperty::FontStyle(if font_italic {
        parley::FontStyle::Italic
    } else {
        parley::FontStyle::Normal
    }));
    builder.push_default(StyleProperty::LetterSpacing(letter_spacing));
    builder.push_default(StyleProperty::FontFamily(parley::FontFamily::from(
        font_family.map_or(DEFAULT_UI_FONT_FAMILY, |family| family.as_ref()),
    )));
    if let Some((value, relative)) = line_height {
        builder.push_default(StyleProperty::LineHeight(if relative {
            parley::LineHeight::FontSizeRelative(value)
        } else {
            parley::LineHeight::Absolute(value)
        }));
    }
    for run in runs.iter() {
        builder.push(StyleProperty::FontSize(run.font_size), run.range.clone());
        builder.push(
            StyleProperty::FontWeight(parley::FontWeight::new(run.font_weight)),
            run.range.clone(),
        );
        builder.push(
            StyleProperty::FontStyle(if run.font_italic {
                parley::FontStyle::Italic
            } else {
                parley::FontStyle::Normal
            }),
            run.range.clone(),
        );
        builder.push(StyleProperty::Brush(run.color), run.range.clone());
        if let Some((value, relative)) = run.line_height {
            builder.push(
                StyleProperty::LineHeight(if relative {
                    parley::LineHeight::FontSizeRelative(value)
                } else {
                    parley::LineHeight::Absolute(value)
                }),
                run.range.clone(),
            );
        }
    }
    let mut layout = builder.build(&text);
    layout.break_all_lines(max_width);
    layout.align(
        match alignment {
            TextAlign::Start => Alignment::Start,
            TextAlign::Center => Alignment::Center,
            TextAlign::End => Alignment::End,
            TextAlign::Justify => Alignment::Justify,
        },
        AlignmentOptions::default(),
    );
    let layout = Arc::new(layout);
    tcx.cache.put(key, layout.clone());
    layout
}

#[allow(clippy::too_many_arguments)]
/// Shape and cache styled text using the default letter spacing.
pub fn layout_text_styled(
    tcx: &mut TextContext,
    text: Arc<str>,
    font_size: f32,
    font_weight: f32,
    font_italic: bool,
    line_height: Option<(f32, bool)>,
    alignment: TextAlign,
    color: [u8; 4],
    runs: Arc<[TextRun]>,
    font_family: Option<&Arc<str>>,
    max_width: Option<f32>,
) -> Arc<Layout<[u8; 4]>> {
    layout_text_styled_with_spacing(
        tcx,
        text,
        font_size,
        font_weight,
        font_italic,
        0.0,
        line_height,
        alignment,
        color,
        runs,
        font_family,
        max_width,
    )
}

/// Measure the intrinsic inline widths of styled text.
///
/// The returned pair is `[min_content, max_content]`. When wrapping is
/// disabled both values equal the natural unbroken width. Results share the
/// same style key as shaped layouts and are invalidated when fonts change.
#[allow(clippy::too_many_arguments)]
pub fn measure_text_intrinsic_widths(
    tcx: &mut TextContext,
    text: Arc<str>,
    font_size: f32,
    font_weight: f32,
    font_italic: bool,
    letter_spacing: f32,
    line_height: Option<(f32, bool)>,
    alignment: TextAlign,
    color: [u8; 4],
    runs: Arc<[TextRun]>,
    font_family: Option<&Arc<str>>,
    wrap: bool,
) -> [f32; 2] {
    let key = text_layout_key(
        text.clone(),
        font_size,
        font_weight,
        font_italic,
        letter_spacing,
        line_height,
        alignment,
        color,
        &runs,
        font_family,
        None,
    );
    if let Some(widths) = tcx.intrinsic_width_cache.get(&(key.clone(), wrap)) {
        return *widths;
    }
    let layout = layout_text_styled_with_spacing(
        tcx,
        text,
        font_size,
        font_weight,
        font_italic,
        letter_spacing,
        line_height,
        alignment,
        color,
        runs,
        font_family,
        None,
    );
    let widths = layout.calculate_content_widths();
    let max = widths.max.max(0.0);
    let measured = if wrap {
        [widths.min.max(0.0).min(max), max]
    } else {
        [max, max]
    };
    tcx.intrinsic_width_cache.put((key, wrap), measured);
    measured
}

/// Shape text and limit the visible result to `max_lines`.
///
/// `max_lines == 0` means unlimited. The truncation point follows grapheme
/// boundaries and is derived from Parley's actual line breaking rather than
/// an estimated character count. `ellipsis` controls whether the final visible
/// line receives an ellipsis or ends at its natural shaped boundary.
#[allow(clippy::too_many_arguments)]
pub fn layout_text_styled_clamped(
    tcx: &mut TextContext,
    text: Arc<str>,
    font_size: f32,
    font_weight: f32,
    font_italic: bool,
    letter_spacing: f32,
    line_height: Option<(f32, bool)>,
    alignment: TextAlign,
    color: [u8; 4],
    runs: Arc<[TextRun]>,
    font_family: Option<&Arc<str>>,
    max_width: Option<f32>,
    max_lines: u32,
    ellipsis: bool,
) -> Arc<Layout<[u8; 4]>> {
    let base = layout_text_styled_with_spacing(
        tcx,
        text.clone(),
        font_size,
        font_weight,
        font_italic,
        letter_spacing,
        line_height,
        alignment,
        color,
        runs.clone(),
        font_family,
        max_width,
    );
    let Ok(line_limit) = usize::try_from(max_lines) else {
        return base;
    };
    if line_limit == 0 || base.len() <= line_limit {
        return base;
    }

    let key = text_layout_key(
        text.clone(),
        font_size,
        font_weight,
        font_italic,
        letter_spacing,
        line_height,
        alignment,
        color,
        &runs,
        font_family,
        max_width,
    );
    if let Some(layout) = tcx.clamp_cache.get(&(key.clone(), max_lines, ellipsis)) {
        return layout.clone();
    }
    let final_line_end = base
        .lines()
        .nth(line_limit - 1)
        .map(|line| line.text_range().end.min(text.len()))
        .unwrap_or(0);
    if !ellipsis {
        let clipped_runs: Arc<[TextRun]> = runs
            .iter()
            .filter(|run| run.range.start < final_line_end)
            .map(|run| TextRun {
                range: run.range.start..run.range.end.min(final_line_end),
                ..run.clone()
            })
            .collect::<Vec<_>>()
            .into();
        let clipped = layout_text_styled_with_spacing(
            tcx,
            Arc::from(&text[..final_line_end]),
            font_size,
            font_weight,
            font_italic,
            letter_spacing,
            line_height,
            alignment,
            color,
            clipped_runs,
            font_family,
            max_width,
        );
        tcx.clamp_cache
            .put((key, max_lines, ellipsis), clipped.clone());
        return clipped;
    }
    let boundaries = std::iter::once(0)
        .chain(
            text[..final_line_end]
                .grapheme_indices(true)
                .map(|(index, grapheme)| index + grapheme.len()),
        )
        .collect::<Vec<_>>();
    let mut low = 0;
    let mut high = boundaries.len();
    let mut best = layout_text_styled_with_spacing(
        tcx,
        Arc::from("…"),
        font_size,
        font_weight,
        font_italic,
        letter_spacing,
        line_height,
        alignment,
        color,
        Arc::from([]),
        font_family,
        max_width,
    );
    while low < high {
        let middle = usize::midpoint(low, high);
        let prefix_end = boundaries[middle];
        let candidate: Arc<str> = Arc::from(format!("{}…", &text[..prefix_end]));
        let candidate_runs: Arc<[TextRun]> = runs
            .iter()
            .filter(|run| run.range.start < prefix_end)
            .map(|run| TextRun {
                range: run.range.start..run.range.end.min(prefix_end),
                ..run.clone()
            })
            .collect::<Vec<_>>()
            .into();
        let candidate_layout = layout_text_styled_with_spacing(
            tcx,
            candidate,
            font_size,
            font_weight,
            font_italic,
            letter_spacing,
            line_height,
            alignment,
            color,
            candidate_runs,
            font_family,
            max_width,
        );
        if candidate_layout.len() <= line_limit {
            best = candidate_layout;
            low = middle + 1;
        } else {
            high = middle;
        }
    }
    tcx.clamp_cache
        .put((key, max_lines, ellipsis), best.clone());
    best
}

/// Layout text with CSS-style single-line ellipsis when it exceeds `max_width`.
/// Truncation follows grapheme boundaries, so emoji sequences and combining
/// marks are never split in the middle.
#[allow(clippy::too_many_arguments)]
pub fn layout_text_styled_overflow(
    tcx: &mut TextContext,
    text: Arc<str>,
    font_size: f32,
    font_weight: f32,
    font_italic: bool,
    letter_spacing: f32,
    line_height: Option<(f32, bool)>,
    alignment: TextAlign,
    color: [u8; 4],
    runs: Arc<[TextRun]>,
    font_family: Option<&Arc<str>>,
    max_width: Option<f32>,
    ellipsis: bool,
) -> Arc<Layout<[u8; 4]>> {
    let layout =
        |tcx: &mut TextContext, text: Arc<str>, runs: Arc<[TextRun]>, max_width: Option<f32>| {
            layout_text_styled_with_spacing(
                tcx,
                text,
                font_size,
                font_weight,
                font_italic,
                letter_spacing,
                line_height,
                alignment,
                color,
                runs,
                font_family,
                max_width,
            )
        };
    let Some(max_width) = max_width.filter(|width| width.is_finite()) else {
        return layout(tcx, text, runs, max_width);
    };
    if !ellipsis {
        return layout(tcx, text, runs, Some(max_width));
    }

    let overflow_key = text_layout_key(
        text.clone(),
        font_size,
        font_weight,
        font_italic,
        letter_spacing,
        line_height,
        alignment,
        color,
        &runs,
        font_family,
        Some(max_width),
    );
    if let Some(layout) = tcx.ellipsis_cache.get(&overflow_key) {
        return layout.clone();
    }

    let unconstrained = layout(tcx, text.clone(), runs.clone(), None);
    if unconstrained.width() <= max_width.max(0.0) {
        tcx.ellipsis_cache.put(overflow_key, unconstrained.clone());
        return unconstrained;
    }

    let boundaries = text
        .grapheme_indices(true)
        .map(|(index, _)| index)
        .chain(std::iter::once(text.len()))
        .collect::<Vec<_>>();
    let mut low = 0;
    let mut high = boundaries.len();
    let mut best = layout(tcx, Arc::from(""), Arc::from([]), None);
    while low < high {
        let middle = usize::midpoint(low, high);
        let prefix_end = boundaries[middle];
        let candidate: Arc<str> = Arc::from(format!("{}…", &text[..prefix_end]));
        let candidate_runs: Arc<[TextRun]> = runs
            .iter()
            .filter(|run| run.range.start < prefix_end)
            .map(|run| TextRun {
                range: run.range.start..run.range.end.min(prefix_end),
                ..run.clone()
            })
            .collect::<Vec<_>>()
            .into();
        let candidate_layout = layout(tcx, candidate, candidate_runs, None);
        if candidate_layout.width() <= max_width.max(0.0) {
            best = candidate_layout;
            low = middle + 1;
        } else {
            high = middle;
        }
    }
    tcx.ellipsis_cache.put(overflow_key, best.clone());
    best
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn font_assets_register_per_context_without_flushing_warm_layouts() {
        let asset = FontAsset::from_static(include_bytes!(
            "../../../backend/examples/web-preview/public/DejaVuSans.ttf"
        ))
        .with_family("Plico font asset test");
        let mut context = TextContext::new();
        assert!(context.register_font(&asset));
        assert!(
            context
                .font_cx
                .collection
                .family_by_name("Plico font asset test")
                .is_some()
        );
        let _ = layout_text_styled(
            &mut context,
            Arc::from("font cache"),
            14.,
            400.,
            false,
            None,
            TextAlign::Start,
            [0, 0, 0, 255],
            Arc::from([]),
            None,
            None,
        );
        let entries = context.cache_entries();
        assert!(entries.0 > 0);
        assert!(!context.register_font(&asset.clone()));
        assert_eq!(context.cache_entries(), entries);
        assert!(TextContext::new().register_font(&asset));
        assert!(!context.register_font(&FontAsset::new(vec![0, 1, 2])));
    }

    #[test]
    fn outline_policy_suppresses_synthetic_bold() {
        assert_eq!(embolden_amount(true, false, 16., 2., 0.), Vec2::ZERO);
        assert_eq!(embolden_amount(false, true, 16., 2., 0.), Vec2::ZERO);
        assert_eq!(
            embolden_amount(true, true, 16., 2., 0.),
            Vec2::new(4. / 3., 4. / 3.)
        );
        let boost = 0.064;
        assert_eq!(
            embolden_amount(true, false, 16., 2., boost),
            Vec2::new(boost, boost)
        );
        assert_eq!(
            embolden_amount(true, true, 16., 2., boost),
            Vec2::new(4. / 3. + boost, 4. / 3. + boost)
        );
    }

    #[test]
    fn stem_boost_is_opt_in_and_rejects_non_finite_values() {
        for value in [
            None,
            Some(""),
            Some("invalid"),
            Some("NaN"),
            Some("inf"),
            Some("-inf"),
        ] {
            assert_eq!(stem_boost_milli_em(value), 0.);
        }
        assert_eq!(stem_boost_milli_em(Some("0")), 0.);
        assert_eq!(stem_boost_milli_em(Some("2")), 2.);
        assert_eq!(stem_boost_milli_em(Some("-2")), -2.);
    }

    #[test]
    fn text_backend_uses_xilem_style_vello_glyph_runs() {
        let context = TextContext::new();
        assert_eq!(context.raster_backend_name(), "vello-outline");
        assert_eq!(context.outline_fallback_name(), "direct-native-weight");
    }

    #[test]
    fn identical_text_layouts_are_shared() {
        let mut context = TextContext::new();
        let text: Arc<str> = Arc::from("repeated UI text");
        let first = layout_text_styled(
            &mut context,
            text.clone(),
            16.0,
            400.0,
            false,
            None,
            TextAlign::Start,
            [0, 0, 0, 255],
            Arc::from([]),
            None,
            None,
        );
        let second = layout_text_styled(
            &mut context,
            text,
            16.0,
            400.0,
            false,
            None,
            TextAlign::Start,
            [0, 0, 0, 255],
            Arc::from([]),
            None,
            None,
        );

        assert!(Arc::ptr_eq(&first, &second));
        assert_eq!(context.cache.len(), 1);
    }

    #[test]
    fn resolved_text_synthesis_is_inspectable_without_rasterizing() {
        let mut context = TextContext::new();
        let layout = layout_text(&mut context, "ordinary text", 16.0);

        assert_eq!(text_synthesis(&layout), TextSynthesis::default());
    }

    #[test]
    fn live_layout_keeps_font_identity_after_source_cache_pruning() {
        fn font_id(layout: &Layout<[u8; 4]>) -> u64 {
            layout
                .lines()
                .flat_map(|line| line.items())
                .find_map(|item| {
                    if let PositionedLayoutItem::GlyphRun(run) = item {
                        Some(run.run().font().data.id())
                    } else {
                        None
                    }
                })
                .expect("fixture must shape a glyph run")
        }
        let mut context = TextContext::new();
        let first = layout_text(&mut context, "keep this font alive", 16.);
        let id = font_id(&first);
        // Parley ages its source cache on every new layout. Old lyric layouts
        // can still own a font after its source cache entry is pruned.
        context.font_cx.source_cache.prune(0, false);
        let second = layout_text(&mut context, "another lyric line", 16.);
        assert_eq!(font_id(&second), id, "must reuse the still-live font Blob");
        for frame in 0..600 {
            let layout = layout_text(&mut context, &format!("animated line {frame}"), 16.);
            assert_eq!(
                font_id(&layout),
                id,
                "font identity changed at frame {frame}"
            );
        }
    }

    #[test]
    fn ellipsis_layout_respects_width_and_grapheme_boundaries() {
        let mut context = TextContext::new();
        let text: Arc<str> = Arc::from("Family 👨‍👩‍👧‍👦 credentials and recovery material");
        let full = layout_text_styled_overflow(
            &mut context,
            text.clone(),
            16.0,
            400.0,
            false,
            0.0,
            None,
            TextAlign::Start,
            [0, 0, 0, 255],
            Arc::from([]),
            None,
            None,
            true,
        );
        let truncated = layout_text_styled_overflow(
            &mut context,
            text,
            16.0,
            400.0,
            false,
            0.0,
            None,
            TextAlign::Start,
            [0, 0, 0, 255],
            Arc::from([]),
            None,
            Some(120.0),
            true,
        );
        let cached = layout_text_styled_overflow(
            &mut context,
            Arc::from("Family 👨‍👩‍👧‍👦 credentials and recovery material"),
            16.0,
            400.0,
            false,
            0.0,
            None,
            TextAlign::Start,
            [0, 0, 0, 255],
            Arc::from([]),
            None,
            Some(120.0),
            true,
        );

        assert!(full.width() > 120.0);
        assert!(truncated.width() <= 120.0);
        assert_eq!(truncated.lines().count(), 1);
        assert!(Arc::ptr_eq(&truncated, &cached));
    }

    #[test]
    fn clamped_layout_uses_actual_line_breaks_and_is_cached() {
        let mut context = TextContext::new();
        let text: Arc<str> = Arc::from(
            "A long native UI description with emoji 👨‍👩‍👧‍👦 that needs several wrapped lines",
        );
        let full = layout_text_styled(
            &mut context,
            text.clone(),
            16.0,
            400.0,
            false,
            None,
            TextAlign::Start,
            [0, 0, 0, 255],
            Arc::from([]),
            None,
            Some(120.0),
        );
        let clamped = layout_text_styled_clamped(
            &mut context,
            text.clone(),
            16.0,
            400.0,
            false,
            0.0,
            None,
            TextAlign::Start,
            [0, 0, 0, 255],
            Arc::from([]),
            None,
            Some(120.0),
            2,
            true,
        );
        let cached = layout_text_styled_clamped(
            &mut context,
            text,
            16.0,
            400.0,
            false,
            0.0,
            None,
            TextAlign::Start,
            [0, 0, 0, 255],
            Arc::from([]),
            None,
            Some(120.0),
            2,
            true,
        );
        let clipped = layout_text_styled_clamped(
            &mut context,
            Arc::from(
                "A long native UI description with emoji 👨‍👩‍👧‍👦 that needs several wrapped lines",
            ),
            16.0,
            400.0,
            false,
            0.0,
            None,
            TextAlign::Start,
            [0, 0, 0, 255],
            Arc::from([]),
            None,
            Some(120.0),
            2,
            false,
        );

        assert!(full.len() > 2);
        assert_eq!(clamped.len(), 2);
        assert!(clamped.height() < full.height());
        assert!(Arc::ptr_eq(&clamped, &cached));
        assert_eq!(clipped.len(), 2);
        assert!(!Arc::ptr_eq(&clamped, &clipped));
    }

    #[test]
    fn letter_spacing_changes_measurement_and_cache_identity() {
        let mut context = TextContext::new();
        let text: Arc<str> = Arc::from("Tracking");
        let normal = layout_text_styled_with_spacing(
            &mut context,
            text.clone(),
            16.0,
            400.0,
            false,
            0.0,
            None,
            TextAlign::Start,
            [0, 0, 0, 255],
            Arc::from([]),
            None,
            None,
        );
        let wide = layout_text_styled_with_spacing(
            &mut context,
            text.clone(),
            16.0,
            400.0,
            false,
            2.0,
            None,
            TextAlign::Start,
            [0, 0, 0, 255],
            Arc::from([]),
            None,
            None,
        );
        let cached = layout_text_styled_with_spacing(
            &mut context,
            text,
            16.0,
            400.0,
            false,
            2.0,
            None,
            TextAlign::Start,
            [0, 0, 0, 255],
            Arc::from([]),
            None,
            None,
        );

        assert!(wide.width() > normal.width());
        assert!(!Arc::ptr_eq(&normal, &wide));
        assert!(Arc::ptr_eq(&wide, &cached));
    }

    #[test]
    fn intrinsic_widths_distinguish_wrapped_and_unbroken_text() {
        let mut context = TextContext::new();
        let text: Arc<str> = Arc::from("short words with one extraordinarilylongtoken");
        let measure = |context: &mut TextContext, wrap| {
            measure_text_intrinsic_widths(
                context,
                text.clone(),
                16.0,
                400.0,
                false,
                0.0,
                None,
                TextAlign::Start,
                [0, 0, 0, 255],
                Arc::from([]),
                None,
                wrap,
            )
        };

        let wrapped = measure(&mut context, true);
        let unbroken = measure(&mut context, false);
        let cached = measure(&mut context, true);

        assert!(wrapped[0] < wrapped[1]);
        assert_eq!(unbroken, [wrapped[1], wrapped[1]]);
        assert_eq!(cached, wrapped);
        assert_eq!(context.cache.len(), 1);
        assert_eq!(context.intrinsic_width_cache.len(), 2);
    }

    #[test]
    fn identical_layout_reuses_encoded_glyph_scene() {
        let mut context = TextContext::new();
        let layout = layout_text(&mut context, "🚀", 28.0);
        let first = context.glyph_scene(&layout);
        let second = context.glyph_scene(&layout);

        assert!(Arc::ptr_eq(&first, &second));
        assert_eq!(context.glyph_cache.len(), 1);
    }

    #[test]
    fn glyph_scene_cache_distinguishes_device_scales() {
        let mut context = TextContext::new();
        let layout = layout_text(&mut context, "🚀", 28.0);
        let one_x = context.glyph_scene_scaled(&layout, 1.0);
        let two_x = context.glyph_scene_scaled(&layout, 2.0);

        assert!(!Arc::ptr_eq(&one_x, &two_x));
        assert_eq!(context.glyph_cache.len(), 2);
    }

    #[test]
    fn single_line_metrics_center_the_line_box_without_baseline_guessing() {
        let mut context = TextContext::new();
        let layout = layout_text(&mut context, "example.com", 14.0);
        let metrics = single_line_text_metrics(&layout, layout.height() + 8.0)
            .expect("a single-line layout should expose metrics");
        let baseline = layout
            .lines()
            .next()
            .expect("the layout should contain one line")
            .metrics()
            .baseline;

        assert_eq!(metrics.line_box[1], 4.0);
        assert_eq!(metrics.baseline, 4.0 + baseline);
    }

    #[test]
    fn constrained_text_wraps_and_uses_a_distinct_cache_entry() {
        let mut context = TextContext::new();
        let text: Arc<str> = Arc::from("a title that should wrap across multiple lines");
        let wide = layout_text_styled(
            &mut context,
            text.clone(),
            16.0,
            400.0,
            false,
            None,
            TextAlign::Start,
            [0, 0, 0, 255],
            Arc::from([]),
            None,
            None,
        );
        let narrow = layout_text_styled(
            &mut context,
            text,
            16.0,
            400.0,
            false,
            None,
            TextAlign::Start,
            [0, 0, 0, 255],
            Arc::from([]),
            None,
            Some(80.0),
        );
        assert!(narrow.height() > wide.height());
        assert_eq!(context.cache.len(), 2);
    }

    #[test]
    fn alignment_is_part_of_the_layout_cache_key() {
        let mut context = TextContext::new();
        let text: Arc<str> = Arc::from("centered");
        layout_text_styled(
            &mut context,
            text.clone(),
            16.0,
            400.0,
            false,
            None,
            TextAlign::Start,
            [0, 0, 0, 255],
            Arc::from([]),
            None,
            Some(200.0),
        );
        layout_text_styled(
            &mut context,
            text,
            16.0,
            400.0,
            false,
            None,
            TextAlign::Center,
            [0, 0, 0, 255],
            Arc::from([]),
            None,
            Some(200.0),
        );
        assert_eq!(context.cache.len(), 2);
    }
}
