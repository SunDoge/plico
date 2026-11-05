//! Hard-break paragraphs composed from retained native lines.
use super::*;
use crate::paragraph_geometry::{hard_lines, line_box};
use std::ops::Range;

/// Paragraph-relative horizontal alignment, resolved per hard paragraph.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Alignment {
    /// Leading edge: left for LTR, right for RTL.
    #[default]
    Start,
    /// Center each line in the available width.
    Center,
    /// Trailing edge: right for LTR, left for RTL.
    End,
    /// Natively justify soft-wrapped lines except the final line of each hard paragraph.
    Justify,
}

/// One retained native line positioned within a paragraph's logical coordinates.
pub struct ParagraphLine {
    /// Original UTF-8 content range, excluding a following hard separator.
    pub range: Range<usize>,
    /// Base direction of the complete hard paragraph, inherited by soft wraps.
    pub right_to_left: bool,
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
/// Rich runs, ellipsis and a renderer-provider bridge are pending.
pub struct Paragraph {
    text: Box<str>,
    lines: Vec<ParagraphLine>,
    size: [f64; 2],
    alignment_width: f64,
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
        Self::with_alignment(request, width, line_height, Alignment::Start)
    }
    /// Lay out with direction-aware alignment. Width is also the soft-wrap limit;
    /// without one, lines align within the maximum intrinsic line advance.
    /// Justification reuses native CTLine shaping and is skipped for hard-final
    /// lines and when no explicit width was provided.
    pub fn with_alignment(
        request: &Request<'_>,
        width: Option<f64>,
        line_height: Option<f64>,
        alignment: Alignment,
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
            let right_to_left = crate::text_direction::paragraph_is_rtl(part.text);
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
                let mut line = wrapped.line;
                if alignment == Alignment::Justify
                    && index < last
                    && let Some(width) = width.filter(|width| *width >= line.metrics().width)
                {
                    // SAFETY: The line is retained and the width was validated.
                    if let Some(native) = unsafe { line.native.justified_line(1., width) } {
                        line = Line::from_native(
                            native,
                            line.font.clone(),
                            &line.text,
                            line.utf16_start,
                        )?;
                    }
                }
                let metrics = line.metrics();
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
                    right_to_left,
                    hard_break,
                    baseline: [0., baseline],
                    vertical_bounds: [y, bottom],
                    line,
                });
                y = bottom;
            }
        }
        let alignment_width = width.unwrap_or(max_width);
        for placed in &mut lines {
            let flush = match alignment {
                Alignment::Center => 0.5,
                Alignment::End if !placed.right_to_left => 1.,
                Alignment::Start | Alignment::Justify if placed.right_to_left => 1.,
                _ => 0.,
            };
            // Native flush accounts for trailing whitespace; subtracting the
            // typographic width here would misalign wrapped whitespace lines.
            // SAFETY: Retained line, finite flush and bounded alignment width.
            let x = unsafe {
                placed
                    .line
                    .native
                    .pen_offset_for_flush(flush, alignment_width)
            };
            if !x.is_finite() || x.abs() > 1e9 {
                return Err(Error::Metrics);
            }
            placed.baseline[0] = x;
        }
        Ok(Self {
            text: request.text.into(),
            lines,
            size: [max_width, y],
            alignment_width,
        })
    }
    /// Retained lines in reading order; each line uses its native bidi geometry.
    pub fn lines(&self) -> &[ParagraphLine] {
        &self.lines
    }
    /// Width used to align lines, independent of their intrinsic advances.
    pub fn alignment_width(&self) -> f64 {
        self.alignment_width
    }
    /// Maximum native advance and total line-box height, in logical points.
    /// Glyph ink/overhang can extend outside these typographic bounds.
    pub fn size(&self) -> [f64; 2] {
        self.size
    }
    /// Primary and secondary caret rectangles at an original UTF-8 boundary.
    /// Rectangles are [left, top, right, bottom], with one logical point width.
    /// At soft-wrap boundaries the following line is selected. A boundary inside
    /// CRLF maps to the preceding content edge; multibyte interiors are rejected.
    pub fn caret_rects(&self, byte: usize) -> Result<[[f64; 4]; 2]> {
        if !self.text.is_char_boundary(byte) {
            return Err(Error::InvalidRequest);
        }
        let index = self
            .lines
            .partition_point(|line| line.range.start <= byte)
            .saturating_sub(1);
        let placed = self.lines.get(index).ok_or(Error::Metrics)?;
        let local = byte
            .saturating_sub(placed.range.start)
            .min(placed.line.text().len());
        let caret = placed.line.caret_offsets(local)?;
        let rect = |x| {
            [
                placed.baseline[0] + x,
                placed.vertical_bounds[0],
                placed.baseline[0] + x + 1.,
                placed.vertical_bounds[1],
            ]
        };
        Ok([rect(caret.primary), rect(caret.secondary)])
    }
    /// Selection rectangles in paragraph-local logical coordinates. Each native
    /// direction run is clipped separately, preserving visual gaps through bidi
    /// text. Hard separators have no glyph rectangle; empty ranges return none.
    pub fn selection_rects(&self, range: Range<usize>) -> Result<Vec<[f64; 4]>> {
        if range.start > range.end
            || !self.text.is_char_boundary(range.start)
            || !self.text.is_char_boundary(range.end)
        {
            return Err(Error::InvalidRequest);
        }
        let mut rectangles = Vec::new();
        for placed in &self.lines {
            let start = range.start.max(placed.range.start);
            let end = range.end.min(placed.range.end);
            if start >= end {
                continue;
            }
            for span in placed
                .line
                .selection_spans(start - placed.range.start..end - placed.range.start)?
            {
                rectangles.push([
                    span[0] + placed.baseline[0],
                    placed.vertical_bounds[0],
                    span[1] + placed.baseline[0],
                    placed.vertical_bounds[1],
                ]);
            }
        }
        Ok(rectangles)
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

#[cfg(test)]
mod alignment_tests {
    use super::*;
    #[test]
    fn alignment_moves_native_lines_and_hit_testing_together() {
        let request = Request::new("NNN");
        let mut offsets = Vec::new();
        for alignment in [Alignment::Start, Alignment::Center, Alignment::End] {
            let paragraph =
                Paragraph::with_alignment(&request, Some(200.), None, alignment).unwrap();
            let line = &paragraph.lines()[0];
            offsets.push(line.baseline[0]);
            let x = line.line.caret_offsets(1).unwrap().primary + line.baseline[0];
            assert_eq!(
                paragraph.hit_test([x, line.vertical_bounds[0] + 1.]),
                Some(1)
            );
            assert_eq!(paragraph.alignment_width(), 200.);
        }
        assert!(offsets[0].abs() < 1e-6);
        assert!(offsets[1] > offsets[0] && offsets[2] > offsets[1]);
        assert!((offsets[1] * 2. - offsets[2]).abs() < 1e-6);
    }
    #[test]
    fn hard_paragraphs_resolve_start_end_independently() {
        let request = Request::new("English\nשלום\n123");
        for alignment in [Alignment::Start, Alignment::End] {
            let paragraph =
                Paragraph::with_alignment(&request, Some(200.), None, alignment).unwrap();
            let lines = paragraph.lines();
            assert_eq!(
                lines
                    .iter()
                    .map(|line| line.right_to_left)
                    .collect::<Vec<_>>(),
                vec![false, true, false]
            );
            for line in lines {
                let right = (alignment == Alignment::Start) == line.right_to_left;
                if right {
                    assert!(line.baseline[0] > 0.);
                } else {
                    assert!(line.baseline[0].abs() < 1e-6);
                }
            }
        }
    }
    #[test]
    fn native_justification_preserves_ranges_and_hard_final_lines() {
        let request = Request::new("one two three four five six seven eight nine ten\nlast");
        let normal = Paragraph::new(&request, Some(140.), None).unwrap();
        let justified =
            Paragraph::with_alignment(&request, Some(140.), None, Alignment::Justify).unwrap();
        assert_eq!(normal.lines().len(), justified.lines().len());
        let mut expanded = false;
        for (before, after) in normal.lines().iter().zip(justified.lines()) {
            assert_eq!(before.range, after.range);
            assert_eq!(before.hard_break, after.hard_break);
            if !before.hard_break.is_empty() || before.range.end == request.text.len() {
                assert_eq!(before.line.metrics().width, after.line.metrics().width);
            } else if after.line.metrics().width > before.line.metrics().width + 0.1 {
                expanded = true;
            }
        }
        assert!(expanded);
    }
}

#[cfg(test)]
mod editing_tests {
    use super::*;
    #[test]
    fn selection_and_carets_include_alignment_and_line_positions() {
        let text = "NNN NNN NNN NNN\r\n\n中文🙂";
        let paragraph =
            Paragraph::with_alignment(&Request::new(text), Some(70.), Some(24.), Alignment::Center)
                .unwrap();
        let rectangles = paragraph.selection_rects(0..text.len()).unwrap();
        assert!(!rectangles.is_empty());
        for line in paragraph.lines() {
            let caret = paragraph.caret_rects(line.range.start).unwrap();
            assert_eq!(caret[0][1], line.vertical_bounds[0]);
            assert_eq!(caret[0][3], line.vertical_bounds[1]);
            assert_eq!(
                caret[0][0],
                line.baseline[0] + line.line.caret_offsets(0).unwrap().primary
            );
            if !line.range.is_empty() {
                assert!(rectangles.iter().any(|r| r[1] == line.vertical_bounds[0]
                    && r[3] == line.vertical_bounds[1]
                    && r[2] > r[0]));
            }
        }
        let crlf = text.find('\r').unwrap();
        assert_eq!(
            paragraph.caret_rects(crlf).unwrap(),
            paragraph.caret_rects(crlf + 1).unwrap()
        );
        assert!(
            paragraph
                .selection_rects(crlf..crlf + 2)
                .unwrap()
                .is_empty()
        );
        assert!(paragraph.selection_rects(0..0).unwrap().is_empty());
        assert!(paragraph.caret_rects(text.len() + 1).is_err());
        let cjk = text.find('中').unwrap();
        assert!(paragraph.caret_rects(cjk + 1).is_err());
        assert!(paragraph.selection_rects(cjk + 1..text.len()).is_err());
    }
}
