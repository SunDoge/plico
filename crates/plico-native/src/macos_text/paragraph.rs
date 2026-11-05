//! Hard-break paragraphs composed from retained native lines.
use super::*;
use crate::paragraph_geometry::{hard_lines, line_box};
use std::ops::Range;

/// One retained native line positioned within a paragraph's logical coordinates.
pub struct ParagraphLine {
    /// Original UTF-8 content range, excluding a following hard separator.
    pub range: Range<usize>,
    /// Following hard separator, or an empty range for a soft wrap/end of text.
    /// CRLF is represented as one separator spanning both bytes.
    pub hard_break: Range<usize>,
    /// Baseline relative to the paragraph top-left, in logical points.
    pub baseline: [f64; 2],
    /// Top and bottom of the allocated line box, independent of glyph ink.
    pub vertical_bounds: [f64; 2],
    /// Retained native line; editing offsets are local to `range`.
    pub line: Line,
}
/// Multiline CoreText geometry with retained lines and no second shaping pass.
/// Rich runs, alignment, ellipsis and a renderer-provider bridge are pending.
pub struct Paragraph {
    lines: Vec<ParagraphLine>,
    size: [f64; 2],
}
impl Paragraph {
    /// Lay out text with optional soft-wrap width and absolute logical line height.
    /// Unlike `Line::new`, this accepts CRLF, CR, LF, NEL, LS and PS separators.
    /// Empty input/trailing separators retain an empty line. At most 4096 visual
    /// lines and 1e9 points of height are accepted; exceeding a budget is an error.
    pub fn new(
        request: &Request<'_>,
        width: Option<f64>,
        line_height: Option<f64>,
    ) -> Result<Self> {
        if !request.valid_paragraph()
            || width.is_some_and(|w| !w.is_finite() || w <= 0. || w > 1e9)
            || line_height.is_some_and(|h| !h.is_finite() || h <= 0. || h > 1e9)
        {
            return Err(Error::InvalidRequest);
        }
        // Reject excessive hard lines before allocating thousands of native fonts.
        if hard_lines(request.text).take(4097).count() > 4096 {
            return Err(Error::Wrapping);
        }
        let mut lines = Vec::new();
        let mut y = 0.;
        let mut max_width = 0f64;
        for (content, separator) in hard_lines(request.text) {
            if lines.len() >= 4096 {
                return Err(Error::Wrapping);
            }
            let part = Request {
                text: &request.text[content.clone()],
                ..*request
            };
            let wrapped = match width {
                Some(width) => Line::wrap(&part, width)?,
                None => vec![WrappedLine {
                    range: 0..part.text.len(),
                    line: Line::new(&part)?,
                }],
            };
            if lines.len() + wrapped.len() > 4096 {
                return Err(Error::Wrapping);
            }
            let last = wrapped.len() - 1;
            for (index, wrapped) in wrapped.into_iter().enumerate() {
                let metrics = wrapped.line.metrics();
                let (baseline, bottom) = line_box(
                    y,
                    [metrics.ascent, metrics.descent, metrics.leading],
                    line_height,
                )
                .ok_or(Error::Metrics)?;
                let range = content.start + wrapped.range.start..content.start + wrapped.range.end;
                let hard_break = if index == last {
                    separator.clone()
                } else {
                    range.end..range.end
                };
                max_width = max_width.max(metrics.width);
                lines.push(ParagraphLine {
                    range,
                    hard_break,
                    baseline: [0., baseline],
                    vertical_bounds: [y, bottom],
                    line: wrapped.line,
                });
                y = bottom;
            }
        }
        Ok(Self {
            lines,
            size: [max_width, y],
        })
    }
    /// Retained lines in reading order; each line uses its native bidi geometry.
    pub fn lines(&self) -> &[ParagraphLine] {
        &self.lines
    }
    /// Maximum native advance and total line-box height, in logical points.
    /// Glyph ink/overhang can extend outside these typographic bounds.
    pub fn size(&self) -> [f64; 2] {
        self.size
    }
    /// Hit-test paragraph-relative coordinates to an original UTF-8 byte boundary.
    /// Y selects the nearest line box; native x failure returns None. Hard
    /// separators have no glyph geometry and resolve to adjacent content edges.
    pub fn hit_test(&self, point: [f64; 2]) -> Option<usize> {
        if point.iter().any(|p| !p.is_finite() || p.abs() > 1e9) {
            return None;
        }
        let index = self
            .lines
            .partition_point(|line| line.vertical_bounds[1] <= point[1])
            .min(self.lines.len().checked_sub(1)?);
        let line = &self.lines[index];
        if line.line.text().is_empty() {
            return Some(line.range.start);
        }
        let local = line.line.hit_test_x(point[0] - line.baseline[0])?;
        Some(line.range.start + local)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn hard_breaks_empty_lines_and_soft_wraps_share_original_ranges() {
        let text = "first line has several words\r\n\n中文🙂日本語\u{2028}";
        let paragraph = Paragraph::new(&Request::new(text), Some(70.), Some(24.)).unwrap();
        assert!(paragraph.lines().len() > 4);
        assert_eq!(
            paragraph.lines().last().unwrap().range,
            text.len()..text.len()
        );
        let mut source = String::new();
        let mut previous = 0;
        for (index, line) in paragraph.lines().iter().enumerate() {
            assert_eq!(line.range.start, previous);
            assert_eq!(line.line.text(), &text[line.range.clone()]);
            assert_eq!(line.hard_break.start, line.range.end);
            source.push_str(&text[line.range.clone()]);
            source.push_str(&text[line.hard_break.clone()]);
            previous = line.hard_break.end;
            assert_eq!(
                line.vertical_bounds,
                [index as f64 * 24., (index + 1) as f64 * 24.]
            );
            assert!(line.baseline[1].is_finite());
            let x = line.line.caret_offsets(0).unwrap().primary;
            if let Some(hit) = paragraph.hit_test([x, line.vertical_bounds[0] + 12.]) {
                assert!(text.is_char_boundary(hit));
                assert!(hit >= line.range.start && hit <= line.range.end);
            }
        }
        assert_eq!(source, text);
        assert_eq!(paragraph.size()[1], paragraph.lines().len() as f64 * 24.);
        assert!(paragraph.hit_test([f64::NAN, 0.]).is_none());
    }
    #[test]
    fn paragraph_limits_are_explicit_and_empty_input_has_font_height() {
        let empty = Paragraph::new(&Request::new(""), None, None).unwrap();
        assert_eq!(empty.lines().len(), 1);
        assert!(empty.size()[1] > 0.);
        assert_eq!(empty.hit_test([0., 0.]), Some(0));
        for height in [0., -1., f64::NAN, f64::INFINITY, 1e9 + 1.] {
            assert!(matches!(
                Paragraph::new(&Request::new("text"), None, Some(height)),
                Err(Error::InvalidRequest)
            ));
        }
        let text = "\n".repeat(4096);
        assert!(matches!(
            Paragraph::new(&Request::new(&text), None, None),
            Err(Error::Wrapping)
        ));
        assert!(matches!(
            Paragraph::new(&Request::new("a\nb"), None, Some(1e9)),
            Err(Error::Metrics)
        ));
    }
}
