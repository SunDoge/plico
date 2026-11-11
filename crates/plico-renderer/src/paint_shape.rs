//! Preserve analytical geometry without eagerly allocating a fallback path.

use kurbo::{BezPath, PathEl, Point, Rect, RoundedRect, Shape};

/// Geometry retained before path conversion; arbitrary paths remain arbitrary paths.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PrimitiveShape {
    /// An axis-aligned source rectangle.
    Rect(Rect),
    /// Source rectangle and its four exact circular corner radii.
    RoundedRect(RoundedRect),
}
#[derive(Clone, Debug, PartialEq)]
enum Geometry {
    Rect(Rect),
    RoundedRect(RoundedRect),
    Path(BezPath),
}

/// Exact primitive geometry or an already flattened arbitrary path.
///
/// Rectangles and circular rounded rectangles stay analytical until a backend
/// actually asks for path elements. The primary wgpu backend consumes their
/// primitive metadata directly, so ordinary boxes allocate no fallback path.
#[derive(Clone, Debug, PartialEq)]
pub struct PaintShape(Geometry);

impl PaintShape {
    /// Record exact source geometry when possible, flattening only arbitrary shapes.
    pub fn record(shape: &impl Shape, tolerance: f64) -> Self {
        if let Some(rect) = shape.as_rect() {
            return Self(Geometry::Rect(rect));
        }
        if let Some(rounded) = shape.as_rounded_rect() {
            return Self(Geometry::RoundedRect(rounded));
        }
        Self(Geometry::Path(shape.to_path(tolerance)))
    }

    /// Original geometry, when supplied as an analytical shape.
    pub fn primitive(&self) -> Option<PrimitiveShape> {
        match self.0 {
            Geometry::Rect(rect) => Some(PrimitiveShape::Rect(rect)),
            Geometry::RoundedRect(rounded) => Some(PrimitiveShape::RoundedRect(rounded)),
            Geometry::Path(_) => None,
        }
    }

    /// Consume this record and materialize a compatible Bezier path.
    pub fn into_bez_path(self) -> BezPath {
        match self.0 {
            Geometry::Rect(rect) => rect.to_path(0.1),
            Geometry::RoundedRect(rounded) => rounded.to_path(0.1),
            Geometry::Path(path) => path,
        }
    }
}

impl From<BezPath> for PaintShape {
    fn from(path: BezPath) -> Self {
        Self(Geometry::Path(path))
    }
}

impl Shape for PaintShape {
    // Path consumers are fallback backends; one iterator box there is cheaper
    // and much smaller than carrying RoundedRect's 464-byte iterator through
    // every call. The wgpu primitive path never constructs this iterator.
    type PathElementsIter<'a> = Box<dyn Iterator<Item = PathEl> + 'a>;

    fn path_elements(&self, tolerance: f64) -> Self::PathElementsIter<'_> {
        match &self.0 {
            Geometry::Path(path) => Box::new(path.elements().iter().copied()),
            Geometry::Rect(rect) => Box::new(rect.path_elements(tolerance)),
            Geometry::RoundedRect(rounded) => Box::new(rounded.path_elements(tolerance)),
        }
    }

    fn area(&self) -> f64 {
        match &self.0 {
            Geometry::Rect(rect) => rect.area(),
            Geometry::RoundedRect(rounded) => rounded.area(),
            Geometry::Path(path) => path.area(),
        }
    }

    fn perimeter(&self, accuracy: f64) -> f64 {
        match &self.0 {
            Geometry::Rect(rect) => rect.perimeter(accuracy),
            Geometry::RoundedRect(rounded) => rounded.perimeter(accuracy),
            Geometry::Path(path) => path.perimeter(accuracy),
        }
    }

    fn winding(&self, point: Point) -> i32 {
        match &self.0 {
            Geometry::Rect(rect) => rect.winding(point),
            Geometry::RoundedRect(rounded) => rounded.winding(point),
            Geometry::Path(path) => path.winding(point),
        }
    }

    fn bounding_box(&self) -> Rect {
        match &self.0 {
            Geometry::Rect(rect) => *rect,
            Geometry::RoundedRect(rounded) => rounded.rect(),
            Geometry::Path(path) => path.bounding_box(),
        }
    }

    fn as_rect(&self) -> Option<Rect> {
        match self.0 {
            Geometry::Rect(rect) => Some(rect),
            _ => None,
        }
    }

    fn as_rounded_rect(&self) -> Option<RoundedRect> {
        match self.0 {
            Geometry::RoundedRect(rounded) => Some(rounded),
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

    #[test]
    fn primitives_materialize_only_when_a_path_consumer_requests_elements() {
        let rect = Rect::new(1., 2., 81., 42.);
        let shape = PaintShape::record(&rect, 0.1);
        assert!(matches!(shape.0, Geometry::Rect(_)));
        assert_eq!(
            shape.path_elements(0.1).collect::<Vec<_>>(),
            rect.to_path(0.1).elements()
        );
    }
}
