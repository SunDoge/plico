//! Presentation geometry uses the same affine composition as scene painting.
use super::*;
use kurbo::{Affine, Point as KurboPoint, Rect as KurboRect};
pub(super) struct Presentation {
    world: Affine,
    opacity: f32,
}
impl Adapter {
    /// Iterate the placed ancestor chain starting at `start`, if it is placed.
    ///
    /// The walk ends at the root, or early at a zero-sized intermediate that
    /// the flattening walk omitted from `placed` while its (overflowing)
    /// children remain placed. Such an omitted node carries no effective clip
    /// — an empty clip would have culled its subtree — so every ancestor-chain
    /// invariant (clip folding, hit containment, scroll propagation, focus
    /// reveal) holds with the walk simply stopping there.
    fn placed_chain_from(&self, start: Option<NodeId>) -> impl Iterator<Item = &PlacedNode> + '_ {
        let mut current = start;
        std::iter::from_fn(move || {
            let id = current?;
            let node = self.placed.get(*self.placed_indices.get(&id)?)?;
            current = node.parent_node_id;
            Some(node)
        })
    }
    /// The placed chain of `id`, including `id` itself when placed.
    pub(super) fn placed_chain(&self, id: NodeId) -> impl Iterator<Item = &PlacedNode> + '_ {
        self.placed_chain_from(Some(id))
    }
    /// The placed ancestors of `id`, excluding `id` itself.
    pub(super) fn placed_ancestors(&self, id: NodeId) -> impl Iterator<Item = &PlacedNode> + '_ {
        let start = self
            .placed_indices
            .get(&id)
            .and_then(|&index| self.placed[index].parent_node_id);
        self.placed_chain_from(start)
    }
    pub(super) fn update_presentation(&mut self) {
        #[cfg(feature = "profiling")]
        let _span = tracing::trace_span!(target: "plico::layout", "backend.presentation").entered();
        if self.geometry_dirty || self.stats.flatten_visits > 0 || self.presentation.is_empty() {
            self.presentation.clear();
            for index in 0..self.placed.len() {
                self.update_presentation_node(index);
            }
            self.presentation_roots.clear();
        } else {
            let mut dirty = std::mem::take(&mut self.presentation_roots);
            for &id in &dirty {
                let covered = self
                    .placed_ancestors(id)
                    .any(|node| dirty.contains(&node.node_id));
                if !covered {
                    self.update_presentation_subtree(id);
                }
            }
            dirty.clear();
            self.presentation_roots = dirty;
        }
        self.presentation_dirty = false;
    }
    fn update_presentation_node(&mut self, index: usize) {
        let node = &self.placed[index];
        let (parent, opacity) = node
            .parent_node_id
            .and_then(|id| self.presentation.get(&id))
            .map_or((Affine::IDENTITY, 1.), |p| (p.world, p.opacity));
        self.presentation.insert(
            node.node_id,
            Presentation {
                world: plico_renderer::scene::resolve_node_transform(node, parent),
                opacity: opacity * node.paint.opacity,
            },
        );
        self.stats.presentation_visits += 1;
    }
    fn update_presentation_subtree(&mut self, id: NodeId) {
        let Some(&start) = self.placed_indices.get(&id) else {
            return;
        };
        let depth = self.placed[start].depth;
        self.update_presentation_node(start);
        // Flattening is preorder. Collapsed/clipped children remain in the
        // retained tree but may have no placed entry: traverse this projection,
        // rather than indexing every retained child as though it was visible.
        let mut index = start + 1;
        while index < self.placed.len() && self.placed[index].depth > depth {
            self.update_presentation_node(index);
            index += 1;
        }
    }
    /// Presentation bounds in logical window coordinates; layout rect stays unchanged.
    pub fn visual_bounds(&self, id: NodeId) -> Option<[f32; 4]> {
        let node = self.placed.get(*self.placed_indices.get(&id)?)?;
        let transform = self.presentation.get(&id)?.world;
        let r = node.rect.map(f64::from);
        let r = transform.transform_rect_bbox(KurboRect::new(r[0], r[1], r[2], r[3]));
        Some([r.x0 as f32, r.y0 as f32, r.x1 as f32, r.y1 as f32])
    }
    pub(super) fn visible_bounds(&self, id: NodeId) -> Option<[f32; 4]> {
        if self.presentation.get(&id)?.opacity <= 0. {
            return None;
        }
        let mut bounds = self.visual_bounds(id)?;
        for node in self.placed_chain(id) {
            if let Some(clip) = node.own_clip {
                let c = self.visual_rect(node.node_id, clip.map(f64::from))?;
                bounds = [
                    bounds[0].max(c[0] as f32),
                    bounds[1].max(c[1] as f32),
                    bounds[2].min(c[2] as f32),
                    bounds[3].min(c[3] as f32),
                ];
            }
        }
        (bounds[0] < bounds[2] && bounds[1] < bounds[3]).then_some(bounds)
    }
    pub(super) fn visual_rect(&self, id: NodeId, rect: [f64; 4]) -> Option<[f64; 4]> {
        let r = self
            .presentation
            .get(&id)?
            .world
            .transform_rect_bbox(KurboRect::new(rect[0], rect[1], rect[2], rect[3]));
        Some([r.x0, r.y0, r.x1, r.y1])
    }
    pub(super) fn layout_point(&self, id: NodeId, x: f64, y: f64) -> Option<KurboPoint> {
        let p = self.presentation.get(&id)?;
        if p.opacity <= 0. || p.world.determinant().abs() < 1e-12 {
            return None;
        }
        Some(p.world.inverse() * KurboPoint::new(x, y))
    }
    pub(super) fn visual_contains(&self, id: NodeId, x: f64, y: f64) -> bool {
        let Some(node) = self.placed_indices.get(&id).map(|i| &self.placed[*i]) else {
            return false;
        };
        let inside = |r: [f32; 4], point: KurboPoint| {
            point.x >= r[0] as f64
                && point.y >= r[1] as f64
                && point.x < r[2] as f64
                && point.y < r[3] as f64
        };
        if !self
            .layout_point(id, x, y)
            .is_some_and(|p| inside(node.rect, p))
        {
            return false;
        }
        self.placed_chain(id).all(|node| {
            node.own_clip.is_none_or(|clip| {
                self.layout_point(node.node_id, x, y)
                    .is_some_and(|p| inside(clip, p))
            })
        })
    }
}
