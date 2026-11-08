//! Declaration settling, frame construction and retained scene caching.
use crate::view::HostViewChild;
use crate::{Adapter, BackendStats, FontAsset, HostAction, HostView, rgb, transition};
use peniko::Color;
use plico::{Engine, Entity};
use plico_renderer::{
    EventResponse, FrameSource, KeyPhase, PointerPhase, Scene, TextContext, UiEvent,
    layout::PlacedNode, style::CursorStyle,
};
use std::{
    collections::{HashMap, HashSet},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
};
use taffy::prelude::NodeId;
use web_time::Instant;

#[cfg(test)]
mod tests;

#[derive(Debug, snafu::Snafu)]
pub enum SourceError {
    #[snafu(display("view declaration failed: {source}"))]
    Declaration { source: plico::FrameError },
    #[snafu(display("view did not settle after eight passes"))]
    DidNotSettle,
    #[snafu(display("host clock exceeds native Instant range"))]
    ClockRange,
}
/// Cumulative UI-thread batch counters; snapshot before/after a workload.
#[derive(Default, Debug, Clone, Copy, PartialEq, Eq)]
pub struct BatchStats {
    pub rebuild_requests: usize,
    pub service_polls: usize,
    pub wake_notifications: usize,
    pub scheduled_wakes: usize,
    pub frame_samples: usize,
    /// Native samples that reused declarations, including presentation-only motion.
    pub retained_samples: usize,
    pub event_flushes: usize,
    pub declaration_batches: usize,
    pub declaration_passes: usize,
    pub backend_commits: usize,
    pub scroll_viewport_visits: usize,
    pub layout_runs: usize,
}
#[derive(Default)]
struct WakeBatch {
    pending: AtomicBool,
    notifications: AtomicUsize,
    scheduled: AtomicUsize,
}
pub struct Source<V: HostView> {
    fonts: Vec<FontAsset>,
    pub last_error: Option<SourceError>,
    pub engine: Engine,
    pub adapter: Adapter,
    /// Direct external mutations require request_rebuild; prefer update().
    pub view: V,
    pub declaration_stats: plico::FrameStats,
    pub declaration_micros: u128,
    revision: u64,
    semantics_enabled: bool,
    pending: bool,
    text_generation: Option<usize>,
    viewport: Option<(u32, u32)>,
    transition: transition::ThemeTransition,
    declarations_reused: bool,
    frame_time: Instant,
    clock: Box<dyn plico::Clock>,
    clock_epoch: Instant,
    last_tick: Option<Instant>,
    requests: plico::Changes,
    target_scene: Option<(Scene, u64, u64)>,
    background: Option<Color>,
    scene_cache: plico_renderer::scene::SceneCache,
    scene_roots: HashSet<NodeId>,
    scene_revisions: HashMap<NodeId, u64>,
    pending_region_revisions: Vec<(Entity, u64)>,
    #[doc(hidden)]
    pub experiment_force_full_scene: bool,
    /// Work accumulated across the current host batch.
    batch_stats: BatchStats,
    wake_batch: Arc<WakeBatch>,
    declared_seen: HashSet<Entity>,
    declared_dirty: Vec<Entity>,
    declared_changes: plico::Changes,
    /// try_declare may be called explicitly before a native frame commits it.
    declarations_pending: bool,
}
impl<V: HostView> Source<V> {
    pub fn new(view: V) -> Self {
        Self::with_clock(view, plico::SystemClock::default())
    }
    pub fn with_clock(view: V, clock: impl plico::Clock + 'static) -> Self {
        let epoch = Instant::now();
        Self {
            fonts: Vec::new(),
            last_error: None,
            engine: Engine::default(),
            adapter: Adapter::default(),
            view,
            declaration_stats: Default::default(),
            declaration_micros: 0,
            revision: 0,
            semantics_enabled: false,
            pending: false,
            text_generation: None,
            viewport: None,
            transition: Default::default(),
            declarations_reused: false,
            frame_time: epoch,
            clock: Box::new(clock),
            clock_epoch: epoch,
            last_tick: None,
            requests: plico::Changes::empty(),
            target_scene: None,
            background: None,
            scene_cache: Default::default(),
            scene_roots: HashSet::new(),
            scene_revisions: HashMap::new(),
            pending_region_revisions: Vec::new(),
            experiment_force_full_scene: false,
            batch_stats: BatchStats::default(),
            wake_batch: Arc::default(),
            declared_seen: HashSet::new(),
            declared_dirty: Vec::new(),
            declared_changes: plico::Changes::empty(),
            declarations_pending: false,
        }
    }
    /// Override the window backdrop, e.g. transparent for an embedded game HUD.
    /// Widget theme colors and hit testing remain unchanged.
    pub fn with_background(mut self, color: Color) -> Self {
        self.background = Some(color);
        self
    }
    /// Attach a resident font asset, shared across frames and captures.
    pub fn with_font(mut self, font: FontAsset) -> Self {
        self.fonts.push(font);
        self.pending = true;
        self.target_scene = None;
        self
    }
    /// Override the host's text font stack, using CSS-style family names.
    /// The system fallback database still supplies missing glyphs and emoji.
    pub fn with_font_family(mut self, family: impl Into<Arc<str>>) -> Self {
        self.set_font_family(family);
        self
    }
    pub fn set_font_family(&mut self, family: impl Into<Arc<str>>) {
        let family = family.into();
        if self.adapter.font_family == family {
            return;
        }
        self.adapter.font_family = family.clone();
        self.adapter.font_dirty = true;
        self.pending = true;
        self.target_scene = None;
    }
    fn clock_instant(&self) -> Option<Instant> {
        self.clock_epoch.checked_add(self.clock.now())
    }
    fn set_tick(&mut self, now: Instant, commit: bool) {
        let now = self.last_tick.map_or(now, |previous| previous.max(now));
        let time = plico::FrameTime {
            now: now.saturating_duration_since(self.clock_epoch),
            delta: self
                .last_tick
                .map_or(std::time::Duration::ZERO, |previous| {
                    now.saturating_duration_since(previous)
                }),
        };
        self.engine.set_time(time);
        self.view.set_frame_time(time);
        if commit {
            self.last_tick = Some(self.last_tick.map_or(now, |previous| previous.max(now)));
        }
    }
    /// Set to Duration::ZERO for instant/reduced-motion theme changes.
    pub fn set_theme_transition_duration(&mut self, duration: std::time::Duration) {
        self.transition.set_duration(duration);
    }
    /// Hit-test an interactive control in window-logical coordinates.
    /// Embedded hosts use this to arbitrate UI and game input.
    pub fn hit_test(&self, point: plico_renderer::Point) -> Option<Entity> {
        self.adapter.hit(&self.engine, point.x, point.y)
    }
    /// Mutate application state on the UI thread and invalidate declarations.
    /// Repeated updates before a frame coalesce; the closures themselves all run.
    pub fn update<R>(&mut self, update: impl FnOnce(&mut V) -> R) -> R {
        self.request_rebuild();
        update(&mut self.view)
    }
    /// Benchmark-only: prepare the retained tree and layout without encoding
    /// a Scene. This has the same invalidation path as a native sampled frame.
    #[doc(hidden)]
    pub fn experiment_prepare_without_scene(
        &mut self,
        tcx: &mut TextContext,
        width: u32,
        height: u32,
    ) {
        if let Some(now) = self.clock_instant() {
            self.prepare_frame_at(tcx, width, height, now, false);
        } else {
            self.last_error = Some(SourceError::ClockRange);
        }
    }

    /// Experiment diagnostics: Engine nodes, retained nodes, Taffy nodes, reverse IDs.
    #[doc(hidden)]
    pub fn experiment_live_counts(&self) -> (usize, usize, usize, usize) {
        (
            self.engine.experiment_live_nodes(),
            self.adapter.nodes.len(),
            self.adapter.tree.total_node_count(),
            self.adapter.reverse.len(),
        )
    }

    /// Experimental local declaration, preserving the normal adapter dirty journal.
    #[doc(hidden)]
    pub fn experiment_redeclare(
        &mut self,
        context: plico::ExperimentContext,
        view: impl FnOnce(&mut plico::UiContext<'_>),
    ) -> Result<plico::FrameStats, plico::FrameError> {
        let stats = self.engine.experiment_redeclare(context, view)?;
        for &entity in self.engine.dirty_nodes() {
            if self.declared_seen.insert(entity) {
                self.declared_dirty.push(entity);
            }
        }
        self.declared_changes |= self.engine.changes();
        self.declarations_pending = true;
        Ok(stats)
    }

    /// Restricted experiment: patch existing text leaves and schedule the same
    /// retained adapter and scene pipeline used by declarations. This is not a
    /// general mutation API; it does not manage application state or ownership.
    #[doc(hidden)]
    pub fn experiment_patch_text(&mut self, entity: Entity, text: &str) -> Option<bool> {
        let changed = self.engine.experiment_patch_text(entity, text)?;
        if changed {
            if self.declared_seen.insert(entity) {
                self.declared_dirty.push(entity);
            }
            self.declared_changes |= plico::Changes::MEASURE
                | plico::Changes::LAYOUT
                | plico::Changes::PAINT
                | plico::Changes::SEMANTICS;
            self.declarations_pending = true;
        }
        Some(changed)
    }

    /// Benchmark-only foreground patch for an existing plain text leaf.
    #[doc(hidden)]
    pub fn experiment_patch_color(&mut self, entity: Entity, color: plico::Color) -> Option<bool> {
        let changed = self.engine.experiment_patch_color(entity, color)?;
        if changed {
            if self.declared_seen.insert(entity) {
                self.declared_dirty.push(entity);
            }
            self.declared_changes |= plico::Changes::PAINT;
            self.declarations_pending = true;
        }
        Some(changed)
    }

    /// Mark externally mutated application data for declaration on the next frame.
    pub fn request_rebuild(&mut self) {
        self.batch_stats.rebuild_requests += 1;
        self.pending = true;
    }
    pub fn batch_stats(&self) -> BatchStats {
        BatchStats {
            wake_notifications: self.wake_batch.notifications.load(Ordering::Relaxed),
            scheduled_wakes: self.wake_batch.scheduled.load(Ordering::Relaxed),
            ..self.batch_stats
        }
    }
    /// Disable theme, property and scroll transitions (host accessibility policy).
    pub fn set_reduced_motion(&mut self, reduced: bool) {
        self.adapter.motions.set_enabled(!reduced);
        self.adapter.smooth_scroll_enabled = !reduced;
        self.adapter.scrollbars.reduced_motion = reduced;
        if reduced {
            self.adapter.finish_scroll();
        }
        self.adapter.shader_motion_enabled = !reduced;
        self.adapter.shader_reset_time = true;
        self.transition.set_enabled(!reduced);
        self.pending = true;
    }
    /// Sample the clock before committing event targets, including after a long idle.
    pub fn prepare_for_event_at(&mut self, tcx: &mut TextContext, now: Instant) {
        self.set_tick(now, false);
        let scroll = self
            .adapter
            .advance_scroll(self.engine.time().now, &self.engine);
        self.pending |= scroll.declarations;
        if self.pending {
            self.batch_stats.event_flushes += 1;
            self.adapter.motions.advance(now);
            if let Err(error) = self.try_declare() {
                self.last_error = Some(error);
                return;
            }
            self.last_error = None;
            if let Some((w, h)) = self.viewport {
                self.update_backend(tcx, w, h);
            }
        } else if (scroll.changed
            || self.adapter.scroll_dirty
            || self.adapter.scrollbars.needs_frame())
            && let Some((w, h)) = self.viewport
        {
            // Event targeting must use the presented scroll position even when
            // no application declaration observes that position.
            self.update_backend(tcx, w, h);
        }
    }
    /// Deterministic host clock entry point, also used by the native frame source.
    pub fn build_frame_at(
        &mut self,
        tcx: &mut TextContext,
        width: u32,
        height: u32,
        now: Instant,
    ) -> Vec<PlacedNode> {
        self.prepare_frame_at(tcx, width, height, now, true);
        self.adapter.presented()
    }
    fn prepare_frame_at(
        &mut self,
        tcx: &mut TextContext,
        width: u32,
        height: u32,
        now: Instant,
        force_declaration: bool,
    ) {
        // Declarations such as Response::focused read the previous frame's
        // backend focus before this frame's traversal moves it.
        self.engine.set_focused(self.adapter.focused());
        self.pending |= self.adapter.update_hover_regions(&self.engine);
        let mut fonts_changed = false;
        for font in &self.fonts {
            fonts_changed |= tcx.register_font(font);
        }
        fonts_changed |= self
            .text_generation
            .is_some_and(|generation| generation != tcx.cache_generation());
        self.text_generation = Some(tcx.cache_generation());
        if fonts_changed {
            self.adapter.font_dirty = true;
            self.pending = true;
            self.target_scene = None;
        }
        self.batch_stats.frame_samples += 1;
        let now = self.last_tick.map_or(now, |previous| previous.max(now));
        self.adapter.stats = BackendStats::default();
        self.declaration_micros = 0;
        self.declaration_stats = plico::FrameStats::default();
        self.frame_time = now;
        self.set_tick(now, true);
        if self.view.snapshot_changed() {
            // External service data or explicit region state changed. The view
            // chooses between local replay and a full synchronized declaration.
            self.pending = true;
        }
        let motion_dirty = self.adapter.motions.advance(now);
        let scroll = self
            .adapter
            .advance_scroll(self.engine.time().now, &self.engine);
        // Only declarations that read scroll geometry (including virtual rows)
        // need to run again; ordinary scrolling is presentation work.
        self.pending |= scroll.declarations;
        if self
            .engine
            .presence_deadline()
            .is_some_and(|deadline| self.engine.time().now >= deadline)
        {
            self.pending = true;
        }
        let shader_active = self.adapter.shader_active();
        let tooltip_active = self.adapter.tips.needs_frame();
        let scrollbar_active = self.adapter.scrollbars.needs_frame();
        // A surface redraw is not itself an application-state change. Native
        // sampling can keep the declaration tree when all invalidation sources
        // are clean. The snapshot API deliberately still evaluates the view.
        if !force_declaration
            && !self.pending
            && !self.requests.contains(plico::Changes::ANIMATE)
            && self.viewport == Some((width, height))
            && self.engine.theme() == self.view.theme()
            && self.last_error.is_none()
        {
            self.batch_stats.retained_samples += 1;
            self.declarations_reused = true;
            if motion_dirty
                || shader_active
                || tooltip_active
                || scrollbar_active
                || scroll.changed
                || self.adapter.scroll_dirty
                || self.adapter.selection_dirty
                || self.declarations_pending
            {
                self.update_backend(tcx, width, height);
            }
            return;
        }
        self.declarations_reused = (self.transition.active()
            || motion_dirty
            || shader_active
            || tooltip_active
            || scrollbar_active)
            && !self.requests.contains(plico::Changes::ANIMATE)
            && !self.pending
            && self.viewport == Some((width, height))
            && self.engine.theme() == self.view.theme()
            && (motion_dirty
                || shader_active
                || tooltip_active
                || scrollbar_active
                || self
                    .target_scene
                    .as_ref()
                    .is_some_and(|(_, _, revision)| *revision == self.revision));
        if self.declarations_reused {
            if motion_dirty || shader_active || tooltip_active || scrollbar_active {
                self.update_backend(tcx, width, height);
                return;
            }
            return;
        }
        let viewport_changed = self.viewport != Some((width, height));
        self.viewport = Some((width, height));
        let result = self
            .engine
            .set_viewport(width as f32, height as f32)
            .map_err(|source| SourceError::Declaration { source })
            .and_then(|()| self.try_declare());
        if let Err(error) = result {
            if self
                .last_error
                .as_ref()
                .is_none_or(|previous| previous.to_string() != error.to_string())
            {
                tracing::error!(%error, "UI declaration failed");
            }
            self.last_error = Some(error);
            return;
        }
        self.last_error = None;
        self.update_backend(tcx, width, height);
        // Fill-mode virtual lists declared from the previous layout's viewport.
        // Commit the new geometry before replaying observers, even while an
        // unrelated scrollbar fade would otherwise reuse declarations.
        if viewport_changed
            && self.adapter.scroll_indices.iter().any(|&index| {
                self.adapter
                    .reverse
                    .get(&self.adapter.placed[index].node_id)
                    .is_some_and(|&entity| self.engine.observes_scroll(entity))
            })
        {
            self.pending = true;
            self.requests.insert(plico::Changes::ANIMATE);
        }
    }
    fn update_backend(&mut self, tcx: &mut TextContext, width: u32, height: u32) {
        self.adapter.requested_focus = self.engine.take_focus_request();
        self.adapter.requested_scroll = self.engine.take_scroll_requests();
        self.adapter.update_retained(
            &mut self.engine,
            tcx,
            width,
            height,
            &self.declared_dirty,
            self.declared_changes,
        );
        self.declared_dirty.clear();
        self.declared_seen.clear();
        self.declared_changes = plico::Changes::empty();
        self.declarations_pending = false;
        for &id in &self.adapter.removed_node_ids {
            self.scene_cache.remove(id);
            self.scene_revisions.remove(&id);
        }
        self.scene_roots = self
            .view
            .scene_cache_roots()
            .into_iter()
            .filter_map(|entity| self.adapter.backend_id(entity))
            .collect();
        for (entity, revision) in std::mem::take(&mut self.pending_region_revisions) {
            if let Some(id) = self.adapter.backend_id(entity)
                && self.scene_revisions.get(&id) != Some(&revision)
            {
                self.scene_revisions.insert(id, revision);
                self.adapter.scene_dirty_nodes.insert(id);
            }
        }
        for &id in &self.scene_roots {
            self.scene_revisions.entry(id).or_insert(0);
        }
        let stats = self.adapter.stats;
        self.batch_stats.backend_commits += 1;
        self.batch_stats.layout_runs += stats.layout_runs;
        if stats.paint_writes > 0
            || stats.layout_runs > 0
            || stats.flatten_visits > 0
            || stats.created > 0
            || stats.removed > 0
        {
            self.revision += 1;
        }
    }
    /// Rebuild device-sized shader fragments when the host's DPI changes.
    pub(crate) fn update_shader_scale(&mut self, tcx: &mut TextContext, scale: f64) -> bool {
        if self.adapter.shader_scale == scale {
            return false;
        }
        self.adapter.shader_scale = scale;
        let updated = self.adapter.update_shaders(&self.engine, tcx);
        if updated {
            self.revision += 1;
        }
        updated
    }
    fn paint_frame_scene(
        &mut self,
        scene: &mut Scene,
        snapshot: Option<&[PlacedNode]>,
        tcx: &mut TextContext,
        size: (u32, u32),
        scale: f64,
    ) {
        self.update_shader_scale(tcx, scale);
        if let Some((cached, cached_scale, _)) = &self.target_scene
            && self
                .target_scene
                .as_ref()
                .is_some_and(|(_, _, revision)| *revision == self.revision)
            && *cached_scale == scale.to_bits()
        {
            *scene = cached.clone();
            self.adapter.stats.scene_reuses += 1;
            #[cfg(feature = "profiling")]
            {
                tracing::trace!(target: "plico::perf", reused = true, commands = scene.commands.len(), revision = self.revision, "scene.work");
                tcx.trace_cache_snapshot();
            }
            return;
        }
        self.adapter.stats.scene_builds += 1;
        let layers = match (self.adapter.shaders.is_empty(), snapshot) {
            (true, Some(nodes)) => [nodes, &[]],
            _ => [self.adapter.placed.as_slice(), self.adapter.tips.layer()],
        };
        let base_color = self.base_color();
        if self.experiment_force_full_scene
            || self.scene_roots.is_empty()
            || !self.adapter.shaders.is_empty()
        {
            plico_renderer::scene::build_scene_scaled_layers(
                scene, &layers, tcx, size.0, size.1, base_color, scale,
            );
        } else {
            if self.adapter.scene_full_dirty {
                self.scene_cache.clear();
            }
            let mut invalid = HashSet::new();
            if self.adapter.scene_full_dirty {
                invalid.extend(self.scene_roots.iter().copied());
            } else {
                for &dirty in &self.adapter.scene_dirty_nodes {
                    let mut cursor = Some(dirty);
                    while let Some(id) = cursor {
                        if self.scene_roots.contains(&id) {
                            invalid.insert(id);
                        }
                        cursor = self
                            .adapter
                            .placed_indices
                            .get(&id)
                            .and_then(|&index| self.adapter.placed[index].parent_node_id);
                    }
                    // A layout patch can move an entire subtree without
                    // changing its parent transform. Fragments nested inside
                    // that subtree contain absolute geometry and must be
                    // rebuilt as well as every enclosing fragment.
                    if let Some(&start) = self.adapter.placed_indices.get(&dirty) {
                        let depth = self.adapter.placed[start].depth;
                        for (offset, node) in self.adapter.placed[start..].iter().enumerate() {
                            if offset > 0 && node.depth <= depth {
                                break;
                            }
                            if self.scene_roots.contains(&node.node_id) {
                                invalid.insert(node.node_id);
                            }
                        }
                    }
                }
            }
            self.adapter.stats.scene_invalid_roots = invalid.len();
            plico_renderer::scene::build_scene_cached(
                scene,
                &layers,
                &self.scene_roots,
                &invalid,
                &mut self.scene_cache,
                tcx,
                plico_renderer::scene::SceneTarget {
                    size: [size.0, size.1],
                    base_color,
                    device_scale: scale,
                },
            );
            self.adapter.stats.scene_fragment_builds = self.scene_cache.built;
            self.adapter.stats.scene_fragment_reuses = self.scene_cache.reused;
            self.adapter.stats.scene_encoded_nodes = self.scene_cache.encoded_nodes;
        }
        self.adapter.stats.scene_full_rebuilds = usize::from(self.adapter.scene_full_dirty);
        self.target_scene = Some((scene.clone(), scale.to_bits(), self.revision));
        #[cfg(feature = "profiling")]
        {
            tracing::trace!(target: "plico::perf", reused = false, commands = scene.commands.len(), revision = self.revision, "scene.work");
            tcx.trace_cache_snapshot();
        }
    }
    pub fn try_declare(&mut self) -> Result<(), SourceError> {
        self.batch_stats.declaration_batches += 1;
        for &index in &self.adapter.scroll_indices {
            let placed = &self.adapter.placed[index];
            self.batch_stats.scroll_viewport_visits += 1;
            if let Some(&entity) = self.adapter.reverse.get(&placed.node_id)
                && self
                    .engine
                    .node(entity)
                    .is_some_and(|node| node.style.scroll_y)
            {
                let offset = self
                    .adapter
                    .scroll_offsets
                    .get(&placed.node_id)
                    .map_or(placed.scroll.offset[1], |offset| offset[1]);
                self.engine.set_scroll_viewport(
                    entity,
                    offset,
                    placed.scroll.port[3] - placed.scroll.port[1],
                );
            }
        }
        let start = Instant::now();
        if self.engine.theme() == self.view.theme()
            && let Some(batch) = self.view.replay_regions(&mut self.engine)
        {
            let batch = batch.map_err(|source| {
                self.pending = true;
                self.declared_changes.insert(plico::Changes::STRUCTURE);
                self.declared_dirty.clear();
                self.declared_seen.clear();
                match source {
                    plico::FrameError::DidNotSettle => SourceError::DidNotSettle,
                    other => SourceError::Declaration { source: other },
                }
            })?;
            self.declaration_stats = batch.stats;
            self.declaration_micros = start.elapsed().as_micros();
            self.batch_stats.declaration_passes += batch.passes;
            self.declared_dirty.extend(batch.dirty);
            self.pending_region_revisions.extend(batch.revisions);
            self.declared_changes |= batch.changes;
            self.requests = batch.changes & plico::Changes::ANIMATE;
            self.pending = false;
            self.declarations_pending = true;
            // A region event can change the view's global theme. Refresh all
            // inherited contexts in this same event, before committing a frame.
            if self.engine.theme() == self.view.theme() {
                return Ok(());
            }
        }
        // One settle protocol, shared with the reload host: rerun declarations
        // while the view asks for another pass, at most eight times.
        #[cfg(feature = "profiling")]
        let span = tracing::trace_span!(target: "plico::perf", "frame.declare");
        #[cfg(feature = "profiling")]
        let _guard = span.enter();
        let passes = self
            .engine
            .declare_until_settled(&mut HostViewChild(&mut self.view))
            .map_err(|source| {
                self.pending = true;
                self.declared_changes.insert(plico::Changes::STRUCTURE);
                self.declared_dirty.clear();
                self.declared_seen.clear();
                match source {
                    plico::FrameError::DidNotSettle => SourceError::DidNotSettle,
                    other => SourceError::Declaration { source: other },
                }
            })?;
        self.batch_stats.declaration_passes += passes as usize;
        self.declaration_stats = self.engine.last_stats();
        self.declared_dirty.extend(self.engine.drain_settle_dirty());
        self.declared_changes |= self.engine.take_settle_changes();
        self.requests = self.engine.changes() & plico::Changes::ANIMATE;
        self.declaration_micros = start.elapsed().as_micros();
        #[cfg(feature = "profiling")]
        tracing::trace!(target: "plico::perf", passes, stats_scope = %"last_pass", visited = self.declaration_stats.visited,
            created = self.declaration_stats.created, reused = self.declaration_stats.reused,
            removed = self.declaration_stats.removed, text_writes = self.declaration_stats.text_writes,
            order_changes = self.declaration_stats.order_changes,
            declaration_us = self.declaration_micros as u64, "declaration.work");
        self.pending = false;
        self.declarations_pending = true;
        Ok(())
    }
}
impl<V: HostView> FrameSource for Source<V> {
    fn take_host_action(&mut self) -> Option<HostAction> {
        self.view.take_host_action()
    }
    fn set_wake_callback(&mut self, wake: plico_renderer::WakeCallback) {
        self.wake_batch.pending.store(false, Ordering::Release);
        let batch = self.wake_batch.clone();
        self.view.set_wake_callback(Arc::new(move || {
            batch.notifications.fetch_add(1, Ordering::Relaxed);
            if !batch.pending.swap(true, Ordering::AcqRel) {
                batch.scheduled.fetch_add(1, Ordering::Relaxed);
                wake();
            }
        }));
    }
    fn poll_async(&mut self) -> bool {
        // Re-arm before draining: a completion racing with poll must schedule
        // another wake, even when it arrives after the drain's final read.
        self.wake_batch.pending.store(false, Ordering::Release);
        self.batch_stats.service_polls += 1;
        let changed = self.view.poll_updates();
        self.pending |= changed;
        changed
    }
    fn prepare_for_event(&mut self, tcx: &mut TextContext) {
        if let Some(now) = self.clock_instant() {
            self.prepare_for_event_at(tcx, now);
        } else {
            self.last_error = Some(SourceError::ClockRange);
        }
    }
    fn build_frame(&mut self, tcx: &mut TextContext, width: u32, height: u32) -> Vec<PlacedNode> {
        if let Some(now) = self.clock_instant() {
            self.build_frame_at(tcx, width, height, now)
        } else {
            self.last_error = Some(SourceError::ClockRange);
            self.adapter.presented()
        }
    }
    fn paint_scene(
        &mut self,
        scene: &mut Scene,
        nodes: &[PlacedNode],
        tcx: &mut TextContext,
        size: (u32, u32),
        scale: f64,
    ) {
        self.paint_frame_scene(scene, Some(nodes), tcx, size, scale);
    }
    fn render_retained_frame(
        &mut self,
        scene: &mut Scene,
        tcx: &mut TextContext,
        size: (u32, u32),
        scale: f64,
    ) -> Option<(usize, f64, f64)> {
        tcx.set_native_paragraph_scale(scale);
        let start = Instant::now();
        if let Some(now) = self.clock_instant() {
            self.prepare_frame_at(tcx, size.0, size.1, now, false);
        } else {
            self.last_error = Some(SourceError::ClockRange);
        }
        let build_ms = start.elapsed().as_secs_f64() * 1000.;
        let start = Instant::now();
        self.paint_frame_scene(scene, None, tcx, size, scale);
        self.paint_debug_overlay(scene, &[], tcx, scale);
        Some((
            self.adapter.placed.len() + self.adapter.tips.layer().len(),
            build_ms,
            start.elapsed().as_secs_f64() * 1000.,
        ))
    }
    fn build_and_paint(
        &mut self,
        scene: &mut Scene,
        tcx: &mut TextContext,
        width: u32,
        height: u32,
        scale: f64,
    ) -> (usize, f64, f64) {
        // Same frame pipeline as build_frame_at, but the scene is encoded
        // straight from the adapter's retained state: the per-node placed
        // list is never materialized for the host.
        tcx.set_native_paragraph_scale(scale);
        let start = Instant::now();
        if let Some(now) = self.clock_instant() {
            self.prepare_frame_at(tcx, width, height, now, true);
        } else {
            self.last_error = Some(SourceError::ClockRange);
        }
        let build_ms = start.elapsed().as_secs_f64() * 1000.;
        let start = Instant::now();
        self.paint_frame_scene(scene, None, tcx, (width, height), scale);
        self.paint_debug_overlay(scene, &[], tcx, scale);
        (
            self.adapter.placed.len() + self.adapter.tips.layer().len(),
            build_ms,
            start.elapsed().as_secs_f64() * 1000.,
        )
    }
    fn base_color(&self) -> Color {
        self.background
            .unwrap_or_else(|| rgb(self.engine.theme().surface))
    }
    fn has_anim(&self) -> bool {
        self.transition.active()
            || self.adapter.motions.active()
            || !self.adapter.scroll_motions.is_empty()
            || self.adapter.shader_active()
            || self.requests.contains(plico::Changes::ANIMATE)
            || self.engine.presence_deadline().is_some()
            || self.adapter.tips.needs_frame()
            || self.adapter.scrollbars.needs_frame()
    }
    fn paint_debug_overlay(
        &mut self,
        scene: &mut Scene,
        _nodes: &[PlacedNode],
        _tcx: &mut TextContext,
        scale: f64,
    ) {
        if let Some((width, height)) = self.viewport {
            self.transition.compose(
                scene,
                self.engine.theme(),
                (width, height, scale),
                self.frame_time,
            );
        }
    }
    fn handle_event(&mut self, event: UiEvent) -> EventResponse {
        let activity = match &event {
            UiEvent::Pointer(pointer) => {
                pointer.phase == PointerPhase::Down
                    || (pointer.phase == PointerPhase::Move && pointer.buttons != 0)
            }
            UiEvent::Key(key) => key.phase == KeyPhase::Down && !key.synthetic,
            UiEvent::TextInput(text) | UiEvent::Paste(text) => !text.is_empty(),
            UiEvent::Ime(plico_renderer::ImeEvent::Commit(text))
            | UiEvent::Ime(plico_renderer::ImeEvent::Preedit { text, .. }) => !text.is_empty(),
            UiEvent::Ime(plico_renderer::ImeEvent::DeleteSurrounding {
                before_bytes,
                after_bytes,
            }) => *before_bytes != 0 || *after_bytes != 0,
            UiEvent::Wheel(wheel) => wheel.delta_x != 0. || wheel.delta_y != 0.,
            UiEvent::Gesture(_) => true,
            _ => false,
        };
        if activity {
            self.view.on_user_activity();
        }
        let mut scroll_changed = false;
        if let Some(now) = self.clock_instant() {
            self.set_tick(now, false);
            let scroll = self
                .adapter
                .advance_scroll(self.engine.time().now, &self.engine);
            self.pending |= scroll.declarations;
            scroll_changed = scroll.changed;
        }
        if let UiEvent::Pointer(pointer) = &event
            && pointer.phase == PointerPhase::Up
        {
            self.transition
                .set_origin(pointer.position.x, pointer.position.y);
        }
        let fallback = match &event {
            UiEvent::Key(key)
                if key.phase == KeyPhase::Down
                    && !key.synthetic
                    && self.adapter.modal_stack.is_empty()
                    && !self
                        .adapter
                        .focused
                        .and_then(|e| self.engine.input(e))
                        .is_some_and(|(buffer, _)| {
                            buffer.is_composing()
                                || (key.key.chars().count() == 1
                                    && !key.modifiers.control()
                                    && !key.modifiers.meta()
                                    && !key.modifiers.alt())
                        }) =>
            {
                Some(key.clone())
            }
            _ => None,
        };
        let (mut r, mut declare) = self
            .adapter
            .event_with_invalidation(&mut self.engine, event);
        if !r.handled && fallback.is_some_and(|key| self.view.on_unhandled_key(&key)) {
            declare = true;
            r.handled = true;
            r.request_redraw = true;
            r.consume_key_text = true;
        }
        self.pending |= declare && r.request_redraw;
        // prepare_for_event may have committed a later scroll position before
        // this event. Request its presentation even if the event hits nothing
        // and the interpolation has just finished.
        r.request_redraw |= scroll_changed
            || self
                .target_scene
                .as_ref()
                .is_some_and(|(_, _, revision)| *revision != self.revision);
        r
    }
    fn ime_state(&self) -> Option<plico_renderer::ImeState> {
        self.adapter.input_ime_state(&self.engine)
    }
    fn set_semantics_enabled(&mut self, enabled: bool) {
        self.semantics_enabled = enabled;
    }
    fn semantic_snapshot(&self) -> Option<Arc<plico_renderer::SemanticSnapshot>> {
        self.semantics_enabled
            .then(|| self.adapter.semantics_shared(&self.engine, self.revision))
    }
    fn handle_semantic_action(&mut self, action: plico_renderer::SemanticAction) -> bool {
        self.adapter.tips.prefer_hover = false;
        if matches!(action, plico_renderer::SemanticAction::Click { .. }) {
            self.adapter.dismiss_tooltip();
        }
        let handled = self.adapter.semantic_action(&mut self.engine, action);
        if handled {
            self.view.on_user_activity();
        }
        self.pending |= handled;
        handled
    }
    fn pointer_cursor(&self) -> CursorStyle {
        self.adapter.cursor()
    }
}
