//! Native run-based selection geometry, preserving visual bidi gaps.
use super::*;
use objc2_core_foundation::{CFArray, CGSize};
use objc2_core_text::{CTRun, CTRunStatus};
use std::{ops::Range, ptr::NonNull};

impl Line {
    /// Visual horizontal selection intervals for a local UTF-8 byte range.
    /// Direction runs are clipped separately so mixed-direction selection can
    /// produce multiple disjoint intervals. Empty ranges produce no intervals;
    /// invalid scalar boundaries/ranges and invalid native geometry are errors.
    pub fn selection_spans(&self, range: Range<usize>) -> Result<Vec<[f64; 2]>> {
        if range.start > range.end {
            return Err(Error::InvalidRequest);
        }
        let start = self
            .indices
            .utf16(range.start)
            .ok_or(Error::InvalidRequest)?
            + self.utf16_start;
        let end = self.indices.utf16(range.end).ok_or(Error::InvalidRequest)? + self.utf16_start;
        if start == end {
            return Ok(Vec::new());
        }
        let line_end =
            self.indices.utf16(self.text.len()).ok_or(Error::Metrics)? + self.utf16_start;
        // SAFETY: CTLineGetGlyphRuns returns a retained immutable array of CFType
        // objects. Each object is additionally checked as CTRun before use.
        let runs =
            unsafe { CFRetained::cast_unchecked::<CFArray<CFType>>(self.native.glyph_runs()) };
        if runs.len() > 100_000 {
            return Err(Error::Metrics);
        }
        let mut spans = Vec::new();
        let mut visited = 0usize;
        for value in runs.iter() {
            let run = value.downcast_ref::<CTRun>().ok_or(Error::Metrics)?;
            // SAFETY: Valid retained run; no borrowed native pointers escape.
            let (native_range, count, status) =
                unsafe { (run.string_range(), run.glyph_count(), run.status()) };
            let run_start = u32::try_from(native_range.location).map_err(|_| Error::Metrics)?;
            let run_end = run_start
                .checked_add(u32::try_from(native_range.length).map_err(|_| Error::Metrics)?)
                .filter(|end| *end <= line_end)
                .ok_or(Error::Metrics)?;
            if run_start < self.utf16_start {
                return Err(Error::Metrics);
            }
            let a = start.max(run_start);
            let b = end.min(run_end);
            if a >= b {
                continue;
            }
            let count = usize::try_from(count).map_err(|_| Error::Metrics)?;
            visited = visited
                .checked_add(count)
                .filter(|n| *n <= 100_000)
                .ok_or(Error::Metrics)?;
            if count == 0 {
                continue;
            }
            if status.contains(CTRunStatus::HasNonIdentityMatrix) {
                return Err(Error::Metrics);
            }
            let mut positions = vec![CGPoint::new(0., 0.); count];
            let mut advances = vec![CGSize::new(0., 0.); count];
            // SAFETY: Both nonempty output buffers hold exactly the checked run
            // glyph count. CFRange(0,0) requests all glyphs. Native writes finish
            // synchronously before Rust reads the buffers.
            unsafe {
                run.positions(
                    CFRange::new(0, 0),
                    NonNull::new(positions.as_mut_ptr()).unwrap(),
                );
                run.advances(
                    CFRange::new(0, 0),
                    NonNull::new(advances.as_mut_ptr()).unwrap(),
                );
            }
            let mut bounds = [f64::INFINITY, f64::NEG_INFINITY];
            for (position, advance) in positions.iter().zip(advances) {
                let x = position.x;
                let end = x + advance.width;
                if [x, end].iter().any(|x| !x.is_finite() || x.abs() > 1e9) {
                    return Err(Error::Metrics);
                }
                bounds[0] = bounds[0].min(x.min(end));
                bounds[1] = bounds[1].max(x.max(end));
            }
            let rtl = status.contains(CTRunStatus::RightToLeft);
            let edge = |unit: u32, leading: bool| -> Result<f64> {
                if leading && unit == run_start {
                    return Ok(bounds[usize::from(rtl)]);
                }
                if !leading && unit == run_end {
                    return Ok(bounds[usize::from(!rtl)]);
                }
                let local = unit.checked_sub(self.utf16_start).ok_or(Error::Metrics)?;
                let byte = self.indices.byte(local).ok_or(Error::Metrics)?;
                let caret = self.caret_offsets(byte)?;
                [caret.primary, caret.secondary]
                    .into_iter()
                    .find(|x| *x >= bounds[0] - 1e-6 && *x <= bounds[1] + 1e-6)
                    .map(|x| x.clamp(bounds[0], bounds[1]))
                    .ok_or(Error::Metrics)
            };
            let x0 = edge(a, true)?;
            let x1 = edge(b, false)?;
            if x0 != x1 {
                spans.push([x0.min(x1), x0.max(x1)]);
            }
        }
        Ok(crate::paragraph_geometry::merge_spans(spans))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn mixed_direction_selection_preserves_unselected_visual_gaps() {
        let line = Line::new(&Request::new("abc אבג def")).unwrap();
        // Logical prefix + first Hebrew letter: the other two Hebrew letters
        // appear visually between them and must remain unselected.
        let spans = line.selection_spans(0.."abc א".len()).unwrap();
        assert_eq!(spans.len(), 2, "{spans:?}");
        assert!(spans[0][1] < spans[1][0]);
        let all = line.selection_spans(0..line.text().len()).unwrap();
        assert_eq!(all.len(), 1);
        assert!(all[0][0] <= spans[0][0] && all[0][1] >= spans[1][1]);
    }
    #[test]
    fn selection_uses_native_carets_inside_ligatures_and_validates_utf8() {
        let line = Line::new(&Request::new("office 中文🙂 e\u{301}")).unwrap();
        for range in [1..2, 2..3, 3..4] {
            let a = line.caret_offsets(range.start).unwrap().primary;
            let b = line.caret_offsets(range.end).unwrap().primary;
            let spans = line.selection_spans(range).unwrap();
            assert_eq!(spans.len(), 1);
            assert!((spans[0][0] - a.min(b)).abs() < 1e-6);
            assert!((spans[0][1] - a.max(b)).abs() < 1e-6);
        }
        assert!(line.selection_spans(0..0).unwrap().is_empty());
        assert!(line.selection_spans(8..9).is_err());
        assert!(line.selection_spans(0..line.text().len() + 1).is_err());
        let reversed = Range { start: 3, end: 1 };
        assert!(line.selection_spans(reversed).is_err());
    }
}
