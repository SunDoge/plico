//! Bounded CPU coverage fallback for uncommon solid paths; text stays native.
use crate::{Error, Result};
use kurbo::{Affine, PathEl, Rect, Shape};
use peniko::{Blob, Brush, Fill, ImageAlphaType, ImageData, ImageFormat};
use plico_renderer::PaintCommand;
use resvg::tiny_skia as sk;
use std::{
    hash::{Hash, Hasher},
    num::NonZeroUsize,
    sync::Arc,
};
struct Raster {
    command: PaintCommand,
    image: ImageData,
    bytes: usize,
}
pub(crate) struct PathCache {
    entries: lru::LruCache<u64, Raster>,
    bytes: usize,
}
impl Default for PathCache {
    fn default() -> Self {
        Self {
            entries: lru::LruCache::new(NonZeroUsize::new(256).unwrap()),
            bytes: 0,
        }
    }
}
fn invalid(reason: &str) -> Error {
    Error::InvalidInput {
        reason: reason.into(),
    }
}
fn unsupported(reason: &str) -> Error {
    Error::Unsupported {
        command: 0,
        reason: reason.into(),
    }
}
impl PathCache {
    pub fn raster(&mut self, command: &PaintCommand) -> Result<Option<(ImageData, Rect)>> {
        let (shape, transform, brush, brush_transform, stroke) = match command {
            PaintCommand::Fill {
                shape,
                transform,
                brush,
                brush_transform,
                ..
            } => (shape, *transform, brush, brush_transform, None),
            PaintCommand::Stroke {
                shape,
                transform,
                brush,
                brush_transform,
                style,
            } => (shape, *transform, brush, brush_transform, Some(style)),
            _ => return Err(unsupported("path fallback command")),
        };
        let Brush::Solid(color) = brush else {
            return Err(unsupported("non-solid general path brush"));
        };
        if brush_transform.is_some_and(|t| t != Affine::IDENTITY) {
            return Err(unsupported("transformed general path brush"));
        }
        let [a, b, c, d, e, f] = transform.as_coeffs();
        if ![a, b, c, d, e, f].iter().all(|v| v.is_finite()) || (a * d - b * c).abs() < 1e-12 {
            return Err(invalid("nonfinite or singular path transform"));
        }
        let mut builder = sk::PathBuilder::new();
        let mut hash = std::collections::hash_map::DefaultHasher::new();
        for el in shape.elements() {
            let points: &[kurbo::Point] = match el {
                PathEl::MoveTo(p) | PathEl::LineTo(p) => std::slice::from_ref(p),
                PathEl::QuadTo(p, q) => &[*p, *q],
                PathEl::CurveTo(p, q, r) => &[*p, *q, *r],
                PathEl::ClosePath => &[],
            };
            for p in points {
                if !p.x.is_finite() || !p.y.is_finite() || p.x.abs() > 1e7 || p.y.abs() > 1e7 {
                    return Err(invalid("path coordinate exceeds fallback precision limit"));
                }
                p.x.to_bits().hash(&mut hash);
                p.y.to_bits().hash(&mut hash);
            }
            std::mem::discriminant(el).hash(&mut hash);
            match *el {
                PathEl::MoveTo(p) => builder.move_to(p.x as f32, p.y as f32),
                PathEl::LineTo(p) => builder.line_to(p.x as f32, p.y as f32),
                PathEl::QuadTo(p, q) => {
                    builder.quad_to(p.x as f32, p.y as f32, q.x as f32, q.y as f32)
                }
                PathEl::CurveTo(p, q, r) => builder.cubic_to(
                    p.x as f32, p.y as f32, q.x as f32, q.y as f32, r.x as f32, r.y as f32,
                ),
                PathEl::ClosePath => builder.close(),
            }
        }
        let Some(path) = builder.finish() else {
            return Ok(None);
        };
        let mut bounds = transform.transform_rect_bbox(shape.bounding_box());
        if let Some(stroke) = stroke {
            if !stroke.width.is_finite()
                || stroke.width < 0.
                || !stroke.miter_limit.is_finite()
                || stroke.miter_limit < 1.
            {
                return Err(invalid("invalid stroke width/miter limit"));
            }
            if stroke.width == 0. {
                return Ok(None);
            }
            if stroke.start_cap != stroke.end_cap {
                return Err(unsupported("different start/end stroke caps"));
            }
            let reach = stroke.width * 0.5 * (a.hypot(b) + c.hypot(d)) * stroke.miter_limit.max(2.);
            bounds = bounds.inflate(reach, reach);
        }
        if bounds.width() <= 0. || bounds.height() <= 0. {
            return Ok(None);
        }
        bounds = Rect::new(
            bounds.x0.floor() - 2.,
            bounds.y0.floor() - 2.,
            bounds.x1.ceil() + 2.,
            bounds.y1.ceil() + 2.,
        );
        let (width, height) = (bounds.width(), bounds.height());
        if ![bounds.x0, bounds.y0, width, height]
            .iter()
            .all(|v| v.is_finite())
            || width > 16384.
            || height > 16384.
            || width * height * 4. > 64. * 1024. * 1024.
        {
            return Err(invalid("path raster exceeds dimensions/64 MiB limit"));
        }
        let local = Affine::new([a, b, c, d, e - bounds.x0, f - bounds.y0]);
        for v in local.as_coeffs() {
            v.to_bits().hash(&mut hash);
        }
        for v in color.components {
            v.to_bits().hash(&mut hash);
        }
        let mut normalized = command.clone();
        match &mut normalized {
            PaintCommand::Fill { transform, .. } | PaintCommand::Stroke { transform, .. } => {
                *transform = local
            }
            _ => unreachable!(),
        }
        std::mem::discriminant(command).hash(&mut hash);
        match command {
            PaintCommand::Fill { fill, .. } => std::mem::discriminant(fill).hash(&mut hash),
            PaintCommand::Stroke { style, .. } => {
                style.width.to_bits().hash(&mut hash);
                style.miter_limit.to_bits().hash(&mut hash);
                style.dash_offset.to_bits().hash(&mut hash);
                std::mem::discriminant(&style.start_cap).hash(&mut hash);
                std::mem::discriminant(&style.end_cap).hash(&mut hash);
                std::mem::discriminant(&style.join).hash(&mut hash);
                for value in &style.dash_pattern {
                    value.to_bits().hash(&mut hash);
                }
            }
            _ => unreachable!(),
        }
        let key = hash.finish();
        if let Some(entry) = self.entries.get(&key)
            && entry.command == normalized
        {
            return Ok(Some((entry.image.clone(), bounds)));
        }
        let mut pixmap = sk::Pixmap::new(width as u32, height as u32)
            .ok_or_else(|| invalid("path raster allocation failed"))?;
        let mut paint = sk::Paint::default();
        let [r, g, b, a] = color.components;
        paint.set_color(
            sk::Color::from_rgba(r, g, b, a).ok_or_else(|| invalid("path color outside 0..=1"))?,
        );
        let [a, b, c, d, e, f] = local.as_coeffs();
        let transform =
            sk::Transform::from_row(a as f32, b as f32, c as f32, d as f32, e as f32, f as f32);
        if let Some(style) = stroke {
            let dash = if style.dash_pattern.is_empty() {
                None
            } else {
                if style.dash_pattern.len() % 2 != 0 {
                    return Err(unsupported("odd-length stroke dash pattern"));
                }
                Some(
                    sk::StrokeDash::new(
                        style.dash_pattern.iter().map(|v| *v as f32).collect(),
                        style.dash_offset as f32,
                    )
                    .ok_or_else(|| invalid("invalid stroke dash pattern"))?,
                )
            };
            let stroke = sk::Stroke {
                width: style.width as f32,
                miter_limit: style.miter_limit as f32,
                line_cap: match style.start_cap {
                    kurbo::Cap::Butt => sk::LineCap::Butt,
                    kurbo::Cap::Round => sk::LineCap::Round,
                    kurbo::Cap::Square => sk::LineCap::Square,
                },
                line_join: match style.join {
                    kurbo::Join::Miter => sk::LineJoin::Miter,
                    kurbo::Join::Round => sk::LineJoin::Round,
                    kurbo::Join::Bevel => sk::LineJoin::Bevel,
                },
                dash,
            };
            pixmap.stroke_path(&path, &paint, &stroke, transform, None);
        } else {
            let rule = match command {
                PaintCommand::Fill {
                    fill: Fill::EvenOdd,
                    ..
                } => sk::FillRule::EvenOdd,
                _ => sk::FillRule::Winding,
            };
            pixmap.fill_path(&path, &paint, rule, transform, None);
        }
        let bytes = pixmap.data().len();
        let image = ImageData {
            data: Blob::new(Arc::new(pixmap.take())),
            format: ImageFormat::Rgba8,
            alpha_type: ImageAlphaType::AlphaPremultiplied,
            width: width as u32,
            height: height as u32,
        };
        while self.bytes + bytes > 64 * 1024 * 1024
            || self.entries.len() == self.entries.cap().get()
        {
            let Some((_, entry)) = self.entries.pop_lru() else {
                break;
            };
            self.bytes -= entry.bytes;
        }
        if let Some(old) = self.entries.put(
            key,
            Raster {
                command: normalized,
                image: image.clone(),
                bytes,
            },
        ) {
            self.bytes -= old.bytes;
        }
        self.bytes += bytes;
        Ok(Some((image, bounds)))
    }
}
