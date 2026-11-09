//! MyGO-compatible continuous-corner geometry for shared fills/borders/clips.
//!
//! The implicit corner formula is adapted from MyGO internal/raster/corner.go
//! and internal/gpu/metal/shader.metal at ee3d4bd9 (MIT).
//! Copyright (c) 2026 MyGo contributors. License: ../third_party/mygo-LICENSE.
//! The contour tracing/cache below is Plico's backend-neutral adaptation.
use crate::{PaintShape, style::CornerRadii};
use kurbo::{Affine, BezPath, Point, Rect};
use std::{cell::RefCell, num::NonZeroUsize, sync::Arc};

const EXTENT: f64 = 1.528665;
#[derive(Clone, Copy)]
struct Corner {
    radius: f64,
    extent: f64,
    effective: f64,
    clamp_x: f64,
    clamp_y: f64,
}
impl Corner {
    fn new(radius: f64, horizontal: f64, vertical: f64, width: f64, height: f64) -> Self {
        let clamp = |side: f64, adjacent: f64| {
            if radius + adjacent == 0. {
                0.
            } else {
                ((EXTENT - side / (radius + adjacent)) / (EXTENT - 1.)).clamp(0., 1.)
            }
        };
        let clamp_x = clamp(width, horizontal);
        let clamp_y = clamp(height, vertical);
        let extent = EXTENT * radius;
        Self {
            radius,
            extent,
            effective: extent + (radius - extent) * clamp_x.max(clamp_y),
            clamp_x,
            clamp_y,
        }
    }
    fn value(self, u: f64, v: f64) -> f64 {
        let ax = (1. - u / self.extent).max(0.);
        let ay = (1. - v / self.extent).max(0.);
        let length = ax.hypot(ay);
        let high = ax.max(ay);
        let rho = if high > 0. {
            (ax.min(ay) / high).min(1.)
        } else {
            0.
        };
        let polynomial =
            (((-0.926054 * rho + 3.15601) * rho - 3.64122) * rho + 1.26803) * rho + 0.268531;
        let continuous = length + 1. - 1. / (1. - rho * rho * length.min(1.) * polynomial);
        let qx = (ax * EXTENT - (EXTENT - 1.)).max(0.);
        let qy = (ay * EXTENT - (EXTENT - 1.)).max(0.);
        let circular = qx.hypot(qy) * 0.654166 + 0.345834;
        let sign = if ay > ax { 1. } else { -1. };
        let weight = (0.5 - sign + sign * rho).clamp(0., 1.);
        let fx = continuous + (circular - continuous) * self.clamp_x;
        let fy = continuous + (circular - continuous) * self.clamp_y;
        ((self.effective - u).max(self.effective - v)).min(0.)
            + self.extent * (fx + (fy - fx) * weight - 1.)
    }
}
struct Contour {
    width: f64,
    height: f64,
    corners: [Corner; 4],
    amount: f64,
}
impl Contour {
    fn value(&self, p: Point) -> f64 {
        let horizontal = [p.x, self.width - p.x, self.width - p.x, p.x];
        let vertical = [p.y, p.y, self.height - p.y, self.height - p.y];
        let edge = (-p.x)
            .max(-p.y)
            .max(p.x - self.width)
            .max(p.y - self.height);
        let mut continuous = edge;
        let mut circular = edge;
        for (i, c) in self.corners.iter().enumerate() {
            let (u, v) = (horizontal[i], vertical[i]);
            if c.radius > 0. {
                if u < c.extent && v < c.extent {
                    continuous = continuous.max(c.value(u, v));
                }
                if u < c.radius && v < c.radius {
                    circular = circular
                        .max((c.radius - u).max(0.).hypot((c.radius - v).max(0.)) - c.radius);
                }
            }
        }
        circular + (continuous - circular) * self.amount
    }
    fn at(&self, angle: f64) -> Point {
        let (dy, dx) = angle.sin_cos();
        let center = Point::new(self.width / 2., self.height / 2.);
        let mut hi =
            (self.width / 2. / dx.abs().max(1e-15)).min(self.height / 2. / dy.abs().max(1e-15));
        let point = |distance| Point::new(center.x + dx * distance, center.y + dy * distance);
        if self.value(point(hi)) <= 1e-12 {
            return point(hi);
        }
        let mut lo = 0.;
        for _ in 0..36 {
            let mid = (lo + hi) * 0.5;
            if self.value(point(mid)) > 0. {
                hi = mid;
            } else {
                lo = mid;
            }
        }
        point((lo + hi) * 0.5)
    }
    fn segment(&self, path: &mut BezPath, a: f64, pa: Point, b: f64, pb: Point, depth: u8) {
        let mid = (a + b) * 0.5;
        let pm = self.at(mid);
        let delta = pb - pa;
        let error = if delta.hypot() > 0. {
            (pm - pa).cross(delta).abs() / delta.hypot()
        } else {
            0.
        };
        if depth < 2 || (error > 0.0125 && depth < 14) {
            self.segment(path, a, pa, mid, pm, depth + 1);
            self.segment(path, mid, pm, b, pb, depth + 1);
        } else {
            path.line_to(pb);
        }
    }
    fn path(&self) -> BezPath {
        let mut path = BezPath::new();
        let start = -std::f64::consts::FRAC_PI_2;
        let mut pa = self.at(start);
        path.move_to(pa);
        for i in 0..4 {
            let a = start + f64::from(i) * std::f64::consts::FRAC_PI_2;
            let b = a + std::f64::consts::FRAC_PI_2;
            let pb = self.at(b);
            self.segment(&mut path, a, pa, b, pb, 0);
            pa = pb;
        }
        path.close_path();
        path
    }
}
// Translation is excluded: moving an unchanged surface reuses its local curve.
// The 128-entry bound keeps arbitrary resize/smoothing histories from accumulating.
type Key = [u64; 7];
thread_local! {
    static PATHS: RefCell<lru::LruCache<Key, Arc<BezPath>>> = RefCell::new(lru::LruCache::new(NonZeroUsize::new(128).unwrap()));
}
/// Zero keeps exact circular primitives. One follows MyGO's implicit curve;
/// intermediate values interpolate the circular and continuous fields.
/// Wgpu currently consumes the cached contour through its path rasterizer,
/// rather than running MyGO's implicit equation in a GPU fragment shader.
pub fn corner_shape(rect: Rect, radii: CornerRadii, smoothing: f32) -> PaintShape {
    let rounded = rect.to_rounded_rect(radii.as_f64_tuple());
    let uniform = radii.top_left == radii.top_right
        && radii.top_left == radii.bottom_right
        && radii.top_left == radii.bottom_left;
    let capsule = uniform && f64::from(radii.max()) >= rect.width().min(rect.height()) * 0.5;
    if radii.max() <= 0.
        || capsule
        || !smoothing.is_finite()
        || smoothing <= 0.
        || rect.width() <= 0.
        || rect.height() <= 0.
    {
        return PaintShape::record(&rounded, 0.1);
    }
    // MyGO fits neighboring radius sums, rather than independently cutting
    // every radius at half the short side (important for unequal corners).
    let mut r = radii.as_f64_tuple();
    let mut scale = 1_f64;
    for (side, sum) in [
        (rect.width(), r.0 + r.1),
        (rect.height(), r.1 + r.2),
        (rect.width(), r.2 + r.3),
        (rect.height(), r.3 + r.0),
    ] {
        if sum > 0. {
            scale = scale.min(side / sum);
        }
    }
    r = (r.0 * scale, r.1 * scale, r.2 * scale, r.3 * scale);
    let amount = f64::from(smoothing.min(1.));
    let key = [
        rect.width().to_bits(),
        rect.height().to_bits(),
        r.0.to_bits(),
        r.1.to_bits(),
        r.2.to_bits(),
        r.3.to_bits(),
        amount.to_bits(),
    ];
    let cached = PATHS.with(|cache| cache.borrow_mut().get(&key).cloned());
    let path = cached.unwrap_or_else(|| {
        let contour = Contour {
            width: rect.width(),
            height: rect.height(),
            amount,
            corners: [
                Corner::new(r.0, r.1, r.3, rect.width(), rect.height()),
                Corner::new(r.1, r.0, r.2, rect.width(), rect.height()),
                Corner::new(r.2, r.3, r.1, rect.width(), rect.height()),
                Corner::new(r.3, r.2, r.0, rect.width(), rect.height()),
            ],
        };
        let path = Arc::new(contour.path());
        if path.elements().len() <= 4096 {
            PATHS.with(|cache| {
                cache.borrow_mut().put(key, path.clone());
            });
        }
        path
    });
    (Affine::translate((rect.x0, rect.y0)) * path.as_ref()).into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use kurbo::Shape;
    #[test]
    fn implicit_values_match_the_local_mygo_go_reference() {
        // Generated by running unmodified MyGO corner.go with a small Go test;
        // includes short-side clamps and unequal neighboring corner radii.
        let vectors = [
            ([28., 28., 28., 300., 160., 5., 10.], 1.211020231),
            ([28., 28., 28., 300., 160., 8., 8.], 0.284462690),
            ([28., 28., 28., 300., 160., 20., 1.], 0.511328697),
            ([28., 28., 28., 300., 160., 30., 0.], 0.288830400),
            ([18., 18., 18., 60., 36., 4., 10.], -1.872126222),
            ([18., 18., 18., 36., 36., 4., 10.], -1.875486732),
            ([12., 4., 20., 28., 40., 3., 5.], -0.597909570),
            ([12., 4., 20., 28., 40., 10., 1.], -0.652200580),
            ([24., 4., 8., 48., 38., 8., 8.], -1.372479796),
            ([24., 4., 8., 48., 38., 20., 2.], -1.304401159),
        ];
        for ([r, rh, rv, w, h, u, v], expected) in vectors {
            let actual = Corner::new(r, rh, rv, w, h).value(u, v);
            assert!(
                (actual - expected).abs() < 0.00002,
                "{actual} != Go {expected}"
            );
        }
    }
    #[test]
    fn contour_follows_the_reference_field_and_reuses_local_geometry() {
        let rect = Rect::new(0., 0., 300., 160.);
        let shape = corner_shape(rect, CornerRadii::uniform(28.), 1.);
        let corner = Corner::new(28., 28., 28., 300., 160.);
        let contour = Contour {
            width: 300.,
            height: 160.,
            corners: [corner; 4],
            amount: 1.,
        };
        for el in shape.elements() {
            if let kurbo::PathEl::MoveTo(p) | kurbo::PathEl::LineTo(p) = el {
                assert!(
                    contour.value(*p).abs() < 0.00001,
                    "point {p:?} left reference curve"
                );
            }
        }
        assert!(shape.elements().len() < 512);
        let moved = corner_shape(
            Rect::new(10., 20., 310., 180.),
            CornerRadii::uniform(28.),
            1.,
        );
        assert_eq!(
            moved.into_bez_path(),
            Affine::translate((10., 20.)) * shape.clone().into_bez_path()
        );
        assert!(shape.contains(Point::new(150., 80.)));
        assert!(!shape.contains(Point::new(0.1, 0.1)));
    }
    #[test]
    fn fully_rounded_circles_and_capsules_keep_exact_arcs_with_continuous_defaults() {
        for rect in [Rect::new(0., 0., 40., 40.), Rect::new(0., 0., 120., 40.)] {
            let radii = CornerRadii::uniform(20.);
            let optimized = corner_shape(rect, radii, 1.);
            assert!(optimized.as_rounded_rect().is_some());
            assert_eq!(optimized, corner_shape(rect, radii, 0.));
        }
    }
    #[test]
    fn explicit_circular_stays_analytical_and_mixed_corners_remain_valid() {
        let rect = Rect::new(0., 0., 100., 70.);
        assert!(
            corner_shape(rect, CornerRadii::uniform(10.), 0.)
                .as_rounded_rect()
                .is_some()
        );
        let radii = CornerRadii {
            top_left: 30.,
            top_right: 4.,
            bottom_right: 0.,
            bottom_left: 8.,
        };
        let shape = corner_shape(rect, radii, 1.);
        assert!(shape.contains(Point::new(99.9, 69.9)));
        assert!(shape.contains(Point::new(50., 35.)));
        assert!(!shape.contains(Point::new(0.1, 0.1)));
    }
}
