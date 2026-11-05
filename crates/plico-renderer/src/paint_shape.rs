//! Preserve analytical geometry alongside the existing compatible Bezier stream.
use kurbo::{BezPath, PathEl, Point, Rect, RoundedRect, Shape};

/// Geometry retained before path conversion; arbitrary paths remain arbitrary paths.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PrimitiveShape {
    /// An axis-aligned source rectangle.
    Rect(Rect),
    /// Source rectangle and its four exact circular corner radii.
    RoundedRect(RoundedRect),
}
/// A retained path with optional exact source primitive. No mutable path access is
/// exposed: changing the fallback path must not leave stale analytical metadata.
#[derive(Clone, Debug, PartialEq)]
pub struct PaintShape {
    path: BezPath,
    primitive: Option<PrimitiveShape>,
}
impl PaintShape {
    /// Record both source geometry and the compatible path at the requested tolerance.
    pub fn record(shape: &impl Shape, tolerance: f64) -> Self {
        let primitive = shape
            .as_rect()
            .map(PrimitiveShape::Rect)
            .or_else(|| shape.as_rounded_rect().map(PrimitiveShape::RoundedRect));
        Self {
            path: shape.to_path(tolerance),
            primitive,
        }
    }
    /// Original geometry, when supplied as an analytical shape.
    pub fn primitive(&self) -> Option<PrimitiveShape> {
        self.primitive
    }
    /// Consume this record and discard analytical metadata.
    pub fn into_bez_path(self) -> BezPath {
        self.path
    }
}
impl From<BezPath> for PaintShape {
    fn from(path: BezPath) -> Self {
        Self {
            path,
            primitive: None,
        }
    }
}
impl std::ops::Deref for PaintShape {
    type Target = BezPath;
    fn deref(&self) -> &BezPath {
        &self.path
    }
}
impl Shape for PaintShape {
    type PathElementsIter<'a> = std::iter::Copied<std::slice::Iter<'a, PathEl>>;
    fn path_elements(&self, _: f64) -> Self::PathElementsIter<'_> {
        self.path.elements().iter().copied()
    }
    fn area(&self) -> f64 {
        self.path.area()
    }
    fn perimeter(&self, accuracy: f64) -> f64 {
        self.path.perimeter(accuracy)
    }
    fn winding(&self, point: Point) -> i32 {
        self.path.winding(point)
    }
    fn bounding_box(&self) -> Rect {
        self.path.bounding_box()
    }
    fn as_rect(&self) -> Option<Rect> {
        match self.primitive {
            Some(PrimitiveShape::Rect(r)) => Some(r),
            _ => None,
        }
    }
    fn as_rounded_rect(&self) -> Option<RoundedRect> {
        match self.primitive {
            Some(PrimitiveShape::RoundedRect(r)) => Some(r),
            _ => None,
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preserves_exact_geometry_without_guessing_arbitrary_paths() {
        let rect = Rect::new(1., 2., 81., 42.);
        let rounded = rect.to_rounded_rect((3., 5., 7., 9.));
        let source = PaintShape::record(&rounded, 0.1);
        assert_eq!(
            source.primitive(),
            Some(PrimitiveShape::RoundedRect(rounded))
        );
        assert_eq!(source.to_path(0.1), rounded.to_path(0.1));
        let path: PaintShape = rounded.to_path(0.1).into();
        assert_eq!(path.primitive(), None);
        assert_eq!(path.to_path(0.1), source.to_path(0.1));
        assert_eq!(PaintShape::record(&rect, 0.1).as_rect(), Some(rect));
    }
}
