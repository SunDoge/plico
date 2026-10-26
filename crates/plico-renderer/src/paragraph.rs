//! Host-selected ordinary-text layout. Native widgets retain their own editors.
use crate::{
    style::Paint,
    style::TextAlign,
    text::{FontAsset, TextRun},
};
use std::{rc::Rc, sync::Arc};

/// Backend-independent paragraph declaration, in logical pixels and UTF-8.
#[derive(Clone, Debug, PartialEq)]
pub struct ParagraphRequest {
    /// Host font-collection revision, so retained drawing detects font changes.
    pub font_revision: u64,
    /// Original UTF-8 contents.
    pub text: Arc<str>,
    /// Base font size.
    pub font_size: f32,
    /// CSS font weight.
    pub font_weight: f32,
    /// Italic face selection.
    pub italic: bool,
    /// Additional spacing in logical pixels.
    pub letter_spacing: f32,
    /// Line height and whether it is a font-size multiplier.
    pub line_height: Option<(f32, bool)>,
    /// Paragraph alignment.
    pub align: TextAlign,
    /// Base RGBA color.
    pub color: [u8; 4],
    /// Range overrides, indexed by UTF-8 bytes.
    pub runs: Arc<[TextRun]>,
    /// Comma-separated family preference list.
    pub family: Arc<str>,
    /// Wrapping/truncation width; absent means intrinsic width.
    pub width: Option<f32>,
    /// Whether wrapping is enabled.
    pub wrap: bool,
    /// Zero means unlimited.
    pub max_lines: usize,
    /// Append an ellipsis to the final truncated line.
    pub ellipsis: bool,
}
impl ParagraphRequest {
    /// Build a request for a resolved text node.
    pub fn from_paint(paint: &Paint, width: Option<f32>) -> Option<Self> {
        Some(Self {
            font_revision: 0,
            text: paint.text.clone()?,
            font_size: paint.font_size,
            font_weight: paint.font_weight,
            italic: paint.font_italic,
            letter_spacing: paint.letter_spacing,
            line_height: paint.line_height,
            align: paint.text_align,
            color: crate::text::brush_for_color(paint.text_color),
            runs: paint.text_runs.clone(),
            family: paint
                .font_family
                .clone()
                .unwrap_or_else(|| crate::text::DEFAULT_UI_FONT_FAMILY.into()),
            width,
            wrap: paint.wrap_text || paint.text_max_lines > 0,
            max_lines: paint.text_max_lines as usize,
            ellipsis: paint.text_ellipsis || (paint.text_max_lines > 0 && !paint.text_clip_lines),
        })
    }
    /// Cache fingerprint. Consumers must also compare the full request on a hit.
    pub fn fingerprint(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        self.font_revision.hash(&mut h);
        self.text.hash(&mut h);
        self.family.hash(&mut h);
        self.color.hash(&mut h);
        self.font_size.to_bits().hash(&mut h);
        self.font_weight.to_bits().hash(&mut h);
        self.italic.hash(&mut h);
        self.letter_spacing.to_bits().hash(&mut h);
        self.line_height.map(|(v, r)| (v.to_bits(), r)).hash(&mut h);
        self.align.hash(&mut h);
        self.width.map(f32::to_bits).hash(&mut h);
        self.max_lines.hash(&mut h);
        self.ellipsis.hash(&mut h);
        self.wrap.hash(&mut h);
        for r in self.runs.iter() {
            r.range.hash(&mut h);
            r.font_size.to_bits().hash(&mut h);
            r.font_weight.to_bits().hash(&mut h);
            r.font_italic.hash(&mut h);
            r.line_height.map(|(v, r)| (v.to_bits(), r)).hash(&mut h);
            r.color.hash(&mut h);
        }
        h.finish()
    }
}
/// Geometry from the same paragraph used to paint and hit-test.
#[derive(Clone, Copy, Debug, Default)]
pub struct ParagraphMetrics {
    /// Content width.
    pub width: f32,
    /// Content height.
    pub height: f32,
    /// Minimum and maximum intrinsic widths.
    pub intrinsic: [f32; 2],
    /// First and last alphabetic baseline.
    pub baselines: [Option<f32>; 2],
    /// First line height, for alignment of control indicators.
    pub first_line_height: f32,
}
/// A native paragraph stays on the host UI thread.
pub trait NativeParagraph {
    /// Measurement in logical pixels.
    fn metrics(&self) -> ParagraphMetrics;
    /// UTF-8 insertion boundary nearest a local point.
    fn hit_test(&self, x: f32, y: f32) -> usize;
    /// Selection rectangles for a UTF-8 byte range.
    fn selection_rects(&self, range: std::ops::Range<usize>) -> Vec<[f32; 4]>;
}
/// Installed by a host; renderer-neutral crates do not depend on Skia.
pub trait ParagraphEngine {
    /// Stable diagnostic name.
    fn name(&self) -> &'static str;
    /// Shape, lay out and cache a complete declaration.
    fn layout(&mut self, request: Arc<ParagraphRequest>) -> Rc<dyn NativeParagraph>;
    /// Register an application font before layout.
    fn register_font(&mut self, asset: &FontAsset);
    /// Release derived paragraph caches.
    fn clear(&mut self);
}

/// Ordinary paragraph geometry used by selection and control alignment.
#[derive(Clone)]
pub enum ParagraphLayout {
    /// Default renderer-neutral text path.
    Parley(Arc<parley::Layout<[u8; 4]>>),
    /// Host-native paragraph path.
    Native(Rc<dyn NativeParagraph>),
}
impl ParagraphLayout {
    /// Content metrics.
    pub fn metrics(&self) -> ParagraphMetrics {
        match self {
            Self::Native(p) => p.metrics(),
            Self::Parley(p) => {
                let mut lines = p.lines();
                let first = lines
                    .next()
                    .map(|l| (l.metrics().baseline, l.metrics().line_height));
                let last = lines
                    .last()
                    .map(|l| l.metrics().baseline)
                    .or(first.map(|l| l.0));
                ParagraphMetrics {
                    width: p.width(),
                    height: p.height(),
                    baselines: [first.map(|l| l.0), last],
                    first_line_height: first.map_or(0., |l| l.1),
                    ..Default::default()
                }
            }
        }
    }
    /// UTF-8 insertion boundary nearest a local point.
    pub fn hit_test(&self, x: f32, y: f32) -> usize {
        match self {
            Self::Native(p) => p.hit_test(x, y),
            Self::Parley(p) => parley::Cursor::from_point(p, x, y).index(),
        }
    }
    /// Selection rectangles in paragraph coordinates.
    pub fn selection_rects(&self, range: std::ops::Range<usize>) -> Vec<[f32; 4]> {
        match self {
            Self::Native(p) => p.selection_rects(range),
            Self::Parley(p) => parley::Selection::new(
                parley::Cursor::from_byte_index(p, range.start, parley::Affinity::Downstream),
                parley::Cursor::from_byte_index(p, range.end, parley::Affinity::Upstream),
            )
            .geometry(p)
            .into_iter()
            .map(|(r, _)| [r.x0 as f32, r.y0 as f32, r.x1 as f32, r.y1 as f32])
            .collect(),
        }
    }
}
