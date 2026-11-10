//! Retained Engine projection, layout, painting, hit testing and semantics.
use crate::{
    button_state, input, layout_style, modal, presentation, scroll_motion, scrollbar_visibility,
    selection, shader, style_motion, toggle, tooltip,
};
use peniko::Color;
use plico::{Capabilities, Color as Token, Engine, Entity, Kind, Theme};
use plico_renderer::{
    EventResponse, KeyPhase, PointerButton, PointerPhase, TextContext, UiEvent,
    layout::{
        PlacedNode, compute_layout_with_widgets, flatten_subtree_with_scroll,
        flatten_with_scroll_into,
    },
    style::{CornerRadii, CursorStyle, Paint},
};
use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    sync::Arc,
    time::Duration,
};
use taffy::{geometry::Rect, prelude::*};
use web_time::Instant;

mod event;
mod hit;
mod layout;
mod semantics;
mod sync;

pub(crate) use semantics::semantic_id;
pub(crate) use sync::make_paint;

pub(crate) fn rgb(n: u32) -> Color {
    Color::from_rgb8((n >> 16) as u8, (n >> 8) as u8, n as u8)
}
fn resolve(c: Token, t: Theme) -> Color {
    rgb(t.resolve(c))
}
pub(crate) fn interactive(k: Kind) -> bool {
    k.interactive()
}
#[derive(Default, Debug, Clone, Copy)]
pub struct BackendStats {
    pub created: usize,
    pub removed: usize,
    /// Portable layout converted into a Taffy style (before equality checks).
    pub layout_style_builds: usize,
    pub style_writes: usize,
    pub paint_writes: usize,
    pub child_writes: usize,
    pub sync_micros: u128,
    pub layout_micros: u128,
    pub sync_visits: usize,
    pub layout_runs: usize,
    pub tooltip_layout_runs: usize,
    pub flatten_visits: usize,
    pub flatten_patches: usize,
    pub layout_cache_checks: usize,
    pub paint_patches: usize,
    pub presentation_visits: usize,
    pub text_measure_invalidations: usize,
    pub placed_node_clones: usize,
    pub selection_visits: usize,
    pub slider_visits: usize,
    pub shader_visits: usize,
    pub tooltip_visits: usize,
    pub input_rebuilds: usize,
    pub input_reuses: usize,
    pub control_align_checks: usize,
    pub input_align_checks: usize,
    pub control_metric_builds: usize,
    pub control_metric_reuses: usize,
    pub scroll_visits: usize,
    pub scene_builds: usize,
    pub scene_reuses: usize,
    pub scene_fragment_builds: usize,
    pub scene_fragment_reuses: usize,
    pub scene_encoded_nodes: usize,
    pub scene_full_rebuilds: usize,
    pub scene_invalid_roots: usize,
    pub svg_parses: usize,
    pub changes: plico::Changes,
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum SyncMode {
    Node,
    Structure,
    Full,
}
pub(super) struct ControlMetrics {
    epoch: usize,
    width: f32,
    metrics: Option<[f32; 3]>,
}
#[derive(Clone, Copy, Default)]
pub(super) struct HitSubtree {
    end: usize,
    first_child: Option<usize>,
    next_sibling: Option<usize>,
    bounds: [f32; 4],
}
#[derive(Default)]
pub(super) struct HitCache {
    pub(super) subtrees: Vec<HitSubtree>,
    pub(super) dirty: bool,
    pub(super) builds: usize,
    pub(super) checks: usize,
    pub(super) pruned: usize,
}

#[derive(Default)]
pub(super) struct SemanticCache {
    epoch: u64,
    snapshot: Option<(u64, u64, Arc<plico_renderer::SemanticSnapshot>)>,
    nodes: HashMap<Entity, plico_renderer::SemanticNode>,
    builds: usize,
    reuses: usize,
    snapshot_reuses: usize,
}

pub(super) struct Retained {
    pub(super) id: NodeId,
    pub(super) style: taffy::Style,
    pub(super) paint_key: PaintKey,
    pub(super) spans: Vec<plico::TextSpan>,
    pub(super) text: Arc<str>,
    pub(super) children: Vec<NodeId>,
    pub(super) layout: Option<taffy::Layout>,
    pub(super) toggle_line_height: Option<f32>,
    pub(super) had_grid: bool,
}

/// Everything that feeds one node's paint: the retained copy is compared
/// against the freshly computed key to decide whether to rewrite the paint
/// payload. The progress floats animate every frame, so their comparison is
/// the "same interpolated value" check, not an epsilon test.
#[derive(Clone, PartialEq)]
pub(super) struct PaintKey {
    style: plico::Style,
    kind: Kind,
    theme: Theme,
    hovered: bool,
    pressed: bool,
    focused: bool,
    checked_progress: f32,
    focus_progress: f32,
    raster_correction: Option<f32>,
}

pub(super) type SvgResult = Result<Arc<plico_renderer::svg::SvgImage>, Arc<str>>;
pub(super) type SvgCache = HashMap<Arc<str>, HashMap<Option<u32>, SvgResult>>;
fn svg_color(source: &str, style: plico::Style, theme: Theme) -> Option<u32> {
    source
        .contains("currentColor")
        .then(|| theme.resolve(style.foreground))
}
pub(super) type ImageCache = HashMap<Arc<str>, Option<Arc<plico_renderer::image::RasterImage>>>;
fn cached_image(
    cache: &mut ImageCache,
    stats: &mut BackendStats,
    encoded: &str,
) -> Option<Arc<plico_renderer::image::RasterImage>> {
    if let Some(image) = cache.get(encoded) {
        return image.clone();
    }
    stats.svg_parses += 1;
    let image = plico::ImageAsset::decode(encoded)
        .and_then(|bytes| plico_renderer::image::RasterImage::decode(&bytes).ok())
        .map(Arc::new);
    cache.insert(Arc::from(encoded), image.clone());
    image
}
fn cached_svg(
    cache: &mut SvgCache,
    stats: &mut BackendStats,
    source: &str,
    color: Option<u32>,
) -> Option<Arc<plico_renderer::svg::SvgImage>> {
    if !cache.contains_key(source) {
        cache.insert(Arc::from(source), HashMap::new());
    }
    let variants = cache.get_mut(source).unwrap();
    variants
        .entry(color)
        .or_insert_with(|| {
            stats.svg_parses += 1;
            let result = match color {
                Some(color) => {
                    plico_renderer::svg::SvgImage::parse_with_current_color(source, color)
                }
                None => plico_renderer::svg::SvgImage::parse(source),
            };
            result
                .map(Arc::new)
                .map_err(|error| Arc::from(error.to_string()))
        })
        .as_ref()
        .ok()
        .cloned()
}

pub struct Adapter {
    pub(super) font_family: Arc<str>,
    pub(super) font_dirty: bool,
    pub(super) selection: selection::TextSelection,
    pub(super) input_pointers: input::InputPointers,
    /// Selection state changed outside a frame; the next update must refresh
    /// paragraphs and rects even when the backend otherwise did no work.
    pub(super) selection_dirty: bool,
    pub(super) tree: TaffyTree<Paint>,
    pub(super) nodes: HashMap<Entity, Retained>,
    pub placed: Vec<PlacedNode>,
    pub stats: BackendStats,
    pub(super) hovered: Option<Entity>,
    /// Latest pointer position in window logical pixels; None outside.
    pub(super) pointer_position: Option<(f64, f64)>,
    pub(super) pressed: Option<Entity>,
    pub(super) secondary_pressed: Option<Entity>,
    pub(super) presentation: HashMap<NodeId, presentation::Presentation>,
    pub(super) presentation_dirty: bool,
    pub(super) presentation_roots: HashSet<NodeId>,
    pub(super) anchor_motion_dirty: bool,
    pub(super) focused: Option<Entity>,
    pub(super) requested_focus: Option<Entity>,
    pub(super) blurred_focus: Option<Entity>,
    pub(super) requested_scroll: HashMap<Entity, f32>,
    pub(super) modal_stack: Vec<(Entity, Option<Entity>)>,
    pub(super) backdrop_pressed: Option<Entity>,
    pub(super) departing_popup: Option<Entity>,
    pub(super) popup_open_last: Option<Entity>,
    pub(super) tips: tooltip::Tooltips,
    pub(super) slider_drag: Option<Entity>,
    /// Which thumb of a range slider the pointer/keyboard edits (0 low, 1 high).
    pub(super) slider_thumbs: HashMap<Entity, usize>,
    pub(super) focus_visible: bool,
    pub(super) scroll_offsets: HashMap<NodeId, [f32; 2]>,
    pub(super) scroll_dirty: bool,
    pub(super) scroll_motions: HashMap<NodeId, scroll_motion::ScrollMotion>,
    pub(super) smooth_scroll_enabled: bool,
    pub(super) reveal_focus: bool,
    pub(super) scrollbars: scrollbar_visibility::Scrollbars,
    pub(super) scrollbar_drag: Option<(NodeId, f64, f32)>,
    pub(super) input_layouts: HashMap<Entity, input::Geometry>,
    pub(super) control_metrics: HashMap<Entity, ControlMetrics>,
    pub(super) selectable_nodes: HashSet<Entity>,
    pub(super) slider_nodes: HashSet<Entity>,
    pub(super) shader_nodes: HashSet<Entity>,
    pub(super) tooltip_nodes: HashSet<Entity>,
    pub(super) shaders: HashMap<Entity, shader::Surface>,
    pub(super) shader_scale: f64,
    pub(super) shader_motion_enabled: bool,
    pub(super) shader_reset_time: bool,
    pub(super) tab_order: Vec<Entity>,
    pub(super) tree_typeahead: (Option<NodeId>, String, Duration),
    pub(super) motions: style_motion::StyleMotions,
    pub(super) reverse: HashMap<NodeId, Entity>,
    pub(super) placed_indices: HashMap<NodeId, usize>,
    pub(super) scroll_indices: Vec<usize>,
    pub(super) control_align_nodes: HashSet<Entity>,
    pub(super) input_align_nodes: HashSet<Entity>,
    pub(super) control_align_indices: Vec<usize>,
    pub(super) input_align_indices: Vec<usize>,
    pub(super) geometry_dirty: bool,
    #[doc(hidden)]
    pub experiment_force_full_geometry: bool,
    #[doc(hidden)]
    pub experiment_force_full_semantics: bool,
    #[doc(hidden)]
    pub experiment_force_full_hit_test: bool,
    pub(super) geometry_roots: HashSet<Entity>,
    pub(super) topology_roots: HashSet<Entity>,
    pub(super) removed_node_ids: Vec<NodeId>,
    pub(crate) scene_dirty_nodes: HashSet<NodeId>,
    pub(crate) scene_full_dirty: bool,
    pub(super) paint_dirty: Vec<NodeId>,
    pub(super) last_theme: Option<Theme>,
    pub(super) interaction_synced: [Option<Entity>; 4],
    pub(super) viewport: Option<(u32, u32)>,
    pub(super) dirty_scratch: HashSet<Entity>,
    pub(super) synced_scratch: HashSet<Entity>,
    pub(super) svg_cache: SvgCache,
    pub(super) image_cache: ImageCache,
    pub(super) semantic: RefCell<SemanticCache>,
    pub(super) hit: RefCell<HitCache>,
}
impl Default for Adapter {
    fn default() -> Self {
        Self {
            font_family: Arc::from(plico_renderer::text::DEFAULT_UI_FONT_FAMILY),
            font_dirty: false,
            selection: Default::default(),
            input_pointers: Default::default(),
            selection_dirty: true,
            tree: TaffyTree::new(),
            nodes: HashMap::new(),
            placed: Vec::new(),
            stats: BackendStats::default(),
            hovered: None,
            pointer_position: None,
            pressed: None,
            secondary_pressed: None,
            presentation: HashMap::new(),
            presentation_dirty: false,
            presentation_roots: HashSet::new(),
            anchor_motion_dirty: false,
            focused: None,
            requested_focus: None,
            blurred_focus: None,
            requested_scroll: HashMap::new(),
            modal_stack: Vec::new(),
            backdrop_pressed: None,
            departing_popup: None,
            popup_open_last: None,
            tips: Default::default(),
            slider_drag: None,
            slider_thumbs: HashMap::new(),
            focus_visible: false,
            scroll_offsets: HashMap::new(),
            scroll_dirty: false,
            scroll_motions: HashMap::new(),
            smooth_scroll_enabled: true,
            reveal_focus: false,
            scrollbars: Default::default(),
            scrollbar_drag: None,
            input_layouts: HashMap::new(),
            control_metrics: HashMap::new(),
            selectable_nodes: HashSet::new(),
            slider_nodes: HashSet::new(),
            shader_nodes: HashSet::new(),
            tooltip_nodes: HashSet::new(),
            shaders: HashMap::new(),
            shader_scale: 1.,
            shader_motion_enabled: true,
            shader_reset_time: false,
            tab_order: Vec::new(),
            tree_typeahead: (None, String::new(), Duration::ZERO),
            motions: Default::default(),
            reverse: HashMap::new(),
            placed_indices: HashMap::new(),
            scroll_indices: Vec::new(),
            control_align_nodes: HashSet::new(),
            input_align_nodes: HashSet::new(),
            control_align_indices: Vec::new(),
            input_align_indices: Vec::new(),
            geometry_dirty: false,
            experiment_force_full_geometry: false,
            experiment_force_full_semantics: false,
            experiment_force_full_hit_test: false,
            geometry_roots: HashSet::new(),
            topology_roots: HashSet::new(),
            removed_node_ids: Vec::new(),
            scene_dirty_nodes: HashSet::new(),
            scene_full_dirty: false,
            paint_dirty: Vec::new(),
            last_theme: None,
            interaction_synced: [None; 4],
            dirty_scratch: HashSet::new(),
            synced_scratch: HashSet::new(),
            svg_cache: HashMap::new(),
            image_cache: HashMap::new(),
            semantic: Default::default(),
            hit: Default::default(),
            viewport: None,
        }
    }
}
impl Adapter {
    pub fn focused(&self) -> Option<Entity> {
        self.focused
    }
    pub fn backend_id(&self, e: Entity) -> Option<NodeId> {
        self.nodes.get(&e).map(|n| n.id)
    }
    pub fn layout(
        &mut self,
        engine: &mut Engine,
        tcx: &mut TextContext,
        width: u32,
        height: u32,
    ) -> Vec<PlacedNode> {
        self.update(engine, tcx, width, height, &[], plico::Changes::STRUCTURE)
    }
    fn update(
        &mut self,
        engine: &mut Engine,
        tcx: &mut TextContext,
        width: u32,
        height: u32,
        declared: &[Entity],
        changes: plico::Changes,
    ) -> Vec<PlacedNode> {
        self.update_retained(engine, tcx, width, height, declared, changes);
        self.presented()
    }
    pub(crate) fn update_retained(
        &mut self,
        engine: &mut Engine,
        tcx: &mut TextContext,
        width: u32,
        height: u32,
        declared: &[Entity],
        changes: plico::Changes,
    ) {
        self.stats = BackendStats::default();
        self.semantic.get_mut().epoch = self.semantic.get_mut().epoch.wrapping_add(1);
        self.anchor_motion_dirty = false;
        self.geometry_dirty = std::mem::take(&mut self.font_dirty);
        if self.geometry_dirty {
            // Font/provider/DPI changes can alter advances without changing
            // Paint. Invalidate Taffy's leaf measurement cache explicitly;
            // merely running layout again can reuse stale min/max-content sizes.
            for node in self.nodes.values() {
                if self
                    .tree
                    .get_node_context(node.id)
                    .is_some_and(|paint| paint.text.is_some())
                {
                    self.tree.mark_dirty(node.id).unwrap();
                    self.stats.text_measure_invalidations += 1;
                }
            }
        }
        self.paint_dirty.clear();
        self.geometry_roots.clear();
        self.topology_roots.clear();
        self.removed_node_ids.clear();
        self.scene_dirty_nodes.clear();
        self.scene_full_dirty = false;
        let start = Instant::now();
        let full = self.nodes.is_empty()
            || (changes.contains(plico::Changes::STRUCTURE) && declared.is_empty());
        let structural = full || changes.contains(plico::Changes::STRUCTURE);
        let mut dirty = std::mem::take(&mut self.dirty_scratch);
        dirty.clear();
        if self.geometry_dirty {
            dirty.extend(self.nodes.keys().copied());
        }
        if changes.intersects(
            plico::Changes::STRUCTURE
                | plico::Changes::LAYOUT
                | plico::Changes::PAINT
                | plico::Changes::SEMANTICS
                | plico::Changes::HIT_TEST
                | plico::Changes::TRANSFORM,
        ) {
            dirty.extend(declared.iter().copied());
        }
        dirty.extend(self.motions.drain_dirty());
        let previous_focus = self.focused;
        for state in [&mut self.hovered, &mut self.pressed] {
            if state.is_some_and(|e| {
                engine
                    .node(e)
                    .is_none_or(|n| !interactive(n.kind) || n.style.disabled)
            }) {
                *state = None;
            }
        }
        if self.focused.is_some_and(|e| {
            engine.node(e).is_none_or(|n| {
                !n.kind.capabilities().contains(Capabilities::FOCUS) || n.style.disabled
            })
        }) {
            self.focused = None;
        }
        self.sync_modal_focus(engine);
        if let Some(entity) = self.requested_focus.take()
            && engine.input(entity).is_some()
            && engine.node(entity).is_some_and(|n| !n.style.disabled)
            && self.modal_allows(engine, entity)
        {
            self.focused = Some(entity);
            self.focus_visible = false;
            self.reveal_focus = true;
        }
        if self.slider_drag.is_some_and(|e| {
            !self.modal_allows(engine, e)
                || engine
                    .node(e)
                    .is_none_or(|n| n.kind != Kind::Slider || n.style.disabled)
        }) {
            self.slider_drag = None;
        }
        if previous_focus != self.focused
            && let Some((buffer, _)) = previous_focus.and_then(|e| engine.input(e))
            && buffer.is_composing()
        {
            buffer.edit(plico::InputEdit::ClearPreedit);
        }
        let interaction = [
            self.hovered,
            self.pressed,
            self.focused.filter(|_| self.focus_visible),
            self.focused,
        ];
        for (old, new) in self.interaction_synced.into_iter().zip(interaction) {
            if old != new {
                dirty.extend(old);
                dirty.extend(new);
            }
        }
        let resized = self.viewport != Some((width, height));
        if resized {
            dirty.insert(engine.root());
            self.geometry_dirty = true;
        }
        let theme_changed = self.last_theme != Some(engine.theme());
        if theme_changed {
            self.motions.snap_theme();
            dirty.extend(self.nodes.keys().copied());
        }
        {
            let semantic = self.semantic.get_mut();
            if full {
                semantic.nodes.clear();
            } else {
                for entity in &dirty {
                    semantic.nodes.remove(entity);
                }
            }
        }
        let mut tab_dirty = structural;
        let mut synced = std::mem::take(&mut self.synced_scratch);
        synced.clear();
        if full {
            self.tab_order.clear();
            self.sync(
                engine,
                engine.root(),
                &mut synced,
                width,
                height,
                SyncMode::Full,
            );
        } else {
            for &e in &dirty {
                if let Some(node) = engine.node(e) {
                    if let Some(previous) = self.nodes.get(&e) {
                        tab_dirty |= previous.paint_key.style.disabled != node.style.disabled
                            || previous.paint_key.style.tab_stop != node.style.tab_stop
                            || previous.paint_key.style.selected != node.style.selected
                            || previous.paint_key.style.role != node.style.role
                            || previous.paint_key.kind != node.kind;
                    }
                    self.sync(
                        engine,
                        e,
                        &mut synced,
                        width,
                        height,
                        if structural {
                            SyncMode::Structure
                        } else {
                            SyncMode::Node
                        },
                    );
                }
            }
        }
        self.synced_scratch = synced;
        if structural {
            // The engine already knows exactly which entities were removed,
            // accumulated across declaration passes; no full-arena rescan.
            let stale: Vec<_> = if full {
                // A compatibility sync may follow simulated frames or another adapter.
                self.nodes
                    .keys()
                    .copied()
                    .filter(|e| engine.node(*e).is_none())
                    .collect()
            } else {
                Vec::new()
            };
            for e in engine.drain_removed().chain(stale) {
                let Some(n) = self.nodes.remove(&e) else {
                    // Created and removed between syncs; never retained here.
                    continue;
                };
                self.motions.remove(e);
                self.input_layouts.remove(&e);
                self.control_metrics.remove(&e);
                self.control_align_nodes.remove(&e);
                self.input_align_nodes.remove(&e);
                self.selection_dirty |= self.selectable_nodes.remove(&e);
                self.slider_nodes.remove(&e);
                self.slider_thumbs.remove(&e);
                self.shader_nodes.remove(&e);
                self.tooltip_nodes.remove(&e);
                self.reverse.remove(&n.id);
                self.semantic.get_mut().nodes.remove(&e);
                self.removed_node_ids.push(n.id);
                self.scroll_offsets.remove(&n.id);
                self.scroll_motions.remove(&n.id);
                self.scrollbars.states.remove(&n.id);
                if self.scrollbar_drag.is_some_and(|(id, ..)| id == n.id) {
                    self.scrollbar_drag = None;
                }
                self.tree.remove(n.id).unwrap();
                self.stats.removed += 1;
                self.geometry_dirty = true;
            }
        }
        if structural || theme_changed || self.stats.svg_parses > 0 {
            #[cfg(feature = "profiling")]
            let _span = tracing::trace_span!(target: "plico::layout", "backend.assets").entered();
            self.image_cache.retain(|encoded, _| {
                self.nodes
                    .values()
                    .any(|n| n.paint_key.kind == Kind::Image && n.text == *encoded)
            });
            self.svg_cache.retain(|source, variants| {
                variants.retain(|color, _| {
                    self.nodes.values().any(|n| {
                        let PaintKey {
                            style, kind, theme, ..
                        } = &n.paint_key;
                        (*kind == Kind::Svg
                            && n.text == *source
                            && svg_color(source, *style, *theme) == *color)
                            || (*kind == Kind::Checkbox
                                && source.as_ref() == plico::icons::CHECK.source()
                                && *color == Some(theme.on_accent))
                    })
                });
                !variants.is_empty()
            });
        }
        self.dirty_scratch = dirty;
        self.last_theme = Some(engine.theme());
        if self.viewport != Some((width, height)) {
            self.reveal_focus = true;
        }
        self.viewport = Some((width, height));
        self.interaction_synced = interaction;
        self.stats.sync_micros = start.elapsed().as_micros();
        self.scroll_motions.retain(|id, _| {
            self.reverse
                .get(id)
                .and_then(|e| engine.node(*e))
                .is_some_and(|n| n.style.scroll_y)
        });
        for (entity, offset) in std::mem::take(&mut self.requested_scroll) {
            if engine.node(entity).is_some_and(|n| n.style.scroll_y)
                && let Some(node) = self.nodes.get(&entity)
            {
                // Immediate reveal must mount and display the same virtual rows.
                self.scroll_to(node.id, offset, false, engine.time().now);
            }
        }
        let start = Instant::now();
        if self.geometry_dirty {
            let root = self.nodes[&engine.root()].id;
            compute_layout_with_widgets(
                &mut self.tree,
                root,
                [width as f32, height as f32],
                tcx,
                1.,
                |_node, _cx| None,
            );
            self.stats.layout_runs = 1;
            let wide_structure = self.placed.len() >= 32
                && (self.stats.created + self.stats.removed) * 4 >= self.placed.len();
            let attempt_patch = !self.experiment_force_full_geometry
                && !full
                && !resized
                && !theme_changed
                // Layout candidates only describe Taffy box changes. A
                // simultaneous scroll can move an unrelated subtree whose
                // boxes did not change; patching candidates alone leaves its
                // placed offset stale and writes it back below, freezing scroll.
                && !self.scroll_dirty
                && !wide_structure
                && !self.geometry_roots.is_empty();
            let patch = attempt_patch.then(|| self.patch_dirty_geometry(engine));
            let incremental = patch == Some(Some(true));
            if !incremental {
                // A forced full-geometry control still has exact Reactive dirty
                // information. Only an initial sync or a failed patch needs to
                // conservatively invalidate every Scene Region.
                self.scene_full_dirty =
                    full || (!self.experiment_force_full_geometry && patch.flatten().is_none());
                flatten_with_scroll_into(&self.tree, root, &self.scroll_offsets, &mut self.placed);
                self.stats.flatten_visits = self.placed.len();
                self.cache_all_layouts();
                self.reindex_placed();
                self.apply_sticky();
            } else {
                if self.stats.flatten_patches > 0 {
                    self.reindex_placed();
                }
                // A fixed-size measurement change can leave every Taffy box
                // unchanged. Its resolved Paint still needs to reach `placed`.
                for id in &self.paint_dirty {
                    if let Some(index) = self.placed_indices.get(id) {
                        self.placed[*index].paint =
                            self.tree.get_node_context(*id).unwrap().clone();
                        self.stats.paint_patches += 1;
                    }
                }
            }
            // geometry_dirty deliberately stays set: tall centered controls
            // realign their text every frame through the geometry branch
            // (alignment.rs); treating the flag as a consumed latch breaks
            // those frames once nothing new sets it. Observers must not read
            // it as "this frame did layout work" - check stats instead.
        } else if self.scroll_dirty {
            flatten_with_scroll_into(
                &self.tree,
                self.nodes[&engine.root()].id,
                &self.scroll_offsets,
                &mut self.placed,
            );
            self.stats.flatten_visits = self.placed.len();
            self.reindex_placed();
            self.apply_sticky();
        } else {
            for id in &self.paint_dirty {
                if let Some(index) = self.placed_indices.get(id) {
                    self.placed[*index].paint = self.tree.get_node_context(*id).unwrap().clone();
                    self.stats.paint_patches += 1;
                }
            }
        }
        if resized || theme_changed || self.scroll_dirty {
            self.scene_full_dirty = true;
        }
        if self.geometry_dirty || self.scroll_dirty {
            #[cfg(feature = "profiling")]
            let _span =
                tracing::trace_span!(target: "plico::layout", "backend.geometry_post").entered();
            self.position_popovers(engine, tcx, width, height);
            let mut clamped = false;
            for p in &self.placed {
                if !p.scroll.scrollable[1]
                    || self
                        .scroll_motions
                        .get(&p.node_id)
                        .is_some_and(|m| m.target > p.scroll.range[1])
                {
                    self.scroll_motions.remove(&p.node_id);
                }
                let offset = [
                    0.,
                    if p.scroll.scrollable[1] {
                        p.scroll.offset[1].clamp(0., p.scroll.range[1])
                    } else {
                        0.
                    },
                ];
                clamped |= offset != p.scroll.offset;
                if p.scroll.scrollable[1] {
                    self.scroll_offsets.insert(p.node_id, offset);
                } else {
                    self.scroll_offsets.remove(&p.node_id);
                }
            }
            if clamped {
                flatten_with_scroll_into(
                    &self.tree,
                    self.nodes[&engine.root()].id,
                    &self.scroll_offsets,
                    &mut self.placed,
                );
                self.stats.flatten_visits += self.placed.len();
                self.reindex_placed();
                self.apply_sticky();
            }
            self.scroll_dirty = false;
            {
                #[cfg(feature = "profiling")]
                let _span =
                    tracing::trace_span!(target: "plico::layout", "backend.align").entered();
                self.align_controls(tcx, engine);
            }
        } else {
            #[cfg(feature = "profiling")]
            let _span = tracing::trace_span!(target: "plico::layout", "backend.align").entered();
            for dirty in 0..self.paint_dirty.len() {
                if let Some(&index) = self.placed_indices.get(&self.paint_dirty[dirty]) {
                    self.align_control_text(index, tcx);
                    self.align_input(index, tcx, engine);
                }
            }
        }
        if tab_dirty {
            let candidates = self
                .placed
                .iter()
                .filter_map(|n| self.reverse.get(&n.node_id).copied())
                .filter(|e| {
                    engine.node(*e).is_some_and(|n| {
                        n.kind.capabilities().contains(Capabilities::TAB_STOP)
                            && !n.style.disabled
                            && n.style.tab_stop
                    })
                })
                .collect();
            self.tab_order = self.roving_tab_order(engine, candidates);
        }
        if self.focused.is_some_and(|e| {
            !self.tab_order.contains(&e) && engine.node(e).is_none_or(|n| n.kind != Kind::Dialog)
        }) {
            self.focused = None;
        }
        let mut focus_scrolled = false;
        if std::mem::take(&mut self.reveal_focus)
            && let Some(focused) = self.focused.and_then(|e| self.nodes.get(&e)).map(|n| n.id)
        {
            // The chain is collected first: revealing may re-flatten and
            // reindex `placed`, which the lazy iterator borrows.
            let chain: Vec<NodeId> = self.placed_ancestors(focused).map(|p| p.node_id).collect();
            for id in chain {
                let Some(scroll) = self
                    .placed_indices
                    .get(&id)
                    .map(|&index| &self.placed[index])
                    .filter(|p| p.scroll.scrollable[1])
                    .map(|p| p.scroll)
                else {
                    continue;
                };
                self.scroll_motions.remove(&id);
                let target = self.placed[self.placed_indices[&focused]].rect;
                let delta = if target[1] < scroll.port[1] {
                    target[1] - scroll.port[1]
                } else if target[3] > scroll.port[3] {
                    target[3] - scroll.port[3]
                } else {
                    0.
                };
                let next = (scroll.offset[1] + delta).clamp(0., scroll.range[1]);
                if next != scroll.offset[1] {
                    focus_scrolled = true;
                    self.scroll_offsets.insert(id, [scroll.offset[0], next]);
                    flatten_with_scroll_into(
                        &self.tree,
                        self.nodes[&engine.root()].id,
                        &self.scroll_offsets,
                        &mut self.placed,
                    );
                    self.stats.flatten_visits += self.placed.len();
                    self.reindex_placed();
                    self.apply_sticky();
                }
            }
        }
        if focus_scrolled {
            self.position_popovers(engine, tcx, width, height);
            #[cfg(feature = "profiling")]
            let _span = tracing::trace_span!(target: "plico::layout", "backend.align").entered();
            self.align_controls(tcx, engine);
        }
        self.update_scrollbars(engine);
        let moved_anchor = self.anchor_motion_dirty && !self.geometry_dirty;
        if self.presentation_dirty {
            // Transform/opacity motion changes Scene output without necessarily
            // rewriting Paint. Invalidate the containing cached Region while
            // keeping unrelated Regions reusable.
            self.scene_dirty_nodes
                .extend(self.presentation_roots.iter().copied());
        }
        if self.geometry_dirty || self.stats.flatten_visits > 0 || self.presentation_dirty {
            self.update_presentation();
        }
        self.scene_dirty_nodes
            .extend(self.paint_dirty.iter().copied());
        if moved_anchor {
            self.position_popovers(engine, tcx, width, height);
        }
        // Layout/structure can move selection geometry. Paint-only leaf
        // controls must not rebuild unrelated read-only paragraphs.
        let selection_geometry_changed = self.stats.flatten_visits > 0
            || self.stats.layout_runs > 0
            || self.stats.created > 0
            || self.stats.removed > 0
            || self.stats.text_measure_invalidations > 0;
        if self.selection_work_needed(engine, selection_geometry_changed) {
            self.refresh_selection(engine, tcx);
            self.selection_dirty = false;
        }
        self.align_sliders(engine);
        self.update_shaders(engine, tcx);
        self.stats.changes |= changes
            & (plico::Changes::SEMANTICS
                | plico::Changes::MEASURE
                | plico::Changes::TRANSFORM
                | plico::Changes::HIT_TEST);
        if structural {
            self.stats.changes.insert(plico::Changes::STRUCTURE);
        }
        if self.geometry_dirty {
            self.stats.changes |= plico::Changes::LAYOUT | plico::Changes::PAINT;
        }
        if self.stats.paint_writes > 0 || self.stats.flatten_visits > 0 {
            self.stats.changes.insert(plico::Changes::PAINT);
        }
        self.update_tooltip(engine, tcx, width, height);
        self.stats.layout_micros = start.elapsed().as_micros();
        #[cfg(feature = "profiling")]
        tracing::trace!(target: "plico::perf", placed = self.placed.len(),
            created = self.stats.created, removed = self.stats.removed, sync_visits = self.stats.sync_visits,
            style_writes = self.stats.style_writes, paint_writes = self.stats.paint_writes,
            child_writes = self.stats.child_writes, layout_runs = self.stats.layout_runs,
            flatten_visits = self.stats.flatten_visits, paint_patches = self.stats.paint_patches,
            presentation_visits = self.stats.presentation_visits,
            selection_visits = self.stats.selection_visits, slider_visits = self.stats.slider_visits,
            shader_visits = self.stats.shader_visits, tooltip_visits = self.stats.tooltip_visits, input_rebuilds = self.stats.input_rebuilds,
            input_reuses = self.stats.input_reuses, scroll_visits = self.stats.scroll_visits,
            control_align_checks = self.stats.control_align_checks, input_align_checks = self.stats.input_align_checks,
            control_metric_builds = self.stats.control_metric_builds, control_metric_reuses = self.stats.control_metric_reuses,
            text_measure_invalidations = self.stats.text_measure_invalidations,
            sync_us = self.stats.sync_micros as u64, backend_update_us = self.stats.layout_micros as u64,
            changes = self.stats.changes.bits(), "backend.work");
    }
}
