//! Experimental CoreText foundation. One retained native line supplies both
//! metrics and editing offsets; no second shaping pass or raster approximation.
//! Native multiline geometry and UTF-8 rich spans share retained attributed
//! lines. The experimental renderer bridge lives in plico-wgpu.
mod editing;
mod fonts;
mod intrinsic;
mod paragraph;
mod raster;
#[cfg(feature = "vertical-text")]
pub(crate) mod vertical;
mod wrapping;
use crate::text_indices::TextIndices;
pub use fonts::FontCollection;
use objc2_core_foundation::{
    CFBoolean, CFDictionary, CFMutableAttributedString, CFNumber, CFRange, CFRetained, CFString,
    CFType, CGPoint,
};
use objc2_core_graphics::CGColor;
use objc2_core_text::{
    CTFont, CTFontDescriptor, CTFontSymbolicTraits, CTFontUIFontType, CTLine, CTTypesetter,
    kCTFontAttributeName, kCTFontTraitsAttribute, kCTFontWeightTrait,
    kCTForegroundColorAttributeName, kCTForegroundColorFromContextAttributeName,
    kCTTrackingAttributeName,
};
pub use paragraph::{Alignment, Paragraph, ParagraphLine};
pub use raster::Rasterized;
use snafu::Snafu;
use std::{marker::PhantomData, ops::Range, rc::Rc};
pub use wrapping::WrappedLine;

/// Invalid requests and native service failures are observable to the caller.
#[derive(Debug, Snafu)]
pub enum Error {
    /// Invalid size, unsupported multiline input or exceeded resource limits.
    #[snafu(display("invalid or oversized CoreText text request"))]
    InvalidRequest,
    /// A native font, attributed string or bitmap resource could not be allocated.
    #[snafu(display("CoreText/CoreGraphics resource allocation failed"))]
    Allocation,
    /// The requested family has no face matching the requested symbolic style.
    #[snafu(display("CoreText font family has no matching bold/italic face"))]
    UnavailableStyle,
    /// Native line breaking failed to make progress or exceeded its line budget.
    #[snafu(display("CoreText wrapping failed or exceeded the 4096-line budget"))]
    Wrapping,
    /// Native metrics were nonfinite, excessive or otherwise inconsistent.
    #[snafu(display("invalid CoreText line metrics"))]
    Metrics,
}
/// Results returned by the CoreText foundation.
pub type Result<T> = std::result::Result<T, Error>;
/// Native face weight selection; CoreText chooses the closest available face.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum FontWeight {
    /// Select the family's regular face using symbolic traits.
    #[default]
    Regular,
    /// Select the family's bold face using symbolic traits.
    Bold,
    /// CoreText normalized weight in `[-1, 1]`, with zero regular/medium.
    /// This is not a CSS weight or an OpenType `wght` axis coordinate.
    Normalized(f64),
}
impl FontWeight {
    /// Map CSS weights through the platform's named weight constants. Values
    /// between hundreds interpolate native anchors; 1..100 and 900..1000 clamp
    /// to the lightest/heaviest named weight. This requests a closest face,
    /// not guaranteed variable-font axis interpolation.
    pub fn from_css(weight: f64) -> Result<Self> {
        use objc2_app_kit::{
            NSFontWeightBlack, NSFontWeightBold, NSFontWeightHeavy, NSFontWeightLight,
            NSFontWeightMedium, NSFontWeightRegular, NSFontWeightSemibold, NSFontWeightThin,
            NSFontWeightUltraLight,
        };
        if !weight.is_finite() || !(1. ..=1000.).contains(&weight) {
            return Err(Error::InvalidRequest);
        }
        // SAFETY: Framework-owned immutable constants; use platform values
        // rather than embedding guessed CoreText/CSS conversion coefficients.
        let anchors = unsafe {
            [
                NSFontWeightUltraLight,
                NSFontWeightThin,
                NSFontWeightLight,
                NSFontWeightRegular,
                NSFontWeightMedium,
                NSFontWeightSemibold,
                NSFontWeightBold,
                NSFontWeightHeavy,
                NSFontWeightBlack,
            ]
        };
        let position = (weight.clamp(100., 900.) - 100.) / 100.;
        let lower = position.floor() as usize;
        let upper = (lower + 1).min(8);
        let fraction = position - lower as f64;
        Ok(Self::Normalized(
            anchors[lower] + (anchors[upper] - anchors[lower]) * fraction,
        ))
    }
}
/// Native face selection; no synthetic emboldening or skew is applied.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct FontStyle {
    /// Native symbolic or normalized face weight.
    pub weight: FontWeight,
    /// Request the family’s italic face.
    pub italic: bool,
}
/// A complete style override over a UTF-8 byte range. Later overlapping spans
/// win because CoreText attributes are applied in declaration order.
#[derive(Clone, Debug, PartialEq)]
pub struct StyleSpan {
    /// UTF-8 byte range in the associated request text.
    pub range: Range<usize>,
    /// Logical font size in macOS points.
    pub size: f64,
    /// Native face selection within the request family.
    pub style: FontStyle,
    /// Straight sRGB foreground color.
    pub color: [u8; 4],
}
fn valid_style_spans(text: &str, spans: &[StyleSpan]) -> bool {
    spans.len() <= 4096
        && spans.iter().all(|span| {
            span.range.start <= span.range.end
                && span.range.end <= text.len()
                && text.is_char_boundary(span.range.start)
                && text.is_char_boundary(span.range.end)
                && span.size.is_finite()
                && span.size > 0.
                && span.size <= 4096.
                && match span.style.weight {
                    FontWeight::Regular | FontWeight::Bold => true,
                    FontWeight::Normalized(weight) => {
                        weight.is_finite() && (-1. ..=1.).contains(&weight)
                    }
                }
        })
}
/// Actual base-face traits, which may differ from a normalized weight request.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ResolvedFontStyle {
    /// Whether CoreText identifies the selected face as bold.
    pub bold: bool,
    /// Whether CoreText identifies the selected face as italic.
    pub italic: bool,
    /// Native normalized weight, if the font reports a finite numeric trait.
    pub normalized_weight: Option<f64>,
}
/// A single-font request in logical macOS points. `Line` rejects hard breaks;
/// `Paragraph` accepts them.
#[derive(Clone, Copy, Debug)]
pub struct Request<'a> {
    /// Original UTF-8 text, at most 1 MiB and 100,000 scalar values.
    pub text: &'a str,
    /// System/PostScript family name; `system-ui` or empty selects the UI font.
    /// CoreText may fall back for unknown names; inspect [`Line::font_family`].
    pub family: &'a str,
    /// Private application faces; no process-wide font registration is performed.
    pub fonts: Option<&'a FontCollection>,
    /// Logical font size, finite and in `(0, 4096]`.
    pub size: f64,
    /// Native tracking in logical points, finite and within `[-4096, 4096]`.
    /// Nonzero tracking lets CoreText suppress nonessential ligatures.
    pub letter_spacing: f64,
    /// Select a face within the resolved family. `None` preserves a named face's
    /// style; `Some(FontStyle::default())` explicitly requests regular/upright.
    pub style: Option<FontStyle>,
}
impl<'a> Request<'a> {
    /// Use the regular system UI font at 14 logical points.
    pub fn new(text: &'a str) -> Self {
        Self {
            text,
            family: "system-ui",
            fonts: None,
            size: 14.,
            letter_spacing: 0.,
            style: None,
        }
    }
    fn valid(&self) -> bool {
        self.valid_paragraph()
            && !self
                .text
                .chars()
                .any(|c| matches!(c, '\n' | '\r' | '\u{85}' | '\u{2028}' | '\u{2029}'))
    }
    fn valid_paragraph(&self) -> bool {
        self.text.len() <= 1024 * 1024
            && self.text.chars().take(100_001).count() <= 100_000
            && self.family.len() <= 4096
            && !self.family.contains('\0')
            && self.size.is_finite()
            && self.size > 0.
            && self.size <= 4096.
            && self.letter_spacing.is_finite()
            && self.letter_spacing.abs() <= 4096.
            && self.style.is_none_or(|style| match style.weight {
                FontWeight::Normalized(weight) => {
                    weight.is_finite() && (-1. ..=1.).contains(&weight)
                }
                _ => true,
            })
    }
}
/// Native typographic metrics; none are physical-pixel raster bounds.
#[derive(Clone, Copy, Debug)]
pub struct Metrics {
    /// Native typographic advance in logical points.
    pub width: f64,
    /// Distance above the baseline, including fallback font runs.
    pub ascent: f64,
    /// Distance below the baseline, including fallback font runs.
    pub descent: f64,
    /// Nonnegative native line leading.
    pub leading: f64,
    /// Number of native shaped glyphs, including fallback runs.
    pub glyphs: usize,
}
/// Primary and secondary native offsets can differ at bidi boundaries.
#[derive(Clone, Copy, Debug)]
pub struct CaretOffsets {
    /// Primary horizontal offset relative to the line origin.
    pub primary: f64,
    /// Secondary horizontal offset relative to the line origin.
    pub secondary: f64,
}
/// Retained CoreText line, deliberately confined to its creating UI thread.
/// UTF-8 scalar boundaries are accepted; this does not enumerate grapheme stops.
pub struct Line {
    native: CFRetained<CTLine>,
    font: CFRetained<CTFont>,
    text: String,
    indices: TextIndices,
    utf16_start: u32,
    metrics: Metrics,
    _thread: PhantomData<Rc<()>>,
}
impl Line {
    /// Shape once through CoreText, retaining its fallback/ligature/bidi result.
    pub fn new(request: &Request<'_>) -> Result<Self> {
        let (font, attributed) = Self::attributed(request)?;
        // SAFETY: The attributed string is valid and no longer being mutated.
        let native = unsafe { CTLine::with_attributed_string(&attributed) };
        Self::from_native(native, font, request.text, 0)
    }
    /// Shape a line once with native font and foreground overrides.
    pub fn new_styled(request: &Request<'_>, spans: &[StyleSpan]) -> Result<Self> {
        let (font, attributed) = Self::attributed_styled(request, spans)?;
        // SAFETY: The attributed string is valid and no longer being mutated.
        let native = unsafe { CTLine::with_attributed_string(&attributed) };
        Self::from_native(native, font, request.text, 0)
    }
    /// Return the source prefix plus a native ellipsis when the complete line
    /// exceeds `width`. The byte endpoint always lies on a CoreText cluster
    /// boundary in the original UTF-8 string. `None` means no truncation is
    /// necessary.
    pub fn ellipsized_text(request: &Request<'_>, width: f64) -> Result<Option<(String, usize)>> {
        Self::ellipsized_text_styled(request, &[], width)
    }
    /// Styled equivalent of [`Self::ellipsized_text`].
    pub fn ellipsized_text_styled(
        request: &Request<'_>,
        spans: &[StyleSpan],
        width: f64,
    ) -> Result<Option<(String, usize)>> {
        snafu::ensure!(
            request.valid() && width.is_finite() && width > 0. && width <= 1e9,
            InvalidRequestSnafu
        );
        if Self::new_styled(request, spans)?.metrics().width <= width {
            return Ok(None);
        }
        let token_request = Request {
            text: "…",
            ..*request
        };
        let token_width = Self::new(&token_request)?.metrics().width;
        if token_width > width {
            return Ok(Some((String::new(), 0)));
        }
        let (_, attributed) = Self::attributed_styled(request, spans)?;
        // SAFETY: The valid attributed string is immutable after construction.
        let typesetter = unsafe { CTTypesetter::with_attributed_string(&attributed) };
        let available = (width - token_width).max(0.);
        // SAFETY: The request starts at UTF-16 offset zero and `available` is a
        // finite nonnegative width. CoreText returns a cluster boundary count.
        let units = unsafe { typesetter.suggest_cluster_break(0, available) };
        let units = u32::try_from(units).map_err(|_| Error::Wrapping)?;
        let indices = TextIndices::new(request.text);
        let end = indices.byte(units).ok_or(Error::Wrapping)?;
        let mut text = String::with_capacity(end + '…'.len_utf8());
        text.push_str(&request.text[..end]);
        text.push('…');
        Ok(Some((text, end)))
    }
    fn attributed(
        request: &Request<'_>,
    ) -> Result<(CFRetained<CTFont>, CFRetained<CFMutableAttributedString>)> {
        Self::attributed_styled(request, &[])
    }
    fn attributed_styled(
        request: &Request<'_>,
        spans: &[StyleSpan],
    ) -> Result<(CFRetained<CTFont>, CFRetained<CFMutableAttributedString>)> {
        Self::attributed_text(request, spans, false)
    }
    fn attributed_text(
        request: &Request<'_>,
        spans: &[StyleSpan],
        paragraph: bool,
    ) -> Result<(CFRetained<CTFont>, CFRetained<CFMutableAttributedString>)> {
        if !(if paragraph {
            request.valid_paragraph()
        } else {
            request.valid()
        }) {
            return Err(Error::InvalidRequest);
        }
        if !valid_style_spans(request.text, spans) {
            return Err(Error::InvalidRequest);
        }
        let font = fonts::resolve(request)?;
        let string = CFString::from_str(request.text);
        let attributed = CFMutableAttributedString::new(None, 0).ok_or(Error::Allocation)?;
        let units = request.text.encode_utf16().count();
        // SAFETY: This newly allocated string has no aliases being mutated. The
        // replacement is at its empty range; the font range spans the inserted
        // UTF-16 string exactly. CoreFoundation retains the attribute value.
        unsafe {
            CFMutableAttributedString::replace_string(
                Some(&attributed),
                CFRange::new(0, 0),
                Some(&string),
            );
            CFMutableAttributedString::set_attribute(
                Some(&attributed),
                CFRange::new(0, units as isize),
                Some(kCTFontAttributeName),
                Some(&font),
            );
            CFMutableAttributedString::set_attribute(
                Some(&attributed),
                CFRange::new(0, units as isize),
                Some(kCTForegroundColorFromContextAttributeName),
                Some(CFBoolean::new(true)),
            );
            // Leave zero tracking absent so the font's ordinary kerning and
            // ligature defaults are untouched. CoreText applies nonzero tracking
            // to clusters before both geometry queries and retained-line drawing.
            if request.letter_spacing != 0. {
                let spacing = CFNumber::new_f64(request.letter_spacing);
                CFMutableAttributedString::set_attribute(
                    Some(&attributed),
                    CFRange::new(0, units as isize),
                    Some(kCTTrackingAttributeName),
                    Some(&spacing),
                );
            }
        }
        let indices = TextIndices::new(request.text);
        for span in spans {
            let start = indices
                .utf16(span.range.start)
                .ok_or(Error::InvalidRequest)?;
            let end = indices.utf16(span.range.end).ok_or(Error::InvalidRequest)?;
            if start == end {
                continue;
            }
            let span_request = Request {
                size: span.size,
                style: Some(span.style),
                ..*request
            };
            let span_font = fonts::resolve(&span_request)?;
            let color = CGColor::new_srgb(
                f64::from(span.color[0]) / 255.,
                f64::from(span.color[1]) / 255.,
                f64::from(span.color[2]) / 255.,
                f64::from(span.color[3]) / 255.,
            );
            let range = CFRange::new(start as isize, (end - start) as isize);
            // SAFETY: UTF-8 endpoints were mapped to valid UTF-16 boundaries.
            // CoreFoundation retains each value after this scope. Sequential
            // writes intentionally give later overlapping spans precedence.
            unsafe {
                CFMutableAttributedString::set_attribute(
                    Some(&attributed),
                    range,
                    Some(kCTFontAttributeName),
                    Some(&span_font),
                );
                CFMutableAttributedString::set_attribute(
                    Some(&attributed),
                    range,
                    Some(kCTForegroundColorFromContextAttributeName),
                    Some(CFBoolean::new(false)),
                );
                CFMutableAttributedString::set_attribute(
                    Some(&attributed),
                    range,
                    Some(kCTForegroundColorAttributeName),
                    Some(&color),
                );
            }
        }
        Ok((font, attributed))
    }
    fn from_native(
        native: CFRetained<CTLine>,
        font: CFRetained<CTFont>,
        text: &str,
        utf16_start: u32,
    ) -> Result<Self> {
        let (mut ascent, mut descent, mut leading) = (0., 0., 0.);
        // SAFETY: Valid retained line/font and writable local metric outputs.
        let metrics = unsafe {
            Metrics {
                width: native.typographic_bounds(&mut ascent, &mut descent, &mut leading),
                ascent: ascent.max(font.ascent()),
                descent: descent.max(font.descent()),
                leading: leading.max(font.leading()).max(0.),
                glyphs: usize::try_from(native.glyph_count()).map_err(|_| Error::Metrics)?,
            }
        };
        if [
            metrics.width,
            metrics.ascent,
            metrics.descent,
            metrics.leading,
        ]
        .iter()
        .any(|value| !value.is_finite() || *value < 0. || *value > 1e9)
            || metrics.glyphs > 100_000
        {
            return Err(Error::Metrics);
        }
        Ok(Self {
            native,
            font,
            text: text.into(),
            indices: TextIndices::new(text),
            utf16_start,
            metrics,
            _thread: PhantomData,
        })
    }
    /// Native ink bounds relative to the baseline, in logical top-down coordinates.
    /// Whitespace/empty lines return None. Glyph overhang is included.
    pub fn ink_bounds(&self) -> Result<Option<[f64; 4]>> {
        // SAFETY: Retained immutable line, no external CGContext state.
        let ink = unsafe { self.native.image_bounds(None) };
        if ink.size.width == 0. || ink.size.height == 0. {
            return Ok(None);
        }
        let bounds = [
            ink.origin.x,
            -ink.origin.y - ink.size.height,
            ink.origin.x + ink.size.width,
            -ink.origin.y,
        ];
        if bounds.iter().any(|n| !n.is_finite() || n.abs() > 1e9)
            || bounds[2] < bounds[0]
            || bounds[3] < bounds[1]
        {
            return Err(Error::Metrics);
        }
        Ok(Some(bounds))
    }
    /// Resolved base face style; fallback runs can use other faces.
    pub fn font_style(&self) -> ResolvedFontStyle {
        // SAFETY: This line retains a valid immutable native font.
        let traits = unsafe { self.font.symbolic_traits() };
        // SAFETY: Valid retained native font and framework key. Downcasting is
        // checked: malformed/missing native traits are observable as None.
        let normalized_weight = unsafe {
            // CTFontCopyTraits returns an immutable CFType dictionary whose
            // documented trait keys are CFStrings; values are checked below.
            let dictionary =
                CFRetained::cast_unchecked::<CFDictionary<CFString, CFType>>(self.font.traits());
            dictionary.get(kCTFontWeightTrait)
        }
        .and_then(|value| value.downcast::<CFNumber>().ok())
        .and_then(|value| value.as_f64())
        .filter(|value| value.is_finite());
        ResolvedFontStyle {
            normalized_weight,
            bold: traits.contains(CTFontSymbolicTraits::TraitBold),
            italic: traits.contains(CTFontSymbolicTraits::TraitItalic),
        }
    }
    /// Resolved base font's PostScript name, useful when diagnosing substitution.
    pub fn font_postscript_name(&self) -> String {
        // SAFETY: This line retains a valid immutable native font.
        unsafe { self.font.post_script_name() }.to_string()
    }
    /// Original UTF-8 contents.
    pub fn text(&self) -> &str {
        &self.text
    }
    /// Native typographic metrics, retained with this exact shaped line.
    pub fn metrics(&self) -> Metrics {
        self.metrics
    }
    /// Actual base font family; fallback runs can use additional native fonts.
    pub fn font_family(&self) -> String {
        // SAFETY: The font is retained by this line wrapper.
        unsafe { self.font.family_name().to_string() }
    }
    /// Native offsets at a valid UTF-8 scalar boundary. Bidi affinity is explicit.
    pub fn caret_offsets(&self, byte: usize) -> Result<CaretOffsets> {
        let units = self.indices.utf16(byte).ok_or(Error::InvalidRequest)?;
        let mut secondary = 0.;
        // SAFETY: A validated scalar boundary in the retained original text and
        // a writable secondary output; the native line owns its shaping data.
        let primary = unsafe {
            self.native
                .offset_for_string_index((units + self.utf16_start) as isize, &mut secondary)
        };
        if !primary.is_finite() || !secondary.is_finite() {
            return Err(Error::Metrics);
        }
        Ok(CaretOffsets { primary, secondary })
    }
    /// Hit-test a logical x coordinate, returning an original UTF-8 boundary.
    /// Native failure/surrogate interiors return None rather than an invalid byte.
    pub fn hit_test_x(&self, x: f64) -> Option<usize> {
        if !x.is_finite() || x.abs() > 1e9 {
            return None;
        }
        // SAFETY: Valid retained line and a finite point relative to its baseline.
        let unit = unsafe { self.native.string_index_for_position(CGPoint::new(x, 0.)) };
        self.indices
            .byte(u32::try_from(unit).ok()?.checked_sub(self.utf16_start)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_single_lines_keep_utf8_geometry_and_fallback_metrics() {
        for text in ["", "ffi e\u{301}", "中文 日本語 🙂", "العربية שלום"] {
            let line = Line::new(&Request::new(text)).unwrap();
            assert_eq!(line.text(), text);
            assert!(!line.font_family().is_empty());
            assert!(line.metrics().ascent > 0.);
            for byte in 0..=text.len() {
                assert_eq!(
                    line.caret_offsets(byte).is_ok(),
                    text.is_char_boundary(byte)
                );
            }
            for x in [0., line.metrics().width / 2., line.metrics().width] {
                if let Some(byte) = line.hit_test_x(x) {
                    assert!(text.is_char_boundary(byte));
                }
            }
            assert!(line.hit_test_x(f64::NAN).is_none());
        }
    }
    #[test]
    fn native_ellipsis_uses_a_source_cluster_boundary_and_fits() {
        let request = Request::new("Office 中文🙂 e\u{301} and more text");
        let full = Line::new(&request).unwrap().metrics().width;
        let width = full * 0.55;
        let (text, end) = Line::ellipsized_text(&request, width).unwrap().unwrap();
        assert!(request.text.is_char_boundary(end));
        assert!(end < request.text.len());
        assert_eq!(&text[..end], &request.text[..end]);
        assert!(text.ends_with('…'));
        assert!(
            Line::new(&Request {
                text: &text,
                ..request
            })
            .unwrap()
            .metrics()
            .width
                <= width + 0.01
        );
        assert!(
            Line::ellipsized_text(&request, full + 1.)
                .unwrap()
                .is_none()
        );
        assert_eq!(
            Line::ellipsized_text(&request, 0.5).unwrap(),
            Some((String::new(), 0))
        );
    }
    #[test]
    fn symbolic_styles_select_native_faces_and_share_raster_geometry() {
        for bold in [false, true] {
            for italic in [false, true] {
                let style = FontStyle {
                    weight: if bold {
                        FontWeight::Bold
                    } else {
                        FontWeight::Regular
                    },
                    italic,
                };
                let line = Line::new(&Request {
                    family: "Helvetica",
                    size: 24.,
                    style: Some(style),
                    ..Request::new("Office S 中文")
                })
                .unwrap();
                assert_eq!(line.font_style().bold, bold);
                assert_eq!(line.font_style().italic, italic);
                assert!(!line.font_postscript_name().is_empty());
                let before = line.metrics();
                let a = line
                    .rasterize(1.5, [0.25, 0.], [30, 50, 70, 255])
                    .unwrap()
                    .unwrap();
                let b = line
                    .rasterize(1.5, [0.25, 0.], [30, 50, 70, 255])
                    .unwrap()
                    .unwrap();
                assert_eq!(a, b);
                assert_eq!(line.metrics().width, before.width);
                // Explicit regular style also removes a named bold face's trait.
                if bold && !italic {
                    let regular = Line::new(&Request {
                        family: "Helvetica-Bold",
                        style: Some(FontStyle::default()),
                        ..Request::new("text")
                    })
                    .unwrap();
                    assert!(!regular.font_style().bold);
                    assert!(!regular.font_style().italic);
                }
            }
        }
    }
    #[test]
    fn normalized_weights_are_native_bounded_and_observable() {
        for weight in [-1., -0.4, 0., 0.23, 0.3, 0.4, 1.] {
            let line = Line::new(&Request {
                style: Some(FontStyle {
                    weight: FontWeight::Normalized(weight),
                    italic: false,
                }),
                ..Request::new("Weight S 中文")
            })
            .unwrap();
            let resolved = line.font_style();
            assert!(!resolved.italic);
            assert!(resolved.normalized_weight.is_some_and(|w| w.is_finite()));
            assert!(!line.font_postscript_name().is_empty());
            assert!(
                line.rasterize(1.25, [0.5, 0.], [0, 0, 0, 255])
                    .unwrap()
                    .is_some()
            );
        }
        for weight in [0., 0.4] {
            let line = Line::new(&Request {
                family: "Helvetica",
                style: Some(FontStyle {
                    weight: FontWeight::Normalized(weight),
                    italic: true,
                }),
                ..Request::new("Italic")
            })
            .unwrap();
            assert!(line.font_style().italic);
        }
        for weight in [-1.01, 1.01, f64::NAN, f64::INFINITY] {
            assert!(matches!(
                Line::new(&Request {
                    style: Some(FontStyle {
                        weight: FontWeight::Normalized(weight),
                        italic: false
                    }),
                    ..Request::new("text")
                }),
                Err(Error::InvalidRequest)
            ));
        }
    }
    #[test]
    fn tracking_updates_native_advance_carets_and_retained_pixels() {
        let normal = Line::new(&Request::new("NNNN")).unwrap();
        let tracked = Line::new(&Request {
            letter_spacing: 3.,
            ..Request::new("NNNN")
        })
        .unwrap();
        assert!(tracked.metrics().width > normal.metrics().width);
        let before = normal.caret_offsets(2).unwrap().primary;
        let after = tracked.caret_offsets(2).unwrap().primary;
        assert!(after > before);
        // Hit testing uses the same tracked line; do not infer positions from
        // character count (which would fail for ligatures, combining text/bidi).
        for byte in 1..4 {
            let caret = tracked.caret_offsets(byte).unwrap().primary;
            assert_eq!(tracked.hit_test_x(caret), Some(byte));
        }
        let normal_pixels = normal
            .rasterize(1.5, [0.25, 0.], [0, 0, 0, 255])
            .unwrap()
            .unwrap();
        let pixels = tracked
            .rasterize(1.5, [0.25, 0.], [0, 0, 0, 255])
            .unwrap()
            .unwrap();
        assert_ne!(pixels, normal_pixels);
        assert_eq!(
            tracked
                .rasterize(1.5, [0.25, 0.], [0, 0, 0, 255])
                .unwrap()
                .unwrap(),
            pixels
        );
        let tight = Line::new(&Request {
            letter_spacing: -0.5,
            ..Request::new("NNNN")
        })
        .unwrap();
        assert!(tight.metrics().width < normal.metrics().width);
        for letter_spacing in [f64::NAN, f64::INFINITY, -4097., 4097.] {
            assert!(matches!(
                Line::new(&Request {
                    letter_spacing,
                    ..Request::new("text")
                }),
                Err(Error::InvalidRequest)
            ));
        }
    }
    #[test]
    fn css_weights_use_platform_anchors_and_ink_bounds_match_native_rasters() {
        let mut previous = -1.;
        for css in [
            1., 100., 200., 300., 400., 450., 500., 600., 700., 800., 900., 1000.,
        ] {
            let FontWeight::Normalized(value) = FontWeight::from_css(css).unwrap() else {
                panic!("expected normalized platform weight")
            };
            assert!(value >= previous && (-1. ..=1.).contains(&value));
            previous = value;
        }
        for css in [0., 1001., f64::NAN, f64::INFINITY] {
            assert!(FontWeight::from_css(css).is_err());
        }
        let line = Line::new(&Request::new("Italic f 中文🙂")).unwrap();
        let bounds = line.ink_bounds().unwrap().unwrap();
        assert!(bounds[2] > bounds[0] && bounds[3] > bounds[1]);
        let raster = line
            .rasterize(1.5, [0.25, 0.], [0, 0, 0, 255])
            .unwrap()
            .unwrap();
        assert!(raster.origin[0] as f64 <= bounds[0] * 1.5 + 0.25);
        assert!(raster.origin[1] as f64 <= bounds[1] * 1.5);
        assert!(
            Line::new(&Request::new("   "))
                .unwrap()
                .ink_bounds()
                .unwrap()
                .is_none()
        );
    }
    #[test]
    fn rejects_multiline_and_invalid_dimensions() {
        for text in ["a\nb", "a\rb", "a\u{2028}b"] {
            assert!(Line::new(&Request::new(text)).is_err());
        }
        for size in [0., -1., f64::NAN, f64::INFINITY, 4097.] {
            assert!(
                Line::new(&Request {
                    size,
                    ..Request::new("text")
                })
                .is_err()
            );
        }
    }
}
