//! Layout cache maintenance and control geometry alignment.
use super::*;

impl Adapter {
    pub(super) fn layout_changed(&mut self, entity: Entity) -> bool {
        self.stats.layout_cache_checks += 1;
        let Some(retained) = self.nodes.get(&entity) else {
            return true;
        };
        retained.layout.as_ref() != self.tree.layout(retained.id).ok()
    }

    pub(super) fn collect_changed_layouts(
        &mut self,
        engine: &Engine,
        entity: Entity,
        candidates: &mut HashSet<Entity>,
    ) {
        if self.layout_changed(entity) {
            candidates.insert(entity);
            return;
        }
        // A fixed-size parent can keep its own box while gap, alignment or
        // distribution changes move its children. Check the whole dirty
        // subtree instead of treating the unchanged parent as clean.
        if let Some(node) = engine.node(entity) {
            for &child in node.children {
                self.collect_changed_layouts(engine, child, candidates);
            }
        }
    }

    pub(super) fn cache_layout_subtree(&mut self, id: NodeId) {
        let layout = self.tree.layout(id).ok().copied();
        let children = self.reverse.get(&id).and_then(|entity| {
            let retained = self.nodes.get_mut(entity)?;
            retained.layout = layout;
            Some(retained.children.clone())
        });
        for child in children.into_iter().flatten() {
            self.cache_layout_subtree(child);
        }
    }

    pub(super) fn cache_all_layouts(&mut self) {
        let ids: Vec<_> = self.nodes.values().map(|retained| retained.id).collect();
        for id in ids {
            let layout = self.tree.layout(id).ok().copied();
            if let Some(entity) = self.reverse.get(&id)
                && let Some(retained) = self.nodes.get_mut(entity)
            {
                retained.layout = layout;
            }
        }
    }

    /// Patch only layout-affected subtrees. The ancestor/sibling checks are
    /// necessary because Taffy's clean cache does not mean that a child's new
    /// intrinsic size left its parent's other children in the same position.
    pub(super) fn patch_dirty_geometry(&mut self, engine: &Engine) -> Option<bool> {
        let mut candidates = HashSet::new();
        for &root in &self.topology_roots {
            if !self.nodes.contains_key(&root) {
                return None;
            }
            // Child insertion, removal and reordering change the preorder even
            // when every surviving Taffy box keeps its old layout.
            candidates.insert(root);
        }
        let dirty_roots: Vec<_> = self.geometry_roots.iter().copied().collect();
        for root in dirty_roots {
            self.collect_changed_layouts(engine, root, &mut candidates);
            let mut cursor = Some(root);
            while let Some(entity) = cursor {
                if !self.nodes.contains_key(&entity) {
                    return None;
                }
                if self.layout_changed(entity) {
                    candidates.insert(entity);
                }
                let Some(parent) = engine.parent(entity) else {
                    break;
                };
                if let Some(parent_node) = engine.node(parent) {
                    for &sibling in parent_node.children {
                        if self.layout_changed(sibling) {
                            candidates.insert(sibling);
                        }
                    }
                }
                cursor = Some(parent);
            }
        }

        // A changed ancestor already covers its descendants. Also promote a
        // previously unplaced (zero-size) node to its nearest placed ancestor,
        // which supplies a valid preorder range and inherited clip.
        let mut roots = Vec::new();
        for &entity in &candidates {
            let mut ancestor = engine.parent(entity);
            let mut covered = false;
            while let Some(parent) = ancestor {
                if candidates.contains(&parent) {
                    covered = true;
                    break;
                }
                ancestor = engine.parent(parent);
            }
            if covered {
                continue;
            }
            let mut cursor = Some(entity);
            let placed = loop {
                let current = cursor?;
                let retained = self.nodes.get(&current)?;
                if self.placed_indices.contains_key(&retained.id) {
                    break retained.id;
                }
                cursor = engine.parent(current);
            };
            roots.push(placed);
        }
        roots.sort_by_key(|id| self.placed_indices[id]);
        roots.dedup();

        let mut patches = Vec::new();
        let mut covered_end = 0;
        for id in roots {
            let start = self.placed_indices[&id];
            if start < covered_end {
                continue;
            }
            let depth = self.placed[start].depth;
            let end = self.placed[start + 1..]
                .iter()
                .position(|node| node.depth <= depth)
                .map_or(self.placed.len(), |offset| start + 1 + offset);
            covered_end = end;
            patches.push((start, end, id));
        }
        let structural =
            self.stats.created > 0 || self.stats.removed > 0 || self.stats.child_writes > 0;
        let covered: usize = patches.iter().map(|(start, end, _)| end - start).sum();
        if structural
            && self.placed.len() >= 32
            && (patches.len() > 1 || covered * 2 >= self.placed.len())
        {
            // Several structural ranges, or one range covering most of the
            // window, make a fresh preorder cheaper than checks and splices.
            // The candidate set is still exact enough for local Scene reuse.
            for entity in candidates.iter().chain(&self.geometry_roots) {
                if let Some(retained) = self.nodes.get(entity) {
                    self.scene_dirty_nodes.insert(retained.id);
                }
            }
            return Some(false);
        }
        for (start, end, id) in patches.into_iter().rev() {
            let entity = self.reverse.get(&id).copied()?;
            let previous_layout = self.nodes[&entity].layout?;
            let replacement = flatten_subtree_with_scroll(
                &self.tree,
                &self.placed[start],
                &previous_layout,
                &self.scroll_offsets,
            );
            self.stats.flatten_visits += replacement.len();
            self.stats.flatten_patches += 1;
            self.placed.splice(start..end, replacement);
            self.cache_layout_subtree(id);
            self.scene_dirty_nodes.insert(id);
        }
        for entity in &self.geometry_roots {
            if let Some(retained) = self.nodes.get(entity) {
                self.scene_dirty_nodes.insert(retained.id);
            }
        }
        Some(true)
    }

    /// Shift sticky nodes within their scroll port after each fresh flatten.
    /// Taffy lays them out in normal flow; this pass applies the CSS sticky
    /// top constraint and its containing-block limit.
    pub(super) fn apply_sticky(&mut self) {
        let mut index = 0;
        while index < self.placed.len() {
            let Some(top) = self.placed[index].paint.sticky_top else {
                index += 1;
                continue;
            };
            let parent = self.placed[index].parent_node_id;
            let mut cursor = parent;
            let mut port_top = None;
            while let Some(id) = cursor {
                let Some(&placed) = self.placed_indices.get(&id) else {
                    break;
                };
                let node = &self.placed[placed];
                if node.scroll.scrollable[1] {
                    port_top = Some(node.scroll.port[1]);
                    break;
                }
                cursor = node.parent_node_id;
            }
            let parent_bottom = parent
                .and_then(|id| self.placed_indices.get(&id))
                .map(|&placed| self.placed[placed].rect[3])
                .unwrap_or(f32::INFINITY);
            let node = &self.placed[index];
            let natural = node.rect[1];
            let height = node.rect[3] - natural;
            let push = node.paint.sticky_push;
            let Some(port_top) = port_top else {
                index += 1;
                continue;
            };
            let depth = node.depth;
            let mut end = index + 1;
            while end < self.placed.len() && self.placed[end].depth > depth {
                end += 1;
            }
            let next_sticky_top = push.then(|| {
                self.placed[end..]
                    .iter()
                    .take_while(|candidate| candidate.depth >= depth)
                    .find(|candidate| {
                        candidate.depth == depth
                            && candidate.parent_node_id == parent
                            && candidate.paint.sticky_top.is_some()
                    })
                    .map(|candidate| candidate.rect[1])
            });
            let mut maximum = (parent_bottom - height - natural).max(0.);
            if let Some(Some(next_top)) = next_sticky_top {
                maximum = maximum.min((next_top - height - natural).max(0.));
            }
            let delta = (port_top + top - natural).clamp(0., maximum);
            if delta > 0.01 {
                for node in &mut self.placed[index..end] {
                    node.rect[1] += delta;
                    node.rect[3] += delta;
                    node.content_origin[1] += delta;
                    if let Some(clip) = &mut node.own_clip {
                        clip[1] += delta;
                        clip[3] += delta;
                    }
                    node.scroll.port[1] += delta;
                    node.scroll.port[3] += delta;
                }
            }
            index = end;
        }
    }
    pub(super) fn reindex_placed(&mut self) {
        #[cfg(feature = "profiling")]
        let _span = tracing::trace_span!(target: "plico::layout", "backend.index").entered();
        self.hit.get_mut().dirty = true;
        self.placed_indices.clear();
        self.scroll_indices.clear();
        self.control_align_indices.clear();
        self.input_align_indices.clear();
        for (index, node) in self.placed.iter_mut().enumerate() {
            self.placed_indices.insert(node.node_id, index);
            // Reset all scroll projections while already rebuilding this index.
            // Later paint-only updates need to visit actual scroll ports only.
            node.scroll.opacity = 0.;
            node.scroll.interaction = if self
                .scrollbar_drag
                .is_some_and(|(id, ..)| id == node.node_id)
            {
                2
            } else {
                0
            };
            if node.scroll.scrollable[1] {
                self.scroll_indices.push(index);
            }
        }
        self.scrollbars.states.retain(|id, _| {
            self.placed_indices
                .get(id)
                .is_some_and(|&index| self.placed[index].scroll.scrollable[1])
        });
        // Membership is maintained by targeted sync. Do not fetch the large
        // retained record for every placed label/container just to classify it.
        for (members, indices) in [
            (&self.control_align_nodes, &mut self.control_align_indices),
            (&self.input_align_nodes, &mut self.input_align_indices),
        ] {
            for entity in members {
                if let Some(index) = self.placed_indices.get(&self.nodes[entity].id) {
                    indices.push(*index);
                }
            }
        }
    }
    pub(super) fn align_controls(&mut self, tcx: &mut TextContext, engine: &Engine) {
        for slot in 0..self.control_align_indices.len() {
            self.align_control_text(self.control_align_indices[slot], tcx);
        }
        for slot in 0..self.input_align_indices.len() {
            self.align_input(self.input_align_indices[slot], tcx, engine);
        }
    }
    /// Center the shaped text block in leaf controls. Indicators then follow its
    /// first line, so wrapped labels do not drag the marker to the paragraph center.
    pub(super) fn align_control_text(&mut self, index: usize, tcx: &mut TextContext) {
        self.stats.control_align_checks += 1;
        let placed = &mut self.placed[index];
        let Some(&entity) = self.reverse.get(&placed.node_id) else {
            return;
        };
        let retained = self.nodes.get_mut(&entity).unwrap();
        let PaintKey {
            style,
            kind,
            theme,
            checked_progress: progress,
            hovered,
            ..
        } = retained.paint_key;
        if !matches!(kind, Kind::Button | Kind::Switch | Kind::Checkbox) {
            return;
        }
        let epoch = tcx.cache_generation();
        let metrics = if let Some(cached) = self.control_metrics.get(&entity)
            && cached.epoch == epoch
            && cached.width == placed.content_size[0]
        {
            self.stats.control_metric_reuses += 1;
            cached.metrics
        } else {
            self.stats.control_metric_builds += 1;
            let metrics = plico_renderer::scene::layout_node_paragraph(tcx, placed).map(|layout| {
                let metrics = layout.metrics();
                [metrics.width, metrics.height, metrics.first_line_height]
            });
            self.control_metrics.insert(
                entity,
                ControlMetrics {
                    epoch,
                    width: placed.content_size[0],
                    metrics,
                },
            );
            metrics
        };
        if let Some([text_width, text_height, _]) = metrics {
            let geometry = self.tree.layout(placed.node_id).unwrap();
            // Rebuild from the box every time: paint-only animation frames reuse
            // placed nodes, and adding an offset would accumulate vertical drift.
            placed.content_origin[1] = placed.rect[1]
                + geometry.border.top
                + geometry.padding.top
                + (placed.content_size[1] - text_height).max(0.) / 2.;
            if kind == Kind::Button {
                placed.content_origin[0] = placed.rect[0]
                    + geometry.border.left
                    + geometry.padding.left
                    + (placed.content_size[0] - text_width).max(0.) / 2.;
            }
        }
        if kind == Kind::Button {
            return;
        }
        let line_height = metrics
            .map(|metrics| metrics[2])
            .unwrap_or(placed.content_size[1]);
        if placed.paint.widget.is_some() && retained.toggle_line_height == Some(line_height) {
            return;
        }
        let check = if kind == Kind::Checkbox && progress > 0. {
            cached_svg(
                &mut self.svg_cache,
                &mut self.stats,
                self.resource_generation,
                plico::icons::CHECK.source(),
                Some(theme.on_accent),
            )
        } else {
            None
        };
        let widget = Arc::new(toggle::paint(
            kind,
            style,
            theme,
            progress,
            line_height,
            hovered,
            check.as_deref(),
        ));
        placed.paint.widget = Some(widget.clone());
        self.tree
            .get_node_context_mut(placed.node_id)
            .unwrap()
            .widget = Some(widget);
        retained.toggle_line_height = Some(line_height);
    }
}
