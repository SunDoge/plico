/// An explicitly embedded SVG. No asset directory scan or global registry is required.
#[derive(Clone, Copy, Debug)]
pub struct SvgAsset {
    source: &'static str,
}
impl SvgAsset {
    pub const fn new(source: &'static str) -> Self {
        Self { source }
    }
    pub const fn source(self) -> &'static str {
        self.source
    }
}
/// Embed one SVG relative to the calling source file, using Rust's include_str!.
#[macro_export]
macro_rules! svg_asset {
    ($path:literal) => {
        $crate::SvgAsset::new(include_str!($path))
    };
}

/// A raster image embedded in the binary: PNG, JPEG, ICO or WebP, decoded by
/// the backend and cached by content. The declaration carries the asset's
/// bytes hex-encoded in the node's text channel — every byte maps to one
/// `char` below 0x100, so the encoding is lossless, UTF-8-safe and comparable
/// for change detection, at 2× the encoded size. Static icons and thumbnails
/// dominate; runtime-generated images should go through a host widget scene.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ImageAsset {
    data: &'static [u8],
}
impl ImageAsset {
    /// Wrap embedded image bytes produced by [`image_asset!`].
    pub const fn new(data: &'static [u8]) -> Self {
        Self { data }
    }
    /// The encoded image bytes.
    pub const fn data(self) -> &'static [u8] {
        self.data
    }
    /// The hex encoding used as the node's change-detection text.
    pub fn encoded(self) -> String {
        const HEX: &[u8; 16] = b"0123456789abcdef";
        let mut out = String::with_capacity(self.data.len() * 2);
        for &byte in self.data {
            out.push(HEX[(byte >> 4) as usize] as char);
            out.push(HEX[(byte & 0xf) as usize] as char);
        }
        out
    }
    /// Recover the bytes from the node's text channel.
    pub fn decode(text: &str) -> Option<Vec<u8>> {
        let bytes = text.as_bytes();
        if !bytes.len().is_multiple_of(2) {
            return None;
        }
        let mut out = Vec::with_capacity(bytes.len() / 2);
        for pair in bytes.chunks(2) {
            let hi = (pair[0] as char).to_digit(16)?;
            let lo = (pair[1] as char).to_digit(16)?;
            out.push((hi * 16 + lo) as u8);
        }
        Some(out)
    }
}
/// Embed one image relative to the calling source file, using Rust's
/// include_bytes!.
#[macro_export]
macro_rules! image_asset {
    ($path:literal) => {
        $crate::ImageAsset::new(include_bytes!($path))
    };
}
