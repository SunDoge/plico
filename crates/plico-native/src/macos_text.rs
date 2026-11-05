//! Experimental CoreText foundation. One retained native line supplies both
//! metrics and editing offsets; no second shaping pass or raster approximation.
//! Native multiline geometry is available; rich text and renderer integration
//! are pending.
mod paragraph;
mod raster;
mod wrapping;
use crate::text_indices::TextIndices;
use objc2_core_foundation::{
    CFBoolean, CFDictionary, CFMutableAttributedString, CFNumber, CFRange, CFRetained, CFString,
    CFType, CGPoint,
};
use objc2_core_text::{
    CTFont, CTFontDescriptor, CTFontSymbolicTraits, CTFontUIFontType, CTLine, kCTFontAttributeName,
    kCTFontTraitsAttribute, kCTFontWeightTrait, kCTForegroundColorFromContextAttributeName,
    kCTTrackingAttributeName,
};
pub use paragraph::{Paragraph, ParagraphLine};
pub use raster::Rasterized;
use snafu::Snafu;
use std::{marker::PhantomData, rc::Rc};
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
/// Native face selection; no synthetic emboldening or skew is applied.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct FontStyle {
    /// Native symbolic or normalized face weight.
    pub weight: FontWeight,
    /// Request the family’s italic face.
    pub italic: bool,
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
    fn attributed(
        request: &Request<'_>,
    ) -> Result<(CFRetained<CTFont>, CFRetained<CFMutableAttributedString>)> {
        if !request.valid() {
            return Err(Error::InvalidRequest);
        }
        let name = CFString::from_str(request.family);
        // SAFETY: CFString values and the null optional transform remain valid
        // throughout each synchronous call. Returned create-rule objects are owned.
        let font = unsafe {
            if matches!(request.family, "" | "system-ui") {
                CTFont::new_ui_font_for_language(CTFontUIFontType::System, request.size, None)
                    .ok_or(Error::Allocation)?
            } else {
                CTFont::with_name(&name, request.size, std::ptr::null())
            }
        };
        let font = if let Some(style) = request.style {
            let mut traits = CTFontSymbolicTraits::empty();
            if style.weight == FontWeight::Bold {
                traits |= CTFontSymbolicTraits::TraitBold;
            }
            if style.italic {
                traits |= CTFontSymbolicTraits::TraitItalic;
            }
            // SAFETY: The retained font is valid and the optional matrix is null.
            // Mask both bits so regular/upright can remove a named face's traits.
            let font = unsafe {
                font.copy_with_symbolic_traits(
                    0.,
                    std::ptr::null(),
                    traits,
                    CTFontSymbolicTraits::TraitBold | CTFontSymbolicTraits::TraitItalic,
                )
            }
            .ok_or(Error::UnavailableStyle)?;
            if let FontWeight::Normalized(weight) = style.weight {
                let number = CFNumber::new_f64(weight);
                // SAFETY: Framework constants are valid retained CFString keys;
                // dictionaries retain their values for the synchronous font copy.
                unsafe {
                    let traits = CFDictionary::<CFString, CFType>::from_slices(
                        &[kCTFontWeightTrait],
                        &[&number],
                    );
                    let attributes = CFDictionary::<CFString, CFType>::from_slices(
                        &[kCTFontTraitsAttribute],
                        &[&traits],
                    );
                    let descriptor = CTFontDescriptor::with_attributes(attributes.as_ref());
                    let selected =
                        font.copy_with_attributes(0., std::ptr::null(), Some(&descriptor));
                    if selected
                        .symbolic_traits()
                        .contains(CTFontSymbolicTraits::TraitItalic)
                        != style.italic
                    {
                        return Err(Error::UnavailableStyle);
                    }
                    selected
                }
            } else {
                font
            }
        } else {
            font
        };
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
