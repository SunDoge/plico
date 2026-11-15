//! Bounded Swash raster fallback for externally shaped glyph runs.
//!
//! Parley already selected the exact font, glyph IDs and positions. Native
//! paragraph APIs cannot reconstruct that run without reshaping it, so the
//! non-Linux desktop backends raster the retained run directly from its font
//! bytes. This is also independent of fonts installed on the host machine.

use crate::{Error, Result};
use kurbo::Rect;
use peniko::{Blob, ImageAlphaType, ImageData, ImageFormat};
use plico_renderer::{Glyph, PaintCommand};
use std::{
    hash::{Hash, Hasher},
    num::NonZeroUsize,
    sync::Arc,
};
use swash::{
    FontRef,
    scale::{Render, ScaleContext, Source, StrikeWith, image::Content},
    zeno::{Format, Transform, Vector},
};

#[derive(Clone, PartialEq)]
struct Key {
    font: u64,
    index: u32,
    size: f32,
    coordinates: Vec<i16>,
    glyph_transform: [f32; 6],
    embolden: f32,
    hint: bool,
    color: [f32; 4],
    glyphs: Vec<Glyph>,
}

struct Raster {
    key: Key,
    image: ImageData,
    origin: [i32; 2],
    bytes: usize,
}

pub(crate) struct GlyphCache {
    context: ScaleContext,
    rasters: lru::LruCache<u64, Raster>,
    pixel_bytes: usize,
}

impl Default for GlyphCache {
    fn default() -> Self {
        Self {
            context: ScaleContext::new(),
            rasters: lru::LruCache::new(NonZeroUsize::new(256).unwrap()),
            pixel_bytes: 0,
        }
    }
}

fn unsupported(reason: &str) -> Error {
    Error::Unsupported {
        command: 0,
        reason: reason.into(),
    }
}

fn byte(value: f32) -> u8 {
    (value.clamp(0., 1.) * 255.).round() as u8
}

fn over(dst: &mut [u8], src: [u8; 4]) {
    let inverse = 255_u16 - u16::from(src[3]);
    for channel in 0..3 {
        dst[channel] = (u16::from(src[channel]) + (u16::from(dst[channel]) * inverse + 127) / 255)
            .min(255) as u8;
    }
    dst[3] = (u16::from(src[3]) + (u16::from(dst[3]) * inverse + 127) / 255).min(255) as u8;
}

impl GlyphCache {
    pub fn stats(&self) -> (usize, usize, usize, usize) {
        (0, 0, self.rasters.len(), self.pixel_bytes)
    }

    pub fn raster(&mut self, command: &PaintCommand) -> Result<Option<(ImageData, Rect)>> {
        let PaintCommand::GlyphRun {
            font,
            font_size,
            hint,
            normalized_coords,
            embolden,
            style,
            brush,
            brush_alpha,
            transform,
            glyph_transform,
            glyphs,
        } = command
        else {
            return Err(unsupported("expected positioned glyphs"));
        };
        if glyphs.is_empty() {
            return Ok(None);
        }
        if glyphs.len() > 100_000
            || font.data.len() > 1024 * 1024 * 1024
            || normalized_coords.len() > 64
            || glyphs.iter().any(|glyph| {
                glyph.id > u32::from(u16::MAX) || !glyph.x.is_finite() || !glyph.y.is_finite()
            })
        {
            return Err(Error::InvalidInput {
                reason: "positioned glyph resource limit exceeded".into(),
            });
        }
        if !matches!(style, peniko::Style::Fill(_)) {
            return Err(unsupported("positioned glyph stroke"));
        }
        let peniko::Brush::Solid(color) = brush else {
            return Err(unsupported("non-solid positioned glyph brush"));
        };
        let [a, b, c, d, x, y] = transform.as_coeffs();
        let scale = a.hypot(b);
        if ![a, b, c, d, x, y, scale, *brush_alpha as f64]
            .iter()
            .all(|value| value.is_finite())
            || scale <= f64::EPSILON
            || !(0. ..=1.).contains(brush_alpha)
            || !font_size.is_finite()
            || *font_size <= 0.
            || !embolden.x.is_finite()
            || !embolden.y.is_finite()
            || embolden.x < 0.
            || embolden.y < 0.
            || color.components.iter().any(|value| !value.is_finite())
        {
            return Err(Error::InvalidInput {
                reason: "invalid positioned glyph transform or alpha".into(),
            });
        }
        let shift = [x.floor(), y.floor()];
        let residual = kurbo::Affine::new([a / scale, b / scale, c / scale, d / scale, 0., 0.])
            * glyph_transform.unwrap_or(kurbo::Affine::IDENTITY);
        let [ra, rb, rc, rd, re, rf] = residual.as_coeffs();
        let color = {
            let mut value = color.components;
            value[3] *= *brush_alpha;
            value
        };
        let key = Key {
            font: font.data.id(),
            index: font.index,
            size: *font_size * scale as f32,
            coordinates: normalized_coords.to_vec(),
            glyph_transform: [
                ra as f32, rb as f32, rc as f32, rd as f32, re as f32, rf as f32,
            ],
            embolden: ((embolden.x + embolden.y) * 0.5 * scale) as f32,
            hint: *hint,
            color,
            glyphs: glyphs
                .iter()
                .map(|glyph| Glyph {
                    id: glyph.id,
                    x: (a * f64::from(glyph.x) + c * f64::from(glyph.y) + x - shift[0]) as f32,
                    y: (b * f64::from(glyph.x) + d * f64::from(glyph.y) + y - shift[1]) as f32,
                })
                .collect(),
        };
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        key.font.hash(&mut hasher);
        key.index.hash(&mut hasher);
        key.size.to_bits().hash(&mut hasher);
        key.coordinates.hash(&mut hasher);
        key.hint.hash(&mut hasher);
        for value in key.glyph_transform.into_iter().chain([key.embolden]) {
            value.to_bits().hash(&mut hasher);
        }
        for value in key.color {
            value.to_bits().hash(&mut hasher);
        }
        for glyph in &key.glyphs {
            glyph.id.hash(&mut hasher);
            glyph.x.to_bits().hash(&mut hasher);
            glyph.y.to_bits().hash(&mut hasher);
        }
        let id = hasher.finish();
        let bounds = |origin: [i32; 2], image: &ImageData| {
            Rect::new(
                shift[0] + f64::from(origin[0]),
                shift[1] + f64::from(origin[1]),
                shift[0] + f64::from(origin[0]) + f64::from(image.width),
                shift[1] + f64::from(origin[1]) + f64::from(image.height),
            )
        };
        if let Some(raster) = self.rasters.get(&id)
            && raster.key == key
        {
            return Ok(Some((
                raster.image.clone(),
                bounds(raster.origin, &raster.image),
            )));
        }

        let font_ref = FontRef::from_index(font.data.as_ref(), font.index as usize)
            .ok_or_else(|| unsupported("invalid positioned glyph font"))?;
        let mut scaler = self
            .context
            .builder(font_ref)
            .size(key.size)
            .hint(key.hint)
            .normalized_coords(key.coordinates.iter().copied())
            .build();
        let transform = Transform::new(
            key.glyph_transform[0],
            key.glyph_transform[1],
            key.glyph_transform[2],
            key.glyph_transform[3],
            key.glyph_transform[4],
            key.glyph_transform[5],
        );
        let sources = [
            Source::ColorOutline(0),
            Source::ColorBitmap(StrikeWith::BestFit),
            Source::Outline,
        ];
        let mut rendered = Vec::with_capacity(key.glyphs.len());
        let mut bounds_i = [i32::MAX, i32::MAX, i32::MIN, i32::MIN];
        for glyph in &key.glyphs {
            let base_x = glyph.x.floor() as i32;
            let base_y = glyph.y.floor() as i32;
            let mut render = Render::new(&sources);
            render
                .format(Format::Alpha)
                .offset(Vector::new(glyph.x.fract(), glyph.y.fract()))
                .transform(Some(transform))
                .embolden(key.embolden.max(0.));
            let Some(image) = render.render(&mut scaler, glyph.id as u16) else {
                continue;
            };
            let left = base_x + image.placement.left;
            let top = base_y - image.placement.top;
            let right = left + image.placement.width as i32;
            let bottom = top + image.placement.height as i32;
            bounds_i[0] = bounds_i[0].min(left);
            bounds_i[1] = bounds_i[1].min(top);
            bounds_i[2] = bounds_i[2].max(right);
            bounds_i[3] = bounds_i[3].max(bottom);
            rendered.push((image, left, top));
        }
        if rendered.is_empty() || bounds_i[2] <= bounds_i[0] || bounds_i[3] <= bounds_i[1] {
            return Ok(None);
        }
        let width = (bounds_i[2] - bounds_i[0]) as u32;
        let height = (bounds_i[3] - bounds_i[1]) as u32;
        let bytes = width as usize * height as usize * 4;
        if width > 16_384 || height > 16_384 || bytes > 64 * 1024 * 1024 {
            return Err(Error::InvalidInput {
                reason: "positioned glyph raster exceeds 64 MiB".into(),
            });
        }
        let mut rgba = vec![0_u8; bytes];
        for (image, left, top) in rendered {
            for row in 0..image.placement.height as usize {
                for column in 0..image.placement.width as usize {
                    let source_index = row * image.placement.width as usize + column;
                    let source = match image.content {
                        Content::Mask => {
                            let alpha = f32::from(image.data[source_index]) / 255. * key.color[3];
                            let alpha_byte = byte(alpha);
                            [
                                byte(key.color[0] * alpha),
                                byte(key.color[1] * alpha),
                                byte(key.color[2] * alpha),
                                alpha_byte,
                            ]
                        }
                        Content::Color => {
                            let source_index = source_index * 4;
                            let alpha =
                                f32::from(image.data[source_index + 3]) / 255. * key.color[3];
                            [
                                byte(f32::from(image.data[source_index]) / 255. * alpha),
                                byte(f32::from(image.data[source_index + 1]) / 255. * alpha),
                                byte(f32::from(image.data[source_index + 2]) / 255. * alpha),
                                byte(alpha),
                            ]
                        }
                        Content::SubpixelMask => continue,
                    };
                    let x = (left - bounds_i[0]) as usize + column;
                    let y = (top - bounds_i[1]) as usize + row;
                    over(&mut rgba[(y * width as usize + x) * 4..][..4], source);
                }
            }
        }
        let image = ImageData {
            data: Blob::new(Arc::new(rgba)),
            format: ImageFormat::Rgba8,
            alpha_type: ImageAlphaType::AlphaPremultiplied,
            width,
            height,
        };
        while self.pixel_bytes + bytes > 64 * 1024 * 1024
            || self.rasters.len() == self.rasters.cap().get()
        {
            let Some((_, old)) = self.rasters.pop_lru() else {
                break;
            };
            self.pixel_bytes -= old.bytes;
        }
        let origin = [bounds_i[0], bounds_i[1]];
        let rect = bounds(origin, &image);
        if bytes <= 64 * 1024 * 1024 {
            if let Some(old) = self.rasters.put(
                id,
                Raster {
                    key,
                    image: image.clone(),
                    origin,
                    bytes,
                },
            ) {
                self.pixel_bytes -= old.bytes;
            }
            self.pixel_bytes += bytes;
        }
        Ok(Some((image, rect)))
    }
}
