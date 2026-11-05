//! Portable UTF-8 hard-break ranges and vertical line placement for CoreText.
use std::ops::Range;

/// Content and following hard separator, including a final empty content range.
pub(crate) fn hard_lines(text: &str) -> impl Iterator<Item = (Range<usize>, Range<usize>)> + '_ {
    let mut cursor = 0;
    let mut finished = false;
    std::iter::from_fn(move || {
        if finished {
            return None;
        }
        let next = text[cursor..]
            .char_indices()
            .find(|(_, c)| matches!(c, '\r' | '\n' | '\u{85}' | '\u{2028}' | '\u{2029}'));
        let Some((offset, ch)) = next else {
            finished = true;
            return Some((cursor..text.len(), text.len()..text.len()));
        };
        let end = cursor + offset;
        let separator_end = end
            + if ch == '\r' && text.as_bytes().get(end + 1) == Some(&b'\n') {
                2
            } else {
                ch.len_utf8()
            };
        let content = cursor..end;
        cursor = separator_end;
        Some((content, end..separator_end))
    })
}

pub(crate) fn line_box(y: f64, metrics: [f64; 3], height: Option<f64>) -> Option<(f64, f64)> {
    let [ascent, descent, leading] = metrics;
    if [y, ascent, descent, leading]
        .iter()
        .any(|n| !n.is_finite() || *n < 0.)
    {
        return None;
    }
    let height = height.unwrap_or(ascent + descent + leading);
    let bottom = y + height;
    let baseline = y + (height - ascent - descent) * 0.5 + ascent;
    if !height.is_finite()
        || height <= 0.
        || bottom <= y
        || !bottom.is_finite()
        || bottom > 1e9
        || !baseline.is_finite()
        || baseline.abs() > 1e9
    {
        return None;
    }
    Some((baseline, bottom))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn hard_breaks_preserve_unicode_crlf_empty_lines_and_all_source_bytes() {
        for text in [
            "",
            "abc",
            "\n",
            "a\r\nb\rc\nd\u{85}e\u{2028}f\u{2029}",
            "🙂\n\n中文",
        ] {
            let mut previous = 0;
            let mut joined = String::new();
            for (content, separator) in hard_lines(text) {
                assert_eq!(content.start, previous);
                assert_eq!(content.end, separator.start);
                joined.push_str(&text[content]);
                joined.push_str(&text[separator.clone()]);
                previous = separator.end;
            }
            assert_eq!(joined, text);
            assert_eq!(previous, text.len());
        }
        assert_eq!(
            hard_lines("a\r\n").collect::<Vec<_>>(),
            vec![(0..1, 1..3), (3..3, 3..3)]
        );
        assert_eq!(hard_lines("\n\n").count(), 3);
    }
    #[test]
    fn baseline_placement_handles_leading_explicit_height_and_overflow() {
        assert_eq!(line_box(0., [10., 3., 2.], None), Some((11., 15.)));
        assert_eq!(line_box(15., [10., 3., 2.], Some(20.)), Some((28.5, 35.)));
        // Short explicit line heights allow ink to overlap adjacent line boxes.
        assert_eq!(line_box(0., [10., 3., 0.], Some(8.)), Some((7.5, 8.)));
        for height in [0., -1., f64::NAN, f64::INFINITY, 1e9 + 1.] {
            assert!(line_box(0., [10., 3., 0.], Some(height)).is_none());
        }
        assert!(line_box(1e9, [10., 3., 0.], None).is_none());
        assert!(line_box(1., [10., 3., 0.], Some(f64::MIN_POSITIVE)).is_none());
        assert!(line_box(0., [f64::NAN, 3., 0.], None).is_none());
    }
}

/// Merge visual overlap/adjacency without filling an unselected bidi gap.
pub(crate) fn merge_spans(mut spans: Vec<[f64; 2]>) -> Vec<[f64; 2]> {
    spans.sort_by(|a, b| a[0].total_cmp(&b[0]));
    let mut written = 0;
    for read in 0..spans.len() {
        let span = spans[read];
        if written > 0 && span[0] <= spans[written - 1][1] + 1e-6 {
            spans[written - 1][1] = spans[written - 1][1].max(span[1]);
        } else {
            spans[written] = span;
            written += 1;
        }
    }
    spans.truncate(written);
    spans
}

#[cfg(test)]
mod selection_tests {
    use super::*;
    #[test]
    fn visual_selection_merges_overlap_but_preserves_gaps_and_reuses_storage() {
        let spans = vec![[50., 60.], [0., 10.], [5., 15.], [15., 20.], [70., 80.]];
        let original = spans.as_ptr();
        let merged = merge_spans(spans);
        assert_eq!(merged, vec![[0., 20.], [50., 60.], [70., 80.]]);
        assert_eq!(merged.as_ptr(), original);
        assert!(merge_spans(Vec::new()).is_empty());
    }
}
