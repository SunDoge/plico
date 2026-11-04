//! Cached resvg raster fallback for retained SVG trees. No Vello scene translation.
use crate::{Error, Result};
use peniko::{Blob, ImageAlphaType, ImageData, ImageFormat};
use plico_renderer::SvgDocument;
use std::{num::NonZeroUsize, sync::Arc};
struct Raster {
    _document: SvgDocument,
    image: ImageData,
    bytes: usize,
}
pub(crate) struct SvgCache {
    entries: lru::LruCache<(usize, u32, u32), Raster>,
    bytes: usize,
}
impl Default for SvgCache {
    fn default() -> Self {
        Self {
            entries: lru::LruCache::new(NonZeroUsize::new(128).unwrap()),
            bytes: 0,
        }
    }
}
impl SvgCache {
    pub fn raster(&mut self, document: &SvgDocument, scale: f64) -> Result<ImageData> {
        let size = document.0.size();
        let width = (size.width() as f64 * scale).ceil();
        let height = (size.height() as f64 * scale).ceil();
        if !width.is_finite()
            || !height.is_finite()
            || width < 1.
            || height < 1.
            || width > 16384.
            || height > 16384.
            || width * height * 4. > 64. * 1024. * 1024.
        {
            return Err(Error::InvalidInput {
                reason: "SVG raster exceeds dimensions/64 MiB limit".into(),
            });
        }
        let (width, height) = (width as u32, height as u32);
        let key = (Arc::as_ptr(&document.0) as usize, width, height);
        if let Some(entry) = self.entries.get(&key) {
            return Ok(entry.image.clone());
        }
        let mut pixmap =
            resvg::tiny_skia::Pixmap::new(width, height).ok_or_else(|| Error::InvalidInput {
                reason: "SVG raster allocation failed".into(),
            })?;
        resvg::render(
            &document.0,
            resvg::tiny_skia::Transform::from_scale(
                width as f32 / size.width(),
                height as f32 / size.height(),
            ),
            &mut pixmap.as_mut(),
        );
        let pixels = pixmap.take();
        let bytes = pixels.len();
        let image = ImageData {
            data: Blob::new(Arc::new(pixels)),
            format: ImageFormat::Rgba8,
            alpha_type: ImageAlphaType::AlphaPremultiplied,
            width,
            height,
        };
        while self.bytes + bytes > 64 * 1024 * 1024
            || self.entries.len() == self.entries.cap().get()
        {
            let Some((_, entry)) = self.entries.pop_lru() else {
                break;
            };
            self.bytes -= entry.bytes;
        }
        self.entries.put(
            key,
            Raster {
                _document: document.clone(),
                image: image.clone(),
                bytes,
            },
        );
        self.bytes += bytes;
        Ok(image)
    }
}
