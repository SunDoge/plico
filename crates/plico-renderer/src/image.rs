//! Decoded, reusable raster images for retained scene painting.

#![warn(missing_docs)]

use std::sync::Arc;

use vello_common::peniko::{Blob, ImageAlphaType, ImageBrush, ImageData, ImageFormat};

/// Decode policy. Exceeding a limit returns an error; pixels are never reduced.
#[derive(Clone, Copy, Debug)]
pub struct DecodeLimits {
    /// Encoded input size, checked before opening the decoder.
    pub max_encoded_bytes: usize,
    /// Largest accepted width or height (also compatible with renderer u16 dimensions).
    pub max_dimension: u32,
    /// Maximum RGBA output size. Decoder scratch allocations may use extra memory.
    pub max_rgba_bytes: u64,
}
impl Default for DecodeLimits {
    fn default() -> Self {
        Self {
            max_encoded_bytes: 64 * 1024 * 1024,
            max_dimension: 16384,
            max_rgba_bytes: 256 * 1024 * 1024,
        }
    }
}
impl DecodeLimits {
    pub(crate) fn check_input(self, bytes: &[u8]) -> image::ImageResult<()> {
        if bytes.len() > self.max_encoded_bytes {
            return Err(memory_limit());
        }
        Ok(())
    }
    pub(crate) fn check_dimensions(self, width: u32, height: u32) -> image::ImageResult<u64> {
        if width == 0
            || height == 0
            || width > self.max_dimension.min(u16::MAX as u32)
            || height > self.max_dimension.min(u16::MAX as u32)
        {
            return Err(image::ImageError::Limits(
                image::error::LimitError::from_kind(image::error::LimitErrorKind::DimensionError),
            ));
        }
        let bytes = u64::from(width) * u64::from(height) * 4;
        if bytes > self.max_rgba_bytes {
            return Err(memory_limit());
        }
        Ok(bytes)
    }
}
pub(crate) fn memory_limit() -> image::ImageError {
    image::ImageError::Limits(image::error::LimitError::from_kind(
        image::error::LimitErrorKind::InsufficientMemory,
    ))
}

/// A decoded RGBA image retained by the host and shared by every node using it.
#[derive(Clone, Debug)]
pub struct RasterImage {
    brush: ImageBrush,
    size: [f32; 2],
    byte_len: usize,
}

impl RasterImage {
    /// Decode a supported raster format by content, with dimensions bounded
    /// before allocating decoded pixels for untrusted sources.
    pub fn decode(bytes: &[u8]) -> Result<Self, image::ImageError> {
        Self::decode_with_limits(bytes, DecodeLimits::default())
    }

    /// Decode full resolution using an explicit resource budget. Animated WebP
    /// must use `AnimatedImage::decode_webp` rather than silently losing frames.
    pub fn decode_with_limits(bytes: &[u8], limits: DecodeLimits) -> image::ImageResult<Self> {
        limits.check_input(bytes)?;
        let image = match image::guess_format(bytes)? {
            image::ImageFormat::Png => {
                let decoder = image::codecs::png::PngDecoder::new(std::io::Cursor::new(bytes))?;
                Self::decode_with(decoder, limits)?
            }
            image::ImageFormat::Ico => {
                let decoder = image::codecs::ico::IcoDecoder::new(std::io::Cursor::new(bytes))?;
                Self::decode_with(decoder, limits)?
            }
            image::ImageFormat::Jpeg => {
                let decoder = image::codecs::jpeg::JpegDecoder::new(std::io::Cursor::new(bytes))?;
                Self::decode_with(decoder, limits)?
            }
            image::ImageFormat::WebP => {
                let decoder = image::codecs::webp::WebPDecoder::new(std::io::Cursor::new(bytes))?;
                if decoder.has_animation() {
                    return Err(image::ImageError::Unsupported(
                        image::error::UnsupportedError::from_format_and_kind(
                            image::ImageFormat::WebP.into(),
                            image::error::UnsupportedErrorKind::GenericFeature(
                                "animated WebP: use AnimatedImage::decode_webp".into(),
                            ),
                        ),
                    ));
                }
                Self::decode_with(decoder, limits)?
            }
            format => {
                return Err(image::ImageError::Unsupported(
                    image::error::UnsupportedError::from_format_and_kind(
                        image::error::ImageFormatHint::Exact(format),
                        image::error::UnsupportedErrorKind::Format(
                            image::error::ImageFormatHint::Exact(format),
                        ),
                    ),
                ));
            }
        };
        Ok(Self::from_rgba(image))
    }

    fn decode_with(
        mut decoder: impl image::ImageDecoder,
        limits: DecodeLimits,
    ) -> image::ImageResult<image::RgbaImage> {
        let (width, height) = decoder.dimensions();
        limits.check_dimensions(width, height)?;
        let mut decoder_limits = image::Limits::default();
        decoder_limits.max_image_width = Some(limits.max_dimension);
        decoder_limits.max_image_height = Some(limits.max_dimension);
        decoder_limits.max_alloc = Some(limits.max_rgba_bytes);
        decoder.set_limits(decoder_limits)?;
        let orientation = decoder.orientation()?;
        let mut image = image::DynamicImage::from_decoder(decoder)?;
        image.apply_orientation(orientation);
        Ok(image.into_rgba8())
    }

    /// Generate a separate aspect-preserving thumbnail; never mutates the source
    /// and never enlarges it. Lanczos3 is used only for this explicit resampling.
    pub fn thumbnail(&self, width: u32, height: u32) -> image::ImageResult<Self> {
        if width == 0 || height == 0 {
            return Err(image::ImageError::Parameter(
                image::error::ParameterError::from_kind(
                    image::error::ParameterErrorKind::DimensionMismatch,
                ),
            ));
        }
        let data = &self.brush.image;
        let rgba = image::RgbaImage::from_raw(data.width, data.height, self.rgba().to_vec())
            .expect("validated RGBA image");
        Ok(Self::from_rgba(
            image::DynamicImage::ImageRgba8(rgba)
                .resize(
                    width.min(data.width),
                    height.min(data.height),
                    image::imageops::FilterType::Lanczos3,
                )
                .into_rgba8(),
        ))
    }

    /// Select a sampling hint without modifying decoded pixels or resource identity.
    pub fn with_quality(mut self, quality: vello_common::peniko::ImageQuality) -> Self {
        self.brush.sampler.quality = quality;
        self
    }

    /// Borrow original decoded, straight-alpha RGBA8 pixels for CPU work such as OCR.
    pub fn rgba(&self) -> &[u8] {
        self.brush.image.data.data()
    }

    /// Build a renderer image from already decoded RGBA pixels.
    pub fn from_rgba(rgba: image::RgbaImage) -> Self {
        let (width, height) = rgba.dimensions();
        let byte_len = rgba.len();
        let data = ImageData {
            data: Blob::new(Arc::new(rgba.into_raw())),
            format: ImageFormat::Rgba8,
            alpha_type: ImageAlphaType::Alpha,
            width,
            height,
        };
        Self {
            brush: ImageBrush::new(data),
            size: [width as f32, height as f32],
            byte_len,
        }
    }

    /// Borrow the retained backend-neutral image brush.
    pub fn brush(&self) -> &ImageBrush {
        &self.brush
    }

    /// Full decoded `[width, height]` after orientation normalization.
    pub fn size(&self) -> [f32; 2] {
        self.size
    }

    /// Number of decoded RGBA bytes retained by this image.
    pub fn byte_len(&self) -> usize {
        self.byte_len
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::ImageEncoder as _;

    #[test]
    fn decodes_png_into_a_reusable_vello_brush() {
        let mut encoded = Vec::new();
        image::codecs::png::PngEncoder::new(&mut encoded)
            .write_image(
                &[255, 0, 0, 255, 0, 255, 0, 255],
                2,
                1,
                image::ExtendedColorType::Rgba8,
            )
            .unwrap();
        let image = RasterImage::decode(&encoded).unwrap();
        assert_eq!(image.size(), [2.0, 1.0]);
        assert_eq!(image.brush().image.data.data().len(), 8);
    }

    #[test]
    fn full_resolution_is_preserved_and_thumbnail_is_explicit() {
        let mut encoded = Vec::new();
        image::codecs::png::PngEncoder::new(&mut encoded)
            .write_image(
                &vec![0; 1140 * 1140 * 4],
                1140,
                1140,
                image::ExtendedColorType::Rgba8,
            )
            .unwrap();
        assert_eq!(
            RasterImage::decode(&encoded).unwrap().size(),
            [1140.0, 1140.0]
        );
        let full = RasterImage::decode(&encoded).unwrap();
        assert_eq!(full.thumbnail(256, 128).unwrap().size(), [128., 128.]);
        assert_eq!(full.thumbnail(2000, 2000).unwrap().size(), [1140., 1140.]);
        assert_eq!(full.size(), [1140., 1140.]);
        assert!(full.thumbnail(0, 20).is_err());
    }

    #[test]
    fn decodes_ico_by_content_instead_of_url_extension() {
        let mut encoded = Vec::new();
        image::codecs::ico::IcoEncoder::new(&mut encoded)
            .write_image(&[255, 0, 0, 255], 1, 1, image::ExtendedColorType::Rgba8)
            .unwrap();
        let image = RasterImage::decode(&encoded).unwrap();
        assert_eq!(image.size(), [1.0, 1.0]);
    }

    #[test]
    fn decodes_jpeg_by_content() {
        let mut encoded = Vec::new();
        image::codecs::jpeg::JpegEncoder::new(&mut encoded)
            .write_image(&[255, 0, 0], 1, 1, image::ExtendedColorType::Rgb8)
            .unwrap();
        assert_eq!(RasterImage::decode(&encoded).unwrap().size(), [1.0, 1.0]);
    }

    #[test]
    fn decodes_webp_by_content() {
        let mut encoded = Vec::new();
        image::codecs::webp::WebPEncoder::new_lossless(&mut encoded)
            .write_image(&[255, 0, 0, 255], 1, 1, image::ExtendedColorType::Rgba8)
            .unwrap();
        assert_eq!(RasterImage::decode(&encoded).unwrap().size(), [1.0, 1.0]);
    }
    #[test]
    fn budgets_fail_explicitly_and_ocr_sized_images_keep_their_pixels() {
        let rgba = image::RgbaImage::from_fn(2304, 2048, |x, y| {
            image::Rgba([(x % 256) as u8, (y % 256) as u8, 127, 255])
        });
        let mut bytes = Vec::new();
        image::codecs::png::PngEncoder::new(&mut bytes)
            .write_image(rgba.as_raw(), 2304, 2048, image::ExtendedColorType::Rgba8)
            .unwrap();
        let decoded = RasterImage::decode(&bytes).unwrap();
        assert_eq!(decoded.size(), [2304., 2048.]);
        assert_eq!(decoded.rgba(), rgba.as_raw());
        for limits in [
            DecodeLimits {
                max_dimension: 1024,
                ..Default::default()
            },
            DecodeLimits {
                max_rgba_bytes: 1024,
                ..Default::default()
            },
            DecodeLimits {
                max_encoded_bytes: 1,
                ..Default::default()
            },
        ] {
            assert!(matches!(
                RasterImage::decode_with_limits(&bytes, limits),
                Err(image::ImageError::Limits(_))
            ));
        }
    }
    #[test]
    fn exif_orientation_is_applied_without_resampling() {
        let mut exif = b"II\x2a\x00\x08\x00\x00\x00\x01\x00\x12\x01\x03\x00\x01\x00\x00\x00\x06\x00\x00\x00\x00\x00\x00\x00".to_vec();
        let mut bytes = Vec::new();
        let mut encoder = image::codecs::png::PngEncoder::new(&mut bytes);
        encoder
            .set_exif_metadata(std::mem::take(&mut exif))
            .unwrap();
        encoder
            .write_image(
                &[255, 0, 0, 255, 0, 255, 0, 255],
                2,
                1,
                image::ExtendedColorType::Rgba8,
            )
            .unwrap();
        let decoded = RasterImage::decode(&bytes).unwrap();
        assert_eq!(decoded.size(), [1., 2.]);
        assert_eq!(decoded.rgba(), &[255, 0, 0, 255, 0, 255, 0, 255]);
    }
}
