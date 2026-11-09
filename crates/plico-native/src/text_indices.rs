//! Valid scalar boundaries, not UTF-16 code-unit offsets inside surrogates.
pub(crate) enum TextIndices {
    Ascii(usize),
    Unicode(Vec<(usize, u32)>),
}
impl TextIndices {
    pub fn new(text: &str) -> Self {
        if text.is_ascii() {
            return Self::Ascii(text.len());
        }
        let mut units = 0;
        let mut entries = Vec::with_capacity(text.chars().count() + 1);
        for (byte, ch) in text.char_indices() {
            entries.push((byte, units));
            units += ch.len_utf16() as u32;
        }
        entries.push((text.len(), units));
        Self::Unicode(entries)
    }
    pub fn utf16(&self, byte: usize) -> Option<u32> {
        match self {
            Self::Ascii(len) => (byte <= *len).then(|| u32::try_from(byte).ok()).flatten(),
            Self::Unicode(entries) => entries
                .binary_search_by_key(&byte, |p| p.0)
                .ok()
                .map(|i| entries[i].1),
        }
    }
    pub fn byte(&self, units: u32) -> Option<usize> {
        match self {
            Self::Ascii(len) => ((units as usize) <= *len).then_some(units as usize),
            Self::Unicode(entries) => entries
                .binary_search_by_key(&units, |p| p.1)
                .ok()
                .map(|i| entries[i].0),
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn scalar_offsets_round_trip_and_surrogate_interiors_are_rejected() {
        for text in ["", "abc", "你好🙂 e\u{301} العربية", "👩‍👩‍👧‍👧\r\n日本語"]
        {
            let indices = TextIndices::new(text);
            for byte in 0..=text.len() {
                if text.is_char_boundary(byte) {
                    let utf16 = text[..byte].encode_utf16().count() as u32;
                    assert_eq!(indices.utf16(byte), Some(utf16));
                    assert_eq!(indices.byte(utf16), Some(byte));
                } else {
                    assert_eq!(indices.utf16(byte), None);
                }
            }
            let mut unit = 0;
            for ch in text.chars() {
                if ch.len_utf16() == 2 {
                    assert_eq!(indices.byte(unit + 1), None);
                    assert_eq!(indices.utf16(text.len() + 1), None);
                }
                unit += ch.len_utf16() as u32;
            }
            assert_eq!(indices.byte(unit + 1), None);
            assert_eq!(indices.utf16(text.len() + 1), None);
        }
    }
}
