//! DirectWrite layout in logical DIPs. No bitmap scaling or second shaping pass.
//! Contexts and layouts stay on their creating UI thread. GPU raster integration
//! is separate; this module supplies editing geometry and raw grayscale run masks.
mod raster;
pub use crate::text_direction::paragraph_is_rtl;
use crate::text_indices::TextIndices;
pub use raster::{ColorMask, GlyphMask, Rasterized};
use snafu::{ResultExt, Snafu};
use std::{marker::PhantomData, ops::Range, rc::Rc};
use windows::{
    Win32::{Globalization::GetUserDefaultLocaleName, Graphics::DirectWrite::*},
    core::{BOOL, HSTRING, Interface},
};

/// DirectWrite/native input failures remain observable to the host.
#[derive(Debug, Snafu)]
pub enum Error {
    /// Invalid correction parameters or foreground color.
    #[snafu(display("DirectWrite coverage correction failed: {source}"))]
    Correction {
        /// Validated correction error.
        source: crate::directwrite_correction::InvalidCorrection,
    },
    /// A raster operation requires coverage not implemented by this prototype.
    #[snafu(display("Unsupported DirectWrite raster operation: {reason}"))]
    Unsupported {
        /// Explanation without document contents.
        reason: &'static str,
    },
    /// Invalid geometry, text offsets or excessive allocation.
    #[snafu(display("Invalid or oversized DirectWrite text request"))]
    InvalidRequest,
    /// Native DirectWrite failure, including missing font service support.
    #[snafu(display("DirectWrite failed: {source}"))]
    Native {
        /// Native error.
        source: windows::core::Error,
    },
}
/// Result returned by the Windows text service.
pub type Result<T> = std::result::Result<T, Error>;
/// Alignment relative to the paragraph's reading direction.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Alignment {
    /// Leading edge (left for LTR, right for RTL).
    #[default]
    Start,
    /// Center within the available width.
    Center,
    /// Trailing edge.
    End,
    /// Expand eligible lines to fill the available width.
    Justify,
}
/// Basic system-font paragraph. Sizes and widths are logical DIPs.
pub struct Request<'a> {
    /// Original UTF-8 contents.
    pub text: &'a str,
    /// One installed system font family (not a CSS preference list).
    pub family: &'a str,
    /// Logical font size.
    pub size: f32,
    /// Numeric font weight.
    pub weight: u16,
    /// Select italic face.
    pub italic: bool,
    /// Available logical width; None measures intrinsic width.
    pub width: Option<f32>,
    /// Wrap within width. Hard line breaks remain effective without wrapping.
    pub wrap: bool,
    /// Explicit paragraph base direction; character bidi still uses DirectWrite.
    pub rtl: bool,
    /// Alignment performed by the retained native layout, including hit testing.
    pub alignment: Alignment,
    /// Uniform logical line-box height. Extra leading is split around the
    /// first natural line's baseline; glyph outlines are never scaled.
    pub line_height: Option<f32>,
    /// Additional logical spacing after each native character in reading order.
    /// Negative values tighten spacing; native advances are clamped at zero.
    pub letter_spacing: f32,
}
impl<'a> Request<'a> {
    /// Default Segoe UI paragraph at 14 DIPs.
    pub fn new(text: &'a str) -> Self {
        Self {
            text,
            family: "Segoe UI",
            size: 14.,
            weight: 400,
            italic: false,
            width: None,
            wrap: true,
            rtl: false,
            alignment: Alignment::Start,
            line_height: None,
            letter_spacing: 0.,
        }
    }
}
/// Logical line geometry and its length in native UTF-16 units.
#[derive(Clone, Copy, Debug)]
pub struct Line {
    /// Logical line-box height.
    pub height: f32,
    /// Baseline relative to the line top.
    pub baseline: f32,
    /// Native source length, including trailing whitespace/newlines.
    pub utf16_len: u32,
}
/// Logical metrics from the exact retained DirectWrite layout.
#[derive(Clone, Copy, Debug)]
pub struct Metrics {
    /// Minimum width without breaking an unbreakable segment. For no-wrap
    /// requests this equals `max_content_width`.
    pub min_content_width: f32,
    /// Longest hard line measured before applying the available width.
    pub max_content_width: f32,
    /// Content width including trailing whitespace.
    pub width: f32,
    /// Content height.
    pub height: f32,
    /// First/last absolute line baselines.
    pub baselines: [Option<f32>; 2],
}
/// An isolated system font/layout factory, without process-global mutation.
pub struct Context {
    factory: IDWriteFactory2,
    fonts: IDWriteFontCollection,
    locale: HSTRING,
    _thread: PhantomData<Rc<()>>,
}
/// Retained native paragraph; source offsets at its public boundary are UTF-8.
pub struct Paragraph {
    layout: IDWriteTextLayout,
    source: String,
    indices: TextIndices,
    metrics: Metrics,
    lines: Vec<Line>,
    ink: [f32; 4],
    // Retain the factory explicitly; it drops after the layout/font references.
    _factory: IDWriteFactory2,
    _thread: PhantomData<Rc<()>>,
}
impl Context {
    /// Create a DirectWrite factory and use the user's default Windows locale.
    pub fn new() -> Result<Self> {
        let mut locale = [0u16; 85];
        // SAFETY: the API receives the complete writable buffer length.
        let length = unsafe { GetUserDefaultLocaleName(&mut locale) };
        let locale = if length > 1 {
            HSTRING::from_wide(&locale[..length as usize - 1])
        } else {
            HSTRING::from("en-US")
        };
        // SAFETY: the typed result acquires the factory COM reference.
        let factory: IDWriteFactory2 =
            unsafe { DWriteCreateFactory(DWRITE_FACTORY_TYPE_ISOLATED) }.context(NativeSnafu)?;
        let mut fonts = None;
        // SAFETY: the factory returns an owned COM reference into valid storage.
        unsafe { factory.GetSystemFontCollection(&mut fonts, false) }.context(NativeSnafu)?;
        let fonts = fonts.ok_or(Error::InvalidRequest)?;
        Ok(Self {
            factory,
            fonts,
            locale,
            _thread: PhantomData,
        })
    }
    /// Resolve an ordered list of family names against this context's system
    /// collection. Names are already parsed, without CSS quotes or separators.
    /// Generic families map to Windows defaults. `None` means no candidate was
    /// installed; the caller decides its fallback rather than silently choosing.
    /// This selects the primary family, not per-character fallback coverage.
    pub fn resolve_family(&self, candidates: &[&str]) -> Result<Option<String>> {
        if candidates.len() > 128 {
            return Err(Error::InvalidRequest);
        }
        for candidate in candidates {
            let candidate = candidate.trim();
            if candidate.is_empty() || candidate.len() > 4096 || candidate.contains('\0') {
                return Err(Error::InvalidRequest);
            }
            let family = if candidate.eq_ignore_ascii_case("sans-serif")
                || candidate.eq_ignore_ascii_case("system-ui")
            {
                "Segoe UI"
            } else if candidate.eq_ignore_ascii_case("serif") {
                "Times New Roman"
            } else if candidate.eq_ignore_ascii_case("monospace") {
                "Consolas"
            } else {
                candidate
            };
            let name = HSTRING::from(family);
            let mut index = 0;
            let mut exists = BOOL::default();
            // SAFETY: UTF-16 name and output pointers live through the call.
            unsafe { self.fonts.FindFamilyName(&name, &mut index, &mut exists) }
                .context(NativeSnafu)?;
            if exists.as_bool() {
                return Ok(Some(family.to_owned()));
            }
        }
        Ok(None)
    }
    /// Shape and retain one bounded paragraph. No system clock is read.
    pub fn layout(&self, request: &Request<'_>) -> Result<Paragraph> {
        if request.text.len() > 1024 * 1024
            || request.family.is_empty()
            || request.family.len() > 4096
            || request.family.contains('\0')
            || !request.size.is_finite()
            || request.size <= 0.
            || request.size > 4096.
            || !(1..=1000).contains(&request.weight)
            || !request.letter_spacing.is_finite()
            || request.letter_spacing.abs() > 4096.
            || request
                .line_height
                .is_some_and(|h| !h.is_finite() || h <= 0. || h > 1e7)
            || request
                .width
                .is_some_and(|w| !w.is_finite() || w <= 0. || w > 1e7)
        {
            return Err(Error::InvalidRequest);
        }
        let utf16: Vec<u16> = request.text.encode_utf16().collect();
        let family = HSTRING::from(request.family);
        // SAFETY: DirectWrite copies text and format properties before the local
        // UTF-16/family buffers drop. The layout owns its COM/font references.
        let layout = unsafe {
            let format = self
                .factory
                .CreateTextFormat(
                    &family,
                    &self.fonts,
                    DWRITE_FONT_WEIGHT(i32::from(request.weight)),
                    if request.italic {
                        DWRITE_FONT_STYLE_ITALIC
                    } else {
                        DWRITE_FONT_STYLE_NORMAL
                    },
                    DWRITE_FONT_STRETCH_NORMAL,
                    request.size,
                    &self.locale,
                )
                .context(NativeSnafu)?;
            format
                .SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP)
                .context(NativeSnafu)?;
            format
                .SetReadingDirection(if request.rtl {
                    DWRITE_READING_DIRECTION_RIGHT_TO_LEFT
                } else {
                    DWRITE_READING_DIRECTION_LEFT_TO_RIGHT
                })
                .context(NativeSnafu)?;
            self.factory
                .CreateTextLayout(&utf16, &format, 16_777_216., 16_777_216.)
                .context(NativeSnafu)?
        };
        if request.letter_spacing != 0. && !utf16.is_empty() {
            let spaced: IDWriteTextLayout1 = layout.cast().context(NativeSnafu)?;
            // SAFETY: the full UTF-16 range is bounded by the validated source;
            // native layout applies spacing before intrinsic measurement.
            unsafe {
                spaced.SetCharacterSpacing(
                    0.,
                    request.letter_spacing,
                    0.,
                    DWRITE_TEXT_RANGE {
                        startPosition: 0,
                        length: utf16.len() as u32,
                    },
                )
            }
            .context(NativeSnafu)?;
        }
        let mut native = DWRITE_TEXT_METRICS::default();
        let max_content_width;
        let min_content_width;
        // SAFETY: output storage is initialized and valid for each native call.
        unsafe {
            layout.GetMetrics(&mut native).context(NativeSnafu)?;
            max_content_width = native.widthIncludingTrailingWhitespace;
            if !max_content_width.is_finite() || !(0. ..=1e7).contains(&max_content_width) {
                return Err(Error::InvalidRequest);
            }
            min_content_width = if request.wrap {
                layout
                    .SetWordWrapping(DWRITE_WORD_WRAPPING_WRAP)
                    .context(NativeSnafu)?;
                layout.DetermineMinWidth().context(NativeSnafu)?
            } else {
                max_content_width
            };
            // Intrinsic requests remain unwrapped; keep RTL text near the
            // origin rather than aligned to the measurement-only width.
            if request.width.is_none() {
                layout
                    .SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP)
                    .context(NativeSnafu)?;
            }
            layout
                .SetMaxWidth(request.width.unwrap_or(max_content_width.ceil().max(1.)))
                .context(NativeSnafu)?;
            layout
                .SetTextAlignment(match request.alignment {
                    Alignment::Start => DWRITE_TEXT_ALIGNMENT_LEADING,
                    Alignment::Center => DWRITE_TEXT_ALIGNMENT_CENTER,
                    Alignment::End => DWRITE_TEXT_ALIGNMENT_TRAILING,
                    Alignment::Justify => DWRITE_TEXT_ALIGNMENT_JUSTIFIED,
                })
                .context(NativeSnafu)?;
            layout.GetMetrics(&mut native).context(NativeSnafu)?;
        }
        if ![
            native.widthIncludingTrailingWhitespace,
            native.height,
            min_content_width,
        ]
        .iter()
        .all(|v| v.is_finite() && *v >= 0. && *v <= 1e7)
        {
            return Err(Error::InvalidRequest);
        }
        let mut count = native.lineCount;
        if count > 100_000 {
            return Err(Error::InvalidRequest);
        }
        let mut raw = vec![DWRITE_LINE_METRICS::default(); count as usize];
        if count > 0 {
            // SAFETY: the slice has exactly the size requested by DirectWrite.
            unsafe { layout.GetLineMetrics(Some(&mut raw), &mut count) }.context(NativeSnafu)?;
        }
        if let Some(height) = request.line_height
            && let Some(first) = raw.first()
        {
            let baseline = first.baseline + (height - first.height) * 0.5;
            if !baseline.is_finite() || baseline < 0. {
                return Err(Error::InvalidRequest);
            }
            // SAFETY: the retained layout owns all shaping data. Uniform line
            // spacing changes vertical geometry, not wrapping or source ranges.
            unsafe {
                layout
                    .SetLineSpacing(DWRITE_LINE_SPACING_METHOD_UNIFORM, height, baseline)
                    .context(NativeSnafu)?;
                layout.GetMetrics(&mut native).context(NativeSnafu)?;
                layout
                    .GetLineMetrics(Some(&mut raw), &mut count)
                    .context(NativeSnafu)?;
            }
            if !native.height.is_finite() || !(0. ..=1e7).contains(&native.height) {
                return Err(Error::InvalidRequest);
            }
        }
        // SAFETY: the retained layout owns all glyph data used by the query.
        let overhang = unsafe { layout.GetOverhangMetrics() }.context(NativeSnafu)?;
        let ink = [
            -overhang.left,
            -overhang.top,
            native.layoutWidth + overhang.right,
            native.layoutHeight + overhang.bottom,
        ];
        if !ink.iter().all(|v| v.is_finite() && v.abs() <= 1e9) {
            return Err(Error::InvalidRequest);
        }
        let lines: Vec<_> = raw
            .into_iter()
            .take(count as usize)
            .map(|l| Line {
                height: l.height,
                baseline: l.baseline,
                utf16_len: l.length,
            })
            .collect();
        let last_top: f32 = lines
            .iter()
            .take(lines.len().saturating_sub(1))
            .map(|l| l.height)
            .sum();
        let metrics = Metrics {
            min_content_width,
            max_content_width,
            width: native.widthIncludingTrailingWhitespace,
            height: native.height,
            baselines: [
                lines.first().map(|l| l.baseline),
                lines.last().map(|l| last_top + l.baseline),
            ],
        };
        Ok(Paragraph {
            layout,
            source: request.text.into(),
            indices: TextIndices::new(request.text),
            metrics,
            lines,
            ink,
            _factory: self.factory.clone(),
            _thread: PhantomData,
        })
    }
}
impl Paragraph {
    /// Unhinted visible bounds [x0,y0,x1,y1] in logical DIPs, including native
    /// alignment and glyph overhang. Add a conservative margin before culling:
    /// native grid fitting and physical pixel snapping can change final bounds.
    pub fn ink_bounds(&self) -> [f32; 4] {
        self.ink
    }
    /// Retained logical metrics.
    pub fn metrics(&self) -> Metrics {
        self.metrics
    }
    /// Native line metrics, without reshaping.
    pub fn lines(&self) -> &[Line] {
        &self.lines
    }
    /// UTF-8 to UTF-16; rejects mid-scalar offsets.
    pub fn utf16_from_byte(&self, byte: usize) -> Option<u32> {
        self.indices.utf16(byte)
    }
    /// UTF-16 to UTF-8; rejects surrogate interiors.
    pub fn byte_from_utf16(&self, units: u32) -> Option<usize> {
        self.indices.byte(units)
    }
    /// Nearest source insertion boundary to a logical point, respecting clusters.
    pub fn hit_test(&self, x: f32, y: f32) -> Result<usize> {
        if !x.is_finite() || !y.is_finite() || x.abs() > 1e9 || y.abs() > 1e9 {
            return Err(Error::InvalidRequest);
        }
        let mut trailing = BOOL::default();
        let mut inside = BOOL::default();
        let mut hit = DWRITE_HIT_TEST_METRICS::default();
        // SAFETY: pointers refer to valid output storage; layout stays alive.
        unsafe {
            self.layout
                .HitTestPoint(x, y, &mut trailing, &mut inside, &mut hit)
        }
        .context(NativeSnafu)?;
        let position = hit
            .textPosition
            .checked_add(if trailing.as_bool() { hit.length } else { 0 })
            .ok_or(Error::InvalidRequest)?;
        self.indices.byte(position).ok_or(Error::InvalidRequest)
    }
    /// One-physical-pixel caret [x,y,width,height] at a valid UTF-8 boundary.
    pub fn caret_rect(&self, byte: usize, scale: f32) -> Result<[f32; 4]> {
        if !scale.is_finite() || scale <= 0. || scale > 8. {
            return Err(Error::InvalidRequest);
        }
        let position = self.indices.utf16(byte).ok_or(Error::InvalidRequest)?;
        let mut x = 0.;
        let mut y = 0.;
        let mut hit = DWRITE_HIT_TEST_METRICS::default();
        // SAFETY: source position was checked against this layout's original text.
        unsafe {
            self.layout
                .HitTestTextPosition(position, false, &mut x, &mut y, &mut hit)
        }
        .context(NativeSnafu)?;
        Ok([x, y, 1. / scale, hit.height])
    }
    /// Bidi-aware selection rectangles [x,y,width,height], in logical DIPs.
    pub fn selection_rects(&self, range: Range<usize>) -> Result<Vec<[f32; 4]>> {
        if range.start > range.end || range.end > self.source.len() {
            return Err(Error::InvalidRequest);
        }
        let start = self
            .indices
            .utf16(range.start)
            .ok_or(Error::InvalidRequest)?;
        let end = self.indices.utf16(range.end).ok_or(Error::InvalidRequest)?;
        if start == end {
            return Ok(Vec::new());
        }
        let mut count = 0;
        // SAFETY: the first query only asks for the required buffer length.
        let first = unsafe {
            self.layout
                .HitTestTextRange(start, end - start, 0., 0., None, &mut count)
        };
        if let Err(error) = first {
            // ERROR_INSUFFICIENT_BUFFER is the expected size-query result.
            if error.code() != windows::core::HRESULT::from_win32(122) {
                return Err(Error::Native { source: error });
            }
        }
        if count == 0 {
            return Ok(Vec::new());
        }
        if count > 100_000 {
            return Err(Error::InvalidRequest);
        }
        let mut raw = vec![DWRITE_HIT_TEST_METRICS::default(); count as usize];
        // SAFETY: checked range and correctly sized writable output slice.
        unsafe {
            self.layout
                .HitTestTextRange(start, end - start, 0., 0., Some(&mut raw), &mut count)
        }
        .context(NativeSnafu)?;
        Ok(raw
            .into_iter()
            .take(count as usize)
            .map(|h| [h.left, h.top, h.width, h.height])
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn character_spacing_changes_intrinsics_and_caret_advances_together() {
        let context = Context::new().unwrap();
        let mut request = Request::new("MMMM");
        let natural = context.layout(&request).unwrap();
        let natural_caret = natural.caret_rect(2, 1.).unwrap()[0];
        request.letter_spacing = 3.;
        let spaced = context.layout(&request).unwrap();
        assert!(spaced.metrics().max_content_width > natural.metrics().max_content_width);
        assert!((spaced.caret_rect(2, 1.).unwrap()[0] - natural_caret - 6.).abs() < 0.01);
        let intrinsic = spaced.metrics().max_content_width;
        request.width = Some(20.);
        let wrapped = context.layout(&request).unwrap();
        assert!(wrapped.lines().len() > 1);
        assert_eq!(wrapped.metrics().max_content_width, intrinsic);
        request.width = None;
        request.letter_spacing = -1.;
        let tight = context.layout(&request).unwrap();
        assert!(tight.metrics().max_content_width < natural.metrics().max_content_width);
        for invalid in [f32::NAN, f32::INFINITY, 4097., -4097.] {
            request.letter_spacing = invalid;
            assert!(matches!(
                context.layout(&request),
                Err(Error::InvalidRequest)
            ));
        }
    }
    #[test]
    fn overhang_estimate_conservatively_contains_native_run_masks() {
        let context = Context::new().unwrap();
        for rtl in [false, true] {
            let mut request = Request::new("fÅj Hello");
            request.italic = true;
            request.rtl = rtl;
            request.width = Some(240.);
            request.alignment = Alignment::Center;
            request.line_height = Some(12.);
            let paragraph = context.layout(&request).unwrap();
            let [left, top, right, bottom] = paragraph.ink_bounds();
            for scale in [1., 1.25, 2., 8.] {
                let margin = 2. * scale + 2.;
                let raster = paragraph.rasterize(scale, [0.75, 0.]).unwrap();
                for mask in raster.masks {
                    assert!(mask.origin[0] as f32 >= left * scale - margin);
                    assert!(mask.origin[1] as f32 >= top * scale - margin);
                    assert!(mask.origin[0] as f32 + mask.size[0] as f32 <= right * scale + margin);
                    assert!(mask.origin[1] as f32 + mask.size[1] as f32 <= bottom * scale + margin);
                }
            }
        }
    }
    #[test]
    fn uniform_line_height_updates_baselines_and_editing_geometry() {
        let context = Context::new().unwrap();
        let mut request = Request::new("Hello\nWorld");
        let natural = context.layout(&request).unwrap();
        let first = natural.lines()[0];
        let height = first.height + 20.;
        request.line_height = Some(height);
        let spaced = context.layout(&request).unwrap();
        assert_eq!(spaced.lines().len(), 2);
        for line in spaced.lines() {
            assert!((line.height - height).abs() < 0.01);
            assert!((line.baseline - first.baseline - 10.).abs() < 0.01);
        }
        assert!((spaced.metrics().height - height * 2.).abs() < 0.01);
        assert_eq!(
            spaced.metrics().max_content_width,
            natural.metrics().max_content_width
        );
        let a = spaced.caret_rect(0, 1.).unwrap();
        let b = spaced.caret_rect(6, 1.).unwrap();
        assert!((b[1] - a[1] - height).abs() < 0.01);
        let selection = spaced.selection_rects(6..11).unwrap();
        assert!(selection.iter().all(|r| r[1] >= height - 0.01));
        for invalid in [f32::NAN, f32::INFINITY, 0., -1., 1e8] {
            request.line_height = Some(invalid);
            assert!(matches!(
                context.layout(&request),
                Err(Error::InvalidRequest)
            ));
        }
    }
    #[test]
    fn alignment_moves_native_editing_geometry_without_changing_intrinsics() {
        let context = Context::new().unwrap();
        let mut request = Request::new("Hello");
        request.width = Some(240.);
        let start = context.layout(&request).unwrap();
        let first = start.caret_rect(0, 1.).unwrap();
        let selected = start.selection_rects(0..5).unwrap();
        let metrics = start.metrics();
        for (alignment, fraction) in [(Alignment::Center, 0.5), (Alignment::End, 1.)] {
            request.alignment = alignment;
            let aligned = context.layout(&request).unwrap();
            let caret = aligned.caret_rect(0, 1.).unwrap();
            let shift = (240. - metrics.width) * fraction;
            assert!((caret[0] - first[0] - shift).abs() < 0.01);
            assert_eq!(
                aligned.metrics().min_content_width,
                metrics.min_content_width
            );
            assert_eq!(
                aligned.metrics().max_content_width,
                metrics.max_content_width
            );
            assert_eq!(
                aligned
                    .hit_test(caret[0], caret[1] + caret[3] * 0.5)
                    .unwrap(),
                0
            );
            let selection = aligned.selection_rects(0..5).unwrap();
            assert_eq!(selection.len(), selected.len());
            for (rect, original) in selection.iter().zip(&selected) {
                assert!((rect[0] - original[0] - shift).abs() < 0.01);
                assert_eq!(rect[2], original[2]);
            }
        }
        request.alignment = Alignment::Justify;
        request.text = "A paragraph with enough words to fill several lines for justification.";
        request.width = Some(100.);
        let justified = context.layout(&request).unwrap();
        assert!(justified.lines().len() > 1);
        assert!(
            justified
                .selection_rects(0..request.text.len())
                .unwrap()
                .len()
                > 1
        );
    }
    #[test]
    fn family_resolution_checks_candidates_in_order() {
        let context = Context::new().unwrap();
        let missing = "Plico deliberately absent family 7D39B617";
        assert_eq!(context.resolve_family(&[missing]).unwrap(), None);
        let generic = context.resolve_family(&["system-ui"]).unwrap();
        assert_eq!(
            context.resolve_family(&[missing, "system-ui"]).unwrap(),
            generic
        );
        if let Some(family) = generic {
            assert_eq!(family, "Segoe UI");
            assert_eq!(
                context.resolve_family(&[&family, missing]).unwrap(),
                Some(family)
            );
        }
        assert!(context.resolve_family(&["bad\0family"]).is_err());
        assert!(context.resolve_family(&[""]).is_err());
        assert!(context.resolve_family(&["serif"; 129]).is_err());
        assert_eq!(context.resolve_family(&[]).unwrap(), None);
    }
    #[test]
    fn native_layout_keeps_geometry_and_utf8_offsets_after_factory_drop() {
        for content in [
            "",
            "Hello ffi e\u{301} 中文 🙂",
            "مرحبا بالعالم",
            "👩‍👩‍👧‍👧\r\n日本語",
        ] {
            let paragraph = {
                let context = Context::new().unwrap();
                let mut request = Request::new(content);
                request.rtl = content.starts_with('م');
                context.layout(&request).unwrap()
            };
            let metrics = paragraph.metrics();
            assert!(metrics.width.is_finite() && metrics.height.is_finite());
            assert!(!paragraph.lines().is_empty());
            for byte in (0..=content.len()).filter(|b| content.is_char_boundary(*b)) {
                let units = content[..byte].encode_utf16().count() as u32;
                assert_eq!(paragraph.utf16_from_byte(byte), Some(units));
                assert_eq!(paragraph.byte_from_utf16(units), Some(byte));
                for scale in [1., 1.25, 1.5, 2.] {
                    let caret = paragraph.caret_rect(byte, scale).unwrap();
                    assert!(caret.iter().all(|v| v.is_finite()));
                    assert_eq!(caret[2], 1. / scale);
                    let hit = paragraph
                        .hit_test(caret[0], caret[1] + caret[3] * 0.5)
                        .unwrap();
                    assert!(content.is_char_boundary(hit));
                }
            }
            let rects = paragraph.selection_rects(0..content.len()).unwrap();
            assert!(rects.iter().flatten().all(|v| v.is_finite()));
            assert!(paragraph.hit_test(f32::NAN, 0.).is_err());
            assert!(paragraph.caret_rect(content.len() + 1, 1.).is_err());
            assert!(paragraph.selection_rects(0..content.len() + 1).is_err());
        }
    }
    #[test]
    fn wrapping_and_invalid_requests_are_explicit() {
        let context = Context::new().unwrap();
        let mut request =
            Request::new("A paragraph with enough content to wrap into several lines.");
        request.width = Some(80.);
        let wrapped = context.layout(&request).unwrap();
        assert!(wrapped.lines().len() > 1);
        assert!(wrapped.metrics().height > request.size);
        let intrinsic = {
            request.width = None;
            context.layout(&request).unwrap().metrics()
        };
        assert!(intrinsic.min_content_width <= intrinsic.max_content_width);
        assert!(intrinsic.max_content_width > 80.);
        for width in [40., 80., 160.] {
            request.width = Some(width);
            let metrics = context.layout(&request).unwrap().metrics();
            assert_eq!(metrics.min_content_width, intrinsic.min_content_width);
            assert_eq!(metrics.max_content_width, intrinsic.max_content_width);
        }
        request.wrap = false;
        let no_wrap = context.layout(&request).unwrap();
        assert_eq!(no_wrap.lines().len(), 1);
        assert_eq!(
            no_wrap.metrics().min_content_width,
            intrinsic.max_content_width
        );
        assert_eq!(
            no_wrap.metrics().max_content_width,
            intrinsic.max_content_width
        );
        request.size = f32::NAN;
        assert!(matches!(
            context.layout(&request),
            Err(Error::InvalidRequest)
        ));
        request.size = 14.;
        request.family = "bad\0family";
        assert!(matches!(
            context.layout(&request),
            Err(Error::InvalidRequest)
        ));
        let mut request = Request::new("🙂");
        request.width = Some(0.);
        assert!(matches!(
            context.layout(&request),
            Err(Error::InvalidRequest)
        ));
    }
}
