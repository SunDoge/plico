//! Opt-in subtree transactions; commit/layout still scan the common tree.
use super::*;

/// Engine-local, generational anchor. Never mix handles from different Engines.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RetainedScope {
    entity: Entity,
    owner: usize,
}
impl RetainedScope {
    pub fn entity(self) -> Entity {
        self.entity
    }
}
pub(super) fn next_owner() -> usize {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static NEXT: AtomicUsize = AtomicUsize::new(1);
    NEXT.try_update(Ordering::Relaxed, Ordering::Relaxed, |id| id.checked_add(1))
        .expect("Engine scope identity exhausted")
}
impl Engine {
    pub(crate) fn retained_token(&self, entity: Entity) -> RetainedScope {
        RetainedScope {
            entity,
            owner: self.scope_owner,
        }
    }
    pub fn scope_mounted(&self, scope: RetainedScope) -> bool {
        scope.owner == self.scope_owner
            && self
                .nodes
                .get(scope.entity)
                .is_some_and(|node| node.retained)
    }
    /// Nearest retained ancestor, used to route input to its owning component.
    pub fn scope_for_entity(&self, mut entity: Entity) -> Option<RetainedScope> {
        loop {
            let node = self.nodes.get(entity)?;
            if node.retained {
                return Some(self.retained_token(entity));
            }
            if entity == self.root {
                return None;
            }
            entity = node.identity.parent;
        }
    }
    /// Nearest enclosing retained anchor, excluding this anchor itself.
    pub fn scope_parent(&self, scope: RetainedScope) -> Option<RetainedScope> {
        if !self.scope_mounted(scope) {
            return None;
        }
        let node = self.nodes.get(scope.entity)?;
        if scope.entity == self.root {
            return None;
        }
        self.scope_for_entity(node.identity.parent)
    }
    pub(super) fn in_subtree(&self, mut entity: Entity, root: Entity) -> bool {
        loop {
            if entity == root {
                return true;
            }
            let Some(node) = self.nodes.get(entity) else {
                return false;
            };
            if entity == self.root {
                return false;
            }
            entity = node.identity.parent;
        }
    }
    pub(super) fn scope_in_button(&self, mut entity: Entity) -> bool {
        while let Some(node) = self.nodes.get(entity) {
            if node.kind == Kind::ButtonBase {
                return true;
            }
            if entity == self.root {
                break;
            }
            entity = node.identity.parent;
        }
        false
    }
    pub(crate) fn preserve_children(&mut self, entity: Entity) {
        let node = self.nodes.get_mut(entity).unwrap();
        node.retained = true;
        node.declared_children = node.children.len();
        node.order_dirty = false;
        self.scope_stack.clear();
        self.scope_stack.extend_from_slice(&node.children);
        while let Some(id) = self.scope_stack.pop() {
            let node = self.nodes.get_mut(id).unwrap();
            node.touched = self.epoch;
            node.declared_children = node.children.len();
            node.order_dirty = false;
            self.scope_stack.extend_from_slice(&node.children);
        }
    }
    pub(super) fn prepare_partial(&mut self, root: Entity) {
        self.scope_members.clear();
        self.scope_stack.clear();
        self.scope_stack.push(root);
        while let Some(id) = self.scope_stack.pop() {
            self.scope_members.insert(id);
            self.scope_stack
                .extend_from_slice(&self.nodes.get(id).unwrap().children);
        }
        // Preserve all nodes outside the participating subtree. This is O(N),
        // intentionally a correctness baseline rather than a dirty-region index.
        for (id, node) in self.nodes.iter_mut() {
            if !self.scope_members.contains(&id) {
                node.touched = self.epoch;
                node.declared_children = node.children.len();
                node.order_dirty = false;
            }
        }
    }
    pub(super) fn retain_scope_events(&mut self, active: Option<Entity>) {
        let nodes = &self.nodes;
        let root = self.root;
        let keep = |mut id: Entity| {
            if nodes.get(id).is_none() {
                return false;
            }
            let owner = loop {
                let node = nodes.get(id).unwrap();
                if node.retained {
                    break Some(id);
                }
                if id == root {
                    break None;
                }
                id = node.identity.parent;
            };
            match active {
                Some(active) => owner != Some(active),
                None => owner.is_some(),
            }
        };
        self.clicks.retain(|&id| keep(id));
        self.values.retain(|&(id, _)| keep(id));
    }
    fn scoped(
        &mut self,
        scope: RetainedScope,
        view: impl FnOnce(&mut UiContext<'_>),
        measure: bool,
    ) -> Result<FrameStats, FrameError> {
        if !self.scope_mounted(scope) {
            return Err(FrameError::MissingScope {
                entity: scope.entity,
            });
        }
        // Clear the active bound even when authored code panics; resume the panic
        // rather than translating it or leaving the Engine stuck in a scope.
        self.partial_root = Some(scope.entity);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            self.frame_target(view, measure, Some(scope.entity))
        }));
        self.partial_root = None;
        if !matches!(result, Ok(Ok(_))) {
            // Newly allocated nodes in a rejected transaction never entered
            // committed children. Remove them now, rather than treating them as
            // unrelated preserved nodes in the next partial transaction.
            for id in self.scope_created.drain(..) {
                if let Some(node) = self.nodes.remove(id) {
                    self.lookup.remove(&node.identity);
                    self.inputs.remove(&id);
                    self.scroll_viewports.remove(&id);
                    self.scroll_requests.remove(&id);
                    if self.focus_request == Some(id) {
                        self.focus_request = None;
                    }
                }
            }
        }
        match result {
            Ok(result) => result,
            Err(panic) => std::panic::resume_unwind(panic),
        }
    }
    /// Update only this anchor's declarations; simulated layout uses the shared root.
    /// Direct queued events for other scopes survive. Path input is not finalized here.
    pub fn frame_scope(
        &mut self,
        scope: RetainedScope,
        view: impl FnOnce(&mut UiContext<'_>),
    ) -> Result<FrameStats, FrameError> {
        self.scoped(scope, view, true)
    }
    /// Same transaction when an external backend owns measurement and layout.
    pub fn frame_scope_declarations(
        &mut self,
        scope: RetainedScope,
        view: impl FnOnce(&mut UiContext<'_>),
    ) -> Result<FrameStats, FrameError> {
        self.scoped(scope, view, false)
    }
}
