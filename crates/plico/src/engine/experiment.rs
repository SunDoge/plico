//! Restricted, feature-gated local declaration protocol for the region experiment.
use super::*;
use std::collections::{HashMap, HashSet};

/// A copied construction context. Contains IDs and values, never user references.
#[derive(Clone, Copy)]
pub struct ExperimentContext {
    pub parent: Entity,
    scope: u64,
    inherited: Style,
    in_button: bool,
}
impl UiContext<'_> {
    /// Re-enter a container declared earlier in the same full declaration pass.
    pub fn experiment_enter(
        &mut self,
        context: ExperimentContext,
        build: impl FnOnce(&mut UiContext<'_>),
    ) {
        build(&mut UiContext {
            engine: &mut *self.engine,
            parent: context.parent,
            scope: context.scope,
            inherited: context.inherited,
            in_button: context.in_button,
        });
    }
    pub fn experiment_context(&self) -> ExperimentContext {
        ExperimentContext {
            parent: self.parent,
            scope: self.scope,
            inherited: self.inherited,
            in_button: self.in_button,
        }
    }
    /// Keep this container's descendants without declaring/scanning them.
    /// The caller must replay children whose inherited context changed.
    pub fn experiment_keep_children(&mut self) {
        assert!(
            self.engine.experiment_active,
            "keep_children requires a local declaration"
        );
        self.engine.experiment_kept.insert(self.parent);
        let node = self.engine.nodes.get_mut(self.parent).unwrap();
        node.declared_children = node.children.len();
        node.order_dirty = false;
    }
}
impl Engine {
    pub fn experiment_replay_ready(&self) -> bool {
        !self.theme_dirty && !self.viewport_dirty
    }
    pub fn experiment_input_entities(&self) -> Vec<Entity> {
        self.clicks
            .iter()
            .copied()
            .chain(self.values.iter().map(|v| v.0))
            .chain(
                self.inputs
                    .iter()
                    .filter_map(|(&id, (buffer, _, previous))| {
                        (buffer.counters() != *previous).then_some(id)
                    }),
            )
            .collect()
    }
    pub fn experiment_parent(&self, entity: Entity) -> Option<Entity> {
        if entity == self.root {
            None
        } else {
            self.nodes.get(entity).map(|n| n.identity.parent)
        }
    }
    pub fn experiment_live_nodes(&self) -> usize {
        self.nodes.len()
    }

    /// Re-declare only an existing container's children. No global sweep, layout,
    /// portals, or automatic settle. The containing node itself remains alive.
    /// Like the normal declaration protocol, errors do not roll back mutations.
    pub fn experiment_redeclare(
        &mut self,
        context: ExperimentContext,
        view: impl FnOnce(&mut UiContext<'_>),
    ) -> Result<FrameStats, FrameError> {
        let parent = context.parent;
        if self.nodes.get(parent).is_none_or(|n| {
            n.kind.leaf()
                || n.style.font_size != context.inherited.font_size
                || n.style.font_weight != context.inherited.font_weight
                || n.style.font_family != context.inherited.font_family
                || n.style.foreground != context.inherited.foreground
                || n.style.disabled != context.inherited.disabled
        }) || self.theme_dirty
            || self.viewport_dirty
        {
            return Err(FrameError::InvalidRegion);
        }
        self.epoch = self
            .epoch
            .checked_add(1)
            .ok_or(FrameError::EpochExhausted)?;
        self.stats = FrameStats::default();
        self.dirty_nodes.clear();
        self.node_changes.clear();
        self.changes = Changes::empty();
        self.error = None;
        self.declarations.clear();
        self.experiment_kept.clear();
        let node = self.nodes.get_mut(parent).unwrap();
        node.touched = self.epoch;
        node.auto = 0;
        node.declared_children = 0;
        node.order_dirty = false;
        self.experiment_active = true;
        view(&mut UiContext {
            engine: self,
            parent,
            scope: context.scope,
            inherited: context.inherited,
            in_button: context.in_button,
        });
        self.experiment_active = false;
        if self
            .declarations
            .iter()
            .any(|(p, _)| self.experiment_kept.contains(p))
        {
            return Err(FrameError::InvalidRegion);
        }
        if let Some(error) = self.error.take() {
            return Err(error);
        }
        // Reject declarations which escaped the boundary, e.g. root portals.
        for &(p, _) in &self.declarations {
            let mut cursor = p;
            while cursor != parent {
                if cursor == self.root {
                    return Err(FrameError::InvalidRegion);
                }
                cursor = self
                    .nodes
                    .get(cursor)
                    .ok_or(FrameError::InvalidRegion)?
                    .identity
                    .parent;
            }
        }
        // Sweep the old hierarchy locally. Preserved child regions are opaque:
        // neither their declarations nor their descendants are visited.
        let mut stack = vec![parent];
        let mut removed = Vec::new();
        while let Some(id) = stack.pop() {
            if self.experiment_kept.contains(&id) {
                continue;
            }
            let node = self.nodes.get(id).unwrap();
            stack.extend(node.children.iter().copied());
            if node.touched != self.epoch {
                removed.push(id);
            }
        }
        for id in removed {
            let node = self.nodes.remove(id).unwrap();
            self.lookup.remove(&node.identity);
            self.inputs.remove(&id);
            self.scroll_viewports.remove(&id);
            self.removed_since_sync.push(id);
            self.stats.removed += 1;
        }
        let mut affected = vec![parent];
        affected.extend(self.declarations.iter().map(|&(_, id)| id));
        let declared: HashSet<_> = affected.iter().copied().collect();
        let kept = &self.experiment_kept;
        let epoch = self.epoch;
        self.locals.retain(|p, seen| {
            self.nodes.get(p).is_some()
                && (!declared.contains(&p) || kept.contains(&p) || seen == epoch)
        });
        let mut order: HashMap<Entity, Vec<Entity>> = HashMap::new();
        for &(p, id) in &self.declarations {
            order.entry(p).or_default().push(id);
        }
        for &id in &affected {
            if self.experiment_kept.contains(&id) {
                continue;
            }
            let node = self.nodes.get_mut(id).unwrap();
            let children = order.remove(&id).unwrap_or_default();
            node.order_dirty = node.children != children;
            if node.order_dirty {
                node.children = children;
                self.stats.order_changes += 1;
            }
        }
        if self.locals.take_changed() | self.storage.take_changed() {
            self.changes.insert(Changes::VIEW);
        }
        for id in affected {
            let node = self.nodes.get_mut(id).unwrap();
            let mut changes = std::mem::take(&mut node.pending_changes);
            let properties = std::mem::take(&mut node.pending_properties);
            if node.order_dirty {
                changes |= Changes::STRUCTURE | Changes::LAYOUT | Changes::SEMANTICS;
            }
            if node.paint_size != Some(node.size) {
                changes |= Changes::LAYOUT | Changes::SEMANTICS;
            }
            if !changes.is_empty() {
                self.dirty_nodes.push(id);
                self.node_changes.push(NodeChange {
                    entity: id,
                    changes,
                    properties,
                });
                self.changes |= changes;
            }
            if changes.intersects(
                Changes::LAYOUT | Changes::PAINT | Changes::MEASURE | Changes::TRANSFORM,
            ) {
                self.stats.paint_updates += 1;
            }
            node.paint_size = Some(node.size);
        }
        if self.stats.created > 0 || self.stats.removed > 0 || self.stats.order_changes > 0 {
            self.changes |=
                Changes::STRUCTURE | Changes::LAYOUT | Changes::PAINT | Changes::SEMANTICS;
        }
        Ok(self.stats)
    }
}
