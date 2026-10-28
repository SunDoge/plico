#[cfg(feature = "experiment-patch")]
mod experiment;
use crate::{Entity, Theme, arena::Arena, model::*, ui::UiContext};
#[cfg(feature = "experiment-patch")]
pub use experiment::ExperimentContext;
use std::{borrow::Cow, collections::HashMap};

/// A view the engine can settle: theme and declarations, driven per pass.
/// Implemented by application types on both the native and reload paths.
pub trait DeclareView {
    fn theme(&self) -> crate::Theme;
    fn show(&mut self, ui: &mut crate::UiContext<'_>);
}
/// Single-threaded, retained storage behind an immediate declaration API.
pub struct Engine {
    pub(crate) nodes: Arena<Node>,
    lookup: HashMap<Identity, Entity>,
    pub(crate) root: Entity,
    pub(crate) epoch: u64,
    pub(crate) stats: FrameStats,
    pub(crate) error: Option<FrameError>,
    declarations: Vec<(Entity, Entity)>,
    #[cfg(feature = "experiment-patch")]
    pub(crate) experiment_kept: std::collections::HashSet<Entity>,
    #[cfg(feature = "experiment-patch")]
    pub(crate) experiment_active: bool,
    pub(crate) locals: crate::local::LocalStore,
    pub(crate) storage: crate::storage::StateStorage,
    removed: Vec<Entity>,
    /// Entities removed since the last drain, kept across declaration passes so
    /// a host syncing once after a settle loop still sees every removal.
    removed_since_sync: Vec<Entity>,
    unmatched_clicks: Vec<Vec<crate::PathKey>>,
    unmatched_values: Vec<(Vec<crate::PathKey>, f64)>,
    settle_dirty: Vec<Entity>,
    settle_node_changes: HashMap<Entity, StyleDiff>,
    settle_changes: Changes,
    dirty_nodes: Vec<Entity>,
    node_changes: Vec<NodeChange>,
    hovered: Option<Entity>,
    hovered_path: Option<Vec<crate::PathKey>>,
    clicks: Vec<Entity>,
    click_paths: Vec<Vec<crate::PathKey>>,
    values: Vec<(Entity, f64)>,
    value_paths: Vec<(Vec<crate::PathKey>, f64)>,
    pub(crate) inputs: HashMap<Entity, (crate::TextBuffer, String, (u64, u64))>,
    pub(crate) focus_request: Option<Entity>,
    pub(crate) scroll_requests: HashMap<Entity, f32>,
    scroll_viewports: HashMap<Entity, [f32; 2]>,
    scroll_paths: Vec<(Vec<crate::PathKey>, [f32; 2])>,
    width: f32,
    height: f32,
    theme: Theme,
    theme_dirty: bool,
    viewport_dirty: bool,
    pub(crate) changes: Changes,
    pub(crate) time: crate::FrameTime,
    pub(crate) format_buffer: String,
}
impl Default for Engine {
    fn default() -> Self {
        let mut nodes = Arena::default();
        let placeholder = Entity::default();
        let root = nodes.insert(Node::new(
            Identity {
                parent: placeholder,
                key: Key::Static("root"),
                scope: 0,
            },
            Kind::Root,
        ));
        Self {
            nodes,
            lookup: HashMap::new(),
            root,
            epoch: 0,
            stats: FrameStats::default(),
            error: None,
            declarations: Vec::new(),
            #[cfg(feature = "experiment-patch")]
            experiment_kept: Default::default(),
            #[cfg(feature = "experiment-patch")]
            experiment_active: false,
            locals: Default::default(),
            storage: Default::default(),
            removed: Vec::new(),
            removed_since_sync: Vec::new(),
            unmatched_clicks: Vec::new(),
            unmatched_values: Vec::new(),
            settle_dirty: Vec::new(),
            settle_node_changes: HashMap::new(),
            settle_changes: Changes::empty(),
            dirty_nodes: Vec::new(),
            node_changes: Vec::new(),
            hovered: None,
            hovered_path: None,
            clicks: Vec::new(),
            click_paths: Vec::new(),
            values: Vec::new(),
            value_paths: Vec::new(),
            inputs: HashMap::new(),
            focus_request: None,
            scroll_requests: HashMap::new(),
            scroll_viewports: HashMap::new(),
            scroll_paths: Vec::new(),
            width: 720.,
            height: 820.,
            theme: Theme::default(),
            theme_dirty: false,
            viewport_dirty: true,
            changes: Changes::empty(),
            time: Default::default(),
            format_buffer: String::new(),
        }
    }
}
impl Engine {
    /// Serialize only when a host requests a checkpoint, never during normal frames.
    pub fn persisted_snapshot(&self) -> Result<crate::StateStorageSnapshot, crate::StorageError> {
        self.storage.snapshot()
    }
    /// Atomically replace storage. Call before declaring restored UI.
    pub fn restore_persisted(
        &mut self,
        snapshot: crate::StateStorageSnapshot,
    ) -> Result<(), crate::StorageError> {
        self.storage.restore(snapshot)
    }
    /// Export storage bytes for host-managed local storage or disk files.
    pub fn dump_persisted(&self) -> Result<Vec<u8>, crate::StorageError> {
        rmp_serde::to_vec_named(&self.persisted_snapshot()?)
            .map_err(|source| crate::StorageError::Encode { source })
    }
    /// Decode and validate before replacing live storage; malformed input is atomic.
    pub fn load_persisted(&mut self, bytes: &[u8]) -> Result<(), crate::StorageError> {
        let snapshot = rmp_serde::from_slice(bytes)
            .map_err(|source| crate::StorageError::Decode { source })?;
        self.restore_persisted(snapshot)
    }
    pub fn clear_persisted(&mut self) {
        self.storage.clear();
    }
    /// Per-slot migration failures; only incompatible slots are reset.
    pub fn take_persisted_resets(&mut self) -> Vec<String> {
        self.storage.take_resets()
    }

    /// Accessible name: explicit text wins; composable buttons derive it from text descendants.
    pub fn accessible_name(&self, entity: Entity) -> Option<Cow<'_, str>> {
        let node = self.nodes.get(entity)?;
        if matches!(node.kind, Kind::Svg | Kind::Shader | Kind::TooltipAnchor) {
            return None;
        }
        if !node.text.is_empty() {
            return Some(Cow::Borrowed(&node.text));
        }
        if node.kind != Kind::ButtonBase {
            return None;
        }
        fn append(engine: &Engine, entity: Entity, text: &mut String) {
            let Some(node) = engine.nodes.get(entity) else {
                return;
            };
            if node.kind == Kind::Text && !node.text.is_empty() {
                if !text.is_empty() {
                    text.push(' ');
                }
                text.push_str(&node.text);
            }
            for child in &node.children {
                append(engine, *child, text);
            }
        }
        let mut text = String::new();
        for child in &node.children {
            append(self, *child, &mut text);
        }
        (!text.is_empty()).then_some(Cow::Owned(text))
    }

    /// Consume a one-shot native focus request. Invalid/unmounted targets are ignored.
    pub fn take_focus_request(&mut self) -> Option<Entity> {
        self.focus_request
            .take()
            .filter(|e| self.nodes.get(*e).is_some())
    }
    /// Consume programmatic scroll requests emitted during declaration.
    pub fn take_scroll_requests(&mut self) -> HashMap<Entity, f32> {
        std::mem::take(&mut self.scroll_requests)
    }
    /// Host feedback in logical pixels: vertical offset and viewport height.
    pub fn set_scroll_viewport(&mut self, entity: Entity, offset: f32, height: f32) {
        if self.nodes.get(entity).is_some() && offset.is_finite() && height.is_finite() {
            self.scroll_viewports
                .insert(entity, [offset.max(0.), height.max(0.)]);
        }
    }
    /// Portable host feedback for modules that reconstruct their engine per call.
    pub fn set_scroll_viewport_paths(&mut self, paths: Vec<(Vec<crate::PathKey>, [f32; 2])>) {
        self.scroll_paths = paths
            .into_iter()
            .filter(|(_, v)| v.iter().all(|n| n.is_finite() && *n >= 0.))
            .collect();
    }
    /// Backend query: did this container's declaration read its scroll geometry?
    #[doc(hidden)]
    pub fn observes_scroll(&self, entity: Entity) -> bool {
        self.nodes
            .get(entity)
            .is_some_and(|node| node.scroll_observed.get())
    }
    pub(crate) fn scroll_viewport(&self, entity: Entity) -> Option<[f32; 2]> {
        if let Some(node) = self.nodes.get(entity) {
            node.scroll_observed.set(true);
        }
        self.scroll_viewports.get(&entity).copied().or_else(|| {
            self.scroll_paths
                .iter()
                .find(|(path, _)| self.matches_path(entity, path))
                .map(|(_, viewport)| *viewport)
        })
    }
    pub(crate) fn lookup_shader(&self, identity: Identity) -> Option<u64> {
        self.lookup
            .get(&identity)
            .and_then(|e| self.nodes.get(*e))
            .map(|n| n.style.shader_revision)
    }
    pub(crate) fn lookup_numeric(&self, identity: Identity) -> Option<u64> {
        self.lookup
            .get(&identity)
            .and_then(|e| self.nodes.get(*e))
            .map(|n| n.style.numeric_revision)
    }
    pub fn viewport_size(&self) -> [f32; 2] {
        [self.width, self.height]
    }
    pub fn set_viewport(&mut self, width: f32, height: f32) -> Result<(), FrameError> {
        if !height.is_finite() || height <= 0. {
            return Err(FrameError::InvalidViewport);
        }
        self.set_width(width)?;
        self.viewport_dirty |= self.height != height;
        self.height = height;
        Ok(())
    }
    pub fn viewport_width(&self) -> f32 {
        self.width
    }
    pub fn set_width(&mut self, width: f32) -> Result<(), FrameError> {
        if !width.is_finite() || width <= 0. {
            return Err(FrameError::InvalidViewport);
        }
        self.viewport_dirty |= self.width != width;
        self.width = width;
        Ok(())
    }
    pub fn set_theme(&mut self, theme: Theme) {
        if self.theme != theme {
            self.theme = theme;
            self.theme_dirty = true;
        }
    }
    pub fn theme(&self) -> Theme {
        self.theme
    }
    pub fn root(&self) -> Entity {
        self.root
    }
    /// Find a unique direct child by explicit local key, across component scopes.
    /// Returns None when several scoped instances use that local key.
    pub fn find(&self, parent: Entity, key: impl Into<Key>) -> Option<Entity> {
        let key = key.into();
        let mut matches = self
            .nodes
            .get(parent)?
            .children
            .iter()
            .copied()
            .filter(|child| {
                self.nodes
                    .get(*child)
                    .is_some_and(|node| node.identity.key == key)
            });
        let first = matches.next()?;
        matches.next().is_none().then_some(first)
    }
    pub fn node(&self, entity: Entity) -> Option<NodeRef<'_>> {
        let n = self.nodes.get(entity)?;
        Some(NodeRef {
            entity,
            kind: n.kind,
            text: &n.text,
            spans: &n.spans,
            grid: n.grid.as_ref(),
            shader: n.shader.as_ref(),
            numeric: n.numeric,
            style: n.style,
            size: n.size,
            children: &n.children,
        })
    }
    pub fn time(&self) -> crate::FrameTime {
        self.time
    }
    pub fn set_time(&mut self, time: crate::FrameTime) {
        self.time = time;
    }
    /// Benchmark-only direct text mutation. This bypasses the declaration protocol
    /// and is valid only for an existing plain text leaf owned by the experiment.
    #[doc(hidden)]
    #[cfg(feature = "experiment-patch")]
    pub fn experiment_patch_text(&mut self, entity: Entity, text: &str) -> Option<bool> {
        let node = self.nodes.get_mut(entity)?;
        if node.kind != Kind::Text || !node.spans.is_empty() {
            return None;
        }
        if node.text == text {
            return Some(false);
        }
        node.text.clear();
        node.text.push_str(text);
        node.text_revision += 1;
        Some(true)
    }

    /// Benchmark-only mutation of a plain text leaf's foreground.
    #[doc(hidden)]
    #[cfg(feature = "experiment-patch")]
    pub fn experiment_patch_color(&mut self, entity: Entity, color: crate::Color) -> Option<bool> {
        let node = self.nodes.get_mut(entity)?;
        if node.kind != Kind::Text || !node.spans.is_empty() {
            return None;
        }
        if node.style.foreground == color {
            return Some(false);
        }
        node.style.foreground = color;
        Some(true)
    }

    /// FrameStats of the most recent declaration pass.
    pub fn last_stats(&self) -> FrameStats {
        self.stats
    }
    /// Nodes whose declared properties changed in the latest successful pass.
    pub fn node_changes(&self) -> &[NodeChange] {
        &self.node_changes
    }
    /// Entities removed since the last drain, accumulated across declaration
    /// passes. Structural hosts call this instead of rescanning the arena;
    /// the simulated-measurement `frame` path clears it per frame instead.
    pub fn take_removed(&mut self) -> Vec<Entity> {
        std::mem::take(&mut self.removed_since_sync)
    }
    /// Consume the removal journal while retaining its allocation for the next batch.
    pub fn drain_removed(&mut self) -> impl Iterator<Item = Entity> + '_ {
        self.removed_since_sync.drain(..)
    }
    /// Path-addressed clicks and values that no declaration consumed. Reload
    /// hosts surface them so identity drift after an edit is observable.
    pub fn take_unmatched_paths(&mut self) -> UnmatchedPaths {
        UnmatchedPaths {
            clicks: std::mem::take(&mut self.unmatched_clicks),
            values: std::mem::take(&mut self.unmatched_values),
        }
    }
    pub fn dirty_nodes(&self) -> &[Entity] {
        &self.dirty_nodes
    }
    pub fn presence_deadline(&self) -> Option<std::time::Duration> {
        self.nodes
            .iter()
            .filter_map(|(_, n)| n.style.presence_until_ms)
            .min()
            .map(std::time::Duration::from_millis)
    }
    pub fn changes(&self) -> Changes {
        self.changes
    }
    /// Native bindings are deliberately excluded from snapshot()/ViewNode.
    pub fn input(&self, entity: Entity) -> Option<(&crate::TextBuffer, &str)> {
        self.inputs
            .get(&entity)
            .map(|(buffer, hint, _)| (buffer, hint.as_str()))
    }
    /// Stable logical path of a node, rebuilt from identity keys and scopes.
    /// Reload transports use it to address native bindings across images.
    pub fn logical_path(&self, entity: Entity) -> Vec<crate::PathKey> {
        let mut path = Vec::new();
        let mut current = entity;
        while let Some(node) = self.nodes.get(current)
            && current != self.root
        {
            path.push(crate::PathKey::scoped(
                node.identity.key,
                node.identity.scope,
            ));
            current = node.identity.parent;
        }
        path.reverse();
        path
    }
    /// All live native input bindings with their logical paths.
    pub fn input_bindings(
        &self,
    ) -> impl Iterator<Item = (Vec<crate::PathKey>, &crate::TextBuffer)> + '_ {
        self.nodes.iter().filter_map(|(entity, _)| {
            self.inputs
                .get(&entity)
                .map(|(buffer, _, _)| (self.logical_path(entity), buffer))
        })
    }
    #[doc(hidden)]
    pub fn restore_input_submit(&mut self, path: &[crate::PathKey], count: u64) {
        let entity = self
            .inputs
            .keys()
            .copied()
            .find(|entity| self.logical_path(*entity) == path);
        if let Some(entity) = entity
            && let Some((buffer, _, previous)) = self.inputs.get_mut(&entity)
        {
            buffer.restore_submit_count(count);
            *previous = buffer.counters();
        }
    }
    #[doc(hidden)]
    pub fn input_hint_path(&self, path: &[crate::PathKey]) -> Option<&str> {
        self.inputs
            .iter()
            .find(|(entity, _)| self.logical_path(**entity) == path)
            .map(|(_, (_, hint, _))| hint.as_str())
    }
    pub fn pending_value(&self, entity: Entity) -> Option<f64> {
        self.values
            .iter()
            .find(|(id, _)| *id == entity)
            .map(|(_, value)| *value)
    }
    pub fn queue_value(&mut self, entity: Entity, value: f64) {
        if !value.is_finite() {
            return;
        }
        if let Some((_, previous)) = self.values.iter_mut().find(|(id, _)| *id == entity) {
            *previous = value;
        } else {
            self.values.push((entity, value));
        }
    }
    pub fn queue_value_path(&mut self, path: Vec<crate::PathKey>, value: f64) {
        if value.is_finite() {
            if let Some((_, previous)) = self
                .value_paths
                .iter_mut()
                .find(|(existing, _)| existing == &path)
            {
                *previous = value;
            } else {
                self.value_paths.push((path, value));
            }
        }
    }
    pub(crate) fn value(&mut self, entity: Entity) -> Option<f64> {
        let value = if let Some(index) = self.values.iter().position(|(id, _)| *id == entity) {
            Some(self.values.swap_remove(index).1)
        } else if let Some(index) = self
            .value_paths
            .iter()
            .rposition(|(path, _)| self.matches_path(entity, path))
        {
            Some(self.value_paths.swap_remove(index).1)
        } else {
            None
        };
        if value.is_some() {
            self.changes.insert(Changes::VIEW);
        }
        value
    }
    /// Host input snapshot. Hover is persistent state, not a consumed event.
    pub fn set_hovered(&mut self, entity: Option<Entity>) {
        self.hovered = entity;
        self.hovered_path = None;
    }
    /// Logical hover identity across module/host arenas. None clears the snapshot.
    pub fn set_hovered_path(&mut self, path: Option<Vec<crate::PathKey>>) {
        self.hovered = None;
        self.hovered_path = path;
    }
    pub(crate) fn hovered(&self, entity: Entity) -> bool {
        self.nodes
            .get(entity)
            .is_some_and(|node| !node.style.disabled)
            && (self.hovered == Some(entity)
                || self
                    .hovered_path
                    .as_ref()
                    .is_some_and(|path| self.matches_path(entity, path)))
    }
    pub fn queue_click(&mut self, entity: Entity) {
        // A declaration exposes a boolean click. Coalesce repeated activation
        // of the same control until it is consumed, including local replays.
        if !self.clicks.contains(&entity) {
            self.clicks.push(entity);
        }
    }
    pub(crate) fn clicked(&mut self, entity: Entity) -> bool {
        let clicked = if let Some(i) = self.clicks.iter().position(|id| *id == entity) {
            self.clicks.swap_remove(i);
            true
        } else if let Some(i) = self
            .click_paths
            .iter()
            .position(|path| self.matches_path(entity, path))
        {
            self.click_paths.swap_remove(i);
            true
        } else {
            false
        };
        if clicked {
            self.changes.insert(Changes::VIEW);
        }
        clicked
    }
    pub fn queue_click_path(&mut self, path: Vec<crate::PathKey>) {
        self.click_paths.push(path);
    }
    fn matches_path(&self, mut entity: Entity, path: &[crate::PathKey]) -> bool {
        for key in path.iter().rev() {
            if entity == self.root {
                return false;
            }
            let Some(node) = self.nodes.get(entity) else {
                return false;
            };
            if !key.matches_scoped(node.identity.key, node.identity.scope) {
                return false;
            }
            entity = node.identity.parent;
        }
        entity == self.root
    }
    pub fn snapshot(&self) -> Vec<crate::ViewNode> {
        fn node(engine: &Engine, e: Entity) -> crate::ViewNode {
            let n = engine.nodes.get(e).unwrap();
            crate::ViewNode {
                key: crate::PathKey::scoped(n.identity.key, n.identity.scope),
                kind: n.kind,
                style: n.style,
                text: n.text.clone(),
                spans: n.spans.clone(),
                grid: n.grid.clone(),
                shader: n.shader.clone(),
                numeric: n.numeric,
                children: n.children.iter().map(|c| node(engine, *c)).collect(),
            }
        }
        self.nodes
            .get(self.root)
            .unwrap()
            .children
            .iter()
            .map(|e| node(self, *e))
            .collect()
    }
    pub(crate) fn auto_key(&mut self, parent: Entity) -> Key {
        let node = self.nodes.get_mut(parent).unwrap();
        let key = Key::Auto(node.auto);
        node.auto += 1;
        key
    }
    /// Release declaration scratch and excess child-list capacity after a large
    /// page is closed. Entity identities, pending events and deltas are preserved.
    /// The next declaration may allocate again; ordinary frames reuse capacity.
    pub fn trim_declaration_buffers(&mut self) {
        self.declarations.clear();
        self.declarations.shrink_to_fit();
        self.format_buffer.clear();
        self.format_buffer.shrink_to_fit();
        for (_, node) in self.nodes.iter_mut() {
            node.children.shrink_to_fit();
        }
    }

    pub(crate) fn declare(
        &mut self,
        parent: Entity,
        key: Key,
        scope: u64,
        kind: Kind,
        style: Style,
        text: Cow<'_, str>,
    ) -> Entity {
        let identity = Identity { parent, key, scope };
        // Stable sibling order is the common case. Position is only a lookup
        // hint: verify the full identity before bypassing the hash table. A
        // reorder/insertion/scope change still resolves by the original key.
        let ordered = self
            .nodes
            .get(parent)
            .and_then(|node| node.children.get(node.declared_children).copied())
            .filter(|id| {
                self.nodes
                    .get(*id)
                    .is_some_and(|node| node.identity == identity)
            });
        let entity = if let Some(id) = ordered.or_else(|| self.lookup.get(&identity).copied()) {
            self.stats.reused += 1;
            id
        } else {
            let id = self.nodes.insert(Node::new(identity, kind));
            self.lookup.insert(identity, id);
            self.stats.created += 1;
            id
        };
        let node = self.nodes.get_mut(entity).unwrap();
        let mut incoming = style;
        incoming.layout.template_revision = node.style.layout.template_revision;
        let same_style = incoming == node.style;
        let style_valid = if same_style {
            node.style_valid
        } else {
            incoming.valid()
        };
        if !style_valid {
            self.error.get_or_insert(FrameError::InvalidStyle);
        }
        if node.touched == self.epoch {
            self.error
                .get_or_insert(FrameError::DuplicateKey { parent, key });
            return entity;
        }
        node.declared_children = 0;
        node.order_dirty = false;
        node.scroll_observed.set(false);
        node.auto = 0;
        node.touched = self.epoch;
        if node.kind != kind {
            self.inputs.remove(&entity);
            node.pending_changes |= Changes::STRUCTURE
                | Changes::MEASURE
                | Changes::LAYOUT
                | Changes::PAINT
                | Changes::SEMANTICS;
            node.pending_properties = StyleProperties::all();
            node.kind = kind;
            node.grid = None;
            if kind != Kind::Text {
                node.spans.clear();
            }
            if kind != Kind::Slider {
                node.numeric = None;
            }
            if kind != Kind::Shader {
                node.shader = None;
            }
            node.measure_key = None;
            node.layout_key = None;
        }
        if node.text != text {
            match text {
                Cow::Owned(text) => node.text = text,
                Cow::Borrowed(text) => {
                    node.text.clear();
                    node.text.push_str(text);
                }
            }
            node.text_revision += 1;
            node.pending_changes |=
                Changes::MEASURE | Changes::LAYOUT | Changes::PAINT | Changes::SEMANTICS;
            self.stats.text_writes += 1;
        }
        // Diff at the mutation site: both the old and new style are in hand
        // here, so no committed-style snapshot is retained for a second pass.
        if !same_style {
            let diff = incoming.diff(node.style);
            node.pending_changes |= diff.changes;
            node.pending_properties |= diff.properties;
            node.style = incoming;
            node.style_valid = style_valid;
        }
        self.stats.visited += 1;
        let parent_node = self.nodes.get_mut(parent).unwrap();
        parent_node.order_dirty |=
            parent_node.children.get(parent_node.declared_children) != Some(&entity);
        parent_node.declared_children += 1;
        self.declarations.push((parent, entity));
        entity
    }
    pub(crate) fn set_grid(&mut self, entity: Entity, grid: Option<&crate::GridTemplate>) {
        if grid.is_some_and(|g| !g.valid()) {
            self.error.get_or_insert(FrameError::InvalidStyle);
            return;
        }
        let node = self.nodes.get_mut(entity).unwrap();
        if node.grid.as_ref() != grid {
            node.grid = grid.cloned();
            node.style.layout.template_revision =
                node.style.layout.template_revision.wrapping_add(1);
            node.pending_changes |= Changes::LAYOUT | Changes::HIT_TEST | Changes::SEMANTICS;
            node.pending_properties |= StyleProperties::LAYOUT;
        }
    }
    pub(crate) fn set_spans(&mut self, entity: Entity, spans: &[crate::TextSpan]) {
        let node = self.nodes.get_mut(entity).unwrap();
        if spans.iter().any(|span| !span.valid_for(&node.text)) {
            self.error.get_or_insert(FrameError::InvalidStyle);
            return;
        }
        if node.spans != spans {
            let metrics_changed = node.spans.len() != spans.len()
                || node.spans.iter().zip(spans).any(|(old, new)| {
                    let mut metrics = old.clone();
                    metrics.color = new.color;
                    metrics != *new
                });
            node.spans.clear();
            node.spans.extend_from_slice(spans);
            node.pending_changes |= Changes::PAINT;
            if metrics_changed {
                node.text_revision += 1;
                node.pending_changes |= Changes::MEASURE | Changes::LAYOUT | Changes::SEMANTICS;
            }
        }
    }
    pub fn frame(
        &mut self,
        view: impl FnOnce(&mut UiContext<'_>),
    ) -> Result<FrameStats, FrameError> {
        self.frame_inner(view, true)
    }
    /// Commit declarations while an external backend owns measurement and layout.
    pub fn frame_declarations(
        &mut self,
        view: impl FnOnce(&mut UiContext<'_>),
    ) -> Result<FrameStats, FrameError> {
        self.frame_inner(view, false)
    }
    /// The single settle protocol: rerun declarations while the view requests
    /// another pass, at most eight times, aggregating dirty nodes and change
    /// flags across passes. Native and reload hosts both drive this; the
    /// semantics can no longer drift between them.
    pub fn declare_until_settled(&mut self, view: &mut dyn DeclareView) -> Result<u8, FrameError> {
        self.settle_dirty.clear();
        self.settle_node_changes.clear();
        self.settle_changes = Changes::empty();
        for pass in 0..8u8 {
            self.set_theme(view.theme());
            if let Err(error) = self.frame_declarations(|ui| view.show(ui)) {
                self.reset_settle();
                return Err(error);
            }
            for change in &self.node_changes {
                match self.settle_node_changes.entry(change.entity) {
                    std::collections::hash_map::Entry::Occupied(mut entry) => {
                        entry.get_mut().changes |= change.changes;
                        entry.get_mut().properties |= change.properties;
                    }
                    std::collections::hash_map::Entry::Vacant(entry) => {
                        entry.insert(StyleDiff {
                            changes: change.changes,
                            properties: change.properties,
                        });
                        self.settle_dirty.push(change.entity);
                    }
                }
            }
            self.settle_changes |= self.changes();
            if !self.changes().contains(Changes::VIEW) {
                return Ok(pass + 1);
            }
        }
        // A failed settle must not leak partial aggregation to the host.
        self.reset_settle();
        Err(FrameError::DidNotSettle)
    }
    /// Dirty entities aggregated across the passes of the last settle loop.
    pub fn take_settle_dirty(&mut self) -> Vec<Entity> {
        std::mem::take(&mut self.settle_dirty)
    }
    /// Drain without discarding reusable allocation capacity.
    pub fn drain_settle_dirty(&mut self) -> impl Iterator<Item = Entity> + '_ {
        self.settle_dirty.drain(..)
    }
    /// Change flags aggregated across the passes of the last settle loop.
    pub fn take_settle_changes(&mut self) -> Changes {
        std::mem::replace(&mut self.settle_changes, Changes::empty())
    }
    /// One settled frame's journal, aggregated for an incremental backend.
    ///
    /// `changes` unions the per-pass node journals and adds every node the
    /// settle loop created (new nodes enter with all channels stale), `removed`
    /// is the unmount journal accumulated across passes, and `frame` is the
    /// frame-wide union with `VIEW` cleared. Taking consumes it: a backend
    /// observes each change exactly once. The per-pass accessors above stay
    /// for hosts that sync after each pass instead.
    pub fn take_frame_delta(&mut self) -> FrameDelta {
        // Per-pass journals are unique by entity. Merge exact flags retained
        // during settling, rather than allocating another set and guessing
        // broad invalidations for nodes changed only in an earlier pass.
        let mut union: Vec<NodeChange> =
            Vec::with_capacity(self.settle_dirty.len().max(self.node_changes.len()));
        for change in self.node_changes.drain(..) {
            if self.nodes.get(change.entity).is_some() {
                let earlier = self
                    .settle_node_changes
                    .remove(&change.entity)
                    .unwrap_or_default();
                union.push(NodeChange {
                    entity: change.entity,
                    changes: change.changes | earlier.changes,
                    properties: change.properties | earlier.properties,
                });
            }
        }
        for entity in self.settle_dirty.drain(..) {
            if let Some(diff) = self.settle_node_changes.remove(&entity)
                && self.nodes.get(entity).is_some()
            {
                union.push(NodeChange {
                    entity,
                    changes: diff.changes,
                    properties: diff.properties,
                });
            }
        }
        self.settle_node_changes.clear();
        // New nodes' first-pass flags are retained exactly like later updates;
        // no broad fallback invalidation is needed for earlier settle passes.
        let removed: Vec<Entity> = self.drain_removed().collect();
        let mut frame = self.take_settle_changes();
        frame.remove(Changes::VIEW);
        FrameDelta {
            changes: union,
            removed,
            frame,
        }
    }
    fn reset_settle(&mut self) {
        self.settle_dirty.clear();
        self.settle_node_changes.clear();
        self.settle_changes = Changes::empty();
    }
    fn frame_inner(
        &mut self,
        view: impl FnOnce(&mut UiContext<'_>),
        measure: bool,
    ) -> Result<FrameStats, FrameError> {
        self.epoch = self
            .epoch
            .checked_add(1)
            .ok_or(FrameError::EpochExhausted)?;
        self.stats = FrameStats::default();
        self.dirty_nodes.clear();
        self.node_changes.clear();
        self.changes = Changes::empty();
        if self.viewport_dirty {
            self.changes |= Changes::LAYOUT | Changes::PAINT | Changes::SEMANTICS;
        }
        if self.theme_dirty {
            self.changes.insert(Changes::PAINT);
        }
        self.error = None;
        if measure {
            // The simulated-measurement path owns the whole tree; nobody drains
            // removals or unmatched events, so per-frame clearing keeps both
            // accumulators bounded.
            self.removed_since_sync.clear();
            self.unmatched_clicks.clear();
            self.unmatched_values.clear();
        }
        let frame_root = self.root;
        let inherited = crate::Style {
            font_size: self.theme.metrics.font_size,
            ..Default::default()
        };
        let namespace = self.nodes.get(frame_root).unwrap().identity.scope;
        let in_button = false;
        let root = self.nodes.get_mut(frame_root).unwrap();
        root.touched = self.epoch;
        root.auto = 0;
        root.scroll_observed.set(false);
        root.declared_children = 0;
        root.order_dirty = false;
        self.declarations.clear();
        let parent = frame_root;
        view(&mut UiContext {
            engine: self,
            parent,
            scope: namespace,
            inherited,
            in_button,
        });
        if let Some(error) = self.error.take() {
            return Err(error);
        }
        if self.locals.take_changed() | self.storage.take_changed() {
            self.changes.insert(Changes::VIEW);
        }
        self.removed.clear();
        // A successful pass declares each entity at most once (duplicate keys
        // return above). If all live nodes were visited, none can be removed.
        if self.stats.visited + 1 != self.nodes.len() {
            for (id, node) in self.nodes.iter() {
                if node.touched != self.epoch {
                    self.removed.push(id);
                }
            }
        }
        for id in self.removed.iter().copied() {
            self.scroll_viewports.remove(&id);
            let node = self.nodes.remove(id).unwrap();
            self.lookup.remove(&node.identity);
            self.inputs.remove(&id);
            self.stats.removed += 1;
        }
        self.locals.retain(|parent, epoch| {
            if self.nodes.get(parent).is_none() {
                return false;
            }
            if epoch == self.epoch {
                return true;
            }
            false
        });
        self.removed_since_sync.extend(self.removed.iter().copied());
        // Keep the old hierarchy intact until declaration succeeds. A single
        // contiguous journal replaces a pending Vec allocation on every parent.
        // Portals are compared in their committed (stable partitioned) order.
        let has_portals = self.declarations.iter().any(|&(parent, id)| {
            parent == self.root && self.nodes.get(id).unwrap().kind == Kind::Overlay
        });
        self.removed.clear();
        if has_portals {
            for overlay in [false, true] {
                self.removed
                    .extend(self.declarations.iter().filter_map(|&(parent, id)| {
                        (parent == self.root
                            && (self.nodes.get(id).unwrap().kind == Kind::Overlay) == overlay)
                            .then_some(id)
                    }));
            }
            let root = self.nodes.get_mut(self.root).unwrap();
            root.order_dirty = root.children != self.removed;
        }
        for (_, node) in self.nodes.iter_mut() {
            node.order_dirty |= node.children.len() != node.declared_children;
            if node.order_dirty {
                self.stats.order_changes += 1;
                node.children.clear();
            }
        }
        for &(parent, entity) in &self.declarations {
            if !has_portals || parent != self.root {
                let node = self.nodes.get_mut(parent).unwrap();
                if node.order_dirty {
                    node.children.push(entity);
                }
            }
        }
        if has_portals {
            let root = self.nodes.get_mut(self.root).unwrap();
            if root.order_dirty {
                root.children.extend_from_slice(&self.removed);
            }
        }
        self.removed.clear();
        if measure {
            self.layout(self.root, self.width);
        }
        for (id, node) in self.nodes.iter_mut() {
            // Drain invalidation accumulated at each mutation site during
            // declaration; add the frame-global causes that only the commit
            // pass can see (ordering, measured sizes, theme).
            let mut changes = std::mem::take(&mut node.pending_changes);
            let mut properties = std::mem::take(&mut node.pending_properties);
            if node.order_dirty {
                changes |= Changes::STRUCTURE | Changes::LAYOUT | Changes::SEMANTICS;
            }
            if node.paint_size != Some(node.size) {
                changes |= Changes::LAYOUT | Changes::SEMANTICS;
            }
            if self.theme_dirty && node.style.themed(node.kind) {
                changes.insert(Changes::PAINT);
                properties |= StyleProperties::BACKGROUND
                    | StyleProperties::FOREGROUND
                    | StyleProperties::BORDER_COLOR;
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
                Changes::LAYOUT | Changes::PAINT | Changes::TRANSFORM | Changes::MEASURE,
            ) {
                self.stats.paint_updates += 1;
            }
            node.paint_size = Some(node.size);
        }
        if self.stats.created > 0 || self.stats.removed > 0 || self.stats.order_changes > 0 {
            self.changes.insert(Changes::STRUCTURE);
        }
        if self.stats.created > 0
            || self.stats.removed > 0
            || self.stats.order_changes > 0
            || self.stats.text_writes > 0
            || self.stats.measure_calls > 0
            || self.stats.layout_runs > 0
        {
            self.changes |= Changes::LAYOUT | Changes::PAINT | Changes::SEMANTICS;
        }
        if self.stats.paint_updates > 0 {
            self.changes.insert(Changes::PAINT);
        }
        self.theme_dirty = false;
        self.viewport_dirty = false;
        self.clicks.clear();
        // Leftover path-addressed events survived no declaration this pass;
        // keep them observable instead of silently discarding them.
        self.unmatched_clicks.append(&mut self.click_paths);
        self.values.clear();
        self.unmatched_values.append(&mut self.value_paths);
        Ok(self.stats)
    }
    fn layout(&mut self, entity: Entity, available: f32) -> bool {
        let node = self.nodes.get(entity).unwrap();
        let kind = node.kind;
        let style = node.style;
        let previous = node.size;
        let width = style.width.unwrap_or(available).min(available).max(0.);
        let px = style.horizontal_padding() + style.border_width;
        let py = style.vertical_padding() + style.border_width;
        let inner = (width - px * 2.).max(1.);
        if kind.leaf() {
            let node = self.nodes.get_mut(entity).unwrap();
            let key = MeasureKey {
                text_revision: node.text_revision,
                font_size: style.font_size,
                available,
                width: style.width,
                height: style.height,
                padding: px,
                padding_y: py,
            };
            if node.measure_key != Some(key) {
                self.stats.measure_calls += 1;
                // Fake text metrics, intentionally isolated from any renderer.
                let natural = node.text.chars().count() as f32 * style.font_size as f32 * 0.55;
                let lines = (natural / inner).ceil().max(1.);
                node.size = Size {
                    width: style.width.unwrap_or(natural + px * 2.).min(available),
                    height: style
                        .height
                        .unwrap_or(lines * style.font_size as f32 * 1.4 + py * 2.),
                };
                node.measure_key = Some(key);
            }
        } else {
            let mut children_changed = false;
            let count = node.children.len();
            let mut flow_count = 0usize;
            let mut extent = 0.;
            let mut cross: f32 = 0.;
            for i in 0..count {
                let child = self.nodes.get(entity).unwrap().children[i];
                children_changed |= self.layout(child, inner);
                let child_node = self.nodes.get(child).unwrap();
                if child_node.style.absolute.is_some() || child_node.style.overlay {
                    continue;
                }
                flow_count += 1;
                let size = child_node.size;
                match kind {
                    Kind::Row | Kind::ButtonBase => {
                        extent += size.width;
                        cross = cross.max(size.height);
                    }
                    _ => {
                        extent += size.height;
                        cross = cross.max(size.width);
                    }
                }
            }
            let node = self.nodes.get_mut(entity).unwrap();
            let key = LayoutKey {
                flow_count,
                width,
                height: style.height,
                gap: style.gap,
                padding: px,
                padding_y: py,
            };
            if node.layout_key != Some(key) || node.order_dirty || children_changed {
                self.stats.layout_runs += 1;
                let gap = flow_count.saturating_sub(1) as f32 * style.gap;
                node.size = Size {
                    width,
                    height: style.height.unwrap_or(
                        if matches!(kind, Kind::Row | Kind::ButtonBase) {
                            cross + py * 2.
                        } else {
                            extent + gap + py * 2.
                        },
                    ),
                };
                node.layout_key = Some(key);
            }
        }
        self.nodes.get(entity).unwrap().size != previous
    }
}

#[cfg(test)]
mod declaration_storage_tests {
    use super::*;

    #[test]
    fn failed_declaration_preserves_order_and_next_pass_recovers() {
        let mut engine = Engine::default();
        engine
            .frame(|cx| {
                cx.text("A").key("a");
                cx.text("B").key("b");
            })
            .unwrap();
        let original = engine.node(engine.root()).unwrap().children.to_vec();
        assert!(
            engine
                .frame(|cx| {
                    cx.text("B").key("b");
                    cx.text("B").key("b");
                })
                .is_err()
        );
        assert_eq!(engine.node(engine.root()).unwrap().children, original);
        engine
            .frame(|cx| {
                cx.text("B").key("b");
                cx.text("A").key("a");
            })
            .unwrap();
        assert_eq!(
            engine.node(engine.root()).unwrap().children,
            [original[1], original[0]]
        );
    }

    #[test]
    fn interleaved_portals_do_not_report_false_order_changes() {
        let mut engine = Engine::default();
        for measure in [true, false] {
            let view = |cx: &mut UiContext<'_>| {
                for (key, kind) in [("overlay", Kind::Overlay), ("normal", Kind::Text)] {
                    let root = cx.engine.root;
                    cx.engine.declare(
                        root,
                        key.into(),
                        0,
                        kind,
                        Style::default(),
                        Cow::Borrowed(""),
                    );
                }
            };
            let first = engine.frame_inner(view, measure).unwrap();
            let children = engine.node(engine.root()).unwrap().children;
            assert_eq!(engine.node(children[0]).unwrap().kind, Kind::Text);
            assert_eq!(engine.node(children[1]).unwrap().kind, Kind::Overlay);
            let second = engine.frame_inner(view, measure).unwrap();
            assert_eq!(second.order_changes, 0);
            assert!(first.created == 0 || first.order_changes > 0);
        }
    }

    #[test]
    fn trim_releases_scratch_without_losing_handles_or_pending_events() {
        let mut engine = Engine::default();
        engine
            .frame(|cx| {
                for i in 0..1024_u64 {
                    cx.button("item").key(i);
                }
            })
            .unwrap();
        engine
            .frame(|cx| {
                cx.button("item").key(0_u64);
            })
            .unwrap();
        let id = engine.node(engine.root()).unwrap().children[0];
        engine.queue_click(id);
        assert!(engine.declarations.capacity() >= 1024);
        assert!(engine.nodes.get(engine.root).unwrap().children.capacity() >= 1024);
        engine.trim_declaration_buffers();
        assert_eq!(engine.declarations.capacity(), 0);
        assert_eq!(
            engine.nodes.get(engine.root).unwrap().children.capacity(),
            1
        );
        assert_eq!(engine.node(engine.root()).unwrap().children, [id]);
        let mut clicked = false;
        let stats = engine
            .frame(|cx| {
                clicked = cx.button("item").key(0_u64).clicked();
            })
            .unwrap();
        assert!(clicked);
        assert_eq!(stats.created, 0);
        assert_eq!(stats.order_changes, 0);
    }
}
