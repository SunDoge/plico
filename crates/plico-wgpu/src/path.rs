//! Bounded CPU coverage fallback for uncommon paths; text stays native.
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
        let gradient = match brush {
            Brush::Solid(_) => None,
            Brush::Gradient(g) => Some(LinearPaint::new(g)?),
            _ => return Err(unsupported("image general path brush")),
        };
        let color = match brush {
            Brush::Solid(c) => *c,
            _ => peniko::Color::WHITE,
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
        if let Some(g) = &gradient {
            for v in [g.start.x, g.start.y, g.direction.x, g.direction.y] {
                v.to_bits().hash(&mut hash);
            }
            std::mem::discriminant(&g.extend).hash(&mut hash);
            g.premultiplied.hash(&mut hash);
            for (offset, color) in &g.stops {
                offset.to_bits().hash(&mut hash);
                for v in color {
                    v.to_bits().hash(&mut hash);
                }
            }
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
        if let Some(gradient) = &gradient {
            let inverse = local.inverse();
            for (i, pixel) in pixmap
                .data_mut()
                .as_chunks_mut::<4>()
                .0
                .iter_mut()
                .enumerate()
            {
                let coverage = pixel[3] as f32 / 255.;
                if coverage == 0. {
                    continue;
                }
                let point = inverse
                    * kurbo::Point::new(
                        (i % width as usize) as f64 + 0.5,
                        (i / width as usize) as f64 + 0.5,
                    );
                let color = gradient.sample(point);
                for channel in 0..4 {
                    pixel[channel] =
                        (color[channel] * coverage * 255.).round().clamp(0., 255.) as u8;
                }
            }
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

// Coverage and color are evaluated separately so both straight and premultiplied
// interpolation retain the shared brush contract, including transparent stops.
struct LinearPaint {
    start: kurbo::Point,
    direction: kurbo::Vec2,
    stops: Vec<(f32, [f32; 4])>,
    extend: peniko::Extend,
    premultiplied: bool,
}
impl LinearPaint {
    fn new(g: &peniko::Gradient) -> Result<Self> {
        let peniko::GradientKind::Linear(line) = g.kind else {
            return Err(unsupported("nonlinear general path gradient"));
        };
        if g.interpolation_cs != peniko::color::ColorSpaceTag::Srgb {
            return Err(unsupported(
                "general path gradient requires sRGB interpolation",
            ));
        }
        let direction = line.end - line.start;
        if ![line.start.x, line.start.y, direction.x, direction.y]
            .iter()
            .all(|v| v.is_finite())
            || direction.hypot2() <= 1e-12
        {
            return Err(invalid("invalid gradient endpoints"));
        }
        let stops: Vec<_> = g
            .stops
            .iter()
            .map(|s| {
                (
                    s.offset,
                    s.color.to_alpha_color::<peniko::color::Srgb>().components,
                )
            })
            .collect();
        if stops.is_empty()
            || stops.windows(2).any(|s| s[0].0 > s[1].0)
            || stops.iter().any(|(offset, c)| {
                !offset.is_finite()
                    || !(0. ..=1.).contains(offset)
                    || c.iter().any(|v| !v.is_finite() || !(0. ..=1.).contains(v))
            })
        {
            return Err(invalid("invalid gradient stops"));
        }
        Ok(Self {
            start: line.start,
            direction,
            stops,
            extend: g.extend,
            premultiplied: g.interpolation_alpha_space
                == peniko::InterpolationAlphaSpace::Premultiplied,
        })
    }
    fn sample(&self, point: kurbo::Point) -> [f32; 4] {
        let t = ((point - self.start).dot(self.direction) / self.direction.hypot2()) as f32;
        let t = match self.extend {
            peniko::Extend::Pad => t.clamp(0., 1.),
            peniko::Extend::Repeat => t.rem_euclid(1.),
            peniko::Extend::Reflect => 1. - (t.rem_euclid(2.) - 1.).abs(),
        };
        let upper = self.stops.partition_point(|s| s.0 <= t);
        let (a, b, u) = if upper == 0 {
            (self.stops[0].1, self.stops[0].1, 0.)
        } else if upper == self.stops.len() {
            let c = self.stops[upper - 1].1;
            (c, c, 0.)
        } else {
            let (x, a) = self.stops[upper - 1];
            let (y, b) = self.stops[upper];
            (a, b, (t - x) / (y - x))
        };
        let alpha = a[3] + (b[3] - a[3]) * u;
        let mut out = [0.; 4];
        for i in 0..3 {
            out[i] = if self.premultiplied {
                a[i] * a[3] * (1. - u) + b[i] * b[3] * u
            } else {
                (a[i] + (b[i] - a[i]) * u) * alpha
            };
        }
        out[3] = alpha;
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use peniko::{Color, Gradient, InterpolationAlphaSpace};
    #[test]
    fn linear_path_gradient_preserves_alpha_interpolation_and_extend() {
        let base = Gradient::new_linear((0., 0.), (10., 0.)).with_stops([
            Color::from_rgba8(255, 0, 0, 0),
            Color::from_rgba8(0, 0, 255, 255),
        ]);
        let premult = LinearPaint::new(
            &base
                .clone()
                .with_interpolation_alpha_space(InterpolationAlphaSpace::Premultiplied),
        )
        .unwrap();
        let straight = LinearPaint::new(
            &base
                .clone()
                .with_interpolation_alpha_space(InterpolationAlphaSpace::Unpremultiplied),
        )
        .unwrap();
        assert_eq!(premult.sample((5., 0.).into()), [0., 0., 0.5, 0.5]);
        assert_eq!(straight.sample((5., 0.).into()), [0.25, 0., 0.25, 0.5]);
        let mut repeated = base.clone();
        repeated.extend = peniko::Extend::Repeat;
        let repeated = LinearPaint::new(&repeated).unwrap();
        assert_eq!(
            repeated.sample((-5., 0.).into()),
            repeated.sample((5., 0.).into())
        );
        let mut reflected = base;
        reflected.extend = peniko::Extend::Reflect;
        let reflected = LinearPaint::new(&reflected).unwrap();
        assert_eq!(
            reflected.sample((15., 0.).into()),
            reflected.sample((5., 0.).into())
        );
    }
    #[test]
    fn multiple_and_duplicate_stops_preserve_hard_transitions() {
        let gradient = Gradient::new_linear((0., 0.), (10., 0.)).with_stops([
            (0., Color::BLACK),
            (0.5, Color::WHITE),
            (0.5, Color::BLACK),
            (1., Color::WHITE),
        ]);
        let paint = LinearPaint::new(&gradient).unwrap();
        assert_eq!(paint.sample((2.5, 0.).into()), [0.5, 0.5, 0.5, 1.]);
        assert_eq!(paint.sample((5., 0.).into()), [0., 0., 0., 1.]);
        assert_eq!(paint.sample((7.5, 0.).into()), [0.5, 0.5, 0.5, 1.]);
    }
    #[test]
    fn gradient_path_cache_reuses_integer_motion_but_not_changed_stops() {
        use kurbo::Shape;
        let mut cache = PathCache::default();
        let gradient =
            Gradient::new_linear((0., 0.), (16., 0.)).with_stops([Color::BLACK, Color::WHITE]);
        let mut scene = plico_renderer::Scene::new();
        use plico_renderer::PaintScene;
        scene.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            &gradient,
            None,
            &Rect::new(0., 0., 16., 16.).to_path(0.1),
        );
        let command = &scene.commands[0];
        let (first, _) = cache.raster(command).unwrap().unwrap();
        let mut moved = command.clone();
        if let PaintCommand::Fill { transform, .. } = &mut moved {
            *transform = Affine::translate((10., 20.));
        }
        let (second, _) = cache.raster(&moved).unwrap().unwrap();
        assert_eq!(first.data.id(), second.data.id());
        let mut changed = command.clone();
        if let PaintCommand::Fill { brush, .. } = &mut changed {
            *brush = Gradient::new_linear((0., 0.), (16., 0.))
                .with_stops([Color::WHITE, Color::BLACK])
                .into();
        }
        let (third, _) = cache.raster(&changed).unwrap().unwrap();
        assert_ne!(first.data.id(), third.data.id());
        assert_eq!(cache.entries.len(), 2);
    }
}
