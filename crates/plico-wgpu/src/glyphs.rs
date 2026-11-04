//! Bounded native raster fallback for externally shaped glyph runs.
use crate::{Error, Result};
use kurbo::Rect;
use peniko::{Blob, ImageAlphaType, ImageData, ImageFormat};
use plico_native::text::positioned::{Font, Glyph};
use plico_renderer::PaintCommand;
use std::{
    hash::{Hash, Hasher},
    num::NonZeroUsize,
    sync::Arc,
};
#[derive(PartialEq)]
struct Key {
    font: u64,
    index: u32,
    size: f64,
    coordinates: Vec<i16>,
    glyph_transform: [f64; 6],
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
    fonts: lru::LruCache<(u64, u32, Vec<i16>), (Font, usize)>,
    rasters: lru::LruCache<u64, Raster>,
    font_bytes: usize,
    sources: std::collections::HashMap<u64, (usize, usize)>,
    pixel_bytes: usize,
}
impl Default for GlyphCache {
    fn default() -> Self {
        Self {
            fonts: lru::LruCache::new(NonZeroUsize::new(32).unwrap()),
            rasters: lru::LruCache::new(NonZeroUsize::new(256).unwrap()),
            font_bytes: 0,
            sources: Default::default(),
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
impl GlyphCache {
    pub fn stats(&self) -> (usize, usize, usize, usize) {
        (
            self.fonts.len(),
            self.font_bytes,
            self.rasters.len(),
            self.pixel_bytes,
        )
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
        if normalized_coords.len() > 64
            || normalized_coords
                .iter()
                .any(|v| !(-16384..=16384).contains(v))
        {
            return Err(Error::InvalidInput {
                reason:
                    "positioned glyph coordinates require at most 64 normalized axes in [-1, 1]"
                        .into(),
            });
        }
        if glyphs.len() > 100_000 || font.data.len() > 1024 * 1024 * 1024 {
            return Err(Error::InvalidInput {
                reason: format!(
                    "positioned glyph/font resource limit: {} glyphs, {} font bytes",
                    glyphs.len(),
                    font.data.len()
                ),
            });
        }
        if *embolden != kurbo::Vec2::ZERO || !matches!(style, peniko::Style::Fill(_)) {
            return Err(unsupported("positioned glyph emboldening or stroke"));
        }
        let peniko::Brush::Solid(color) = brush else {
            return Err(unsupported("non-solid positioned glyph brush"));
        };
        if !brush_alpha.is_finite() || !(0. ..=1.).contains(brush_alpha) {
            return Err(unsupported("invalid glyph brush alpha"));
        }
        let [a, b, c, d, x, y] = transform.as_coeffs();
        let determinant = a * d - b * c;
        if ![a, b, c, d, x, y, determinant]
            .iter()
            .all(|v| v.is_finite())
            || determinant.abs() < 1e-12
        {
            return Err(Error::InvalidInput {
                reason: "positioned glyph run has a nonfinite or singular transform".into(),
            });
        }
        let scale = a.hypot(b);
        let shift = [x.floor(), y.floor()];
        let mut color = color.components;
        color[3] *= brush_alpha;
        let key = Key {
            font: font.data.id(),
            index: font.index,
            size: *font_size as f64 * scale,
            coordinates: normalized_coords.clone(),
            glyph_transform: {
                let combined =
                    kurbo::Affine::new([a / scale, b / scale, c / scale, d / scale, 0., 0.])
                        * glyph_transform.unwrap_or(kurbo::Affine::IDENTITY);
                let [ga, gb, gc, gd, ge, gf] = combined.as_coeffs();
                [ga, gb, gc, gd, ge * scale, gf * scale]
            },
            hint: *hint,
            color,
            glyphs: glyphs
                .iter()
                .map(|g| Glyph {
                    id: g.id,
                    x: a * g.x as f64 + c * g.y as f64 + x - shift[0],
                    y: b * g.x as f64 + d * g.y as f64 + y - shift[1],
                })
                .collect(),
        };
        let mut hash = std::collections::hash_map::DefaultHasher::new();
        key.font.hash(&mut hash);
        key.index.hash(&mut hash);
        key.size.to_bits().hash(&mut hash);
        key.hint.hash(&mut hash);
        key.coordinates.hash(&mut hash);
        for v in key.glyph_transform {
            v.to_bits().hash(&mut hash);
        }
        for v in key.color {
            v.to_bits().hash(&mut hash);
        }
        for g in &key.glyphs {
            g.id.hash(&mut hash);
            g.x.to_bits().hash(&mut hash);
            g.y.to_bits().hash(&mut hash);
        }
        let id = hash.finish();
        let bounds = |origin: [i32; 2], image: &ImageData| {
            Rect::new(
                shift[0] + origin[0] as f64,
                shift[1] + origin[1] as f64,
                shift[0] + origin[0] as f64 + image.width as f64,
                shift[1] + origin[1] as f64 + image.height as f64,
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
        let font_id = (font.data.id(), font.index, normalized_coords.clone());
        if !self.fonts.contains(&font_id) {
            let bytes = font.data.len();
            let native = Font::from_shared_variations(
                std::rc::Rc::new(font.data.clone()),
                font.index,
                normalized_coords,
            )
            .map_err(|source| Error::Text { source })?;
            let added = if self.sources.contains_key(&font_id.0) {
                0
            } else {
                bytes
            };
            while self.font_bytes + added > 1024 * 1024 * 1024
                || self.fonts.len() == self.fonts.cap().get()
            {
                let Some(((source, _, _), _)) = self.fonts.pop_lru() else {
                    break;
                };
                let info = self.sources.get_mut(&source).unwrap();
                info.0 -= 1;
                if info.0 == 0 {
                    self.font_bytes -= info.1;
                    self.sources.remove(&source);
                }
            }
            let source = self.sources.entry(font_id.0).or_insert_with(|| {
                self.font_bytes += bytes;
                (0, bytes)
            });
            source.0 += 1;
            self.fonts.put(font_id.clone(), (native, bytes));
        }
        let Some(bitmap) = self
            .fonts
            .get(&font_id)
            .unwrap()
            .0
            .raster_transformed(
                &key.glyphs,
                key.size,
                key.hint,
                key.color,
                key.glyph_transform,
            )
            .map_err(|source| Error::Text { source })?
        else {
            return Ok(None);
        };
        let bytes = bitmap.rgba.len()
            + key.glyphs.len() * std::mem::size_of::<Glyph>()
            + key.coordinates.len() * std::mem::size_of::<i16>();
        while self.pixel_bytes + bytes > 64 * 1024 * 1024
            || self.rasters.len() == self.rasters.cap().get()
        {
            let Some((_, old)) = self.rasters.pop_lru() else {
                break;
            };
            self.pixel_bytes -= old.bytes;
        }
        let image = ImageData {
            data: Blob::new(Arc::new(bitmap.rgba)),
            format: ImageFormat::Rgba8,
            alpha_type: ImageAlphaType::AlphaPremultiplied,
            width: bitmap.size[0],
            height: bitmap.size[1],
        };
        let rect = bounds(bitmap.origin, &image);
        if bytes > 64 * 1024 * 1024 {
            return Ok(Some((image, rect)));
        }
        if let Some(old) = self.rasters.put(
            id,
            Raster {
                key,
                image: image.clone(),
                origin: bitmap.origin,
                bytes,
            },
        ) {
            self.pixel_bytes -= old.bytes;
        }
        self.pixel_bytes += bytes;
        Ok(Some((image, rect)))
    }
}
