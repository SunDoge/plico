/// A sampled rectangle anchored to the layout position it will have at rest.
///
/// Rectangles are logical `[left, top, width, height]` in the same containing
/// block. The resting origin and width stay in layout, where static geometry
/// is snapped by the renderer. Translation and uniform scale carry the
/// motion in paint. The layout height changes with the aspect ratio.
///
/// Inspired by MyGO's shared-element pinning (`ui/match.go`, MIT,
/// revision `f651b1061d917336f31c3ede523cb49c524172eb`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PinnedRect {
    pub(crate) origin: [f32; 2],
    size: [f32; 2],
    pub(crate) translation: [f32; 2],
    scale: f32,
    shown_origin: [f32; 2],
}

impl PinnedRect {
    /// Resolve a sampled rectangle to layout and paint. Empty rectangles and
    /// non-finite inputs produce no placement.
    pub fn new(rest: [f32; 4], shown: [f32; 4]) -> Option<Self> {
        let [x, y, width, height] = rest;
        let [shown_x, shown_y, shown_width, shown_height] = shown;
        if !rest.into_iter().chain(shown).all(f32::is_finite)
            || width <= 0.
            || height <= 0.
            || shown_width <= 0.
            || shown_height <= 0.
        {
            return None;
        }
        let scale = shown_width / width;
        let layout_height = shown_height / scale;
        // Avoid relayout when only floating-point noise changes the height
        // of a uniformly scaled rectangle.
        let layout_height = if (layout_height - height).abs() <= height * (4. * f32::EPSILON) {
            height
        } else {
            layout_height
        };
        // Layout scales around its center. Carry the difference between
        // centers in the translation that follows pixel snapping.
        let translation = [
            (shown_x - x) + (shown_width - width) * 0.5,
            (shown_y - y) + (shown_height - layout_height) * 0.5,
        ];
        if scale <= 0.
            || layout_height <= 0.
            || ![scale, layout_height, translation[0], translation[1]]
                .into_iter()
                .all(f32::is_finite)
        {
            return None;
        }
        Some(Self {
            origin: [x, y],
            size: [width, layout_height],
            translation,
            scale,
            shown_origin: [shown_x, shown_y],
        })
    }

    /// Layout size before applying the paint transform.
    pub fn layout_size(self) -> [f32; 2] {
        self.size
    }

    /// Uniform paint scale, also useful for keeping a radius fixed on screen.
    pub fn scale(self) -> f32 {
        self.scale
    }

    /// Map a sampled rectangle from the containing block into this node's
    /// content coordinates. Use the result to pin a child of an unpadded
    /// container with no border. The child's resting rectangle is relative to
    /// the parent's resting origin.
    pub fn to_local(self, [x, y, width, height]: [f32; 4]) -> [f32; 4] {
        [
            (x - self.shown_origin[0]) / self.scale,
            (y - self.shown_origin[1]) / self.scale,
            width / self.scale,
            height / self.scale,
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::PinnedRect;

    #[test]
    fn uniform_motion_preserves_layout_and_fractional_translation() {
        let rest = [20.25, 30.125, 120., 60.];
        let still = PinnedRect::new(rest, rest).unwrap();
        assert_eq!(still.translation, [0., 0.]);
        assert_eq!(still.scale(), 1.);

        let frame = PinnedRect::new(rest, [20.5, 30.25, 150., 75.]).unwrap();
        assert_eq!(frame.origin, still.origin);
        assert_eq!(frame.layout_size(), still.layout_size());
        assert_eq!(frame.scale(), 1.25);
        assert_eq!(frame.translation, [15.25, 7.625]);
    }

    #[test]
    fn changing_crop_maps_a_child_through_its_parent() {
        let parent = PinnedRect::new([10., 20., 100., 80.], [4., 12., 200., 120.]).unwrap();
        assert_eq!(parent.layout_size(), [100., 60.]);
        let child = parent.to_local([14., 22., 180., 100.]);
        assert_eq!(child, [5., 5., 90., 50.]);
        assert!(PinnedRect::new(child, [0., 0., 0., 30.]).is_none());
    }
}
