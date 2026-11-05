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
    pub fn stats(&self) -> (usize, usize) {
        (self.entries.len(), self.bytes)
    }
    pub fn raster(&mut self, command: &PaintCommand) -> Result<Option<(ImageData, Rect)>> {
        self.raster_clipped(
            command,
            Rect::new(
                f64::NEG_INFINITY,
                f64::NEG_INFINITY,
                f64::INFINITY,
                f64::INFINITY,
            ),
        )
    }
    /// Raster with a visible-frame clip: allocations cover the intersection
    /// of the path bounds and `viewport` instead of the whole path. Scenes
    /// pass their root clip so large off-screen geometry cannot exceed the
    /// 64 MiB limit when only a small part is visible.
    pub fn raster_clipped(
        &mut self,
        command: &PaintCommand,
        viewport: Rect,
    ) -> Result<Option<(ImageData, Rect)>> {
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
            Brush::Gradient(g) => Some(GradientPaint::new(g)?),
            _ => return Err(unsupported("image general path brush")),
        };
        let color = match brush {
            Brush::Solid(c) => *c,
            _ => peniko::Color::WHITE,
        };
        let brush_matrix = brush_transform.unwrap_or(Affine::IDENTITY);
        if brush_matrix != Affine::IDENTITY && gradient.is_none() {
            return Err(unsupported("transformed general path brush"));
        }
        if gradient.is_some()
            && (!brush_matrix.as_coeffs().iter().all(|v| v.is_finite())
                || !brush_matrix.determinant().is_finite()
                || brush_matrix.determinant().abs() < 1e-12)
        {
            return Err(invalid("nonfinite or singular gradient brush transform"));
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
        let clipped = bounds.intersect(viewport);
        if clipped.width() <= 0. || clipped.height() <= 0. {
            return Ok(None);
        }
        // A coverage image lives on the physical pixel grid, including when
        // its viewport originates at a fractional HiDPI/filter boundary.
        bounds = Rect::new(
            clipped.x0.floor(),
            clipped.y0.floor(),
            clipped.x1.ceil(),
            clipped.y1.ceil(),
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
        // Cropping the right/bottom edge changes the image but not the local
        // transform. Extents therefore participate in cache identity as well.
        width.to_bits().hash(&mut hash);
        height.to_bits().hash(&mut hash);
        let local = Affine::new([a, b, c, d, e - bounds.x0, f - bounds.y0]);
        for v in local.as_coeffs() {
            v.to_bits().hash(&mut hash);
        }
        if let Some(g) = &gradient {
            for v in brush_matrix.as_coeffs() {
                v.to_bits().hash(&mut hash);
            }
            std::mem::discriminant(&g.kind).hash(&mut hash);
            match g.kind {
                peniko::GradientKind::Linear(line) => {
                    for v in [line.start.x, line.start.y, line.end.x, line.end.y] {
                        v.to_bits().hash(&mut hash);
                    }
                }
                peniko::GradientKind::Radial(radial) => {
                    for v in [
                        radial.start_center.x,
                        radial.start_center.y,
                        radial.end_center.x,
                        radial.end_center.y,
                        f64::from(radial.start_radius),
                        f64::from(radial.end_radius),
                    ] {
                        v.to_bits().hash(&mut hash);
                    }
                }
                peniko::GradientKind::Sweep(sweep) => {
                    for v in [
                        sweep.center.x,
                        sweep.center.y,
                        f64::from(sweep.start_angle),
                        f64::from(sweep.end_angle),
                    ] {
                        v.to_bits().hash(&mut hash);
                    }
                }
            }
            std::mem::discriminant(&g.extend).hash(&mut hash);
            g.premultiplied.hash(&mut hash);
            g.color_key.hash(&mut hash);
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
            && entry.image.width == width as u32
            && entry.image.height == height as u32
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
            let inverse = (local * brush_matrix).inverse();
            if !inverse.as_coeffs().iter().all(|v| v.is_finite()) {
                return Err(invalid("gradient brush inverse exceeds precision limit"));
            }
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
struct GradientPaint {
    kind: peniko::GradientKind,
    stops: Vec<(f32, [f32; 4])>,
    extend: peniko::Extend,
    premultiplied: bool,
    segments: Vec<ColorSegment>,
    color_key: u64,
}
enum ColorSegment {
    Premultiplied(peniko::color::Interpolator),
    Straight(peniko::color::UnpremultipliedInterpolator),
}
impl ColorSegment {
    fn sample(&self, t: f32) -> [f32; 4] {
        let color = match self {
            Self::Premultiplied(i) => i.eval(t),
            Self::Straight(i) => i.eval(t),
        };
        let rgba = color.to_alpha_color::<peniko::color::Srgb>().components;
        let alpha = rgba[3].clamp(0., 1.);
        [
            rgba[0].clamp(0., 1.) * alpha,
            rgba[1].clamp(0., 1.) * alpha,
            rgba[2].clamp(0., 1.) * alpha,
            alpha,
        ]
    }
}
impl GradientPaint {
    fn new(g: &peniko::Gradient) -> Result<Self> {
        match g.kind {
            peniko::GradientKind::Linear(line) => {
                let direction = line.end - line.start;
                if ![line.start.x, line.start.y, direction.x, direction.y]
                    .iter()
                    .all(|v| v.is_finite())
                    || !direction.hypot2().is_finite()
                    || direction.hypot2() <= 1e-12
                {
                    return Err(invalid("invalid gradient endpoints"));
                }
            }
            peniko::GradientKind::Radial(radial) => {
                if ![
                    radial.start_center.x,
                    radial.start_center.y,
                    radial.end_center.x,
                    radial.end_center.y,
                    f64::from(radial.start_radius),
                    f64::from(radial.end_radius),
                ]
                .iter()
                .all(|v| v.is_finite() && v.abs() <= 1e7)
                    || radial.start_radius < 0.
                    || radial.end_radius < 0.
                    || (radial.start_center == radial.end_center
                        && radial.start_radius == radial.end_radius)
                {
                    return Err(invalid("invalid or degenerate radial gradient"));
                }
            }
            peniko::GradientKind::Sweep(sweep) => {
                if ![
                    sweep.center.x,
                    sweep.center.y,
                    f64::from(sweep.start_angle),
                    f64::from(sweep.end_angle),
                ]
                .iter()
                .all(|v| v.is_finite() && v.abs() <= 1e7)
                    || sweep.start_angle == sweep.end_angle
                {
                    return Err(invalid("invalid or degenerate sweep gradient"));
                }
            }
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
                    || c.iter().any(|v| !v.is_finite())
                    || !(0. ..=1.).contains(&c[3])
            })
        {
            return Err(invalid("invalid gradient stops"));
        }
        let premultiplied =
            g.interpolation_alpha_space == peniko::InterpolationAlphaSpace::Premultiplied;
        let mut key = std::collections::hash_map::DefaultHasher::new();
        g.interpolation_cs.hash(&mut key);
        std::mem::discriminant(&g.hue_direction).hash(&mut key);
        for stop in g.stops.iter() {
            if stop.color.components.iter().any(|v| !v.is_finite()) {
                return Err(invalid("nonfinite gradient color"));
            }
            stop.color.cs.hash(&mut key);
            stop.color.flags.hash(&mut key);
            for v in stop.color.components {
                v.to_bits().hash(&mut key);
            }
        }
        let dynamic = g.interpolation_cs != peniko::color::ColorSpaceTag::Srgb
            || g.stops.iter().any(|s| !s.color.flags.missing().is_empty());
        let segments = if dynamic {
            g.stops
                .windows(2)
                .map(|pair| {
                    if premultiplied {
                        ColorSegment::Premultiplied(pair[0].color.interpolate(
                            pair[1].color,
                            g.interpolation_cs,
                            g.hue_direction,
                        ))
                    } else {
                        ColorSegment::Straight(pair[0].color.interpolate_unpremultiplied(
                            pair[1].color,
                            g.interpolation_cs,
                            g.hue_direction,
                        ))
                    }
                })
                .collect()
        } else {
            Vec::new()
        };
        Ok(Self {
            segments,
            color_key: key.finish(),
            kind: g.kind,
            stops,
            extend: g.extend,
            premultiplied,
        })
    }
    fn sample(&self, point: kurbo::Point) -> [f32; 4] {
        let t = match self.kind {
            peniko::GradientKind::Linear(line) => {
                let direction = line.end - line.start;
                (point - line.start).dot(direction) / direction.hypot2()
            }
            peniko::GradientKind::Radial(radial) => {
                let Some(t) = radial_position(radial, point) else {
                    return [0.; 4];
                };
                t
            }
            peniko::GradientKind::Sweep(sweep) => {
                let p = point - sweep.center;
                let angle = p.y.atan2(p.x).rem_euclid(std::f64::consts::TAU);
                (angle - f64::from(sweep.start_angle))
                    / (f64::from(sweep.end_angle) - f64::from(sweep.start_angle))
            }
        };
        if !t.is_finite() {
            return [0.; 4];
        }
        let t = match self.extend {
            peniko::Extend::Pad => t.clamp(0., 1.),
            peniko::Extend::Repeat => t.rem_euclid(1.),
            peniko::Extend::Reflect => 1. - (t.rem_euclid(2.) - 1.).abs(),
        } as f32;
        let upper = self.stops.partition_point(|s| s.0 <= t);
        if !self.segments.is_empty() {
            if upper == 0 {
                return self.segments[0].sample(0.);
            }
            if upper == self.stops.len() {
                return self.segments.last().unwrap().sample(1.);
            }
            let u = (t - self.stops[upper - 1].0) / (self.stops[upper].0 - self.stops[upper - 1].0);
            return self.segments[upper - 1].sample(u);
        }
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
            out[i] = (if self.premultiplied {
                a[i] * a[3] * (1. - u) + b[i] * b[3] * u
            } else {
                (a[i] + (b[i] - a[i]) * u) * alpha
            })
            .clamp(0., alpha);
        }
        out[3] = alpha;
        out
    }
}

// A point lies on the interpolated circle when |q-t*d|²=(r0+t*dr)².
// The last valid circle (largest t with nonnegative radius) supplies the color;
// points outside the cone are transparent, regardless of the extension mode.
fn radial_position(radial: peniko::RadialGradientPosition, point: kurbo::Point) -> Option<f64> {
    let d = radial.end_center - radial.start_center;
    let q = point - radial.start_center;
    let r0 = f64::from(radial.start_radius);
    let dr = f64::from(radial.end_radius) - r0;
    let a = d.hypot2() - dr * dr;
    let b = -2. * (q.dot(d) + r0 * dr);
    let c = q.hypot2() - r0 * r0;
    let valid = |t: f64| t.is_finite() && r0 + dr * t >= 0.;
    if a == 0. {
        let t = -c / b;
        return valid(t).then_some(t);
    }
    let discriminant = b * b - 4. * a * c;
    if discriminant < 0. {
        return None;
    }
    // Stable quadratic evaluation avoids subtracting nearly equal roots.
    let root = -0.5 * (b + discriminant.sqrt().copysign(b));
    let roots = if root == 0. {
        [-b / (2. * a); 2]
    } else {
        [root / a, c / root]
    };
    roots
        .into_iter()
        .filter(|t| valid(*t))
        .max_by(f64::total_cmp)
}

#[cfg(test)]
mod tests {
    use super::*;
    use peniko::{Color, Gradient, InterpolationAlphaSpace};
    #[test]
    fn clipped_raster_dimensions_follow_resize_and_fractional_viewports() {
        use plico_renderer::PaintScene;
        let mut scene = plico_renderer::Scene::new();
        scene.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            peniko::Color::WHITE,
            None,
            &Rect::new(0., 0., 500., 500.),
        );
        let mut cache = PathCache::default();
        for viewport in [
            Rect::new(0., 0., 300., 200.),
            Rect::new(0., 0., 120., 80.),
            Rect::new(0., 0., 300., 200.),
            Rect::new(0.25, 0.75, 120.5, 80.5),
        ] {
            let (image, bounds) = cache
                .raster_clipped(&scene.commands[0], viewport)
                .unwrap()
                .unwrap();
            assert_eq!(image.width as f64, bounds.width());
            assert_eq!(image.height as f64, bounds.height());
            assert_eq!(bounds.x0.fract(), 0.);
            assert_eq!(bounds.y0.fract(), 0.);
            let mask = crate::mask::MaskCache::default()
                .intersect(image, bounds, None)
                .unwrap();
            assert_eq!(mask.rect[2], mask.image.width as f32);
            assert_eq!(mask.rect[3], mask.image.height as f32);
        }
        assert!(
            cache
                .raster_clipped(&scene.commands[0], Rect::new(600., 600., 700., 700.))
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn linear_path_gradient_preserves_alpha_interpolation_and_extend() {
        let base = Gradient::new_linear((0., 0.), (10., 0.)).with_stops([
            Color::from_rgba8(255, 0, 0, 0),
            Color::from_rgba8(0, 0, 255, 255),
        ]);
        let premult = GradientPaint::new(
            &base
                .clone()
                .with_interpolation_alpha_space(InterpolationAlphaSpace::Premultiplied),
        )
        .unwrap();
        let straight = GradientPaint::new(
            &base
                .clone()
                .with_interpolation_alpha_space(InterpolationAlphaSpace::Unpremultiplied),
        )
        .unwrap();
        assert_eq!(premult.sample((5., 0.).into()), [0., 0., 0.5, 0.5]);
        assert_eq!(straight.sample((5., 0.).into()), [0.25, 0., 0.25, 0.5]);
        let mut repeated = base.clone();
        repeated.extend = peniko::Extend::Repeat;
        let repeated = GradientPaint::new(&repeated).unwrap();
        assert_eq!(
            repeated.sample((-5., 0.).into()),
            repeated.sample((5., 0.).into())
        );
        let mut reflected = base;
        reflected.extend = peniko::Extend::Reflect;
        let reflected = GradientPaint::new(&reflected).unwrap();
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
        let paint = GradientPaint::new(&gradient).unwrap();
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
    #[test]
    fn two_circle_radial_sampling_matches_tiny_skia_across_topologies_and_extend() {
        let cases = [
            ((32., 32.), 0., (32., 32.), 20.),
            ((32., 32.), 24., (32., 32.), 5.),
            ((12., 32.), 4., (44., 32.), 12.),
            ((12., 32.), 10., (44., 32.), 10.),
            ((44., 32.), 12., (12., 32.), 4.),
            ((30., 32.), 2., (32., 32.), 24.),
            ((12., 32.), 0., (32., 32.), 20.),
        ];
        for (c0, r0, c1, r1) in cases {
            for (extend, mode) in [
                (peniko::Extend::Pad, sk::SpreadMode::Pad),
                (peniko::Extend::Repeat, sk::SpreadMode::Repeat),
                (peniko::Extend::Reflect, sk::SpreadMode::Reflect),
            ] {
                let gradient = Gradient::new_two_point_radial(c0, r0, c1, r1)
                    .with_stops([Color::BLACK, Color::WHITE])
                    .with_extend(extend);
                let ours = GradientPaint::new(&gradient).unwrap();
                let shader = sk::RadialGradient::new(
                    sk::Point::from_xy(c0.0 as f32, c0.1 as f32),
                    r0,
                    sk::Point::from_xy(c1.0 as f32, c1.1 as f32),
                    r1,
                    vec![
                        sk::GradientStop::new(0., sk::Color::BLACK),
                        sk::GradientStop::new(1., sk::Color::WHITE),
                    ],
                    mode,
                    sk::Transform::identity(),
                )
                .unwrap();
                let paint = sk::Paint {
                    shader,
                    anti_alias: false,
                    force_hq_pipeline: true,
                    ..Default::default()
                };
                let mut image = sk::Pixmap::new(64, 64).unwrap();
                image.fill_rect(
                    sk::Rect::from_xywh(0., 0., 64., 64.).unwrap(),
                    &paint,
                    sk::Transform::identity(),
                    None,
                );
                for y in 0..64 {
                    for x in 0..64 {
                        let actual = ours
                            .sample(kurbo::Point::new(x as f64 + 0.5, y as f64 + 0.5))
                            .map(|v| (v * 255.).round() as u8);
                        let expected = &image.data()[(y * 64 + x) * 4..(y * 64 + x) * 4 + 4];
                        assert!(
                            actual
                                .iter()
                                .zip(expected)
                                .all(|(a, b)| a.abs_diff(*b) <= 2),
                            "radial {c0:?}/{r0}->{c1:?}/{r1} {extend:?} at {x},{y}: {actual:?} != {expected:?}"
                        );
                    }
                }
            }
        }
    }
    #[test]
    fn sweep_cardinals_direction_extension_and_cache_identity() {
        let gradient = Gradient::new_sweep((8., 8.), 0., std::f32::consts::TAU)
            .with_stops([Color::BLACK, Color::WHITE]);
        let paint = GradientPaint::new(&gradient).unwrap();
        for (point, expected) in [
            ((12., 8.), 0.),
            ((8., 12.), 0.25),
            ((4., 8.), 0.5),
            ((8., 4.), 0.75),
            ((8., 8.), 0.),
        ] {
            let actual = paint.sample(point.into());
            assert!((actual[0] - expected).abs() < 1e-6);
            assert_eq!(actual[3], 1.);
        }
        let reversed = Gradient::new_sweep((8., 8.), std::f32::consts::TAU, 0.)
            .with_stops([Color::BLACK, Color::WHITE]);
        assert!(
            (GradientPaint::new(&reversed)
                .unwrap()
                .sample((8., 12.).into())[0]
                - 0.75)
                .abs()
                < 1e-6
        );
        for (extend, expected) in [
            (peniko::Extend::Pad, 1.),
            (peniko::Extend::Repeat, 0.5),
            (peniko::Extend::Reflect, 0.5),
        ] {
            let short = Gradient::new_sweep((8., 8.), 0., std::f32::consts::FRAC_PI_2)
                .with_stops([Color::BLACK, Color::WHITE])
                .with_extend(extend);
            assert!(
                (GradientPaint::new(&short).unwrap().sample((4., 12.).into())[0] - expected).abs()
                    < 1e-6
            );
        }
        use plico_renderer::PaintScene;
        let mut scene = plico_renderer::Scene::new();
        scene.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            &gradient,
            None,
            &Rect::new(0., 0., 16., 16.),
        );
        let mut cache = PathCache::default();
        let (first, _) = cache.raster(&scene.commands[0]).unwrap().unwrap();
        let mut moved = scene.commands[0].clone();
        if let PaintCommand::Fill { transform, .. } = &mut moved {
            *transform = Affine::translate((20., 10.));
        }
        let (second, _) = cache.raster(&moved).unwrap().unwrap();
        assert_eq!(first.data.id(), second.data.id());
        if let PaintCommand::Fill { brush, .. } = &mut moved {
            *brush = reversed.into();
        }
        let (third, _) = cache.raster(&moved).unwrap().unwrap();
        assert_ne!(first.data.id(), third.data.id());
        assert_ne!(first.data.data(), third.data.data());
        if let PaintCommand::Fill { brush, .. } = &mut moved {
            *brush = Gradient::new_sweep((10., 10.), std::f32::consts::TAU, 0.)
                .with_stops([Color::BLACK, Color::WHITE])
                .into();
        }
        let (center_changed, _) = cache.raster(&moved).unwrap().unwrap();
        assert_ne!(third.data.id(), center_changed.data.id());
        assert_ne!(third.data.data(), center_changed.data.data());
        if let PaintCommand::Fill {
            brush_transform, ..
        } = &mut moved
        {
            *brush_transform = Some(Affine::translate((1., 0.)));
        }
        let (brush_changed, _) = cache.raster(&moved).unwrap().unwrap();
        assert_ne!(center_changed.data.id(), brush_changed.data.id());
        assert_ne!(center_changed.data.data(), brush_changed.data.data());
        assert!(
            GradientPaint::new(
                &Gradient::new_sweep((0., 0.), 1., 1.).with_stops([Color::BLACK, Color::WHITE])
            )
            .is_err()
        );
    }
    #[test]
    fn mature_color_interpolation_preserves_space_hue_missing_and_cache_identity() {
        use peniko::color::{ColorSpaceTag as Cs, DynamicColor, Flags, HueDirection, Missing};
        let base =
            Gradient::new_linear((0., 0.), (10., 0.)).with_stops([Color::BLACK, Color::WHITE]);
        for (cs, expected) in [
            (Cs::Srgb, 0.5),
            (Cs::LinearSrgb, 0.735357),
            (Cs::Oklab, 0.388573),
        ] {
            let color = GradientPaint::new(&base.clone().with_interpolation_cs(cs))
                .unwrap()
                .sample((5., 0.).into());
            assert!((color[0] - expected).abs() < 0.0001, "{cs:?}: {color:?}");
        }
        let hue = Gradient::new_linear((0., 0.), (10., 0.))
            .with_stops([Color::from_rgb8(255, 0, 0), Color::from_rgb8(0, 0, 255)])
            .with_interpolation_cs(Cs::Hsl);
        for (direction, expected) in [
            (HueDirection::Shorter, [1., 0., 1., 1.]),
            (HueDirection::Longer, [0., 1., 0., 1.]),
        ] {
            let color = GradientPaint::new(&hue.clone().with_hue_direction(direction))
                .unwrap()
                .sample((5., 0.).into());
            assert!(
                color
                    .iter()
                    .zip(expected)
                    .all(|(a, b)| (*a - b).abs() < 0.001)
            );
        }
        let mut missing: DynamicColor = Color::BLACK.into();
        missing.flags = Flags::from_missing(Missing::single(0));
        let borrowed = Gradient::new_linear((0., 0.), (10., 0.))
            .with_stops([(0., missing), (1., DynamicColor::from(Color::WHITE))]);
        assert_eq!(
            GradientPaint::new(&borrowed)
                .unwrap()
                .sample((5., 0.).into()),
            [1., 0.5, 0.5, 1.]
        );
        let wide = peniko::color::AlphaColor::<peniko::color::DisplayP3>::new([0., 1., 0., 0.5]);
        let wide = Gradient::new_linear((0., 0.), (10., 0.)).with_stops([
            (0., DynamicColor::from(wide)),
            (1., DynamicColor::from(wide)),
        ]);
        let color = GradientPaint::new(&wide).unwrap().sample((5., 0.).into());
        assert!(color.iter().all(|v| *v >= 0. && *v <= 0.5));
        use plico_renderer::PaintScene;
        let mut cache = PathCache::default();
        let mut ids = Vec::new();
        for cs in [Cs::Srgb, Cs::Oklab, Cs::Srgb] {
            let mut scene = plico_renderer::Scene::new();
            scene.fill(
                Fill::NonZero,
                Affine::IDENTITY,
                &base.clone().with_interpolation_cs(cs),
                None,
                &Rect::new(0., 0., 10., 8.),
            );
            ids.push(
                cache
                    .raster(&scene.commands[0])
                    .unwrap()
                    .unwrap()
                    .0
                    .data
                    .id(),
            );
        }
        assert_ne!(ids[0], ids[1]);
        assert_eq!(ids[0], ids[2]);
    }
}
