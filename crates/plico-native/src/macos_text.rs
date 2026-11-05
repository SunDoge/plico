//! Experimental CoreText foundation. One retained native line supplies both
//! metrics and editing offsets; no second shaping pass or raster approximation.
//! Multiline layout, rasterization, rich text and renderer integration are pending.
use crate::text_indices::TextIndices;
use objc2_core_foundation::{CFMutableAttributedString, CFRange, CFRetained, CFString, CGPoint};
use objc2_core_text::{CTFont, CTFontUIFontType, CTLine, kCTFontAttributeName};
use snafu::Snafu;
use std::{marker::PhantomData, rc::Rc};

/// Invalid requests and native service failures are observable to the caller.
#[derive(Debug, Snafu)]
pub enum Error {
    /// Invalid size, unsupported multiline input or exceeded resource limits.
    #[snafu(display("invalid or oversized CoreText single-line request"))]
    InvalidRequest,
    /// The system font or attributed string could not be allocated.
    #[snafu(display("CoreText font or attributed-string allocation failed"))]
    Allocation,
    /// Native metrics were nonfinite, excessive or otherwise inconsistent.
    #[snafu(display("invalid CoreText line metrics"))]
    Metrics,
}
/// Results returned by the CoreText foundation.
pub type Result<T> = std::result::Result<T, Error>;
/// A single regular-font line in logical macOS points.
#[derive(Clone, Copy, Debug)]
pub struct Request<'a> {
    /// Original UTF-8 text, at most 1 MiB and 100,000 scalar values.
    pub text: &'a str,
    /// System/PostScript family name; `system-ui` or empty selects the UI font.
    /// CoreText may fall back for unknown names; inspect [`Line::font_family`].
    pub family: &'a str,
    /// Logical font size, finite and in `(0, 4096]`.
    pub size: f64,
}
impl<'a> Request<'a> {
    /// Use the regular system UI font at 14 logical points.
    pub fn new(text: &'a str) -> Self {
        Self {
            text,
            family: "system-ui",
            size: 14.,
        }
    }
    fn valid(&self) -> bool {
        self.text.len() <= 1024 * 1024
            && self.text.chars().take(100_001).count() <= 100_000
            && !self
                .text
                .chars()
                .any(|c| matches!(c, '\n' | '\r' | '\u{85}' | '\u{2028}' | '\u{2029}'))
            && self.family.len() <= 4096
            && !self.family.contains('\0')
            && self.size.is_finite()
            && self.size > 0.
            && self.size <= 4096.
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
    metrics: Metrics,
    _thread: PhantomData<Rc<()>>,
}
impl Line {
    /// Shape once through CoreText, retaining its fallback/ligature/bidi result.
    pub fn new(request: &Request<'_>) -> Result<Self> {
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
        let string = CFString::from_str(request.text);
        let attributed = CFMutableAttributedString::new(None, 0).ok_or(Error::Allocation)?;
        let units = request.text.encode_utf16().count();
        // SAFETY: This newly allocated string has no aliases being mutated. The
        // replacement is at its empty range; the font range spans the inserted
        // UTF-16 string exactly. CoreFoundation retains the attribute value.
        let native = unsafe {
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
            CTLine::with_attributed_string(&attributed)
        };
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
            text: request.text.into(),
            indices: TextIndices::new(request.text),
            metrics,
            _thread: PhantomData,
        })
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
                .offset_for_string_index(units as isize, &mut secondary)
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
        self.indices.byte(u32::try_from(unit).ok()?)
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
