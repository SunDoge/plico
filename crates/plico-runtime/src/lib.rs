//! Host-agnostic declaration runtime: Engine adapter, Source, View and motion.
//! Window and GPU backends live in plico-desktop / plico-shell; no window deps here.
use peniko::Color;
use plico::{Capabilities, Color as Token, Engine, Entity, Kind, Theme};
use plico_renderer::{
    EventResponse, FrameSource, KeyPhase, PointerButton, PointerPhase, Scene, TextContext, UiEvent,
    layout::{
        PlacedNode, compute_and_walk_with_scroll, compute_and_walk_with_scroll_into,
        compute_layout_with_widgets, flatten_subtree_with_scroll, flatten_with_scroll_into,
    },
    style::{CornerRadii, CursorStyle, Paint},
};
use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
};
use taffy::{geometry::Rect, prelude::*};
use web_time::Instant;

pub use plico_renderer::text::FontAsset;

pub use plico_renderer::{
    HostAction, KeyEvent, RenderBackend, WakeCallback, WindowCommand, WindowLevel, WindowOptions,
    WindowResizeEdges,
};

mod input;
mod layout_style;
mod modal;
mod popover;
mod presentation;
pub mod regions;
#[cfg(feature = "hot-reload")]
pub mod reload;
mod scroll_motion;
mod selection;
mod services;
pub use services::ServiceView;
mod button_state;
mod shader;
mod slider;
mod style_motion;
mod toggle;
mod tooltip;
mod transition;

fn rgb(n: u32) -> Color {
    Color::from_rgb8((n >> 16) as u8, (n >> 8) as u8, n as u8)
}
fn resolve(c: Token, t: Theme) -> Color {
    rgb(t.resolve(c))
}
fn interactive(k: Kind) -> bool {
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
enum SyncMode {
    Node,
    Structure,
    Full,
}
struct ControlMetrics {
    epoch: usize,
    width: f32,
    metrics: Option<[f32; 3]>,
}
#[derive(Clone, Copy, Default)]
struct HitSubtree {
    end: usize,
    first_child: Option<usize>,
    next_sibling: Option<usize>,
    bounds: [f32; 4],
}
#[derive(Default)]
struct HitCache {
    subtrees: Vec<HitSubtree>,
    dirty: bool,
    builds: usize,
    checks: usize,
    pruned: usize,
}

#[derive(Default)]
struct SemanticCache {
    epoch: u64,
    snapshot: Option<(u64, u64, Arc<plico_renderer::SemanticSnapshot>)>,
    nodes: HashMap<Entity, plico_renderer::SemanticNode>,
    builds: usize,
    reuses: usize,
    snapshot_reuses: usize,
}

struct Retained {
    id: NodeId,
    style: taffy::Style,
    paint_key: PaintKey,
    spans: Vec<plico::TextSpan>,
    text: Arc<str>,
    children: Vec<NodeId>,
    layout: Option<taffy::Layout>,
    toggle_line_height: Option<f32>,
    had_grid: bool,
}

/// Everything that feeds one node's paint: the retained copy is compared
/// against the freshly computed key to decide whether to rewrite the paint
/// payload. The progress floats animate every frame, so their comparison is
/// the "same interpolated value" check, not an epsilon test.
#[derive(Clone, PartialEq)]
struct PaintKey {
    style: plico::Style,
    kind: Kind,
    theme: Theme,
    hovered: bool,
    pressed: bool,
    focused: bool,
    checked_progress: f32,
    focus_progress: f32,
}

type SvgResult = Result<Arc<plico_renderer::svg::SvgImage>, Arc<str>>;
type SvgCache = HashMap<Arc<str>, HashMap<Option<u32>, SvgResult>>;
fn svg_color(source: &str, style: plico::Style, theme: Theme) -> Option<u32> {
    source
        .contains("currentColor")
        .then(|| theme.resolve(style.foreground))
}
type ImageCache = HashMap<Arc<str>, Option<Arc<plico_renderer::image::RasterImage>>>;
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
    font_family: Arc<str>,
    font_dirty: bool,
    selection: selection::TextSelection,
    /// Selection state changed outside a frame; the next update must refresh
    /// paragraphs and rects even when the backend otherwise did no work.
    selection_dirty: bool,
    tree: TaffyTree<Paint>,
    nodes: HashMap<Entity, Retained>,
    pub placed: Vec<PlacedNode>,
    pub stats: BackendStats,
    hovered: Option<Entity>,
    pressed: Option<Entity>,
    presentation: HashMap<NodeId, presentation::Presentation>,
    presentation_dirty: bool,
    presentation_roots: HashSet<NodeId>,
    anchor_motion_dirty: bool,
    focused: Option<Entity>,
    requested_focus: Option<Entity>,
    blurred_focus: Option<Entity>,
    requested_scroll: HashMap<Entity, f32>,
    modal_stack: Vec<(Entity, Option<Entity>)>,
    backdrop_pressed: Option<Entity>,
    departing_popup: Option<Entity>,
    popup_open_last: Option<Entity>,
    tips: tooltip::Tooltips,
    slider_drag: Option<Entity>,
    focus_visible: bool,
    scroll_offsets: HashMap<NodeId, [f32; 2]>,
    scroll_dirty: bool,
    scroll_motions: HashMap<NodeId, scroll_motion::ScrollMotion>,
    smooth_scroll_enabled: bool,
    reveal_focus: bool,
    scrollbar_drag: Option<(NodeId, f64, f32)>,
    input_layouts: HashMap<Entity, input::Geometry>,
    control_metrics: HashMap<Entity, ControlMetrics>,
    selectable_nodes: HashSet<Entity>,
    slider_nodes: HashSet<Entity>,
    shader_nodes: HashSet<Entity>,
    tooltip_nodes: HashSet<Entity>,
    shaders: HashMap<Entity, shader::Surface>,
    shader_scale: f64,
    shader_motion_enabled: bool,
    shader_reset_time: bool,
    tab_order: Vec<Entity>,
    motions: style_motion::StyleMotions,
    reverse: HashMap<NodeId, Entity>,
    placed_indices: HashMap<NodeId, usize>,
    scroll_indices: Vec<usize>,
    control_align_nodes: HashSet<Entity>,
    input_align_nodes: HashSet<Entity>,
    control_align_indices: Vec<usize>,
    input_align_indices: Vec<usize>,
    geometry_dirty: bool,
    #[doc(hidden)]
    pub experiment_force_full_geometry: bool,
    #[doc(hidden)]
    pub experiment_force_full_semantics: bool,
    #[doc(hidden)]
    pub experiment_force_full_hit_test: bool,
    geometry_roots: HashSet<Entity>,
    topology_roots: HashSet<Entity>,
    removed_node_ids: Vec<NodeId>,
    pub(crate) scene_dirty_nodes: HashSet<NodeId>,
    pub(crate) scene_full_dirty: bool,
    paint_dirty: Vec<NodeId>,
    last_theme: Option<Theme>,
    interaction_synced: [Option<Entity>; 4],
    viewport: Option<(u32, u32)>,
    dirty_scratch: HashSet<Entity>,
    synced_scratch: HashSet<Entity>,
    svg_cache: SvgCache,
    image_cache: ImageCache,
    semantic: RefCell<SemanticCache>,
    hit: RefCell<HitCache>,
}
impl Default for Adapter {
    fn default() -> Self {
        Self {
            font_family: Arc::from(plico_renderer::text::DEFAULT_UI_FONT_FAMILY),
            font_dirty: false,
            selection: Default::default(),
            selection_dirty: true,
            tree: TaffyTree::new(),
            nodes: HashMap::new(),
            placed: Vec::new(),
            stats: BackendStats::default(),
            hovered: None,
            pressed: None,
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
            focus_visible: false,
            scroll_offsets: HashMap::new(),
            scroll_dirty: false,
            scroll_motions: HashMap::new(),
            smooth_scroll_enabled: true,
            reveal_focus: false,
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
    fn update_retained(
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
                self.shader_nodes.remove(&e);
                self.tooltip_nodes.remove(&e);
                self.reverse.remove(&n.id);
                self.semantic.get_mut().nodes.remove(&e);
                self.removed_node_ids.push(n.id);
                self.scroll_offsets.remove(&n.id);
                self.scroll_motions.remove(&n.id);
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
                .is_some_and(|n| n.style.smooth_scroll && n.style.scroll_y)
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
            self.tab_order = self
                .placed
                .iter()
                .filter_map(|n| self.reverse.get(&n.node_id).copied())
                .filter(|e| {
                    engine.node(*e).is_some_and(|n| {
                        n.kind.capabilities().contains(Capabilities::TAB_STOP) && !n.style.disabled
                    })
                })
                .collect();
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
                }
            }
        }
        if focus_scrolled {
            self.position_popovers(engine, tcx, width, height);
            #[cfg(feature = "profiling")]
            let _span = tracing::trace_span!(target: "plico::layout", "backend.align").entered();
            self.align_controls(tcx, engine);
        }
        for &index in &self.scroll_indices {
            let p = &mut self.placed[index];
            self.stats.scroll_visits += 1;
            p.scroll.opacity = if p.scroll.range[1] > 0. { 1. } else { 0. };
            p.scroll.interaction = if self.scrollbar_drag.is_some_and(|(id, ..)| id == p.node_id) {
                2
            } else {
                0
            };
        }
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
    fn layout_changed(&mut self, entity: Entity) -> bool {
        self.stats.layout_cache_checks += 1;
        let Some(retained) = self.nodes.get(&entity) else {
            return true;
        };
        retained.layout.as_ref() != self.tree.layout(retained.id).ok()
    }

    fn collect_changed_layouts(
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

    fn cache_layout_subtree(&mut self, id: NodeId) {
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

    fn cache_all_layouts(&mut self) {
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
    fn patch_dirty_geometry(&mut self, engine: &Engine) -> Option<bool> {
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
                let Some(parent) = engine.experiment_parent(entity) else {
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
            let mut ancestor = engine.experiment_parent(entity);
            let mut covered = false;
            while let Some(parent) = ancestor {
                if candidates.contains(&parent) {
                    covered = true;
                    break;
                }
                ancestor = engine.experiment_parent(parent);
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
                cursor = engine.experiment_parent(current);
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

    fn reindex_placed(&mut self) {
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
            node.scroll.opacity = if node.scroll.range[1] > 0. { 1. } else { 0. };
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
    fn align_controls(&mut self, tcx: &mut TextContext, engine: &Engine) {
        for slot in 0..self.control_align_indices.len() {
            self.align_control_text(self.control_align_indices[slot], tcx);
        }
        for slot in 0..self.input_align_indices.len() {
            self.align_input(self.input_align_indices[slot], tcx, engine);
        }
    }
    /// Center the shaped text block in leaf controls. Indicators then follow its
    /// first line, so wrapped labels do not drag the marker to the paragraph center.
    fn align_control_text(&mut self, index: usize, tcx: &mut TextContext) {
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
            check.as_deref(),
        ));
        placed.paint.widget = Some(widget.clone());
        self.tree
            .get_node_context_mut(placed.node_id)
            .unwrap()
            .widget = Some(widget);
        retained.toggle_line_height = Some(line_height);
    }
    fn sync(
        &mut self,
        engine: &Engine,
        e: Entity,
        live: &mut HashSet<Entity>,
        width: u32,
        height: u32,
        recursive: SyncMode,
    ) -> NodeId {
        if !live.insert(e) {
            return self.nodes[&e].id;
        }
        self.stats.sync_visits += 1;
        let n = engine.node(e).unwrap();
        self.selection_dirty |= if n.style.selectable {
            self.selectable_nodes.insert(e)
        } else {
            self.selectable_nodes.remove(&e)
        };
        for (set, member) in [
            (
                &mut self.control_align_nodes,
                matches!(n.kind, Kind::Button | Kind::Switch | Kind::Checkbox),
            ),
            (&mut self.input_align_nodes, n.kind == Kind::TextInput),
            (&mut self.slider_nodes, n.kind == Kind::Slider),
            (&mut self.shader_nodes, n.kind == Kind::Shader),
            (
                &mut self.tooltip_nodes,
                n.kind == Kind::TooltipAnchor && !n.text.is_empty(),
            ),
        ] {
            if member {
                set.insert(e);
            } else {
                set.remove(&e);
            }
        }
        let mut target = n.style;
        let styled_button = n.kind == Kind::Button || n.style.styled_button;
        if n.style.styled_button && n.style.disabled {
            // Core attenuates composed buttons for all backends. This backend
            // moves that attenuation into the host-owned interaction weights.
            target.opacity /= 0.45;
        }
        if n.kind == Kind::Slider {
            target.checked = self.pressed == Some(e);
        }
        if styled_button {
            target.background = target.background.or(Some(Token::Control));
            let inferred = match target.background {
                Some(Token::Control) => (Some(Token::ControlHover), Some(Token::ControlPressed)),
                Some(Token::Accent) => (Some(Token::AccentHover), Some(Token::AccentPressed)),
                _ => (None, None),
            };
            target.hover_background = target.hover_background.or(inferred.0);
            target.pressed_background = target.pressed_background.or(inferred.1);
        }
        if interactive(n.kind) && !styled_button && !target.disabled {
            let base = target
                .background
                .or((n.kind == Kind::Button).then_some(Token::Control));
            let inferred = match base {
                Some(Token::Control) => (Some(Token::ControlHover), Some(Token::ControlPressed)),
                Some(Token::Accent) => (Some(Token::AccentHover), Some(Token::AccentPressed)),
                _ => (None, None),
            };
            if self.pressed == Some(e) {
                target.background = target.pressed_background.or(inferred.1).or(base);
            } else if self.hovered == Some(e) {
                target.background = target.hover_background.or(inferred.0).or(base);
            } else {
                target.background = base;
            }
        }
        let visual_style =
            self.motions
                .style(e, target, engine.theme(), !self.nodes.contains_key(&e));
        let (visual_style, focus_progress) = if styled_button {
            self.motions.button_style(
                e,
                visual_style,
                engine.theme(),
                button_state::Interaction {
                    hovered: self.hovered == Some(e),
                    pressed: self.pressed == Some(e),
                    focused: self.focus_visible && self.focused == Some(e),
                    disabled: target.disabled,
                },
            )
        } else {
            self.motions.remove_button(e);
            (visual_style, 0.)
        };
        let checked_progress = self.motions.checked(e, n.style.checked);
        let svg = if n.kind == Kind::Svg {
            cached_svg(
                &mut self.svg_cache,
                &mut self.stats,
                n.text,
                svg_color(n.text, visual_style, engine.theme()),
            )
        } else {
            None
        };
        let image = if n.kind == Kind::Image {
            cached_image(&mut self.image_cache, &mut self.stats, n.text)
        } else {
            None
        };
        let is_control = interactive(n.kind);
        if recursive != SyncMode::Node
            && is_control
            && n.kind != Kind::Overlay
            && !visual_style.disabled
        {
            self.tab_order.push(e);
        }
        let children: Vec<_> = if recursive != SyncMode::Node {
            n.children
                .iter()
                .map(|c| {
                    if recursive != SyncMode::Full && self.nodes.contains_key(c) {
                        self.nodes[c].id
                    } else {
                        self.sync(engine, *c, live, width, height, recursive)
                    }
                })
                .collect()
        } else {
            Vec::new()
        };
        // Reuse the retained layout for paint/text/transform-only updates.
        // Grid templates remain conservative: variable-size grid data lives
        // outside Copy Style, so layout_eq alone cannot validate them.
        let rebuild_layout = self.nodes.get(&e).is_none_or(|previous| {
            n.grid.is_some()
                || previous.had_grid
                || previous.paint_key.kind != n.kind
                || (matches!(n.kind, Kind::Switch | Kind::Checkbox)
                    && previous.paint_key.theme.metrics != engine.theme().metrics)
                || !visual_style.layout_eq(previous.paint_key.style)
                || (n.kind == Kind::Root
                    && previous.style.size
                        != Size {
                            width: length(width as f32),
                            height: length(height as f32),
                        })
        });
        let style = rebuild_layout.then(|| {
            self.stats.layout_style_builds += 1;
            let mut style = taffy::Style {
                display: Display::Flex,
                overflow: taffy::geometry::Point {
                    x: if visual_style.clip || visual_style.scroll_y {
                        taffy::Overflow::Hidden
                    } else {
                        taffy::Overflow::Visible
                    },
                    y: if visual_style.scroll_y {
                        taffy::Overflow::Scroll
                    } else if visual_style.clip {
                        taffy::Overflow::Hidden
                    } else {
                        taffy::Overflow::Visible
                    },
                },
                flex_direction: if matches!(n.kind, Kind::Row | Kind::ButtonBase) {
                    FlexDirection::Row
                } else {
                    FlexDirection::Column
                },
                align_items: visual_style.align.map_or_else(
                    || {
                        if matches!(
                            n.kind,
                            Kind::Root | Kind::Column | Kind::Dialog | Kind::TooltipAnchor
                        ) {
                            AlignItems::STRETCH
                        } else {
                            AlignItems::CENTER
                        }
                    },
                    |align| match align {
                        plico::Align::Start => AlignItems::START,
                        plico::Align::Center => AlignItems::CENTER,
                        plico::Align::End => AlignItems::END,
                        plico::Align::Stretch => AlignItems::STRETCH,
                        plico::Align::Baseline => AlignItems::BASELINE,
                    },
                ),
                justify_content: visual_style
                    .justify
                    .map_or(JustifyContent::NORMAL, |justify| match justify {
                        plico::Justify::Start => JustifyContent::START,
                        plico::Justify::Center => JustifyContent::CENTER,
                        plico::Justify::End => JustifyContent::END,
                        plico::Justify::SpaceBetween => JustifyContent::SPACE_BETWEEN,
                        plico::Justify::SpaceAround => JustifyContent::SPACE_AROUND,
                        plico::Justify::SpaceEvenly => JustifyContent::SPACE_EVENLY,
                        plico::Justify::Stretch => JustifyContent::STRETCH,
                    }),
                flex_wrap: if visual_style.wrap {
                    FlexWrap::Wrap
                } else {
                    FlexWrap::NoWrap
                },
                flex_grow: visual_style.grow,
                min_size: Size {
                    width: visual_style.min_width.map_or(length(0.), length),
                    height: visual_style.min_height.map_or(auto(), length),
                },
                max_size: Size {
                    width: visual_style.max_width.map_or(auto(), length),
                    height: visual_style.max_height.map_or(auto(), length),
                },
                gap: Size {
                    width: length(visual_style.gap),
                    height: length(visual_style.gap),
                },
                border: Rect {
                    left: length(visual_style.border_width),
                    right: length(visual_style.border_width),
                    top: length(visual_style.border_width),
                    bottom: length(visual_style.border_width),
                },
                padding: Rect {
                    left: length(visual_style.horizontal_padding()),
                    right: length(visual_style.horizontal_padding()),
                    top: length(visual_style.vertical_padding()),
                    bottom: length(visual_style.vertical_padding()),
                },
                size: Size {
                    width: visual_style.width_percent.map_or_else(
                        || visual_style.width.map_or(auto(), length),
                        |v| percent(v / 100.),
                    ),
                    height: visual_style.height_percent.map_or_else(
                        || visual_style.height.map_or(auto(), length),
                        |v| percent(v / 100.),
                    ),
                },
                flex_shrink: if is_control { 0. } else { visual_style.shrink },
                ..Default::default()
            };
            if let Some([left, top]) = visual_style.absolute {
                style.position = Position::Absolute;
                style.inset.left = length(left);
                style.inset.top = length(top);
            }
            if visual_style.overlay {
                style.position = Position::Absolute;
                style.inset = Rect {
                    left: length(0.),
                    right: length(0.),
                    top: length(0.),
                    bottom: length(0.),
                };
                style.size = Size {
                    width: percent(1.),
                    height: percent(1.),
                };
            }
            if n.kind == Kind::Root {
                style.size = Size {
                    width: length(width as f32),
                    height: length(height as f32),
                };
            }
            if is_control && !matches!(n.kind, Kind::ButtonBase | Kind::Overlay) {
                style.min_size.height = visual_style.min_height.map_or(length(36.), length);
                style.padding = Rect {
                    left: length(visual_style.horizontal_padding()),
                    right: length(visual_style.horizontal_padding()),
                    top: length(visual_style.vertical_padding()),
                    bottom: length(visual_style.vertical_padding()),
                };
            }
            layout_style::apply(&mut style, visual_style, n.grid);
            if matches!(n.kind, Kind::Switch | Kind::Checkbox) {
                style.padding.left =
                    length(toggle::left_padding(n.kind, visual_style, engine.theme()));
            }
            style
        });
        let key = PaintKey {
            style: visual_style,
            kind: n.kind,
            theme: engine.theme(),
            hovered: self.hovered == Some(e),
            pressed: self.pressed == Some(e),
            focused: (self.focus_visible || n.kind == Kind::TextInput) && self.focused == Some(e),
            checked_progress,
            focus_progress,
        };
        let visual_style = key.style;
        let font_family = resolve_font_family(visual_style.font_family, &self.font_family);
        if !self.nodes.contains_key(&e) {
            let id = self
                .tree
                .new_leaf_with_context(
                    style.as_ref().expect("new node layout").clone(),
                    Paint::default(),
                )
                .unwrap();
            self.nodes.insert(
                e,
                Retained {
                    id,
                    style: style.as_ref().expect("new node layout").clone(),
                    paint_key: key.clone(),
                    spans: n.spans.to_vec(),
                    text: Arc::from(n.text),
                    children: Vec::new(),
                    layout: None,
                    toggle_line_height: None,
                    had_grid: n.grid.is_some(),
                },
            );
            // Force the initial paint write, even for an empty label.
            self.reverse.insert(id, e);
            self.geometry_dirty = true;
            self.stats.created += 1;
            let mut paint = make_paint(
                n.kind,
                visual_style,
                self.nodes[&e].text.clone(),
                engine.theme(),
                (key.hovered, key.pressed, key.focused, key.checked_progress),
                font_family.clone(),
            );
            apply_button_focus(
                &mut paint,
                styled_button,
                key.focus_progress,
                engine.theme(),
            );
            paint.text_runs = rich_runs(n.spans, visual_style, engine.theme());
            paint.svg = svg.clone();
            paint.image = image.clone();
            self.tree.set_node_context(id, Some(paint)).unwrap();
            self.stats.paint_writes += 1;
        }
        let retained = self.nodes.get_mut(&e).unwrap();
        retained.had_grid = n.grid.is_some();
        if let Some(style) = style
            && retained.style != style
        {
            self.tree.set_style(retained.id, style.clone()).unwrap();
            retained.style = style;
            self.stats.style_writes += 1;
            self.geometry_dirty = true;
            self.geometry_roots.insert(e);
        }
        let visual_changes = visual_style.changes_from(retained.paint_key.style);
        self.stats.changes |= visual_changes;
        if visual_changes.contains(plico::Changes::TRANSFORM) {
            fn has_anchor(engine: &Engine, entity: Entity) -> bool {
                engine.node(entity).is_some_and(|n| {
                    n.style.anchor_id.is_some() || n.children.iter().any(|e| has_anchor(engine, *e))
                })
            }
            self.anchor_motion_dirty |= has_anchor(engine, e);
        }
        if visual_changes.contains(plico::Changes::TRANSFORM)
            || visual_style.opacity != retained.paint_key.style.opacity
        {
            self.presentation_dirty = true;
            self.presentation_roots.insert(retained.id);
        }
        let paint_needed = (matches!(
            n.kind,
            Kind::Switch | Kind::Checkbox | Kind::Slider | Kind::TextInput
        ) && visual_changes.contains(plico::Changes::LAYOUT))
            || visual_changes.intersects(plico::Changes::PAINT | plico::Changes::MEASURE)
            || retained.paint_key.kind != key.kind
            || retained.paint_key.theme != key.theme
            || retained.paint_key.focus_progress != key.focus_progress
            || (
                retained.paint_key.hovered,
                retained.paint_key.pressed,
                retained.paint_key.focused,
                retained.paint_key.checked_progress,
            ) != (key.hovered, key.pressed, key.focused, key.checked_progress)
            || retained.text.as_ref() != n.text
            || retained.spans != n.spans
            || self
                .tree
                .get_node_context(retained.id)
                .unwrap()
                .font_family
                .as_deref()
                != Some(font_family.as_ref());
        if paint_needed {
            // This is the single retained paint mutation path: invalidate all
            // text-affecting values conservatively, including rich runs/theme.
            self.control_metrics.remove(&e);
            if retained.text.as_ref() != n.text {
                retained.text = Arc::from(n.text);
            }
            let mut paint = make_paint(
                n.kind,
                key.style,
                retained.text.clone(),
                engine.theme(),
                (key.hovered, key.pressed, key.focused, key.checked_progress),
                font_family.clone(),
            );
            apply_button_focus(
                &mut paint,
                styled_button,
                key.focus_progress,
                engine.theme(),
            );
            paint.text_runs = rich_runs(n.spans, visual_style, engine.theme());
            retained.spans = n.spans.to_vec();
            paint.svg = svg;
            paint.image = image;
            // Paint-only changes must not invalidate Taffy's text measurement cache.
            let old = self.tree.get_node_context(retained.id).unwrap();
            let measurement_changed = old.font_family != paint.font_family
                || old.text != paint.text
                || old.font_size != paint.font_size
                || old.font_weight != paint.font_weight
                || old.text_runs.len() != paint.text_runs.len()
                || old
                    .text_runs
                    .iter()
                    .zip(paint.text_runs.iter())
                    .any(|(old, new)| {
                        let mut metrics = old.clone();
                        metrics.color = new.color;
                        metrics != *new
                    });
            *self.tree.get_node_context_mut(retained.id).unwrap() = paint;
            self.paint_dirty.push(retained.id);
            if measurement_changed {
                self.stats.text_measure_invalidations += 1;
                self.geometry_dirty = true;
                self.geometry_roots.insert(e);
                self.tree.mark_dirty(retained.id).unwrap();
            }
            self.stats.paint_writes += 1;
        }
        retained.paint_key = key;
        if recursive != SyncMode::Node && retained.children != children {
            self.tree.set_children(retained.id, &children).unwrap();
            retained.children = children;
            self.stats.child_writes += 1;
            self.geometry_dirty = true;
            self.geometry_roots.insert(e);
            self.topology_roots.insert(e);
        }
        retained.id
    }
    fn hit(&self, engine: &Engine, x: f64, y: f64) -> Option<Entity> {
        if self.experiment_force_full_hit_test {
            return self.placed.iter().rev().find_map(|placed| {
                if !self.visual_contains(placed.node_id, x, y) {
                    return None;
                }
                let entity = *self.reverse.get(&placed.node_id)?;
                engine
                    .node(entity)
                    .filter(|node| {
                        interactive(node.kind)
                            && node.kind != Kind::Overlay
                            && !node.style.disabled
                            && self.modal_allows(engine, entity)
                    })
                    .map(|_| entity)
            });
        }
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
    fn rebuild_hit_cache(&self) {
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
    pub fn experiment_hit_cache_stats(&self) -> (usize, usize, usize) {
        let cache = self.hit.borrow();
        (cache.builds, cache.checks, cache.pruned)
    }
    pub fn event(&mut self, engine: &mut Engine, event: UiEvent) -> EventResponse {
        self.event_with_invalidation(engine, event).0
    }
    /// A repaint does not always require evaluating application declarations.
    /// Keep this distinction internal; the public event response still tells
    /// every host when it needs to present another frame.
    fn event_with_invalidation(
        &mut self,
        engine: &mut Engine,
        event: UiEvent,
    ) -> (EventResponse, bool) {
        self.semantic.get_mut().epoch = self.semantic.get_mut().epoch.wrapping_add(1);
        if self.tooltip_event(engine, &event) {
            return (
                EventResponse {
                    handled: true,
                    request_redraw: true,
                    consume_key_text: true,
                    ..EventResponse::IGNORED
                },
                true,
            );
        }
        let mut declare = true;
        let mut response = self.event_inner(engine, event, &mut declare);
        engine.set_hovered(self.hovered);
        response.request_redraw |= self.tips.needs_frame();
        (response, declare)
    }
    fn event_inner(
        &mut self,
        engine: &mut Engine,
        mut event: UiEvent,
        declare: &mut bool,
    ) -> EventResponse {
        if let Some(response) = self.slider_event(engine, &event) {
            return response;
        }
        if let Some(response) = self.modal_event(engine, &event) {
            return response;
        }
        if let Some(response) = self.input_event(engine, &mut event) {
            return response;
        }
        if let Some(response) = self.selection_event(engine, &event) {
            return response;
        }
        let old_focus = self.focused;
        let mut handled = false;
        match event {
            UiEvent::Pointer(p) => {
                use plico_renderer::scrollbar::{self, ScrollAxis, ScrollbarPart};
                let scrollbar_hit = if p.phase == PointerPhase::Down
                    && p.button == Some(PointerButton::Primary)
                {
                    self.placed.iter().rev().find_map(|node| {
                        if !self.modal_allows(engine, self.reverse[&node.node_id]) {
                            return None;
                        }
                        if !self.visual_contains(node.node_id, p.position.x, p.position.y) {
                            return None;
                        }
                        let point = self.layout_point(node.node_id, p.position.x, p.position.y)?;
                        scrollbar::hit(node, point)
                            .filter(|hit| hit.axis == ScrollAxis::Vertical)
                            .map(|hit| (node.node_id, hit.part))
                    })
                } else {
                    None
                };
                if let Some((id, part)) = scrollbar_hit {
                    let node = &self.placed[self.placed_indices[&id]];
                    if part == ScrollbarPart::Thumb {
                        self.scroll_motions.remove(&id);
                        self.scrollbar_drag = self
                            .layout_point(id, p.position.x, p.position.y)
                            .map(|point| (id, point.y, node.scroll.offset[1]));
                    } else {
                        let direction = if part == ScrollbarPart::TrackBefore {
                            -1.
                        } else {
                            1.
                        };
                        let next = (self
                            .scroll_motions
                            .get(&id)
                            .map_or(node.scroll.offset[1], |m| m.target)
                            + direction * (node.scroll.port[3] - node.scroll.port[1]) * 0.9)
                            .clamp(0., node.scroll.range[1]);
                        let smooth = engine
                            .node(self.reverse[&id])
                            .is_some_and(|n| n.style.smooth_scroll);
                        self.scroll_to(id, next, smooth, engine.time().now);
                    }
                    self.pressed = None;
                    self.hovered = None;
                    self.scroll_dirty = true;
                    return EventResponse {
                        handled: true,
                        request_redraw: true,
                        ..EventResponse::IGNORED
                    };
                }
                if let Some((id, start, offset)) = self.scrollbar_drag {
                    if p.phase == PointerPhase::Move
                        && let Some(&index) = self.placed_indices.get(&id)
                    {
                        let node = &self.placed[index];
                        let local_y = self
                            .layout_point(id, p.position.x, p.position.y)
                            .map_or(start, |point| point.y);
                        let delta =
                            (local_y - start) * scrollbar::drag_ratio(node, ScrollAxis::Vertical);
                        let next = (offset + delta as f32).clamp(0., node.scroll.range[1]);
                        self.scroll_offsets.insert(id, [0., next]);
                        self.scroll_dirty = true;
                        return EventResponse {
                            handled: true,
                            request_redraw: true,
                            ..EventResponse::IGNORED
                        };
                    }
                    if matches!(p.phase, PointerPhase::Up | PointerPhase::Cancel) {
                        self.scrollbar_drag = None;
                        self.scroll_dirty = true;
                        return EventResponse {
                            handled: true,
                            request_redraw: true,
                            ..EventResponse::IGNORED
                        };
                    }
                }
                let hit = self.hit(engine, p.position.x, p.position.y);
                match p.phase {
                    PointerPhase::Down if p.button == Some(PointerButton::Primary) => {
                        self.hovered = hit;
                        self.pressed = hit;
                        self.focused = hit;
                        self.focus_visible = false;
                        self.input_pointer(engine, hit, p.position, p.modifiers.shift());
                        handled = true;
                    }
                    PointerPhase::Up if p.button == Some(PointerButton::Primary) => {
                        self.hovered = hit;
                        if let Some(e) = self.pressed.take().filter(|e| Some(*e) == hit)
                            && engine.node(e).is_some_and(|n| {
                                n.kind.capabilities().contains(Capabilities::ACTIVATE)
                            })
                        {
                            engine.queue_click(e);
                        }
                        handled = true;
                    }
                    PointerPhase::Cancel | PointerPhase::Leave => {
                        self.pressed = None;
                        self.hovered = None;
                        handled = true;
                    }
                    PointerPhase::Move | PointerPhase::Enter if self.hovered != hit => {
                        self.hovered = hit;
                        if hit.and_then(|e| engine.node(e)).is_some_and(|n| {
                            matches!(
                                n.style.role,
                                Some(plico::WidgetRole::Option | plico::WidgetRole::MenuItem)
                            )
                        }) {
                            self.focused = hit;
                            self.focus_visible = false;
                        }
                        handled = true;
                    }
                    _ => {}
                }
            }
            UiEvent::Wheel(w) if w.delta_y.is_finite() => {
                // Plain wheel scrolling changes host geometry. Observers only
                // need declarations when the actual offset changes, not when
                // a smooth-scroll destination is retargeted.
                *declare = false;
                let start = self
                    .placed
                    .iter()
                    .rev()
                    .find(|p| {
                        p.scroll.scrollable[1]
                            && self.visual_contains(p.node_id, w.position.x, w.position.y)
                            && self.modal_allows(engine, self.reverse[&p.node_id])
                    })
                    .map(|p| p.node_id);
                let mut remaining = w.delta_y.clamp(-1.0e6, 1.0e6) as f32;
                if let Some(start) = start {
                    // Scrolling mutates offsets and motions; collect the chain
                    // so the lazy ancestor iterator is not held across it.
                    let chain: Vec<NodeId> = self.placed_chain(start).map(|p| p.node_id).collect();
                    for id in chain {
                        let Some(scroll) = self
                            .placed_indices
                            .get(&id)
                            .map(|&index| &self.placed[index])
                            .filter(|p| p.scroll.scrollable[1])
                            .map(|p| p.scroll)
                        else {
                            if remaining.abs() < 0.01 {
                                break;
                            }
                            continue;
                        };
                        let old = self
                            .scroll_offsets
                            .get(&id)
                            .copied()
                            .unwrap_or(scroll.offset);
                        let destination = self.scroll_motions.get(&id).map_or(old[1], |m| m.target);
                        let next = (destination + remaining).clamp(0., scroll.range[1]);
                        remaining -= next - destination;
                        if next != destination {
                            let entity = self.reverse[&id];
                            let smooth = engine.node(entity).is_some_and(|n| n.style.smooth_scroll);
                            *declare |= engine.observes_scroll(entity)
                                && !(smooth && self.smooth_scroll_enabled);
                            self.scroll_to(id, next, smooth, engine.time().now);
                            handled = true;
                        }
                        if remaining.abs() < 0.01 {
                            break;
                        }
                    }
                }
            }
            UiEvent::Focus(false) => {
                self.blurred_focus = self.focused;
                self.scrollbar_drag = None;
                self.scroll_motions.clear();
                self.scroll_dirty = true;
                self.focused = None;
                self.pressed = None;
                handled = true;
            }
            UiEvent::Focus(true) => {
                if self.focused.is_none() {
                    self.focused = self.blurred_focus.take().filter(|e| {
                        self.modal_allows(engine, *e)
                            && engine.node(*e).is_some_and(|n| {
                                n.kind.capabilities().contains(Capabilities::FOCUS)
                                    && !n.style.disabled
                            })
                    });
                    self.reveal_focus = true;
                    handled = self.focused.is_some();
                }
            }
            UiEvent::Key(k) if k.phase == KeyPhase::Down => {
                let tab_order: Vec<_> = self
                    .tab_order
                    .iter()
                    .copied()
                    .filter(|e| self.modal_allows(engine, *e))
                    .collect();
                if k.key == "Tab" && !tab_order.is_empty() {
                    let len = tab_order.len();
                    let at = self
                        .focused
                        .and_then(|e| tab_order.iter().position(|n| *n == e));
                    let next = if k.modifiers.shift() {
                        at.map_or(len - 1, |i| (i + len - 1) % len)
                    } else {
                        at.map_or(0, |i| (i + 1) % len)
                    };
                    self.focused = Some(tab_order[next]);
                    self.focus_visible = true;
                    self.reveal_focus = true;
                    handled = true;
                } else if matches!(k.key.as_str(), "ArrowDown" | "ArrowUp")
                    && let Some(e) = self.focused
                    && engine.node(e).is_some_and(|n| {
                        !n.style.disabled
                            && n.style.anchor_id.is_some()
                            && n.style.expanded == Some(false)
                    })
                {
                    self.popup_open_last = (k.key == "ArrowUp").then_some(e);
                    engine.queue_click(e);
                    handled = true;
                } else if matches!(k.key.as_str(), "Enter" | " " | "Space")
                    && !k.repeat
                    && let Some(e) = self.focused
                    && engine.node(e).is_some_and(|n| {
                        !n.style.disabled && n.kind.capabilities().contains(Capabilities::ACTIVATE)
                    })
                {
                    self.focus_visible = true;
                    engine.queue_click(e);
                    handled = true;
                }
            }
            _ => {}
        }
        if old_focus != self.focused
            && let Some((buffer, _)) = old_focus.and_then(|e| engine.input(e))
        {
            buffer.edit(plico::InputEdit::ClearPreedit);
        }
        EventResponse {
            handled,
            request_redraw: handled,
            text_input: Some(self.focused.is_some_and(|e| engine.input(e).is_some())),
            ..EventResponse::IGNORED
        }
    }
    /// Invalid SVG sources remain cached while referenced, avoiding parse retries every frame.
    pub fn svg_diagnostics(&self) -> impl Iterator<Item = &str> {
        self.svg_cache
            .values()
            .flat_map(|variants| variants.values())
            .filter_map(|v| v.as_ref().err().map(|e| e.as_ref()))
    }
    pub fn semantics(&self, engine: &Engine, revision: u64) -> plico_renderer::SemanticSnapshot {
        (*self.semantics_shared(engine, revision)).clone()
    }
    /// Share an immutable semantic projection until the next backend commit or input event.
    pub fn semantics_shared(
        &self,
        engine: &Engine,
        revision: u64,
    ) -> Arc<plico_renderer::SemanticSnapshot> {
        let epoch = self.semantic.borrow().epoch;
        if self.experiment_force_full_semantics {
            self.semantic.borrow_mut().nodes.clear();
            self.semantic.borrow_mut().builds = 0;
            self.semantic.borrow_mut().reuses = 0;
            return Arc::new(self.build_semantics(engine, revision));
        }
        let cached = self.semantic.borrow().snapshot.clone();
        if let Some((cached_epoch, cached_revision, snapshot)) = cached
            && cached_epoch == epoch
            && cached_revision == revision
        {
            self.semantic.borrow_mut().snapshot_reuses += 1;
            return snapshot;
        }
        self.semantic.borrow_mut().builds = 0;
        self.semantic.borrow_mut().reuses = 0;
        let snapshot = Arc::new(self.build_semantics(engine, revision));
        self.semantic.borrow_mut().snapshot = Some((epoch, revision, snapshot.clone()));
        snapshot
    }
    #[doc(hidden)]
    pub fn experiment_semantic_cache_stats(&self) -> (usize, usize, usize) {
        let cache = self.semantic.borrow();
        (cache.builds, cache.reuses, cache.snapshot_reuses)
    }
    fn build_semantics(&self, engine: &Engine, revision: u64) -> plico_renderer::SemanticSnapshot {
        use plico_renderer::{
            SemanticNode, SemanticRole as Role, SemanticStates, SemanticToggleState,
        };
        let mut nodes: Vec<SemanticNode> = self
            .placed
            .iter()
            .filter_map(|p| {
                let e = *self.reverse.get(&p.node_id)?;
                let n = engine.node(e)?;
                let cacheable =
                    n.kind != Kind::ButtonBase && n.style.role != Some(plico::WidgetRole::ComboBox);
                let cached = cacheable
                    .then(|| self.semantic.borrow().nodes.get(&e).cloned())
                    .flatten();
                let mut semantic = if let Some(cached) = cached {
                    self.semantic.borrow_mut().reuses += 1;
                    cached
                } else {
                    self.semantic.borrow_mut().builds += 1;
                    SemanticNode {
                        id: semantic_id(e),
                        role: match n.style.role {
                            Some(plico::WidgetRole::ComboBox) => Role::ComboBox,
                            Some(plico::WidgetRole::ListBox) => Role::ListBox,
                            Some(plico::WidgetRole::Option) => Role::Option,
                            Some(plico::WidgetRole::Menu) => Role::Menu,
                            Some(plico::WidgetRole::MenuButton) => Role::Button,
                            Some(plico::WidgetRole::MenuItem) => Role::MenuItem,
                            None => match n.kind {
                                Kind::Button | Kind::ButtonBase => Role::Button,
                                Kind::Slider => Role::Slider,
                                Kind::Dialog => Role::Dialog,
                                Kind::TextInput => Role::TextInput,
                                Kind::Switch => Role::Switch,
                                Kind::Checkbox => Role::CheckBox,
                                Kind::Text => Role::Label,
                                _ => Role::Group,
                            },
                        },
                        label: engine.accessible_name(e).map(|name| name.into_owned()),
                        value: if n.style.role == Some(plico::WidgetRole::ComboBox) {
                            n.children
                                .iter()
                                .filter_map(|e| engine.node(*e))
                                .find(|n| n.kind == Kind::Text)
                                .map(|n| n.text.to_owned())
                        } else {
                            n.numeric.map(|range| range.value.to_string())
                        },
                        numeric_value: n.numeric.map(|range| range.value),
                        min_numeric_value: n.numeric.map(|range| range.min),
                        max_numeric_value: n.numeric.map(|range| range.max),
                        bounds: p.rect,
                        children: n.children.iter().map(|e| semantic_id(*e)).collect(),
                        controls: Vec::new(),
                        described_by: Vec::new(),
                        active_descendant: None,
                        disabled: n.style.disabled,
                        states: SemanticStates {
                            selected: n.style.selected,
                            expanded: n.style.expanded,
                            checked: matches!(n.kind, Kind::Switch | Kind::Checkbox).then_some(
                                if n.style.checked {
                                    SemanticToggleState::On
                                } else {
                                    SemanticToggleState::Off
                                },
                            ),
                            ..Default::default()
                        },
                    }
                };
                if cacheable {
                    self.semantic.borrow_mut().nodes.insert(e, semantic.clone());
                }
                semantic.bounds = self.visual_bounds(p.node_id).unwrap_or(p.rect);
                semantic.described_by = self.tooltip_description(engine, e).into_iter().collect();
                semantic.disabled |=
                    interactive(n.kind) && n.kind != Kind::Overlay && !self.modal_allows(engine, e);
                semantic.states.modal = (n.kind == Kind::Dialog && n.style.role.is_none())
                    .then_some(!self.is_popup_panel(engine, e));
                Some(semantic)
            })
            .collect();
        // Portals remain independent in layout. In the accessibility tree, an
        // anchored popup belongs to the dialog containing its trigger, otherwise
        // modal projection would hide the newly opened panel from assistive tech.
        for (root, _) in &self.modal_stack {
            let Some(token) = engine.node(*root).and_then(|n| n.style.popup_anchor) else {
                continue;
            };
            let Some(anchor) = self
                .placed
                .iter()
                .filter_map(|p| self.reverse.get(&p.node_id).copied())
                .find(|e| {
                    engine
                        .node(*e)
                        .is_some_and(|n| n.style.anchor_id == Some(token))
                })
            else {
                continue;
            };
            let panel = engine
                .node(*root)
                .unwrap()
                .children
                .iter()
                .copied()
                .find(|e| engine.node(*e).is_some_and(|n| n.kind == Kind::Dialog));
            if let Some(trigger) = nodes.iter_mut().find(|n| n.id == semantic_id(anchor)) {
                trigger.controls = panel.map(semantic_id).into_iter().collect();
            }
            let enclosing = self
                .placed
                .iter()
                .rev()
                .filter_map(|p| self.reverse.get(&p.node_id).copied())
                .find(|e| {
                    engine.node(*e).is_some_and(|n| n.kind == Kind::Dialog)
                        && modal::contains(engine, *e, anchor)
                });
            if let Some(enclosing) = enclosing {
                let root_id = semantic_id(*root);
                if let Some(parent) = nodes
                    .iter_mut()
                    .find(|n| n.id == semantic_id(engine.root()))
                {
                    parent.children.retain(|id| *id != root_id);
                }
                if let Some(parent) = nodes.iter_mut().find(|n| n.id == semantic_id(enclosing)) {
                    parent.children.push(root_id);
                }
            }
        }
        self.tooltip_semantics(engine, &mut nodes);
        plico_renderer::SemanticSnapshot {
            revision,
            nodes,
            root_children: vec![semantic_id(engine.root())],
            focus: self.focused.map(semantic_id),
            modal_root: self.accessible_modal(engine).map(semantic_id),
        }
    }
    pub fn semantic_action(
        &mut self,
        engine: &mut Engine,
        action: plico_renderer::SemanticAction,
    ) -> bool {
        use plico_renderer::SemanticAction;
        self.semantic.get_mut().epoch = self.semantic.get_mut().epoch.wrapping_add(1);
        let target = match action {
            SemanticAction::Click { target }
            | SemanticAction::Focus { target }
            | SemanticAction::Blur { target }
            | SemanticAction::SetNumericValue { target, .. }
            | SemanticAction::Increment { target }
            | SemanticAction::Decrement { target } => target,
            _ => return false,
        };
        let Some(e) = self
            .tab_order
            .iter()
            .copied()
            .find(|e| semantic_id(*e) == target)
            .or_else(|| {
                self.modal_dialog(engine)
                    .filter(|e| semantic_id(*e) == target)
            })
        else {
            return false;
        };
        if !self.modal_allows(engine, e) || !engine.node(e).is_some_and(|n| !n.style.disabled) {
            return false;
        }
        let caps = engine.node(e).unwrap().kind.capabilities();
        let allowed = match action {
            SemanticAction::Focus { .. } | SemanticAction::Blur { .. } => {
                caps.contains(Capabilities::FOCUS)
            }
            SemanticAction::Click { .. } => {
                caps.intersects(Capabilities::ACTIVATE | Capabilities::FOCUS)
            }
            SemanticAction::SetNumericValue { .. }
            | SemanticAction::Increment { .. }
            | SemanticAction::Decrement { .. } => caps.contains(Capabilities::VALUE_EDIT),
            _ => false,
        };
        if !allowed {
            return false;
        }
        let action = if matches!(action, SemanticAction::Click { .. })
            && engine.node(e).is_some_and(|n| {
                n.kind.capabilities().contains(Capabilities::FOCUS)
                    && !n.kind.capabilities().contains(Capabilities::ACTIVATE)
            }) {
            SemanticAction::Focus { target }
        } else {
            action
        };
        let old_focus = self.focused;
        if let Some(range) = engine.node(e).and_then(|n| n.numeric) {
            let current = engine.pending_value(e).unwrap_or(range.value);
            let value = match action {
                SemanticAction::SetNumericValue { value, .. } => Some(value),
                SemanticAction::Increment { .. } => Some(current + range.increment()),
                SemanticAction::Decrement { .. } => Some(current - range.increment()),
                _ => None,
            };
            if let Some(value) = value {
                if !value.is_finite() {
                    return false;
                }
                engine.queue_value(e, range.normalize(value));
                return true;
            }
        } else if matches!(
            action,
            SemanticAction::SetNumericValue { .. }
                | SemanticAction::Increment { .. }
                | SemanticAction::Decrement { .. }
        ) {
            return false;
        }
        match action {
            SemanticAction::Click { .. } => {
                if !engine
                    .node(e)
                    .is_some_and(|n| n.kind.capabilities().contains(Capabilities::ACTIVATE))
                {
                    return false;
                }
                engine.queue_click(e);
            }
            SemanticAction::Focus { .. } => {
                self.focused = Some(e);
                self.focus_visible = true;
                self.reveal_focus = true;
            }
            SemanticAction::Blur { .. } if self.focused == Some(e) => {
                self.focused = None;
            }
            _ => {}
        }
        if old_focus != self.focused
            && let Some((buffer, _)) = old_focus.and_then(|e| engine.input(e))
        {
            buffer.edit(plico::InputEdit::ClearPreedit);
        }
        true
    }
    pub fn cursor(&self) -> CursorStyle {
        if self.selection.hovered {
            return CursorStyle::Text;
        }
        if self
            .hovered
            .is_some_and(|e| self.input_layouts.contains_key(&e))
        {
            CursorStyle::Text
        } else if self.hovered.is_some() {
            CursorStyle::Pointer
        } else {
            CursorStyle::Default
        }
    }
}
fn semantic_id(e: Entity) -> u64 {
    e.raw()
}
fn resolve_font_family(family: plico::FontFamily, default: &Arc<str>) -> Arc<str> {
    use plico::FontFamily;
    use std::sync::LazyLock;
    static MONO: LazyLock<Arc<str>> = LazyLock::new(|| Arc::from("monospace"));
    static SANS: LazyLock<Arc<str>> = LazyLock::new(|| Arc::from("sans-serif"));
    static SERIF: LazyLock<Arc<str>> = LazyLock::new(|| Arc::from("serif"));
    match family {
        FontFamily::Default => default.clone(),
        FontFamily::Monospace => MONO.clone(),
        FontFamily::SansSerif => SANS.clone(),
        FontFamily::Serif => SERIF.clone(),
    }
}

fn rich_runs(
    spans: &[plico::TextSpan],
    style: plico::Style,
    theme: Theme,
) -> Arc<[plico_renderer::text::TextRun]> {
    spans
        .iter()
        .map(|span| plico_renderer::text::TextRun {
            range: span.range.clone(),
            font_size: style.font_size as f32,
            font_weight: span.weight as f32,
            font_italic: span.italic,
            line_height: None,
            color: plico_renderer::text::brush_for_color(resolve(
                span.color.unwrap_or(style.foreground),
                theme,
            )),
        })
        .collect()
}
fn apply_button_focus(paint: &mut Paint, styled: bool, progress: f32, theme: Theme) {
    if styled {
        paint.outline_width = if progress > 0. { 1.5 } else { 0. };
        paint.outline_offset = 1.;
        paint.outline_color =
            (progress > 0.).then(|| resolve(Token::Focus, theme).with_alpha(progress));
    }
}

fn make_paint(
    kind: Kind,
    style: plico::Style,
    text: Arc<str>,
    theme: Theme,
    interaction: (bool, bool, bool, f32),
    font_family: Arc<str>,
) -> Paint {
    let (_, _, focus, _) = interaction;
    let mut p = Paint {
        text: (!text.is_empty()
            && !matches!(
                kind,
                Kind::Svg
                    | Kind::Shader
                    | Kind::Image
                    | Kind::Slider
                    | Kind::ButtonBase
                    | Kind::TextInput
                    | Kind::Dialog
                    | Kind::Overlay
                    | Kind::TooltipAnchor
            ))
        .then_some(text),
        font_size: style.font_size as f32,
        font_weight: style.font_weight as f32,
        font_family: Some(font_family),
        text_color: resolve(style.foreground, theme),
        backdrop_blur: style.backdrop_blur,
        background: style
            .background
            .map(|c| resolve(c, theme).with_alpha(style.background_alpha)),
        opacity: style.opacity,
        runtime_transform: Some([
            style.scale[0],
            0.,
            0.,
            style.scale[1],
            style.translate[0],
            style.translate[1],
        ]),
        border_radii: CornerRadii::uniform(style.radius),
        border: (style.border_width > 0.)
            .then(|| (style.border_width, resolve(style.border_color, theme))),
        wrap_text: !style.text_ellipsis,
        text_ellipsis: style.text_ellipsis,
        ..Default::default()
    };
    if style.scroll_y {
        let muted = theme.muted;
        p.scrollbar = Some(Arc::new(plico_renderer::style::ScrollbarStyle {
            thickness: 6.,
            margin: 2.,
            thumb_color: Color::from_rgba8(
                (muted >> 16) as u8,
                (muted >> 8) as u8,
                muted as u8,
                150,
            ),
            hover_color: resolve(Token::Muted, theme),
            active_color: resolve(Token::Muted, theme),
            ..Default::default()
        }));
    }
    if interactive(kind) {
        if style.disabled && !matches!(kind, Kind::ButtonBase | Kind::Button) {
            p.opacity *= 0.45;
        }
        if kind == Kind::Button && p.background.is_none() {
            p.background = Some(resolve(Token::Control, theme));
        }
        if focus {
            p.outline_width = 1.5;
            p.outline_offset = 1.;
            p.outline_color = Some(resolve(Token::Focus, theme));
        }
    }

    p
}

/// A statically linked application using the same App implementation as hot reload.
pub struct NativeApp<A>(pub A);

#[doc(hidden)]
pub use plico as __ui_core;
#[doc(hidden)]
pub use plico_dev as __app_protocol;

/// Shared forwarding for both the wrapper and application's direct View implementation.
#[doc(hidden)]
#[macro_export]
macro_rules! __plico_app_view_methods {
    ($this:ident, $app:expr, $shared:expr) => {
        fn set_frame_time(&mut $this, time: $crate::__ui_core::FrameTime) {
            $crate::__app_protocol::App::set_frame_time($app, time);
        }
        fn on_unhandled_key(&mut $this, key: &$crate::KeyEvent) -> bool {
            $crate::__app_protocol::App::on_key_event($app, key)
        }
        fn show(&mut $this, ui: &mut $crate::__ui_core::UiContext<'_>) {
            $crate::__app_protocol::App::show($app, ui);
        }
        fn theme(&$this) -> $crate::__ui_core::Theme {
            $crate::__app_protocol::App::theme($shared)
        }
    };
}
/// Implement View using the same forwarding as NativeApp.
#[macro_export]
macro_rules! impl_view_for_app {
    ($app:ty) => {
        impl $crate::View for $app {
            $crate::__plico_app_view_methods!(self, self, self);
        }
    };
}
impl<A: plico_dev::App> View for NativeApp<A> {
    crate::__plico_app_view_methods!(self, &mut self.0, &self.0);
}

/// A view owns application data; the adapter never retains its closures.
/// Consuming an event automatically requests a settling declaration.
pub trait View {
    #[doc(hidden)]
    fn replay_regions(
        &mut self,
        _engine: &mut Engine,
    ) -> Option<Result<regions::ReplayBatch, plico::FrameError>> {
        None
    }
    #[doc(hidden)]
    fn scene_cache_roots(&self) -> Vec<Entity> {
        Vec::new()
    }
    /// Window-local fallback after controls decline a key. Not called during IME
    /// composition or while a popup/modal owns input. Return true to consume it.
    fn on_unhandled_key(&mut self, _key: &plico_renderer::KeyEvent) -> bool {
        false
    }
    /// Drain one OS action. Actions remain native and never enter view snapshots.
    fn take_host_action(&mut self) -> Option<HostAction> {
        None
    }
    fn set_frame_time(&mut self, _time: plico::FrameTime) {}
    fn set_wake_callback(&mut self, _wake: plico_renderer::WakeCallback) {}
    fn poll_reload(&mut self) -> bool {
        false
    }
    /// Whether the view's own data source changed since the last rendered
    /// snapshot. Service-backed views override this so revision bumps force
    /// a re-declaration with structural synchronization.
    fn snapshot_changed(&self) -> bool {
        false
    }
    fn show(&mut self, ui: &mut plico::UiContext<'_>);
    fn theme(&self) -> Theme {
        Theme::default()
    }
}
/// Local adapter: a `View` driven by the engine's settle loop.
struct ViewChild<'a, V: View>(&'a mut V);
impl<V: View> plico::DeclareView for ViewChild<'_, V> {
    fn theme(&self) -> plico::Theme {
        View::theme(self.0)
    }
    fn show(&mut self, ui: &mut plico::UiContext<'_>) {
        View::show(self.0, ui)
    }
}
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
pub struct Source<V: View> {
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
    viewport: Option<(u32, u32)>,
    transition: transition::ThemeTransition,
    declarations_reused: bool,
    frame_time: Instant,
    clock: Box<dyn plico::Clock>,
    clock_epoch: Instant,
    last_tick: Option<Instant>,
    requests: plico::Changes,
    target_scene: Option<(Scene, u64, u64)>,
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
impl<V: View> Source<V> {
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
            viewport: None,
            transition: Default::default(),
            declarations_reused: false,
            frame_time: epoch,
            clock: Box::new(clock),
            clock_epoch: epoch,
            last_tick: None,
            requests: plico::Changes::empty(),
            target_scene: None,
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
    /// Attach a resident font asset, shared by normal and hot-reload rendering.
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
        } else if (scroll.changed || self.adapter.scroll_dirty)
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
        let mut fonts_changed = false;
        for font in &self.fonts {
            fonts_changed |= tcx.register_font(font);
        }
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
            // A service revision arrived: re-declare with full structural sync
            // so re-declared subtrees reach the adapter even when only text
            // changed deep inside retained panels.
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
                || scroll.changed
                || self.adapter.scroll_dirty
                || self.declarations_pending
            {
                self.update_backend(tcx, width, height);
            }
            return;
        }
        self.declarations_reused =
            (self.transition.active() || motion_dirty || shader_active || tooltip_active)
                && !self.requests.contains(plico::Changes::ANIMATE)
                && !self.pending
                && self.viewport == Some((width, height))
                && self.engine.theme() == self.view.theme()
                && (motion_dirty
                    || shader_active
                    || tooltip_active
                    || self
                        .target_scene
                        .as_ref()
                        .is_some_and(|(_, _, revision)| *revision == self.revision));
        if self.declarations_reused {
            if motion_dirty || shader_active || tooltip_active {
                self.update_backend(tcx, width, height);
                return;
            }
            return;
        }
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
        self.update_backend(tcx, width, height)
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
    fn paint_frame_scene(
        &mut self,
        scene: &mut Scene,
        snapshot: Option<&[PlacedNode]>,
        tcx: &mut TextContext,
        size: (u32, u32),
        scale: f64,
    ) {
        if self.adapter.shader_scale != scale {
            self.adapter.shader_scale = scale;
            if self.adapter.update_shaders(&self.engine, tcx) {
                self.revision += 1;
            }
        }
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
        let span = tracing::trace_span!(target: "wabou::perf", "frame.declare");
        #[cfg(feature = "profiling")]
        let _guard = span.enter();
        let passes = self
            .engine
            .declare_until_settled(&mut ViewChild(&mut self.view))
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
impl<V: View> FrameSource for Source<V> {
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
        let changed = self.view.poll_reload();
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
        rgb(self.engine.theme().surface)
    }
    fn has_anim(&self) -> bool {
        self.transition.active()
            || self.adapter.motions.active()
            || !self.adapter.scroll_motions.is_empty()
            || self.adapter.shader_active()
            || self.requests.contains(plico::Changes::ANIMATE)
            || self.engine.presence_deadline().is_some()
            || self.adapter.tips.needs_frame()
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
        self.pending |= handled;
        handled
    }
    fn pointer_cursor(&self) -> CursorStyle {
        self.adapter.cursor()
    }
}

#[cfg(test)]
mod theme_tests {
    use super::*;
    use std::time::Duration;
    struct Counting {
        gallery: gallery::Gallery,
        calls: usize,
    }
    impl View for Counting {
        fn show(&mut self, ui: &mut plico::UiContext<'_>) {
            self.calls += 1;
            plico_dev::App::show(&mut self.gallery, ui)
        }
        fn theme(&self) -> Theme {
            plico_dev::App::theme(&self.gallery)
        }
    }
    #[test]
    fn animated_theme_reuses_scene_and_accepts_events_without_stale_geometry() {
        let mut s = Source::new(Counting {
            gallery: gallery::Gallery {
                router: plico::Router::new(gallery::Page::Controls),
                ..Default::default()
            },
            calls: 0,
        });
        let mut tcx = TextContext::new();
        let now = Instant::now();
        let mut scene = Scene::new();
        let nodes = s.build_frame(&mut tcx, 920, 780);
        s.paint_scene(&mut scene, &nodes, &mut tcx, (920, 780), 1.);
        s.transition
            .compose(&mut scene, s.engine.theme(), (920, 780, 1.), now);
        s.view.gallery.dark = true;
        let nodes = s.build_frame(&mut tcx, 920, 780);
        let rects: Vec<_> = nodes.iter().map(|n| n.rect).collect();
        s.paint_scene(&mut scene, &nodes, &mut tcx, (920, 780), 1.);
        s.transition
            .compose(&mut scene, s.engine.theme(), (920, 780, 1.), now);
        assert!(s.has_anim());
        let calls = s.view.calls;
        let mut frames = Vec::new();
        for ms in [0, 100, 200] {
            let nodes = s.build_frame(&mut tcx, 920, 780);
            assert_eq!(s.view.calls, calls, "animation must not re-declare app");
            assert_eq!(nodes.iter().map(|n| n.rect).collect::<Vec<_>>(), rects);
            s.paint_scene(&mut scene, &nodes, &mut tcx, (920, 780), 1.);
            s.transition.compose(
                &mut scene,
                s.engine.theme(),
                (920, 780, 1.),
                now + Duration::from_millis(ms),
            );
            frames.push(scene.clone());
        }
        assert!(!s.has_anim());
        assert_ne!(frames[0], frames[1]);
        assert_ne!(frames[1], frames[2]);
        if let Ok(directory) = std::env::var("PLICO_THEME_REPORT_DIR") {
            std::fs::create_dir_all(&directory).unwrap();
            for (ms, scene) in [0, 100, 200].into_iter().zip(&frames) {
                plico_shell::renderer::render_to_png(
                    scene,
                    920,
                    780,
                    s.base_color(),
                    &format!("{directory}/{ms}.png"),
                )
                .unwrap();
            }
        }
        // Start another transition, then commit input before its next vsync.
        s.view.gallery.dark = false;
        let nodes = s.build_frame(&mut tcx, 920, 780);
        s.paint_scene(&mut scene, &nodes, &mut tcx, (920, 780), 1.);
        s.transition.compose(
            &mut scene,
            s.engine.theme(),
            (920, 780, 1.),
            now + Duration::from_millis(201),
        );
        let target = s
            .adapter
            .semantics(&s.engine, 0)
            .node_by_role(plico_renderer::SemanticRole::Button, "Increment", None)
            .unwrap()
            .id;
        assert!(s.handle_semantic_action(plico_renderer::SemanticAction::Click { target }));
        s.prepare_for_event(&mut tcx);
        let nodes = s.build_frame(&mut tcx, 920, 780);
        assert!(!s.declarations_reused);
        assert_eq!(s.view.gallery.count, 1);
        assert!(
            nodes
                .iter()
                .any(|n| n.paint.text.as_deref() == Some("Clicked 1 times"))
        );
    }
}

#[cfg(not(target_arch = "wasm32"))]
mod surface;
#[cfg(not(target_arch = "wasm32"))]
pub use surface::SurfaceSource;

mod zoom;
pub use zoom::{ZoomController, ZoomSource};

/// Semantic command protocol and deterministic headless test driver.
pub mod testing;
