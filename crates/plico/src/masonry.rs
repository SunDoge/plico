/// Logical placement of one item in a masonry layout.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MasonrySlot {
    pub column: usize,
    pub top: f32,
}

/// Places items of varying height in the shortest column, breaking ties by
/// column order. Use the same slots for drawing and for geometry such as
/// scroll anchors or shared-element destinations.
pub struct MasonryLayout {
    next_tops: Vec<f32>,
    gap: f32,
}

impl MasonryLayout {
    /// Construct a layout with at least one column and a finite, nonnegative gap.
    pub fn new(columns: std::num::NonZeroUsize, gap: f32) -> Self {
        assert!(gap.is_finite() && gap >= 0.);
        Self {
            next_tops: vec![0.; columns.get()],
            gap,
        }
    }

    /// Allocate the next slot for a finite, nonnegative height.
    pub fn push(&mut self, height: f32) -> MasonrySlot {
        assert!(height.is_finite() && height >= 0.);
        let column = (0..self.next_tops.len())
            .min_by(|&a, &b| self.next_tops[a].total_cmp(&self.next_tops[b]))
            .expect("masonry has at least one column");
        let top = self.next_tops[column];
        self.next_tops[column] += height + self.gap;
        MasonrySlot { column, top }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maintains_column_order_and_includes_gap_in_each_slot() {
        let mut layout = MasonryLayout::new(3.try_into().unwrap(), 4.);
        let slots = [20., 10., 10., 8., 30., 10.].map(|height| layout.push(height));
        assert_eq!(slots.map(|slot| slot.column), [0, 1, 2, 1, 2, 0]);
        assert_eq!(slots.map(|slot| slot.top), [0., 0., 0., 14., 14., 24.]);
    }
}
