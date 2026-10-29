//! Run taffy layout on a retained tree and flatten the result into an ordered
//! list of absolutely-positioned boxes for the renderer.

#![warn(missing_docs)]

use std::collections::HashMap;

use taffy::geometry::Size;
use taffy::{AvailableSpace, NodeId, TaffyTree, TraversePartialTree};

use crate::style::{CornerRadii, Paint};
use crate::text::TextContext;

fn text_measurement_width(
    known_width: Option<f32>,
    available_width: AvailableSpace,
    wraps: bool,
    intrinsic: [f32; 2],
) -> (f32, Option<f32>) {
    if let Some(width) = known_width {
        return (width, wraps.then_some(width));
    }
    let [min_content, max_content] = intrinsic;
    let selected = match available_width {
        AvailableSpace::MinContent => min_content,
        AvailableSpace::MaxContent => max_content,
        AvailableSpace::Definite(limit) if wraps => limit.min(max_content).max(min_content),
        AvailableSpace::Definite(_) => max_content,
    }
    .ceil();
    let constraint = (wraps && selected < max_content).then_some(selected);
    (selected, constraint)
}

/// A node after layout: its absolute border-box rect + resolved paint.
#[derive(Clone)]
pub struct PlacedNode {
    /// The taffy NodeId — lets the host associate Rust-side widgets with
    /// placed nodes (e.g. to call `Widget::paint` after layout).
    pub node_id: taffy::NodeId,
    /// Parent node in the flattened layout, or `None` for the root.
    pub parent_node_id: Option<taffy::NodeId>,
    /// Zero-based depth in the retained tree.
    pub depth: usize,
    /// Absolute border-box: (x0, y0, x1, y1).
    pub rect: [f32; 4],
    /// Absolute content-box top-left: (x, y). Text runs are anchored here.
    pub content_origin: [f32; 2],
    /// Content-box size after removing border and padding.
    pub content_size: [f32; 2],
    /// Accumulated ancestor overflow clip in absolute coordinates.
    pub clip: Option<[f32; 4]>,
    /// Radius of the accumulated clip when its nearest edge is rounded.
    pub clip_radius: f32,
    /// Per-corner radii of the accumulated clip.
    pub clip_radii: CornerRadii,
    /// Depth of the nearest ancestor that established `clip`.
    pub clip_depth: Option<usize>,
    /// This node's own overflow clip, used for replaced/native widget content.
    /// Descendants receive the same geometry through `clip` on their entries.
    pub own_clip: Option<[f32; 4]>,
    /// Corner radius applied to [`Self::own_clip`].
    pub own_clip_radius: f32,
    /// Per-corner radii applied to [`Self::own_clip`].
    pub own_clip_radii: CornerRadii,
    /// Resolved physical border widths: top, right, bottom, left.
    pub border_widths: [f32; 4],
    /// Scroll geometry and overlay-scrollbar state for this node.
    pub scroll: ScrollMetrics,
    /// Resolved visual properties and optional native-widget fragment.
    pub paint: Paint,
}

/// Finite scroll geometry in logical window coordinates. Axis-specific clips
/// are intentionally separate because their unconstrained edges may be infinite.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ScrollMetrics {
    /// Scroll port `(x0, y0, x1, y1)` in logical window coordinates.
    pub port: [f32; 4],
    /// Whether the horizontal and vertical axes can scroll.
    pub scrollable: [bool; 2],
    /// Maximum horizontal and vertical offsets.
    pub range: [f32; 2],
    /// Current clamped horizontal and vertical offsets.
    pub offset: [f32; 2],
    /// Host-owned overlay visibility. Zero also disables native hit testing.
    pub opacity: f32,
    /// 0 resting, 1 hovered, 2 actively dragged.
    pub interaction: u8,
}

#[derive(Clone, Copy, Default)]
struct ClipState {
    rect: Option<[f32; 4]>,
    radii: CornerRadii,
    depth: Option<usize>,
}

/// A depth-derived traversal boundary for the flattened retained tree.
/// `Exit` is the reusable paint/hit boundary for owner-local overlays.
#[derive(Clone, Copy)]
pub enum SubtreeEvent<'a> {
    /// Traversal entered a node before visiting its descendants.
    Enter(&'a PlacedNode),
    /// Traversal left a node after visiting all descendants.
    Exit(&'a PlacedNode),
}

/// Iterator that reconstructs enter/exit events from depth-first flat nodes.
pub struct SubtreeEvents<'a> {
    nodes: &'a [PlacedNode],
    index: usize,
    open: Vec<&'a PlacedNode>,
}

/// Traverse a flattened depth-first layout with balanced enter/exit events.
pub fn subtree_events(nodes: &[PlacedNode]) -> SubtreeEvents<'_> {
    SubtreeEvents {
        nodes,
        index: 0,
        open: Vec::new(),
    }
}

impl<'a> Iterator for SubtreeEvents<'a> {
    type Item = SubtreeEvent<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(next) = self.nodes.get(self.index)
            && self
                .open
                .last()
                .is_some_and(|open| open.depth >= next.depth)
        {
            return self.open.pop().map(SubtreeEvent::Exit);
        }
        if let Some(node) = self.nodes.get(self.index) {
            self.index += 1;
            self.open.push(node);
            return Some(SubtreeEvent::Enter(node));
        }
        self.open.pop().map(SubtreeEvent::Exit)
    }
}

/// Compute layout and flatten it without native-widget intrinsic measurement.
pub fn compute_and_walk_with_scroll(
    tree: &mut TaffyTree<Paint>,
    root: NodeId,
    width: f32,
    height: f32,
    tcx: &mut TextContext,
    scroll_offsets: &HashMap<NodeId, [f32; 2]>,
) -> Vec<PlacedNode> {
    compute_and_walk_with_scroll_and_widgets(
        tree,
        root,
        [width, height],
        tcx,
        1.0,
        |_node, _cx| None,
        scroll_offsets,
    )
}

/// Compute layout with native-widget measurement and flatten the result.
///
/// `measure_widget` runs inside Taffy's measurement pass. It must not mutate
/// the tree or retain the supplied [`crate::widget::MeasureContext`].
pub fn compute_and_walk_with_scroll_and_widgets(
    tree: &mut TaffyTree<Paint>,
    root: NodeId,
    viewport: [f32; 2],
    tcx: &mut TextContext,
    device_scale: f64,
    mut measure_widget: impl FnMut(NodeId, &mut crate::widget::MeasureContext<'_>) -> Option<[f32; 2]>,
    scroll_offsets: &HashMap<NodeId, [f32; 2]>,
) -> Vec<PlacedNode> {
    compute_layout_with_widgets(tree, root, viewport, tcx, device_scale, &mut measure_widget);
    flatten_with_scroll(tree, root, scroll_offsets)
}

/// Compute and flatten into retained storage, preserving the output allocation.
pub fn compute_and_walk_with_scroll_into(
    tree: &mut TaffyTree<Paint>,
    root: NodeId,
    viewport: [f32; 2],
    tcx: &mut TextContext,
    scroll_offsets: &HashMap<NodeId, [f32; 2]>,
    out: &mut Vec<PlacedNode>,
) {
    compute_layout_with_widgets(tree, root, viewport, tcx, 1., |_node, _cx| None);
    flatten_with_scroll_into(tree, root, scroll_offsets, out);
}

fn compute_layout_with_widgets(
    tree: &mut TaffyTree<Paint>,
    root: NodeId,
    viewport: [f32; 2],
    tcx: &mut TextContext,
    device_scale: f64,
    mut measure_widget: impl FnMut(NodeId, &mut crate::widget::MeasureContext<'_>) -> Option<[f32; 2]>,
) {
    #[cfg(feature = "profiling")]
    let _span = tracing::trace_span!(target: "plico::layout", "layout.compute").entered();
    let available = Size {
        width: AvailableSpace::Definite(viewport[0]),
        height: AvailableSpace::Definite(viewport[1]),
    };
    // Measure ordinary text with the selected paragraph engine. The closure
    // takes ownership of the `&mut TextContext` reborrow for the duration of
    // layout; the caller may reuse `tcx` afterwards.
    let _ = tree.compute_layout_with_measure(
        root,
        available,
        move |input, id, ctx: Option<&mut Paint>, style| {
            let mut result = taffy::compute_leaf_layout(
                input,
                style,
                |_, _| 0.0,
                |known, avail| {
                    let available_axis = |space| match space {
                        AvailableSpace::Definite(value) => {
                            crate::widget::WidgetAvailableSpace::Definite(value)
                        }
                        AvailableSpace::MinContent => {
                            crate::widget::WidgetAvailableSpace::MinContent
                        }
                        AvailableSpace::MaxContent => {
                            crate::widget::WidgetAvailableSpace::MaxContent
                        }
                    };
                    let mut measure = crate::widget::MeasureContext::new(
                        [known.width, known.height],
                        [available_axis(avail.width), available_axis(avail.height)],
                        device_scale,
                        tcx,
                    );
                    if let Some([width, height]) = measure_widget(id, &mut measure) {
                        return Size { width, height };
                    }
                    if let Some(paint) = ctx.as_deref() {
                        if let Some(text) = &paint.text {
                            // Taffy supplies border-box known dimensions but its
                            // available space already excludes padding/borders.
                            // Shaping must use the content box on both axes.
                            let content_known = |known: Option<f32>, available| {
                                known.and_then(|_| match available {
                                    AvailableSpace::Definite(value) => Some(value.max(0.0)),
                                    _ => None,
                                })
                            };
                            let known = Size {
                                width: content_known(known.width, avail.width),
                                height: content_known(known.height, avail.height),
                            };
                            let wraps = paint.wrap_text || paint.text_max_lines > 0;
                            if measure.text().has_native_paragraphs() {
                                let request =
                                    measure.text().paragraph_request(paint, None).unwrap();
                                let mut intrinsic_request = request.clone();
                                intrinsic_request.max_lines = 0;
                                intrinsic_request.ellipsis = false;
                                let intrinsic = measure
                                    .text()
                                    .native_paragraph(std::sync::Arc::new(intrinsic_request))
                                    .unwrap()
                                    .metrics()
                                    .intrinsic;
                                let intrinsic = if wraps { intrinsic } else { [intrinsic[1]; 2] };
                                let (width, layout_width) = text_measurement_width(
                                    known.width,
                                    avail.width,
                                    wraps,
                                    intrinsic,
                                );
                                let mut request = request;
                                request.width = layout_width;
                                let metrics = measure
                                    .text()
                                    .native_paragraph(std::sync::Arc::new(request))
                                    .unwrap()
                                    .metrics();
                                return Size {
                                    width,
                                    height: known.height.unwrap_or(metrics.height),
                                };
                            }
                            let intrinsic = if known.width.is_none() {
                                let [min_content, max_content] =
                                    crate::text::measure_text_intrinsic_widths(
                                        measure.text(),
                                        text.clone(),
                                        paint.font_size,
                                        paint.font_weight,
                                        paint.font_italic,
                                        paint.letter_spacing,
                                        paint.line_height,
                                        paint.text_align,
                                        crate::text::brush_for_color(paint.text_color),
                                        paint.text_runs.clone(),
                                        paint.font_family.as_ref(),
                                        wraps,
                                    );
                                [min_content, max_content]
                            } else {
                                [0.0, 0.0]
                            };
                            let (measured_width, layout_width) =
                                text_measurement_width(known.width, avail.width, wraps, intrinsic);
                            let l = crate::text::layout_text_styled_clamped(
                                measure.text(),
                                text.clone(),
                                paint.font_size,
                                paint.font_weight,
                                paint.font_italic,
                                paint.letter_spacing,
                                paint.line_height,
                                paint.text_align,
                                crate::text::brush_for_color(paint.text_color),
                                paint.text_runs.clone(),
                                paint.font_family.as_ref(),
                                layout_width,
                                paint.text_max_lines,
                                !paint.text_clip_lines,
                            );
                            return Size {
                                width: measured_width,
                                height: known.height.unwrap_or_else(|| l.height()),
                            };
                        }
                        if let Some([width, height]) = paint.intrinsic_size {
                            return Size {
                                width: known.width.unwrap_or(width),
                                height: known.height.unwrap_or(height),
                            };
                        }
                    }
                    Size::ZERO
                },
            );
            // Taffy 0.15 otherwise keeps a former container's hoisted absolute
            // children when it becomes a leaf. An empty positioning pass clears
            // that history before rounding can visit a removed NodeId.
            if result.oof_candidates.is_empty() {
                result.oof_positioning_area = Some(taffy::OofPositioningArea {
                    size: result.size,
                    offset: taffy::Point::ZERO,
                });
            }
            // Taffy needs text baselines, including when fixed dimensions let it
            // skip the intrinsic measurement callback. Use the painted
            // paragraph and resolve CSS percentage padding against parent width.
            if let Some(paint) = ctx.as_deref()
                && let Some(text) = &paint.text
            {
                use taffy::util::ResolveOrZero;
                let padding = style
                    .padding
                    .resolve_or_zero(input.parent_size.width, |_, _| 0.);
                let border = style
                    .border
                    .resolve_or_zero(input.parent_size.width, |_, _| 0.);
                let width =
                    (result.size.width - padding.left - padding.right - border.left - border.right)
                        .max(0.);
                if tcx.has_native_paragraphs() {
                    let request = tcx
                        .paragraph_request(
                            paint,
                            (paint.wrap_text || paint.text_ellipsis || paint.text_max_lines > 0)
                                .then_some(width),
                        )
                        .unwrap();
                    let metrics = tcx
                        .native_paragraph(std::sync::Arc::new(request))
                        .unwrap()
                        .metrics();
                    let top = padding.top + border.top;
                    result.baselines = taffy::tree::Baselines {
                        first: metrics.baselines[0].map(|b| top + b),
                        last: metrics.baselines[1].map(|b| top + b),
                    };
                } else {
                    let layout = crate::text::layout_text_styled_clamped(
                        tcx,
                        text.clone(),
                        paint.font_size,
                        paint.font_weight,
                        paint.font_italic,
                        paint.letter_spacing,
                        paint.line_height,
                        paint.text_align,
                        crate::text::brush_for_color(paint.text_color),
                        paint.text_runs.clone(),
                        paint.font_family.as_ref(),
                        paint.wrap_text.then_some(width),
                        paint.text_max_lines,
                        !paint.text_clip_lines,
                    );
                    let top = padding.top + border.top;
                    result.baselines = taffy::tree::Baselines {
                        first: layout
                            .lines()
                            .next()
                            .map(|line| top + line.metrics().baseline),
                        last: layout
                            .lines()
                            .last()
                            .map(|line| top + line.metrics().baseline),
                    };
                }
            }
            result
        },
    );
}

/// Re-flatten a previously computed Taffy layout with new scroll offsets.
/// Scrolling changes paint coordinates and clips, not intrinsic layout, so the
/// interactive host can use this path without remeasuring the entire tree.
pub fn flatten_with_scroll(
    tree: &TaffyTree<Paint>,
    root: NodeId,
    scroll_offsets: &HashMap<NodeId, [f32; 2]>,
) -> Vec<PlacedNode> {
    let mut out = Vec::new();
    flatten_with_scroll_into(tree, root, scroll_offsets, &mut out);
    out
}

/// Reuse the caller's node buffer when scrolling or rebuilding layout.
pub fn flatten_with_scroll_into(
    tree: &TaffyTree<Paint>,
    root: NodeId,
    scroll_offsets: &HashMap<NodeId, [f32; 2]>,
    out: &mut Vec<PlacedNode>,
) {
    #[cfg(feature = "profiling")]
    let _span = tracing::trace_span!(target: "plico::layout", "layout.flatten").entered();
    let mut output = FlattenOutput {
        nodes: out,
        used: 0,
    };
    walk(
        tree,
        root,
        None,
        0,
        0.0,
        0.0,
        ClipState::default(),
        scroll_offsets,
        &mut output,
    );
    output.nodes.truncate(output.used);
}

// Only storage is reused. Every node, geometry field and paint value is visited
// and refreshed, including when order, clipping or topology changes.
struct FlattenOutput<'a> {
    nodes: &'a mut Vec<PlacedNode>,
    used: usize,
}
impl FlattenOutput<'_> {
    fn last_mut(&mut self) -> Option<&mut PlacedNode> {
        self.nodes.get_mut(self.used.checked_sub(1)?)
    }
}

/// Depth-first walk accumulating absolute positions. `x0`,`y0` is the absolute
/// border-box origin of the *parent*. A node's own border-box origin is
/// `(parent_x0 + node.location.x, parent_y0 + node.location.y)`.
fn intersect(a: [f32; 4], b: [f32; 4]) -> [f32; 4] {
    [
        a[0].max(b[0]),
        a[1].max(b[1]),
        a[2].min(b[2]),
        a[3].min(b[3]),
    ]
}

fn node_clip(
    style: &taffy::Style,
    paint: Option<&Paint>,
    layout: &taffy::Layout,
    rect: [f32; 4],
    depth: usize,
) -> Option<ClipState> {
    let [x0, y0, x1, y1] = rect;
    let clips_x = style.overflow.x != taffy::Overflow::Visible;
    let clips_y = style.overflow.y != taffy::Overflow::Visible;
    if !clips_x && !clips_y {
        return None;
    }
    let mut own = [
        f32::NEG_INFINITY,
        f32::NEG_INFINITY,
        f32::INFINITY,
        f32::INFINITY,
    ];
    if clips_x {
        own[0] = x0 + layout.border.left;
        own[2] = x1 - layout.border.right;
    }
    if clips_y {
        own[1] = y0 + layout.border.top;
        own[3] = y1 - layout.border.bottom;
    }
    let radii = if clips_x && clips_y {
        paint.map_or_else(CornerRadii::default, |paint| {
            paint.border_radii.inset(
                layout
                    .border
                    .top
                    .max(layout.border.right)
                    .max(layout.border.bottom)
                    .max(layout.border.left),
            )
        })
    } else {
        CornerRadii::default()
    };
    Some(ClipState {
        rect: Some(own),
        radii,
        depth: Some(depth),
    })
}

fn intersect_clip(own: ClipState, inherited: ClipState, clips_both_axes: bool) -> ClipState {
    let bounds = own.rect.unwrap();
    let rect = inherited
        .rect
        .map_or(bounds, |clip| intersect(clip, bounds));
    let radii = if clips_both_axes && rect == bounds {
        own.radii
    } else if inherited.rect == Some(rect) {
        inherited.radii
    } else {
        CornerRadii::default()
    };
    ClipState {
        rect: Some(rect),
        radii,
        depth: own.depth,
    }
}

fn ordered_children(tree: &TaffyTree<Paint>, node: NodeId) -> impl Iterator<Item = NodeId> {
    // The child iterator has no exact size hint. Reserve once rather than grow
    // 4/8/16/... slots, and resolve paint order once rather than per comparison.
    let mut children = Vec::with_capacity(tree.child_count(node));
    children.extend(tree.child_ids(node).map(|child| {
        let key = tree
            .get_node_context(child)
            .map_or_else(Default::default, |paint| {
                (paint.overlay_plane, paint.z_index)
            });
        (key, child)
    }));
    children.sort_by_key(|(key, _)| *key);
    children.into_iter().map(|(_, child)| child)
}

#[allow(clippy::too_many_arguments)]
fn walk(
    tree: &TaffyTree<Paint>,
    node: NodeId,
    parent_node_id: Option<NodeId>,
    depth: usize,
    parent_x0: f32,
    parent_y0: f32,
    inherited_clip: ClipState,
    scroll_offsets: &HashMap<NodeId, [f32; 2]>,
    out: &mut FlattenOutput<'_>,
) {
    let layout = match tree.layout(node) {
        Ok(l) => l,
        Err(_) => return,
    };
    let style = match tree.style(node) {
        Ok(style) if style.display != taffy::Display::None => style,
        _ => return,
    };
    let paint = tree.get_node_context(node);

    let x0 = parent_x0 + layout.location.x;
    let y0 = parent_y0 + layout.location.y;
    let w = layout.size.width;
    let h = layout.size.height;
    let rect = [x0, y0, x0 + w, y0 + h];

    let cx = x0 + layout.border.left + layout.padding.left;
    let cy = y0 + layout.border.top + layout.padding.top;
    let content_width =
        (w - layout.border.left - layout.border.right - layout.padding.left - layout.padding.right)
            .max(0.0);
    let content_height =
        (h - layout.border.top - layout.border.bottom - layout.padding.top - layout.padding.bottom)
            .max(0.0);
    let scroll = scroll_offsets.get(&node).copied().unwrap_or([0.0, 0.0]);
    let scrollable = [
        style.overflow.x == taffy::Overflow::Scroll,
        style.overflow.y == taffy::Overflow::Scroll,
    ];
    let scroll_range = [layout.scroll_width(), layout.scroll_height()];

    if let Some(paint) = paint
        && w > 0.0
        && h > 0.0
    {
        if out.used == out.nodes.len() {
            out.nodes.push(PlacedNode {
                node_id: node,
                parent_node_id,
                depth,
                rect,
                content_origin: [cx, cy],
                content_size: [content_width, content_height],
                clip: inherited_clip.rect,
                clip_radius: inherited_clip.radii.max(),
                clip_radii: inherited_clip.radii,
                clip_depth: inherited_clip.depth,
                own_clip: None,
                own_clip_radius: 0.0,
                own_clip_radii: CornerRadii::default(),
                border_widths: [
                    layout.border.top,
                    layout.border.right,
                    layout.border.bottom,
                    layout.border.left,
                ],
                scroll: ScrollMetrics {
                    port: [
                        x0 + layout.border.left,
                        y0 + layout.border.top,
                        x0 + w - layout.border.right,
                        y0 + h - layout.border.bottom,
                    ],
                    scrollable,
                    range: scroll_range,
                    offset: scroll,
                    opacity: 0.0,
                    interaction: 0,
                },
                paint: paint.clone(),
            });
        } else {
            out.nodes[out.used].paint.clone_from(paint);
        }
        let PlacedNode {
            node_id: slot_node_id,
            parent_node_id: slot_parent_node_id,
            depth: slot_depth,
            rect: slot_rect,
            content_origin: slot_content_origin,
            content_size: slot_content_size,
            clip: slot_clip,
            clip_radius: slot_clip_radius,
            clip_radii: slot_clip_radii,
            clip_depth: slot_clip_depth,
            own_clip: slot_own_clip,
            own_clip_radius: slot_own_clip_radius,
            own_clip_radii: slot_own_clip_radii,
            border_widths: slot_border_widths,
            scroll: slot_scroll,
            paint: _,
        } = &mut out.nodes[out.used];
        *slot_node_id = node;
        *slot_parent_node_id = parent_node_id;
        *slot_depth = depth;
        *slot_rect = rect;
        *slot_content_origin = [cx, cy];
        *slot_content_size = [content_width, content_height];
        *slot_clip = inherited_clip.rect;
        *slot_clip_radius = inherited_clip.radii.max();
        *slot_clip_radii = inherited_clip.radii;
        *slot_clip_depth = inherited_clip.depth;
        *slot_own_clip = None;
        *slot_own_clip_radius = 0.0;
        *slot_own_clip_radii = CornerRadii::default();
        *slot_border_widths = [
            layout.border.top,
            layout.border.right,
            layout.border.bottom,
            layout.border.left,
        ];
        *slot_scroll = ScrollMetrics {
            port: [
                x0 + layout.border.left,
                y0 + layout.border.top,
                x0 + w - layout.border.right,
                y0 + h - layout.border.bottom,
            ],
            scrollable,
            range: scroll_range,
            offset: scroll,
            opacity: 0.0,
            interaction: 0,
        };
        out.used += 1;
    }

    // Compute the local boundary once. Its ancestor intersection is only a
    // projection: presentation transforms still need the un-intersected clip.
    let own_clip = node_clip(style, paint, layout, rect, depth);
    let child_clip = own_clip.map_or(inherited_clip, |own| {
        intersect_clip(
            own,
            inherited_clip,
            style.overflow.x != taffy::Overflow::Visible
                && style.overflow.y != taffy::Overflow::Visible,
        )
    });
    if let Some(clip) = own_clip
        && let Some(placed) = out.last_mut()
        && placed.node_id == node
    {
        placed.own_clip = clip.rect;
        placed.own_clip_radius = clip.radii.max();
        placed.own_clip_radii = clip.radii;
    }
    // Only an intrinsically empty clipping boundary can prune layout here.
    // Effective intersections are resolved after presentation transforms. The
    // zero-sized clipping node itself may have been omitted from `out`; walking
    // its descendants would therefore lose the layer boundary and flash them
    // un-clipped for one frame during collapse animations.
    if own_clip
        .and_then(|clip| clip.rect)
        .is_some_and(|clip| clip[2] <= clip[0] || clip[3] <= clip[1])
    {
        return;
    }
    if tree.child_count(node) < 2 {
        for child in tree.child_ids(node) {
            walk(
                tree,
                child,
                Some(node),
                depth + 1,
                x0 - scroll[0],
                y0 - scroll[1],
                child_clip,
                scroll_offsets,
                out,
            );
        }
        return;
    }
    // Most siblings already have paint order (usually all z=0). Avoid a
    // temporary child vector and stable-sort scratch storage in that case.
    let order_key = |child| {
        tree.get_node_context(child)
            .map_or_else(Default::default, |paint| {
                (paint.overlay_plane, paint.z_index)
            })
    };
    let mut previous = None;
    let ordered = tree.child_ids(node).all(|child| {
        let key = order_key(child);
        let sorted = previous.is_none_or(|old| old <= key);
        previous = Some(key);
        sorted
    });
    let mut visit = |child| {
        walk(
            tree,
            child,
            Some(node),
            depth + 1,
            x0 - scroll[0],
            y0 - scroll[1],
            child_clip,
            scroll_offsets,
            out,
        );
    };
    if ordered {
        for child in tree.child_ids(node) {
            visit(child);
        }
    } else {
        // Stable order for equal z values is part of paint and hit-test order.
        for child in ordered_children(tree, node) {
            visit(child);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn removing_last_absolute_child_clears_hoisted_layout_state() {
        let mut tree = TaffyTree::<Paint>::new();
        let child = tree
            .new_leaf(taffy::Style {
                position: taffy::Position::Absolute,
                size: Size {
                    width: taffy::Dimension::length(20.),
                    height: taffy::Dimension::length(20.),
                },
                ..Default::default()
            })
            .unwrap();
        let root = tree
            .new_with_children(
                taffy::Style {
                    size: Size {
                        width: taffy::Dimension::length(100.),
                        height: taffy::Dimension::length(100.),
                    },
                    ..Default::default()
                },
                &[child],
            )
            .unwrap();
        for id in [root, child] {
            tree.set_node_context(id, Some(Paint::default())).unwrap();
        }
        let mut text = TextContext::new();
        compute_and_walk_with_scroll(&mut tree, root, 100., 100., &mut text, &HashMap::new());
        tree.set_children(root, &[]).unwrap();
        tree.remove(child).unwrap();
        let nodes =
            compute_and_walk_with_scroll(&mut tree, root, 100., 100., &mut text, &HashMap::new());
        assert_eq!(nodes.len(), 1);
        assert!(tree.hoisted_children(root).unwrap().is_empty());
    }

    #[test]
    fn ample_intrinsic_space_reuses_the_unconstrained_layout() {
        let intrinsic = [80.25, 140.25];
        assert_eq!(
            text_measurement_width(None, AvailableSpace::Definite(500.0), true, intrinsic,),
            (141.0, None)
        );
        assert_eq!(
            text_measurement_width(None, AvailableSpace::MaxContent, true, intrinsic),
            (141.0, None)
        );
        assert_eq!(
            text_measurement_width(None, AvailableSpace::Definite(100.0), true, intrinsic),
            (100.0, Some(100.0))
        );
        assert_eq!(
            text_measurement_width(Some(96.0), AvailableSpace::MaxContent, true, intrinsic),
            (96.0, Some(96.0))
        );
    }

    #[test]
    fn taffy_receives_distinct_min_and_max_content_text_widths() {
        let mut tree = TaffyTree::<Paint>::new();
        let min_text = tree.new_leaf(taffy::Style::default()).unwrap();
        let max_text = tree.new_leaf(taffy::Style::default()).unwrap();
        let root = tree
            .new_with_children(
                taffy::Style {
                    display: taffy::Display::Grid,
                    grid_template_columns: vec![
                        taffy::style_helpers::min_content(),
                        taffy::style_helpers::max_content(),
                    ],
                    size: Size {
                        width: taffy::Dimension::length(500.0),
                        height: taffy::Dimension::auto(),
                    },
                    ..Default::default()
                },
                &[min_text, max_text],
            )
            .unwrap();
        tree.set_node_context(root, Some(Paint::default())).unwrap();
        for node in [min_text, max_text] {
            tree.set_node_context(
                node,
                Some(Paint {
                    text: Some(Arc::from("short words with one extraordinarilylongtoken")),
                    wrap_text: true,
                    ..Paint::default()
                }),
            )
            .unwrap();
        }
        let mut text = TextContext::new();
        let placed =
            compute_and_walk_with_scroll(&mut tree, root, 500.0, 200.0, &mut text, &HashMap::new());
        let width = |id| {
            placed
                .iter()
                .find(|node| node.node_id == id)
                .unwrap()
                .content_size[0]
        };
        let min = width(min_text);
        let max = width(max_text);

        assert!(min > 0.0);
        assert!(
            min < max,
            "min-content {min} should be below max-content {max}"
        );
    }

    #[test]
    fn known_text_height_wins_over_shaped_height() {
        let mut tree = TaffyTree::<Paint>::new();
        let root = tree
            .new_leaf(taffy::Style {
                size: Size {
                    width: taffy::Dimension::length(200.0),
                    height: taffy::Dimension::length(40.0),
                },
                ..Default::default()
            })
            .unwrap();
        tree.set_node_context(
            root,
            Some(Paint {
                text: Some(Arc::from("short words with one extraordinarilylongtoken")),
                wrap_text: true,
                ..Paint::default()
            }),
        )
        .unwrap();
        let mut text = TextContext::new();
        let placed =
            compute_and_walk_with_scroll(&mut tree, root, 500.0, 200.0, &mut text, &HashMap::new());
        let node = placed.iter().find(|node| node.node_id == root).unwrap();
        assert_eq!(node.content_size, [200.0, 40.0]);
    }

    #[test]
    fn widget_measurement_runs_inside_taffys_constrained_layout_pass() {
        let mut tree = TaffyTree::<Paint>::new();
        let widget = tree.new_leaf(taffy::Style::default()).unwrap();
        let root = tree
            .new_with_children(
                taffy::Style {
                    size: Size {
                        width: taffy::Dimension::length(200.0),
                        height: taffy::Dimension::length(100.0),
                    },
                    align_items: taffy::AlignItems::FLEX_START,
                    ..Default::default()
                },
                &[widget],
            )
            .unwrap();
        for node in [root, widget] {
            tree.set_node_context(node, Some(Paint::default())).unwrap();
        }
        let mut observed = None;
        let mut text = TextContext::new();
        let placed = compute_and_walk_with_scroll_and_widgets(
            &mut tree,
            root,
            [200.0, 100.0],
            &mut text,
            2.0,
            |node, cx| {
                (node == widget).then(|| {
                    observed = Some((cx.known_size(), cx.available_space(), cx.device_scale()));
                    cx.resolve_size([80.0, 30.0])
                })
            },
            &HashMap::new(),
        );

        assert!(observed.is_some_and(|(_, _, scale)| scale == 2.0));
        let widget = placed
            .iter()
            .find(|placed| placed.node_id == widget)
            .unwrap();
        assert_eq!(widget.content_size, [80.0, 30.0]);
    }

    #[test]
    fn scroll_offsets_children_and_propagates_overflow_clip() {
        let mut tree = TaffyTree::<Paint>::new();
        let child = tree
            .new_leaf(taffy::Style {
                size: Size {
                    width: taffy::Dimension::length(100.0),
                    height: taffy::Dimension::length(300.0),
                },
                flex_shrink: 0.0,
                ..Default::default()
            })
            .unwrap();
        let container = tree
            .new_with_children(
                taffy::Style {
                    size: Size {
                        width: taffy::Dimension::length(100.0),
                        height: taffy::Dimension::length(100.0),
                    },
                    overflow: taffy::Point {
                        x: taffy::Overflow::Visible,
                        y: taffy::Overflow::Scroll,
                    },
                    ..Default::default()
                },
                &[child],
            )
            .unwrap();
        let root = tree
            .new_with_children(
                taffy::Style {
                    size: Size {
                        width: taffy::Dimension::length(200.0),
                        height: taffy::Dimension::length(200.0),
                    },
                    ..Default::default()
                },
                &[container],
            )
            .unwrap();
        for node in [root, container, child] {
            tree.set_node_context(node, Some(Paint::default())).unwrap();
        }
        let mut scroll = HashMap::new();
        scroll.insert(container, [0.0, 50.0]);
        let mut text = TextContext::new();
        let placed =
            compute_and_walk_with_scroll(&mut tree, root, 200.0, 200.0, &mut text, &scroll);

        assert_eq!(placed[2].rect, [0.0, -50.0, 100.0, 250.0]);
        assert_eq!(
            placed[2].clip,
            Some([f32::NEG_INFINITY, 0.0, f32::INFINITY, 100.0])
        );
        assert_eq!(placed[2].clip_depth, Some(1));
    }

    #[test]
    fn rounded_overflow_propagates_inner_clip_radius() {
        let mut tree = TaffyTree::<Paint>::new();
        let child = tree
            .new_leaf(taffy::Style {
                size: Size {
                    width: taffy::Dimension::length(10.0),
                    height: taffy::Dimension::length(10.0),
                },
                ..Default::default()
            })
            .unwrap();
        let container = tree
            .new_with_children(
                taffy::Style {
                    size: Size {
                        width: taffy::Dimension::length(100.0),
                        height: taffy::Dimension::length(100.0),
                    },
                    border: taffy::Rect::length(1.0_f32),
                    overflow: taffy::Point {
                        x: taffy::Overflow::Hidden,
                        y: taffy::Overflow::Hidden,
                    },
                    ..Default::default()
                },
                &[child],
            )
            .unwrap();
        let root = tree
            .new_with_children(taffy::Style::default(), &[container])
            .unwrap();
        tree.set_node_context(root, Some(Paint::default())).unwrap();
        tree.set_node_context(
            container,
            Some(Paint {
                border_radii: CornerRadii::uniform(12.0),
                ..Paint::default()
            }),
        )
        .unwrap();
        tree.set_node_context(child, Some(Paint::default()))
            .unwrap();

        let mut text = TextContext::new();
        let placed =
            compute_and_walk_with_scroll(&mut tree, root, 100.0, 100.0, &mut text, &HashMap::new());

        assert_eq!(placed[2].clip, Some([1.0, 1.0, 99.0, 99.0]));
        assert_eq!(placed[2].clip_radius, 11.0);
        assert_eq!(placed[1].own_clip, Some([1.0, 1.0, 99.0, 99.0]));
        assert_eq!(placed[1].own_clip_radius, 11.0);
    }

    #[test]
    fn nested_clip_keeps_local_bounds_until_presentation_transforms() {
        let mut tree = TaffyTree::<Paint>::new();
        let size = |width, height| Size {
            width: taffy::Dimension::length(width),
            height: taffy::Dimension::length(height),
        };
        let child = tree
            .new_leaf(taffy::Style {
                size: size(80., 20.),
                ..Default::default()
            })
            .unwrap();
        let row = tree
            .new_with_children(
                taffy::Style {
                    size: size(100., 30.),
                    position: taffy::Position::Absolute,
                    inset: taffy::Rect {
                        top: taffy::LengthPercentageAuto::length(200.),
                        left: taffy::LengthPercentageAuto::auto(),
                        right: taffy::LengthPercentageAuto::auto(),
                        bottom: taffy::LengthPercentageAuto::auto(),
                    },
                    overflow: taffy::Point {
                        x: taffy::Overflow::Hidden,
                        y: taffy::Overflow::Hidden,
                    },
                    ..Default::default()
                },
                &[child],
            )
            .unwrap();
        let root = tree
            .new_with_children(
                taffy::Style {
                    size: size(100., 100.),
                    overflow: taffy::Point {
                        x: taffy::Overflow::Hidden,
                        y: taffy::Overflow::Hidden,
                    },
                    ..Default::default()
                },
                &[row],
            )
            .unwrap();
        for node in [root, row, child] {
            tree.set_node_context(node, Some(Paint::default())).unwrap();
        }
        let placed = compute_and_walk_with_scroll(
            &mut tree,
            root,
            100.,
            100.,
            &mut TextContext::new(),
            &HashMap::new(),
        );
        // A presentation transform may move this row into the root viewport.
        // Layout must retain both its local clip and its descendants.
        assert!(placed.iter().any(|node| node.node_id == child));
        let row = placed.iter().find(|node| node.node_id == row).unwrap();
        assert_eq!(row.own_clip, Some([0., 200., 100., 230.]));
    }

    #[test]
    fn empty_overflow_clip_prunes_visible_descendants() {
        let mut tree = TaffyTree::<Paint>::new();
        let child = tree
            .new_leaf(taffy::Style {
                size: Size {
                    width: taffy::Dimension::length(80.0),
                    height: taffy::Dimension::length(20.0),
                },
                ..Default::default()
            })
            .unwrap();
        let collapsed = tree
            .new_with_children(
                taffy::Style {
                    size: Size {
                        width: taffy::Dimension::length(100.0),
                        height: taffy::Dimension::length(0.0),
                    },
                    overflow: taffy::Point {
                        x: taffy::Overflow::Hidden,
                        y: taffy::Overflow::Hidden,
                    },
                    ..Default::default()
                },
                &[child],
            )
            .unwrap();
        let root = tree
            .new_with_children(
                taffy::Style {
                    size: Size {
                        width: taffy::Dimension::length(100.0),
                        height: taffy::Dimension::length(100.0),
                    },
                    ..Default::default()
                },
                &[collapsed],
            )
            .unwrap();
        for node in [root, collapsed, child] {
            tree.set_node_context(node, Some(Paint::default())).unwrap();
        }

        let mut text = TextContext::new();
        let placed =
            compute_and_walk_with_scroll(&mut tree, root, 100.0, 100.0, &mut text, &HashMap::new());

        assert_eq!(placed.len(), 1);
        assert_eq!(placed[0].node_id, root);
    }

    #[test]
    fn reused_flatten_slots_match_fresh_scene_after_paint_order_and_clip_changes() {
        use crate::{
            Scene,
            scene::build_scene,
            style::{PaintTransform, Shadow},
        };
        use peniko::Color;
        let mut tree = TaffyTree::new();
        let style = taffy::Style {
            size: Size {
                width: taffy::Dimension::length(90.),
                height: taffy::Dimension::length(28.),
            },
            flex_shrink: 0.,
            ..Default::default()
        };
        let children: Vec<_> = (0..24)
            .map(|_| {
                tree.new_leaf_with_context(style.clone(), Paint::default())
                    .unwrap()
            })
            .collect();
        let root_style = taffy::Style {
            flex_direction: taffy::FlexDirection::Column,
            size: Size {
                width: taffy::Dimension::length(120.),
                height: taffy::Dimension::length(300.),
            },
            ..Default::default()
        };
        let root = tree
            .new_with_children(root_style.clone(), &children)
            .unwrap();
        tree.set_node_context(root, Some(Paint::default())).unwrap();
        let mut out = Vec::new();
        let mut text = TextContext::new();
        for frame in 0..18 {
            let mut style = root_style.clone();
            style.overflow.y = if frame % 3 == 0 {
                taffy::Overflow::Scroll
            } else {
                taffy::Overflow::Visible
            };
            style.overflow.x = if frame % 2 == 0 {
                taffy::Overflow::Hidden
            } else {
                taffy::Overflow::Visible
            };
            tree.set_style(root, style).unwrap();
            let count = if frame % 3 == 0 {
                0
            } else if frame % 3 == 1 {
                13
            } else {
                24
            };
            tree.set_children(root, &children[..count]).unwrap();
            for (index, id) in children.iter().enumerate() {
                let mut paint = Paint {
                    text: Some(Arc::from(format!("Frame {frame}, label {index}"))),
                    background: Some(Color::from_rgb8(index as u8 * 8, 80, 120)),
                    opacity: 0.5 + (index % 2) as f32 * 0.5,
                    z_index: ((index + frame) % 4) as i32,
                    border_radii: CornerRadii::uniform((frame % 5) as f32),
                    ..Paint::default()
                };
                if frame % 2 == 0 {
                    paint.transform.push(PaintTransform::Scale(0.95, 1.));
                    paint.shadows.push(Shadow {
                        offset_x: 1.,
                        offset_y: 2.,
                        spread: 1.,
                        std_dev: 1.,
                        color: Color::BLACK,
                        radius: None,
                    });
                    paint.selection_rects = Arc::from([[0., 0., 10., 12.]]);
                }
                tree.set_node_context(*id, Some(paint)).unwrap();
            }
            tree.compute_layout(
                root,
                Size {
                    width: AvailableSpace::MaxContent,
                    height: AvailableSpace::MaxContent,
                },
            )
            .unwrap();
            let offsets = HashMap::from([(root, [0., (frame * 2) as f32])]);
            flatten_with_scroll_into(&tree, root, &offsets, &mut out);
            let fresh = flatten_with_scroll(&tree, root, &offsets);
            assert_eq!(out.len(), fresh.len());
            for (a, b) in out.iter().zip(&fresh) {
                assert_eq!(
                    (
                        a.node_id,
                        a.parent_node_id,
                        a.depth,
                        a.rect,
                        a.content_origin,
                        a.content_size
                    ),
                    (
                        b.node_id,
                        b.parent_node_id,
                        b.depth,
                        b.rect,
                        b.content_origin,
                        b.content_size
                    )
                );
                assert_eq!(
                    (
                        a.clip,
                        a.clip_radii,
                        a.clip_depth,
                        a.own_clip,
                        a.own_clip_radii,
                        a.border_widths,
                        a.scroll
                    ),
                    (
                        b.clip,
                        b.clip_radii,
                        b.clip_depth,
                        b.own_clip,
                        b.own_clip_radii,
                        b.border_widths,
                        b.scroll
                    )
                );
            }
            let (mut actual, mut expected) = (Scene::new(), Scene::new());
            build_scene(&mut actual, &out, &mut text, 160, 350, Color::WHITE);
            build_scene(&mut expected, &fresh, &mut text, 160, 350, Color::WHITE);
            assert_eq!(
                actual, expected,
                "frame {frame}: stale geometry or Paint in reused slot"
            );
        }
    }

    #[test]
    fn reused_flatten_releases_removed_payloads_and_reuses_transform_storage() {
        use crate::style::PaintTransform;
        let mut tree = TaffyTree::new();
        let label: Arc<str> = Arc::from("Removed payload");
        let weak = Arc::downgrade(&label);
        let root = tree
            .new_leaf_with_context(
                taffy::Style {
                    size: Size {
                        width: taffy::Dimension::length(100.),
                        height: taffy::Dimension::length(40.),
                    },
                    ..Default::default()
                },
                Paint {
                    text: Some(label),
                    transform: vec![PaintTransform::Scale(1., 1.)],
                    ..Paint::default()
                },
            )
            .unwrap();
        tree.compute_layout(
            root,
            Size {
                width: AvailableSpace::MaxContent,
                height: AvailableSpace::MaxContent,
            },
        )
        .unwrap();
        let mut out = flatten_with_scroll(&tree, root, &HashMap::new());
        let storage = out[0].paint.transform.as_ptr();
        flatten_with_scroll_into(&tree, root, &HashMap::new(), &mut out);
        assert_eq!(storage, out[0].paint.transform.as_ptr());
        tree.get_node_context_mut(root).unwrap().text = None;
        flatten_with_scroll_into(&tree, root, &HashMap::new(), &mut out);
        assert!(weak.upgrade().is_none());
        let label: Arc<str> = Arc::from("Clipped payload");
        let weak = Arc::downgrade(&label);
        tree.get_node_context_mut(root).unwrap().text = Some(label);
        flatten_with_scroll_into(&tree, root, &HashMap::new(), &mut out);
        tree.set_node_context(root, None).unwrap();
        flatten_with_scroll_into(&tree, root, &HashMap::new(), &mut out);
        assert!(out.is_empty());
        assert!(
            weak.upgrade().is_none(),
            "unused tail slots must drop resources"
        );
    }

    #[test]
    fn retained_flatten_buffer_preserves_order_and_drops_removed_nodes() {
        let mut tree = TaffyTree::<Paint>::new();
        let style = taffy::Style {
            size: Size {
                width: taffy::Dimension::length(20.),
                height: taffy::Dimension::length(20.),
            },
            ..Default::default()
        };
        let a = tree
            .new_leaf_with_context(style.clone(), Paint::default())
            .unwrap();
        let b = tree
            .new_leaf_with_context(style.clone(), Paint::default())
            .unwrap();
        let c = tree
            .new_leaf_with_context(style.clone(), Paint::default())
            .unwrap();
        let root = tree.new_with_children(style, &[a, b, c]).unwrap();
        tree.set_node_context(root, Some(Paint::default())).unwrap();
        let mut out = Vec::with_capacity(32);
        let pointer = out.as_ptr();
        let mut tcx = TextContext::new();
        let offsets = HashMap::new();
        compute_and_walk_with_scroll_into(
            &mut tree,
            root,
            [100., 100.],
            &mut tcx,
            &offsets,
            &mut out,
        );
        assert_eq!(
            out.iter().map(|n| n.node_id).collect::<Vec<_>>(),
            [root, a, b, c]
        );
        // Force the sorting path; equal-z siblings must retain declaration order.
        tree.get_node_context_mut(a).unwrap().z_index = 1;
        flatten_with_scroll_into(&tree, root, &offsets, &mut out);
        assert_eq!(
            out.iter().map(|n| n.node_id).collect::<Vec<_>>(),
            [root, b, c, a]
        );
        tree.set_children(root, &[c, b]).unwrap();
        tree.get_node_context_mut(a).unwrap().z_index = 0;
        compute_and_walk_with_scroll_into(
            &mut tree,
            root,
            [100., 100.],
            &mut tcx,
            &offsets,
            &mut out,
        );
        assert_eq!(
            out.iter().map(|n| n.node_id).collect::<Vec<_>>(),
            [root, c, b]
        );
        assert_eq!(out.as_ptr(), pointer);
        assert_eq!(out.capacity(), 32);
        tree.get_node_context_mut(b).unwrap().opacity = 0.25;
        flatten_with_scroll_into(&tree, root, &offsets, &mut out);
        assert_eq!(out.last().unwrap().paint.opacity, 0.25);
    }

    #[test]
    fn overlay_plane_orders_before_z_index() {
        use crate::style::OverlayPlane;

        let mut tree = TaffyTree::<Paint>::new();
        let leaf_style = taffy::Style {
            size: Size {
                width: taffy::Dimension::length(10.0),
                height: taffy::Dimension::length(10.0),
            },
            ..Default::default()
        };
        let modal = tree.new_leaf(leaf_style.clone()).unwrap();
        let content = tree.new_leaf(leaf_style).unwrap();
        let root = tree
            .new_with_children(
                taffy::Style {
                    size: Size {
                        width: taffy::Dimension::length(100.0),
                        height: taffy::Dimension::length(100.0),
                    },
                    ..Default::default()
                },
                &[modal, content],
            )
            .unwrap();
        tree.set_node_context(root, Some(Paint::default())).unwrap();
        tree.set_node_context(
            content,
            Some(Paint {
                overlay_plane: OverlayPlane::Content,
                z_index: 10_000,
                ..Paint::default()
            }),
        )
        .unwrap();
        tree.set_node_context(
            modal,
            Some(Paint {
                overlay_plane: OverlayPlane::Modal,
                z_index: -10_000,
                ..Paint::default()
            }),
        )
        .unwrap();

        let mut text = TextContext::new();
        let placed =
            compute_and_walk_with_scroll(&mut tree, root, 100.0, 100.0, &mut text, &HashMap::new());
        assert_eq!(
            placed.iter().map(|node| node.node_id).collect::<Vec<_>>(),
            vec![root, content, modal]
        );
    }
}
