//! Declaration-time culling in explicit logical coordinate spaces.
use std::ops::Range;

#[derive(Debug, Clone, Copy, PartialEq, Default)]
enum Bounds {
    #[default]
    Unbounded,
    Empty,
    Rect([f32; 4]),
}

/// A conservative visible area, independent of layout, rendering and node storage.
/// Bounds use `[left, top, right, bottom]` in one logical coordinate space.
/// Unknown/non-finite geometry disables culling rather than hiding content.
///
/// This skips declarations only when the application uses the query result.
/// It does not set a drawing clip, retain unmounted nodes, or build a spatial index.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct VisibleRegion(Bounds);

impl VisibleRegion {
    pub const UNBOUNDED: Self = Self(Bounds::Unbounded);
    pub const EMPTY: Self = Self(Bounds::Empty);

    pub fn from_rect(rect: [f32; 4]) -> Self {
        if !rect.iter().all(|v| v.is_finite()) {
            Self::UNBOUNDED
        } else if rect[0] >= rect[2] || rect[1] >= rect[3] {
            Self::EMPTY
        } else {
            Self(Bounds::Rect(rect))
        }
    }

    pub fn from_size([width, height]: [f32; 2]) -> Self {
        Self::from_rect([0., 0., width, height])
    }

    /// None means unbounded; an empty region returns `[0.; 4]`.
    pub fn bounds(self) -> Option<[f32; 4]> {
        match self.0 {
            Bounds::Unbounded => None,
            Bounds::Empty => Some([0.; 4]),
            Bounds::Rect(rect) => Some(rect),
        }
    }

    pub fn is_empty(self) -> bool {
        matches!(self.0, Bounds::Empty)
    }

    /// Positive-area overlap. Edge-touching rectangles are outside the region.
    /// Include shadows, strokes and other visual overflow in the supplied bounds.
    pub fn intersects(self, rect: [f32; 4]) -> bool {
        !self.intersect(Self::from_rect(rect)).is_empty()
    }

    /// Combine clips expressed in the same coordinate space.
    pub fn intersect(self, other: Self) -> Self {
        match (self.0, other.0) {
            (Bounds::Empty, _) | (_, Bounds::Empty) => Self::EMPTY,
            (Bounds::Unbounded, _) => other,
            (_, Bounds::Unbounded) => self,
            (Bounds::Rect(a), Bounds::Rect(b)) => Self::from_rect([
                a[0].max(b[0]),
                a[1].max(b[1]),
                a[2].min(b[2]),
                a[3].min(b[3]),
            ]),
        }
    }

    /// Add non-negative horizontal/vertical overscan in this coordinate space.
    pub fn overscan(self, [x, y]: [f32; 2]) -> Self {
        if self.is_empty() {
            return self;
        }
        if !x.is_finite() || !y.is_finite() || x < 0. || y < 0. {
            return Self::UNBOUNDED;
        }
        match self.0 {
            Bounds::Rect([left, top, right, bottom]) => {
                Self::from_rect([left - x, top - y, right + x, bottom + y])
            }
            _ => self,
        }
    }

    /// Map the viewport into content coordinates using the inverse of a
    /// local-to-viewport affine `[a, b, c, d, tx, ty]`:
    /// `view_x = a*x + c*y + tx`, `view_y = b*x + d*y + ty`.
    /// Rotation/skew use a conservative axis-aligned bounding box. A singular
    /// or invalid transform disables culling. No pixel snapping is applied.
    pub fn to_local(self, local_to_view: [f32; 6]) -> Self {
        let Bounds::Rect([left, top, right, bottom]) = self.0 else {
            return self;
        };
        if !local_to_view.iter().all(|v| v.is_finite()) {
            return Self::UNBOUNDED;
        }
        let [a, b, c, d, tx, ty] = local_to_view.map(f64::from);
        let determinant = a * d - b * c;
        if determinant == 0. {
            return Self::UNBOUNDED;
        }
        let mut min = [f64::INFINITY; 2];
        let mut max = [f64::NEG_INFINITY; 2];
        for [x, y] in [[left, top], [right, top], [left, bottom], [right, bottom]] {
            let x = f64::from(x) - tx;
            let y = f64::from(y) - ty;
            let point = [(d * x - c * y) / determinant, (a * y - b * x) / determinant];
            for axis in 0..2 {
                min[axis] = min[axis].min(point[axis]);
                max[axis] = max[axis].max(point[axis]);
            }
        }
        // Round outwards so conversion to f32 cannot drop a boundary object.
        let lower = |value: f64| {
            let rounded = value as f32;
            if f64::from(rounded) > value {
                rounded.next_down()
            } else {
                rounded
            }
        };
        let upper = |value: f64| {
            let rounded = value as f32;
            if f64::from(rounded) < value {
                rounded.next_up()
            } else {
                rounded
            }
        };
        Self::from_rect([lower(min[0]), lower(min[1]), upper(max[0]), upper(max[1])])
    }

    /// Visible fixed-height rows beginning at y=0, including extra rows on each
    /// side. Use the row pitch (including gaps) as `stride`. O(1), no allocation.
    pub fn rows(self, count: usize, stride: f32, overscan: usize) -> Range<usize> {
        self.axis_range(1, count, stride, overscan)
    }

    /// Visible fixed-width columns beginning at x=0; see [`Self::rows`].
    pub fn columns(self, count: usize, stride: f32, overscan: usize) -> Range<usize> {
        self.axis_range(0, count, stride, overscan)
    }

    fn axis_range(self, axis: usize, count: usize, stride: f32, overscan: usize) -> Range<usize> {
        if self.is_empty() || count == 0 {
            return 0..0;
        }
        if !stride.is_finite() || stride <= 0. {
            return 0..count;
        }
        let Bounds::Rect(bounds) = self.0 else {
            return 0..count;
        };
        let stride = f64::from(stride);
        let low = f64::from(bounds[axis]);
        let high = f64::from(bounds[axis + 2]);
        if high <= 0. || low >= count as f64 * stride {
            return 0..0;
        }
        let start = ((low.max(0.) / stride).floor() as usize)
            .saturating_sub(overscan)
            .min(count);
        let end = ((high.max(0.) / stride).ceil() as usize)
            .saturating_add(overscan)
            .min(count);
        start..end
    }
}
