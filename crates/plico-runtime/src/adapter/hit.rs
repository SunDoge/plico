//! Cached subtree hit testing.
use super::*;

impl Adapter {
    pub(crate) fn hit(&self, engine: &Engine, x: f64, y: f64) -> Option<Entity> {
        if self.placed.is_empty() {
            return None;
        }
        if self.hit.borrow().dirty || self.hit.borrow().subtrees.len() != self.placed.len() {
            self.rebuild_hit_cache();
        }
        let contains = |bounds: [f32; 4]| {
            x >= f64::from(bounds[0])
                && y >= f64::from(bounds[1])
                && x < f64::from(bounds[2])
                && y < f64::from(bounds[3])
        };
        let mut checks = 0;
        let mut pruned = 0;
        let mut result = None;
        let mut tasks = vec![(0, false)];
        let cache = self.hit.borrow();
        while let Some((index, visited)) = tasks.pop() {
            let subtree = cache.subtrees[index];
            if !contains(subtree.bounds) {
                pruned += 1;
                continue;
            }
            if !visited {
                // Children are pushed in source order and popped in reverse,
                // preserving placed.iter().rev() hit priority. The parent is
                // tested only after every descendant.
                tasks.push((index, true));
                let mut child = subtree.first_child;
                while let Some(index) = child {
                    tasks.push((index, false));
                    child = cache.subtrees[index].next_sibling;
                }
                continue;
            }
            checks += 1;
            let placed = &self.placed[index];
            if !self.visual_contains(placed.node_id, x, y) {
                continue;
            }
            let Some(entity) = self.reverse.get(&placed.node_id).copied() else {
                continue;
            };
            if engine.node(entity).is_some_and(|node| {
                interactive(node.kind)
                    && node.kind != Kind::Overlay
                    && !node.style.disabled
                    && self.modal_allows(engine, entity)
            }) {
                result = Some(entity);
                break;
            }
        }
        drop(cache);
        let mut cache = self.hit.borrow_mut();
        cache.checks = checks;
        cache.pruned = pruned;
        result
    }
    pub(super) fn rebuild_hit_cache(&self) {
        let mut cache = self.hit.borrow_mut();
        cache.subtrees.clear();
        cache
            .subtrees
            .resize(self.placed.len(), HitSubtree::default());
        let mut ancestors: Vec<usize> = Vec::new();
        let mut last_children: Vec<Option<usize>> = vec![None; self.placed.len()];
        for index in 0..self.placed.len() {
            let depth = self.placed[index].depth;
            while ancestors.len() > depth {
                let ended = ancestors.pop().unwrap();
                cache.subtrees[ended].end = index;
            }
            if let Some(&parent) = ancestors.last() {
                let first = &mut cache.subtrees[parent].first_child;
                if first.is_none() {
                    *first = Some(index);
                }
                if let Some(previous) = last_children[parent] {
                    cache.subtrees[previous].next_sibling = Some(index);
                }
                last_children[parent] = Some(index);
            }
            ancestors.push(index);
        }
        while let Some(ended) = ancestors.pop() {
            cache.subtrees[ended].end = self.placed.len();
        }
        for index in (0..self.placed.len()).rev() {
            let placed = &self.placed[index];
            let rect = self.visual_bounds(placed.node_id).unwrap_or(placed.rect);
            let mut bounds = (rect[0] < rect[2] && rect[1] < rect[3]).then_some(rect);
            let mut child = cache.subtrees[index].first_child;
            while let Some(child_index) = child {
                let child_bounds = cache.subtrees[child_index].bounds;
                if child_bounds[0] < child_bounds[2] && child_bounds[1] < child_bounds[3] {
                    bounds = Some(bounds.map_or(child_bounds, |old| {
                        [
                            old[0].min(child_bounds[0]),
                            old[1].min(child_bounds[1]),
                            old[2].max(child_bounds[2]),
                            old[3].max(child_bounds[3]),
                        ]
                    }));
                }
                child = cache.subtrees[child_index].next_sibling;
            }
            cache.subtrees[index].bounds = bounds.unwrap_or_default();
        }
        cache.builds += 1;
        cache.dirty = false;
    }
    #[doc(hidden)]
    pub fn hit_cache_stats(&self) -> (usize, usize, usize) {
        let cache = self.hit.borrow();
        (cache.builds, cache.checks, cache.pruned)
    }
}
