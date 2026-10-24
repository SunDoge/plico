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
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ImageAsset {
    /// Embedded bytes, or None when the asset owns runtime-loaded bytes.
    data: Option<&'static [u8]>,
    /// Runtime-loaded bytes (PNG/JPEG/ICO/WebP as loaded). Empty when the
    /// asset wraps embedded bytes.
    owned: Vec<u8>,
}
impl ImageAsset {
    /// Wrap embedded image bytes produced by [`image_asset!`].
    pub const fn new(data: &'static [u8]) -> Self {
        Self {
            data: Some(data),
            owned: Vec::new(),
        }
    }
    /// Wrap runtime-loaded image bytes — a downloaded avatar, a generated
    /// thumbnail, anything not known at compile time. The bytes are copied
    /// once into the asset; declaring the same asset repeatedly does not
    /// re-copy, and swapping in a new asset re-declares and re-decodes.
    pub fn from_bytes(bytes: &[u8]) -> Self {
        Self {
            data: None,
            owned: bytes.to_vec(),
        }
    }
    /// The encoded image bytes.
    pub fn data(&self) -> &[u8] {
        self.data.unwrap_or(&self.owned)
    }
    /// The hex encoding used as the node's change-detection text.
    pub fn encoded(&self) -> String {
        const HEX: &[u8; 16] = b"0123456789abcdef";
        let data = self.data();
        let mut out = String::with_capacity(data.len() * 2);
        for &byte in data {
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
