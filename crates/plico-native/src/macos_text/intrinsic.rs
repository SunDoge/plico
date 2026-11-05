//! Native line-break opportunities and retained-context intrinsic measurement.
use super::*;
use crate::paragraph_geometry::hard_lines;
use objc2_core_foundation::{
    CFStringTokenizer, CFStringTokenizerTokenType, kCFStringTokenizerUnitLineBreak,
};
use objc2_core_text::CTTypesetter;

impl Paragraph {
    /// Word-wrap minimum and unwrapped maximum advances in logical points.
    /// Native line-break tokenization supplies opportunities (including CJK and
    /// nonbreaking spaces); each segment is measured through the same attributed
    /// typesetter, rather than reshaping isolated Rust substrings. Trailing native
    /// whitespace is excluded from the minimum, retained in the maximum.
    /// Locale is inferred by CoreFoundation; no hyphenation or custom locale is
    /// requested. This does not promise CSS browser line-break-policy equivalence.
    pub fn measure_intrinsic(request: &Request<'_>) -> Result<[f64; 2]> {
        if !request.valid_paragraph() {
            return Err(Error::InvalidRequest);
        }
        if hard_lines(request.text).take(4097).count() > 4096 {
            return Err(Error::Wrapping);
        }
        let mut minimum = 0f64;
        let mut maximum = 0f64;
        for (content, _) in hard_lines(request.text) {
            let part = Request {
                text: &request.text[content],
                ..*request
            };
            if part.text.is_empty() {
                continue;
            }
            // Keep the face alive explicitly; attributes retain it as well.
            let (_font, attributed) = Line::attributed(&part)?;
            let indices = TextIndices::new(part.text);
            let units = indices.utf16(part.text.len()).ok_or(Error::Metrics)?;
            let string = CFString::from_str(part.text);
            // SAFETY: The range spans the valid immutable UTF-16 string. Default
            // allocator and automatic locale are explicitly allowed by this API.
            let tokenizer = unsafe {
                CFStringTokenizer::new(
                    None,
                    Some(&string),
                    CFRange::new(0, units as isize),
                    kCFStringTokenizerUnitLineBreak,
                    None,
                )
            }
            .ok_or(Error::Allocation)?;
            // SAFETY: Valid attributed string, not mutated while typesetting.
            let typesetter = unsafe { CTTypesetter::with_attributed_string(&attributed) };
            let measure = |start: u32, end: u32, trim: bool| -> Result<f64> {
                if start == end {
                    return Ok(0.);
                }
                if start > end
                    || end > units
                    || indices.byte(start).is_none()
                    || indices.byte(end).is_none()
                {
                    return Err(Error::Metrics);
                }
                // SAFETY: Validated scalar boundaries in this typesetter's
                // original UTF-16 string. Null metric outputs are optional.
                let (width, trailing) = unsafe {
                    let line =
                        typesetter.line(CFRange::new(start as isize, (end - start) as isize));
                    (
                        line.typographic_bounds(
                            std::ptr::null_mut(),
                            std::ptr::null_mut(),
                            std::ptr::null_mut(),
                        ),
                        if trim {
                            line.trailing_whitespace_width()
                        } else {
                            0.
                        },
                    )
                };
                if [width, trailing]
                    .iter()
                    .any(|v| !v.is_finite() || *v < 0. || *v > 1e9)
                {
                    return Err(Error::Metrics);
                }
                Ok((width - trailing).max(0.))
            };
            maximum = maximum.max(measure(0, units, false)?);
            let mut end = 0;
            let mut tokens = 0usize;
            while tokenizer.advance_to_next_token() != CFStringTokenizerTokenType::None {
                tokens += 1;
                if tokens > 100_000 {
                    return Err(Error::Wrapping);
                }
                let range = tokenizer.current_token_range();
                let start = u32::try_from(range.location).map_err(|_| Error::Metrics)?;
                let next = start
                    .checked_add(u32::try_from(range.length).map_err(|_| Error::Metrics)?)
                    .filter(|next| *next > start && *next <= units)
                    .ok_or(Error::Metrics)?;
                if start < end {
                    return Err(Error::Metrics);
                }
                // Gaps are measured too: no source content disappears if native
                // tokenization omits a separator or returns no tokens at all.
                minimum = minimum
                    .max(measure(end, start, true)?)
                    .max(measure(start, next, true)?);
                end = next;
            }
            minimum = minimum.max(measure(end, units, true)?);
        }
        // Native context/kerning can make isolated break segments fractionally
        // wider than their containing line; intrinsic constraints must be ordered.
        Ok([minimum.min(maximum), maximum])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_intrinsics_distinguish_breaks_nbsp_cjk_and_empty_lines() {
        for text in [
            "",
            "\r\n",
            "  ",
            "Hello world",
            "中文日本語",
            "a\u{a0}b",
            "ffi e\u{301} العربية",
        ] {
            let request = Request::new(text);
            let [min, max] = Paragraph::measure_intrinsic(&request).unwrap();
            assert!(min.is_finite() && min >= 0. && max >= min);
            assert!((max - Paragraph::new(&request, None, None).unwrap().size()[0]).abs() < 1e-6);
        }
        let words = Paragraph::measure_intrinsic(&Request::new("Hello world")).unwrap();
        assert!(words[0] > 0. && words[0] < words[1]);
        let cjk = Paragraph::measure_intrinsic(&Request::new("中文日本語")).unwrap();
        assert!(cjk[0] > 0. && cjk[0] < cjk[1]);
        let nbsp = Paragraph::measure_intrinsic(&Request::new("a\u{a0}b")).unwrap();
        assert!((nbsp[0] - nbsp[1]).abs() < 1e-6);
        let line = Paragraph::measure_intrinsic(&Request::new("unbreakable")).unwrap();
        assert!((line[0] - line[1]).abs() < 1e-6);
        let hard = Paragraph::measure_intrinsic(&Request::new("short\r\nunbreakable\n")).unwrap();
        assert_eq!(hard, line);
    }
    #[test]
    fn intrinsic_measurement_uses_tracking_and_validates_style() {
        let normal = Paragraph::measure_intrinsic(&Request::new("NNNN NNNN")).unwrap();
        let tracked = Paragraph::measure_intrinsic(&Request {
            letter_spacing: 2.,
            ..Request::new("NNNN NNNN")
        })
        .unwrap();
        assert!(tracked[0] > normal[0] && tracked[1] > normal[1]);
        assert!(
            Paragraph::measure_intrinsic(&Request {
                size: f64::NAN,
                ..Request::new("")
            })
            .is_err()
        );
    }
}
