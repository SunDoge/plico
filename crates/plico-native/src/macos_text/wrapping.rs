//! Bounded native line breaking for one logical paragraph.
use super::*;
use objc2_core_text::CTTypesetter;
use std::ops::Range;

/// One native wrapped line and its UTF-8 range in the original request.
pub struct WrappedLine {
    /// Original text range; whitespace is retained rather than reconstructed.
    pub range: Range<usize>,
    /// Retained line with local UTF-8 editing offsets and native rasterization.
    pub line: Line,
}
impl Line {
    /// Wrap a single logical paragraph through CoreText, preserving cluster and
    /// contextual shaping decisions. Width is finite, positive, at most 1e9 points.
    /// At most 4096 lines are produced; overflow is an error, not silent truncation.
    /// An empty paragraph produces one empty line. Hard breaks remain unsupported
    /// by `Request`; no line alignment, vertical placement or ellipsis is supplied.
    pub fn wrap(request: &Request<'_>, width: f64) -> Result<Vec<WrappedLine>> {
        if !width.is_finite() || width <= 0. || width > 1e9 {
            return Err(Error::InvalidRequest);
        }
        if request.text.is_empty() {
            return Ok(vec![WrappedLine {
                range: 0..0,
                line: Self::new(request)?,
            }]);
        }
        let (font, attributed) = Self::attributed(request)?;
        // SAFETY: The valid attributed string is not mutated after this call.
        let typesetter = unsafe { CTTypesetter::with_attributed_string(&attributed) };
        let indices = TextIndices::new(request.text);
        let total = indices.utf16(request.text.len()).ok_or(Error::Wrapping)?;
        let mut start = 0u32;
        let mut byte_start = 0;
        let mut lines = Vec::new();
        while start < total {
            if lines.len() == 4096 {
                return Err(Error::Wrapping);
            }
            // SAFETY: start is a validated source UTF-16 boundary. CoreText owns
            // line-breaking/contextual shaping; no Rust scalar slicing chooses a
            // linguistic cluster boundary.
            let count = unsafe { typesetter.suggest_line_break(start as isize, width) };
            let count = u32::try_from(count).map_err(|_| Error::Wrapping)?;
            let end = start
                .checked_add(count)
                .filter(|end| *end > start && *end <= total)
                .ok_or(Error::Wrapping)?;
            let byte_end = indices.byte(end).ok_or(Error::Wrapping)?;
            // SAFETY: Native suggestion was checked for progress, scalar boundary
            // and containment in the typesetter's original UTF-16 string.
            let native = unsafe { typesetter.line(CFRange::new(start as isize, count as isize)) };
            let line = Self::from_native(
                native,
                font.clone(),
                &request.text[byte_start..byte_end],
                start,
            )?;
            lines.push(WrappedLine {
                range: byte_start..byte_end,
                line,
            });
            start = end;
            byte_start = byte_end;
        }
        Ok(lines)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_wrapping_keeps_complete_utf8_ranges_and_local_editing_offsets() {
        for text in [
            "One two three four five six seven eight nine ten",
            "日本語の文章と中文测试🙂🙂続きの文章",
            "ffi e\u{301} ffi office e\u{301} more words here",
            "العربية שלום العربية שלום العربية",
        ] {
            let lines = Line::wrap(&Request::new(text), 70.).unwrap();
            assert!(lines.len() > 1);
            let mut end = 0;
            for wrapped in &lines {
                assert_eq!(wrapped.range.start, end);
                assert!(wrapped.range.end > end);
                assert_eq!(wrapped.line.text(), &text[wrapped.range.clone()]);
                assert!(wrapped.line.metrics().width.is_finite());
                for byte in 0..=wrapped.line.text().len() {
                    if wrapped.line.text().is_char_boundary(byte) {
                        let caret = wrapped.line.caret_offsets(byte).unwrap();
                        assert!(caret.primary.is_finite() && caret.secondary.is_finite());
                        if let Some(hit) = wrapped.line.hit_test_x(caret.primary) {
                            assert!(wrapped.line.text().is_char_boundary(hit));
                        }
                    }
                }
                let a = wrapped
                    .line
                    .rasterize(1.25, [0.25, 0.], [50, 60, 70, 255])
                    .unwrap();
                let b = wrapped
                    .line
                    .rasterize(1.25, [0.25, 0.], [50, 60, 70, 255])
                    .unwrap();
                assert_eq!(a, b);
                end = wrapped.range.end;
            }
            assert_eq!(end, text.len());
        }
    }

    #[test]
    fn later_lines_map_native_indices_back_to_local_utf8() {
        let text = "NNNN NNNN NNNN NNNN NNNN";
        let lines = Line::wrap(&Request::new(text), 60.).unwrap();
        assert!(lines.len() > 1);
        for wrapped in &lines[1..] {
            assert!(wrapped.range.start > 0);
            let line = &wrapped.line;
            for (byte, _) in line.text().char_indices() {
                let x = line.caret_offsets(byte).unwrap().primary;
                assert_eq!(line.hit_test_x(x), Some(byte));
            }
        }
    }

    #[test]
    fn wrapping_limits_are_observable_and_empty_text_keeps_a_line() {
        let lines = Line::wrap(&Request::new(""), 10.).unwrap();
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].range, 0..0);
        assert!(lines[0].line.metrics().ascent > 0.);
        for width in [0., -1., f64::NAN, f64::INFINITY, 1e9 + 1.] {
            assert!(matches!(
                Line::wrap(&Request::new("text"), width),
                Err(Error::InvalidRequest)
            ));
        }
        let long = "word ".repeat(6000);
        assert!(matches!(
            Line::wrap(&Request::new(&long), 40.),
            Err(Error::Wrapping)
        ));
        assert!(Line::wrap(&Request::new("a\nb"), 40.).is_err());
        // An impossibly narrow width must either consume a native cluster or
        // return an error; it cannot spin forever or emit empty ranges.
        if let Ok(lines) = Line::wrap(&Request::new("👩‍👩‍👧‍👧"), f64::MIN_POSITIVE)
        {
            assert_eq!(lines.len(), 1);
            assert_eq!(lines[0].range.end, "👩‍👩‍👧‍👧".len());
        }
    }
}
