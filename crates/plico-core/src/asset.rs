use std::{
    hash::{Hash, Hasher},
    sync::{Arc, OnceLock},
};

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
/// bytes hex-encoded in the node's text channel — every byte maps to two
/// ASCII characters, so the encoding is lossless, UTF-8-safe and comparable
/// for change detection, at 2× the encoded size. Static icons and thumbnails
/// dominate; runtime-generated images should go through a host widget scene.
#[derive(Debug, Clone)]
pub struct ImageAsset {
    data: ImageData,
    /// Encoded on first declaration; initialized clones share the result.
    encoded: OnceLock<Arc<str>>,
}
#[derive(Debug, Clone)]
enum ImageData {
    Static(&'static [u8]),
    Shared(Arc<[u8]>),
}
// Cache initialization must never change content identity or a HashMap key.
impl PartialEq for ImageAsset {
    fn eq(&self, other: &Self) -> bool {
        self.data() == other.data()
    }
}
impl Eq for ImageAsset {}
impl Hash for ImageAsset {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.data().hash(state);
    }
}
impl ImageAsset {
    /// Wrap embedded image bytes.
    pub const fn new(data: &'static [u8]) -> Self {
        Self {
            data: ImageData::Static(data),
            encoded: OnceLock::new(),
        }
    }
    /// Wrap runtime-loaded image bytes — a downloaded avatar, a generated
    /// thumbnail, anything not known at compile time. The bytes are copied
    /// once into the asset; declaring the same asset repeatedly does not
    /// re-copy, and swapping in a new asset re-declares and re-decodes.
    pub fn from_bytes(bytes: &[u8]) -> Self {
        Self {
            data: ImageData::Shared(Arc::from(bytes)),
            encoded: OnceLock::new(),
        }
    }
    /// The encoded image bytes.
    pub fn data(&self) -> &[u8] {
        match &self.data {
            ImageData::Static(data) => data,
            ImageData::Shared(data) => data,
        }
    }
    /// The hex encoding used as the node's change-detection text.
    /// Encoding is cached; this compatibility method copies the cached string.
    pub fn encoded(&self) -> String {
        self.encoded_str().to_owned()
    }
    /// Borrow the lazily cached, lossless encoding without allocating.
    pub fn encoded_str(&self) -> &str {
        self.encoded.get_or_init(|| {
            const HEX: &[u8; 16] = b"0123456789abcdef";
            let data = self.data();
            let mut out = String::with_capacity(data.len() * 2);
            for &byte in data {
                out.push(HEX[(byte >> 4) as usize] as char);
                out.push(HEX[(byte & 0xf) as usize] as char);
            }
            Arc::from(out)
        })
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::hash_map::DefaultHasher;

    #[test]
    fn cached_image_encoding_preserves_identity_and_shares_initialized_clones() {
        let asset = ImageAsset::from_bytes(&[0, 127, 128, 255]);
        let digest = |asset: &ImageAsset| {
            let mut hasher = DefaultHasher::new();
            asset.hash(&mut hasher);
            hasher.finish()
        };
        let hash = digest(&asset);
        assert_eq!(asset.encoded_str(), "007f80ff");
        assert_eq!(
            hash,
            digest(&asset),
            "warming the cache must not change its hash"
        );
        assert_eq!(asset, ImageAsset::new(&[0, 127, 128, 255]));
        let clone = asset.clone();
        assert!(std::ptr::eq(asset.encoded_str(), clone.encoded_str()));
        assert!(std::ptr::eq(asset.data(), clone.data()));
        assert_eq!(
            ImageAsset::decode(asset.encoded_str()).unwrap(),
            asset.data()
        );
    }
}
