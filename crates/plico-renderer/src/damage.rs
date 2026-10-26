//! Optional redraw information, independent of layout and window APIs.
use vello_common::kurbo::Rect;

/// Damage in physical surface pixels, relative to the last successful draw.
/// Consumers may always promote a region to `Full`. Producers must include
/// both old and new ink bounds, including effects and overlapping content.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum DamageRegion {
    /// Contents or preservation are unknown; replace the entire surface.
    #[default]
    Full,
    /// The previous surface contents remain valid without drawing.
    Empty,
    /// Redraw this rectangle (consumers round outwards to pixel boundaries).
    Rect(Rect),
}

impl DamageRegion {
    /// Round outwards and intersect the surface. Invalid geometry falls back
    /// to full damage; a finite empty or offscreen rectangle becomes empty.
    pub fn for_surface(self, width: u32, height: u32) -> Self {
        match self {
            Self::Rect(r) => {
                if ![r.x0, r.y0, r.x1, r.y1].iter().all(|v| v.is_finite()) {
                    return Self::Full;
                }
                if r.x0 >= r.x1 || r.y0 >= r.y1 {
                    return Self::Empty;
                }
                let r = Rect::new(r.x0.floor(), r.y0.floor(), r.x1.ceil(), r.y1.ceil())
                    .intersect(Rect::new(0., 0., width as f64, height as f64));
                if r.width() <= 0. || r.height() <= 0. {
                    Self::Empty
                } else {
                    Self::Rect(r)
                }
            }
            other => other,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn normalization_rounds_outwards_clamps_and_rejects_nonfinite_geometry() {
        assert_eq!(
            DamageRegion::Rect(Rect::new(-2.2, 1.1, 8.1, 12.)).for_surface(10, 10),
            DamageRegion::Rect(Rect::new(0., 1., 9., 10.))
        );
        assert_eq!(
            DamageRegion::Rect(Rect::new(12., 0., 14., 1.)).for_surface(10, 10),
            DamageRegion::Empty
        );
        assert_eq!(
            DamageRegion::Rect(Rect::new(2., 0., 1., 1.)).for_surface(10, 10),
            DamageRegion::Empty
        );
        assert_eq!(
            DamageRegion::Rect(Rect::new(0., 0., f64::INFINITY, 1.)).for_surface(10, 10),
            DamageRegion::Full
        );
    }
}
