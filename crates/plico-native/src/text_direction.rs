//! Base direction only; native layout performs shaping and visual bidi order.

/// Infer the first paragraph's direction with Unicode bidi rules P2/P3.
/// Isolated contents do not determine the outer direction. Neutral/empty text
/// defaults to LTR. This does not implement independent directions for multiple
/// hard paragraphs within one native layout.
pub fn paragraph_is_rtl(text: &str) -> bool {
    unicode_bidi::get_base_direction(text) == unicode_bidi::Direction::Rtl
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn base_direction_uses_strong_characters_outside_isolates() {
        for text in [
            "مرحبا",
            "123 שלום",
            "🙂 العربية",
            "\u{2066}English\u{2069}مرحبا",
        ] {
            assert!(paragraph_is_rtl(text), "{text:?}");
        }
        for text in [
            "",
            "123 🙂",
            "English العربية",
            "中文 العربية",
            "\u{2067}مرحبا\u{2069}English",
            "\u{2068}مرحبا\u{2069}",
            "123\nمرحبا",
        ] {
            assert!(!paragraph_is_rtl(text), "{text:?}");
        }
    }
}
