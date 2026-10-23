//! Build a retained Wabou paint scene from the flattened layout list.

use std::sync::Arc;

use crate::{PaintScene, Scene};
use vello_common::kurbo::{Affine, Rect, Stroke};
use vello_common::peniko::{BlendMode, Color, Fill};

use crate::layout::{PlacedNode, SubtreeEvent, subtree_events};
use crate::scrollbar::{ScrollAxis, thumb as scrollbar_thumb, track as scrollbar_track};
use crate::style::{CornerRadii, IrLength, PaintTransform, Shadow};
use crate::text::TextContext;

/// Resolve the node-local static CSS and runtime affine transforms separately.
pub fn resolve_local_transforms(node: &PlacedNode) -> (Affine, Affine) {
    let [x0, y0, x1, y1] = node.rect;
    let resolve = |length: &IrLength, size: f32| match length {
        IrLength::Px { value } => *value as f64,
        IrLength::Percent { value } => (*value * size) as f64,
        IrLength::Auto => 0.0,
    };
    let mut static_transform = Affine::IDENTITY;
    for transform in &node.paint.transform {
        static_transform *= match transform {
            PaintTransform::Translate(x, y) => {
                Affine::translate((resolve(x, x1 - x0), resolve(y, y1 - y0)))
            }
            PaintTransform::Scale(x, y) => Affine::scale_non_uniform(*x as f64, *y as f64),
            PaintTransform::Rotate(angle) => Affine::rotate(*angle as f64),
            PaintTransform::Skew(x, y) => Affine::skew(*x as f64, *y as f64),
            PaintTransform::Matrix(matrix) => Affine::new(matrix.map(f64::from)),
        };
    }
    let presented_transform = node
        .paint
        .motion_transform
        .or(node.paint.runtime_transform)
        .map_or(Affine::IDENTITY, |matrix| {
            Affine::new(matrix.map(f64::from))
        });
    (static_transform, presented_transform)
}

/// Resolve static CSS and host-driven runtime state into window coordinates.
pub fn resolve_node_transform(node: &PlacedNode, parent_transform: Affine) -> Affine {
    let [x0, y0, x1, y1] = node.rect;
    let rect = Rect::new(x0 as f64, y0 as f64, x1 as f64, y1 as f64);
    let resolve = |length: &IrLength, size: f32| match length {
        IrLength::Px { value } => f64::from(*value),
        IrLength::Percent { value } => f64::from(*value * size),
        IrLength::Auto => 0.0,
    };
    let (static_transform, runtime_transform) = resolve_local_transforms(node);
    let origin = vello_common::kurbo::Vec2::new(
        rect.x0 + resolve(&node.paint.transform_origin[0], x1 - x0),
        rect.y0 + resolve(&node.paint.transform_origin[1], y1 - y0),
    );
    parent_transform
        * Affine::translate(origin)
        * static_transform
        * runtime_transform
        * Affine::translate(-origin)
}

fn widget_clip(node: &PlacedNode) -> Option<([f32; 4], CornerRadii)> {
    let [x0, y0, x1, y1] = node.rect;
    let [top, right, bottom, left] = node.border_widths;
    let radii = node.paint.border_radii;
    if radii.max() > 0.0 {
        let border_inset = top.max(right).max(bottom).max(left);
        return Some((
            [x0 + left, y0 + top, x1 - right, y1 - bottom],
            radii.inset(border_inset),
        ));
    }
    node.own_clip.map(|clip| (clip, node.own_clip_radii))
}

fn append_widget(scene: &mut Scene, widget: &Scene, transform: Affine) {
    // Widget fragments are already clipped in content-local coordinates by
    // PaintContext. Wrapping an appended fragment in a second rounded layer
    // duplicates clip and draw-info encoding on every frame, including the
    // transient scene produced for IME preedit text.
    append_fragment(scene, widget, Some(transform));
}

/// Append one retained backend-neutral scene fragment.
fn append_fragment(scene: &mut Scene, fragment: &Scene, transform: Option<Affine>) {
    scene.append_scene(fragment.clone(), transform.unwrap_or(Affine::IDENTITY));
}

fn shadow_geometry(rect: Rect, node_radius: f64, shadow: &Shadow) -> (Rect, f64, f64) {
    let spread = f64::from(shadow.spread);
    let shadow_rect = Rect::new(
        rect.x0 + f64::from(shadow.offset_x) - spread,
        rect.y0 + f64::from(shadow.offset_y) - spread,
        rect.x1 + f64::from(shadow.offset_x) + spread,
        rect.y1 + f64::from(shadow.offset_y) + spread,
    );
    let radius = shadow
        .radius
        .map(f64::from)
        .unwrap_or_else(|| (node_radius + spread).max(0.0));
    (shadow_rect, radius, f64::from(shadow.std_dev))
}

fn visual_bounds(node: &PlacedNode) -> Rect {
    let [x0, y0, x1, y1] = node.rect;
    let border_box = Rect::new(x0 as f64, y0 as f64, x1 as f64, y1 as f64);
    let outline = f64::from((node.paint.outline_width + node.paint.outline_offset.abs()).max(0.0));
    let mut bounds = border_box.inflate(outline, outline);
    for shadow in &node.paint.shadows {
        let (shadow_rect, _, std_dev) =
            shadow_geometry(border_box, f64::from(node.paint.border_radii.max()), shadow);
        let blur_extent = std_dev * 3.0;
        bounds = bounds.union(shadow_rect.inflate(blur_extent, blur_extent));
    }
    bounds
}

fn paints_viewport(node: &PlacedNode, transform: Affine, viewport: Rect) -> bool {
    let bounds = transform.transform_rect_bbox(visual_bounds(node));
    bounds.x1 >= viewport.x0
        && bounds.x0 <= viewport.x1
        && bounds.y1 >= viewport.y0
        && bounds.y0 <= viewport.y1
}

fn draw_scrollbars(scene: &mut Scene, node: &PlacedNode, transform: Affine) {
    if node.scroll.opacity <= 0.0 {
        return;
    }
    let style = node.paint.scrollbar_style();
    let fade = |color: Color| {
        let rgba = color.to_rgba8();
        Color::from_rgba8(
            rgba.r,
            rgba.g,
            rgba.b,
            ((rgba.a as f32) * node.scroll.opacity) as u8,
        )
    };
    for axis in [ScrollAxis::Vertical, ScrollAxis::Horizontal] {
        if let Some(track) = scrollbar_track(node, axis) {
            scene.fill(
                Fill::NonZero,
                transform,
                fade(style.track_color),
                None,
                &track,
            );
        }
        let Some(rect) = scrollbar_thumb(node, axis) else {
            continue;
        };
        let radius = if style.radius < 0.0 {
            match axis {
                ScrollAxis::Horizontal => rect.height() * 0.5,
                ScrollAxis::Vertical => rect.width() * 0.5,
            }
        } else {
            f64::from(style.radius)
        };
        let color = match node.scroll.interaction {
            2 => style.active_color,
            1 => style.hover_color,
            _ => style.thumb_color,
        };
        scene.fill(
            Fill::NonZero,
            transform,
            fade(color),
            None,
            &rect.to_rounded_rect(radius),
        );
    }
}

fn draw_node_box(scene: &mut Scene, node: &PlacedNode, transform: Affine) {
    let [x0, y0, x1, y1] = node.rect;
    let rect = Rect::new(f64::from(x0), f64::from(y0), f64::from(x1), f64::from(y1));
    let radii = node.paint.border_radii;
    for shadow in &node.paint.shadows {
        let (shadow_rect, radius, std_dev) = shadow_geometry(rect, f64::from(radii.max()), shadow);
        scene.draw_box_shadow(transform, shadow_rect, shadow.color, radius, std_dev);
    }
    if let Some(background) = node.paint.background {
        scene.fill(
            Fill::NonZero,
            transform,
            background,
            None,
            &rect.to_rounded_rect(radii.as_f64_tuple()),
        );
    }
    if node.paint.outline_width > 0.0
        && let Some(color) = node.paint.outline_color
    {
        let width = f64::from(node.paint.outline_width);
        let expansion = f64::from(node.paint.outline_offset) + width * 0.5;
        let outline = Rect::new(
            f64::from(x0) - expansion,
            f64::from(y0) - expansion,
            f64::from(x1) + expansion,
            f64::from(y1) + expansion,
        );
        scene.stroke(
            &Stroke::new(width),
            transform,
            color,
            None,
            &outline.to_rounded_rect(radii.expand(expansion as f32).as_f64_tuple()),
        );
    }
    let Some((_, border_color)) = node.paint.border else {
        return;
    };
    let [top, right, bottom, left] = node.border_widths;
    if top > 0.0 && top == right && top == bottom && top == left {
        let width = f64::from(top);
        let half = width / 2.0;
        let inset = Rect::new(
            f64::from(x0) + half,
            f64::from(y0) + half,
            f64::from(x1) - half,
            f64::from(y1) - half,
        );
        scene.stroke(
            &Stroke::new(width),
            transform,
            border_color,
            None,
            &inset.to_rounded_rect(radii.inset(half as f32).as_f64_tuple()),
        );
        return;
    }
    // Side-specific utility borders must not turn into a uniform rectangle.
    // Clip their straight center lines to the outer rounded border box. Without
    // this, a single `border-t`/`border-r` reaches the rectangular corner even
    // though the node background and contents follow `border-radius`.
    let clips_to_rounded_border = radii.max() > 0.0;
    if clips_to_rounded_border {
        scene.push_clip_layer(transform, &rect.to_rounded_rect(radii.as_f64_tuple()));
    }
    let sides = [
        (top, (x0, y0 + top * 0.5), (x1, y0 + top * 0.5)),
        (right, (x1 - right * 0.5, y0), (x1 - right * 0.5, y1)),
        (bottom, (x0, y1 - bottom * 0.5), (x1, y1 - bottom * 0.5)),
        (left, (x0 + left * 0.5, y0), (x0 + left * 0.5, y1)),
    ];
    for (width, from, to) in sides {
        if width > 0.0 {
            let line = vello_common::kurbo::Line::new(
                (f64::from(from.0), f64::from(from.1)),
                (f64::from(to.0), f64::from(to.1)),
            );
            scene.stroke(
                &Stroke::new(f64::from(width)),
                transform,
                border_color,
                None,
                &line,
            );
        }
    }
    if clips_to_rounded_border {
        scene.pop_layer();
    }
}

fn draw_svg(scene: &mut Scene, node: &PlacedNode, transform: Affine) {
    let Some(svg) = &node.paint.svg else {
        return;
    };
    let [svg_width, svg_height] = svg.size();
    let [x0, y0, x1, y1] = node.rect;
    let width = (x1 - x0).max(0.0);
    let height = (y1 - y0).max(0.0);
    if svg_width <= 0.0 || svg_height <= 0.0 || width <= 0.0 || height <= 0.0 {
        return;
    }
    let scale = f64::from((width / svg_width).min(height / svg_height));
    let dx = f64::from(x0) + (f64::from(width) - f64::from(svg_width) * scale) * 0.5;
    let dy = f64::from(y0) + (f64::from(height) - f64::from(svg_height) * scale) * 0.5;
    scene.draw_svg(
        svg.document().clone(),
        transform * Affine::translate((dx, dy)) * Affine::scale(scale),
    );
}

fn draw_vector_path(scene: &mut Scene, node: &PlacedNode, transform: Affine) {
    let Some(vector) = &node.paint.vector_path else {
        return;
    };
    let local = transform
        * Affine::translate((
            f64::from(node.content_origin[0]),
            f64::from(node.content_origin[1]),
        ));
    if let Some(fill) = vector.fill {
        scene.fill(
            if vector.even_odd {
                Fill::EvenOdd
            } else {
                Fill::NonZero
            },
            local,
            fill,
            None,
            vector.path.as_ref(),
        );
    }
    if let Some(stroke) = vector.stroke {
        scene.stroke(
            &vector.stroke_style,
            local,
            stroke,
            None,
            vector.path.as_ref(),
        );
    }
}

fn draw_image(scene: &mut Scene, node: &PlacedNode, transform: Affine) {
    let Some(image) = &node.paint.image else {
        return;
    };
    let [image_width, image_height] = image.size();
    let [x0, y0, x1, y1] = node.rect;
    let width = (x1 - x0).max(0.0);
    let height = (y1 - y0).max(0.0);
    if image_width <= 0.0 || image_height <= 0.0 || width <= 0.0 || height <= 0.0 {
        return;
    }
    let scale = f64::from((width / image_width).min(height / image_height));
    let dx = f64::from(x0) + (f64::from(width) - f64::from(image_width) * scale) * 0.5;
    let dy = f64::from(y0) + (f64::from(height) - f64::from(image_height) * scale) * 0.5;
    scene.draw_image(
        image.brush().into(),
        transform * Affine::translate((dx, dy)) * Affine::scale(scale),
    );
}

fn draw_text(
    scene: &mut Scene,
    node: &PlacedNode,
    tcx: &mut TextContext,
    transform: Affine,
    device_scale: f64,
) {
    let Some(layout) = layout_node_text(tcx, node) else {
        return;
    };

    let origin = Affine::translate((
        f64::from(node.content_origin[0]),
        f64::from(node.content_origin[1]),
    ));
    for &[x0, y0, x1, y1] in node.paint.selection_rects.iter() {
        scene.fill(
            Fill::NonZero,
            transform * origin,
            Color::from_rgba8(59, 130, 246, 105),
            None,
            &Rect::new(f64::from(x0), f64::from(y0), f64::from(x1), f64::from(y1)),
        );
    }
    let text_transform = transform * origin;
    draw_text_layout_into(
        scene,
        tcx,
        &layout,
        Affine::IDENTITY,
        text_transform,
        device_scale,
    );
}

/// Paint a shaped layout as hinted Vello glyph runs at the final device scale.
pub fn draw_text_layout_into(
    scene: &mut Scene,
    tcx: &mut TextContext,
    layout: &Arc<parley::Layout<[u8; 4]>>,
    destination_to_output: Affine,
    text_to_output: Affine,
    device_scale: f64,
) {
    draw_outline_text(
        scene,
        tcx,
        layout,
        destination_to_output.inverse() * text_to_output,
        device_scale,
    );
}

/// Build the exact text layout used to paint one placed node.
///
/// DevTools uses this same cached path so diagnostics describe the glyph runs
/// that painting will consume rather than independently approximating them.
pub fn layout_node_text(
    tcx: &mut TextContext,
    node: &PlacedNode,
) -> Option<Arc<parley::Layout<[u8; 4]>>> {
    let text = node.paint.text.as_ref()?;
    Some(if node.paint.text_max_lines > 0 {
        crate::text::layout_text_styled_clamped(
            tcx,
            text.clone(),
            node.paint.font_size,
            node.paint.font_weight,
            node.paint.font_italic,
            node.paint.letter_spacing,
            node.paint.line_height,
            node.paint.text_align,
            crate::text::brush_for_color(node.paint.text_color),
            node.paint.text_runs.clone(),
            node.paint.font_family.as_ref(),
            (node.paint.wrap_text || node.paint.text_ellipsis || node.paint.text_max_lines > 0)
                .then_some(node.content_size[0].max(0.0)),
            node.paint.text_max_lines,
            !node.paint.text_clip_lines,
        )
    } else {
        crate::text::layout_text_styled_overflow(
            tcx,
            text.clone(),
            node.paint.font_size,
            node.paint.font_weight,
            node.paint.font_italic,
            node.paint.letter_spacing,
            node.paint.line_height,
            node.paint.text_align,
            crate::text::brush_for_color(node.paint.text_color),
            node.paint.text_runs.clone(),
            node.paint.font_family.as_ref(),
            (node.paint.wrap_text || node.paint.text_ellipsis)
                .then_some(node.content_size[0].max(0.0)),
            node.paint.text_ellipsis,
        )
    })
}

fn draw_outline_text(
    scene: &mut Scene,
    tcx: &mut TextContext,
    layout: &Arc<parley::Layout<[u8; 4]>>,
    text_transform: Affine,
    device_scale: f64,
) {
    let transform = text_transform * Affine::scale(device_scale.recip());
    // Direct encoding avoids a retained-fragment issue on Metal. It also
    // keeps transformed/faux-style fallback text at the native font weight
    // instead of geometrically emboldening it.
    tcx.draw_native_weight_layout_into(scene, layout, transform, device_scale);
}

/// Replay the prior application scene into a GPU-filtered layer, then clip its
/// result to the glass panel. Foreground commands are recorded afterwards.
/// This prototype trades replay work for a backend-neutral command stream;
/// it does not claim a persistent blurred-texture cache.
fn draw_backdrop(scene: &mut Scene, node: &PlacedNode, transform: Affine, open_layers: usize) {
    let sigma = node.paint.backdrop_blur;
    if !sigma.is_finite() || sigma <= 0. || scene.commands.is_empty() {
        return;
    }
    let mut background = scene.clone();
    // Complete the snapshot without closing the live scene's ancestor layers.
    for _ in 0..open_layers {
        background.pop_layer();
    }
    let [x0, y0, x1, y1] = node.rect.map(f64::from);
    let clip = Rect::new(x0, y0, x1, y1).to_rounded_rect(node.paint.border_radii.as_f64_tuple());
    scene.push_layer(BlendMode::default(), 1., transform, &clip, None, None);
    let filter = vello_common::filter_effects::Filter::from_function(
        vello_common::filter_effects::FilterFunction::Blur {
            radius: sigma.min(64.),
        },
    );
    // Filter only the card's own extent: the output is confined by the outer
    // rounded clip anyway, so a viewport-wide filter layer would blur the whole
    // frame at vsync cost for a panel-sized result.
    scene.push_layer(
        BlendMode::default(),
        1.,
        transform,
        &clip,
        Some(Arc::new(filter)),
        None,
    );
    scene.append_scene(background, Affine::IDENTITY);
    scene.pop_layer();
    scene.pop_layer();
}

enum Layer {
    Clip { depth: usize },
    Opacity { depth: usize },
}

impl Layer {
    fn depth(&self) -> usize {
        match self {
            Self::Clip { depth } | Self::Opacity { depth } => *depth,
        }
    }
}

/// Paint `nodes` into `scene` over a `base_color` background. Text nodes are
/// laid out with parley and rendered as vello glyph runs.
pub fn build_scene(
    scene: &mut Scene,
    nodes: &[PlacedNode],
    tcx: &mut TextContext,
    width: u32,
    height: u32,
    base_color: Color,
) {
    build_scene_scaled(scene, nodes, tcx, width, height, base_color, 1.0);
}

/// Build a logical-pixel scene and transform it to physical pixels at encode
/// time. Layout, hit testing and font sizes stay in CSS-like logical units.
pub fn build_scene_scaled(
    scene: &mut Scene,
    nodes: &[PlacedNode],
    tcx: &mut TextContext,
    width: u32,
    height: u32,
    base_color: Color,
    device_scale: f64,
) {
    build_scene_scaled_layers(
        scene,
        &[nodes],
        tcx,
        width,
        height,
        base_color,
        device_scale,
    );
}

/// Command ranges per placed node from one full encode pass.
///
/// A node's range covers exactly the commands it emitted at its own Enter
/// (its box, own layers, clips, text — not its descendants'). On a paint-only
/// frame the tree shape, depth ordering and every ancestor transform are
/// unchanged, so re-encoding one node's range and splicing it into the
/// previous command buffer reproduces a full rebuild.
#[derive(Default, Clone)]
pub struct SceneRanges {
    /// Enter segment plus any Exit-tail segments (scrollbars), in order.
    ranges: std::collections::HashMap<taffy::NodeId, Vec<std::ops::Range<usize>>>,
    /// Whether the full pass emitted a base fill before any node command,
    /// and the color it used — an incremental pass must not reuse a base
    /// fill recorded under a different theme.
    base_fill: Option<Color>,
    /// Viewport size recorded at encode time; the incremental pass must
    /// cull against the same rectangle.
    viewport: (u32, u32),
    /// Nodes whose encoding reads global scene state (backdrop blur clones
    /// the live command prefix) cannot be re-encoded in isolation.
    unsafe_nodes: std::collections::HashSet<taffy::NodeId>,
}

impl SceneRanges {
    /// Whether an incremental reassembly would carry the same base fill.
    pub fn base_color_matches(&self, current: Color) -> bool {
        self.base_fill.is_none_or(|recorded| recorded == current)
    }
    fn record(&mut self, node: &PlacedNode, range: std::ops::Range<usize>) {
        if node.paint.backdrop_blur > 0. {
            self.unsafe_nodes.insert(node.node_id);
        }
        self.ranges.insert(node.node_id, vec![range]);
    }
    fn append_tail(&mut self, node: &PlacedNode, range: std::ops::Range<usize>) {
        if !range.is_empty()
            && let Some(segments) = self.ranges.get_mut(&node.node_id)
        {
            segments.push(range);
        }
    }
    /// Whether a node's range can be spliced without a full rebuild.
    fn splicable(&self, node: &PlacedNode) -> bool {
        !self.unsafe_nodes.contains(&node.node_id) && self.ranges.contains_key(&node.node_id)
    }
}

/// Encode independent paint-ordered root layers without concatenating their node lists.
pub fn build_scene_scaled_layers(
    scene: &mut Scene,
    roots: &[&[PlacedNode]],
    tcx: &mut TextContext,
    width: u32,
    height: u32,
    base_color: Color,
    device_scale: f64,
) {
    build_scene_tracked(
        scene,
        roots,
        tcx,
        width,
        height,
        base_color,
        device_scale,
        None,
    )
}

/// Full encode, optionally recording per-node command ranges for the
/// incremental re-encoder.
#[allow(clippy::too_many_arguments)]
pub fn build_scene_tracked(
    scene: &mut Scene,
    roots: &[&[PlacedNode]],
    tcx: &mut TextContext,
    width: u32,
    height: u32,
    base_color: Color,
    device_scale: f64,
    mut ranges: Option<&mut SceneRanges>,
) {
    if let Some(ranges) = ranges.as_deref_mut() {
        ranges.viewport = (width, height);
        ranges.base_fill = (base_color != Color::TRANSPARENT).then_some(base_color);
    }
    scene.reset();
    let device = Affine::scale(device_scale);

    let bg = Rect::new(0.0, 0.0, width as f64, height as f64);
    let viewport = bg;
    if base_color != Color::TRANSPARENT {
        scene.fill(Fill::NonZero, device, base_color, None, &bg);
    }

    // Ancestor transforms keyed by depth. Enter/Exit events are balanced
    // preorder, so a node's parent is the deepest still-open entry and its
    // own transform stays on the stack until its Exit: no per-node hashing
    // on the scene-encoding path.
    let mut transforms: Vec<(usize, Affine)> = Vec::new();
    let mut layers = Vec::new();
    for event in roots.iter().flat_map(|nodes| subtree_events(nodes)) {
        let SubtreeEvent::Enter(n) = event else {
            let SubtreeEvent::Exit(n) = event else {
                unreachable!()
            };
            // All descendants have exited and popped their entries, so this
            // node's own transform is on top of the stack.
            if let Some(&(_, transform)) = transforms.last().filter(|(depth, _)| *depth == n.depth)
                && paints_viewport(n, transform, viewport)
            {
                let tail_start = scene.commands.len();
                draw_scrollbars(scene, n, device * transform);
                if let Some(ranges) = ranges.as_deref_mut() {
                    // Scrollbars paint at Exit, after all descendants; they
                    // belong to the node's own range as a trailing segment.
                    ranges.append_tail(n, tail_start..scene.commands.len());
                }
            }
            while transforms
                .last()
                .is_some_and(|(depth, _)| *depth >= n.depth)
            {
                transforms.pop();
            }
            while layers
                .last()
                .is_some_and(|layer: &Layer| layer.depth() >= n.depth)
            {
                scene.pop_layer();
                layers.pop();
            }
            continue;
        };
        let has_rounded_corners = n.paint.border_radii.max() > 0.0;
        while transforms
            .last()
            .is_some_and(|(depth, _)| *depth >= n.depth)
        {
            transforms.pop();
        }
        let parent_transform = transforms
            .last()
            .map_or(Affine::IDENTITY, |(_, transform)| *transform);
        let css_transform = resolve_node_transform(n, parent_transform);
        transforms.push((n.depth, css_transform));
        let node_transform = device * css_transform;
        let paints_node = paints_viewport(n, css_transform, viewport);

        let range_start = scene.commands.len();
        let enc = EncodeCtx {
            node: n,
            node_transform,
            paints_node,
            has_rounded_corners,
            device,
            bg: &bg,
            viewport,
            device_scale,
        };
        encode_node(scene, tcx, &enc, &mut layers);
        if let Some(ranges) = ranges.as_deref_mut() {
            ranges.record(n, range_start..scene.commands.len());
        }
    }
    while layers.pop().is_some() {
        scene.pop_layer();
    }
}

/// Per-Enter encoding inputs; split out so the incremental re-encoder can
/// drive it for a single node with the same ancestor transform.
struct EncodeCtx<'a> {
    node: &'a PlacedNode,
    node_transform: Affine,
    paints_node: bool,
    has_rounded_corners: bool,
    device: Affine,
    bg: &'a Rect,
    viewport: Rect,
    device_scale: f64,
}
fn encode_node(
    scene: &mut Scene,
    tcx: &mut TextContext,
    enc: &EncodeCtx<'_>,
    layers: &mut Vec<Layer>,
) {
    let EncodeCtx {
        node: n,
        node_transform,
        paints_node,
        has_rounded_corners,
        device,
        bg,
        viewport,
        device_scale,
    } = enc;
    let n: &PlacedNode = n;
    let device = *device;
    let bg = *bg;
    let viewport = *viewport;
    let device_scale = *device_scale;
    let paints_node = *paints_node;
    let has_rounded_corners = *has_rounded_corners;

    // Keep ancestor transform, opacity, and clip state even when this
    // node's own visual bounds are offscreen: a transformed descendant can
    // still enter the viewport. Only node-owned paint is culled here.
    let opacity = n.paint.opacity * n.paint.motion_opacity.unwrap_or(1.0).clamp(0.0, 1.0);
    if opacity < 1.0 {
        scene.push_layer(BlendMode::default(), opacity, device, &bg, None, None);
        layers.push(Layer::Opacity { depth: n.depth });
    }

    if paints_node {
        draw_backdrop(scene, n, *node_transform, layers.len());
        draw_node_box(scene, n, *node_transform);
    }

    if let Some([cx0, cy0, cx1, cy1]) = n.own_clip {
        let extent = viewport.width().max(viewport.height()) * 4.0 + 4096.0;
        let finite = |value: f32| {
            if value == f32::NEG_INFINITY {
                -extent
            } else if value == f32::INFINITY {
                extent
            } else {
                f64::from(value)
            }
        };
        let clip_rect = Rect::new(finite(cx0), finite(cy0), finite(cx1), finite(cy1));
        scene.push_clip_layer(
            *node_transform,
            &clip_rect.to_rounded_rect(n.own_clip_radii.as_f64_tuple()),
        );
        layers.push(Layer::Clip { depth: n.depth });
    }

    if paints_node {
        draw_svg(scene, n, *node_transform);
        draw_vector_path(scene, n, *node_transform);
        draw_image(scene, n, *node_transform);
    }

    if paints_node && let Some(ws) = &n.paint.widget {
        // Keep rounded widget clipping inside the fragment itself. Some
        // GPU backends do not reliably carry a parent clip across an
        // appended scene at HiDPI; local encoding also avoids mixing
        // absolute logical and fragment coordinates.
        let outer_widget_clip = (!has_rounded_corners).then(|| widget_clip(n)).flatten();
        if let Some(([cx0, cy0, cx1, cy1], radii)) = outer_widget_clip {
            let widget_clip = Rect::new(
                cx0.max(0.0) as f64,
                cy0.max(0.0) as f64,
                cx1.min(viewport.width() as f32) as f64,
                cy1.min(viewport.height() as f32) as f64,
            );
            scene.push_clip_layer(
                *node_transform,
                &widget_clip.to_rounded_rect(radii.as_f64_tuple()),
            );
        }
        append_widget(
            scene,
            ws,
            *node_transform
                * Affine::translate((n.content_origin[0] as f64, n.content_origin[1] as f64)),
        );
        if outer_widget_clip.is_some() {
            scene.pop_layer();
        }
    }

    if paints_node {
        draw_text(scene, n, tcx, *node_transform, device_scale);
    }
}

/// Re-encode only the dirty nodes into a previous scene.
///
/// The previous frame's command buffer and its `SceneRanges` describe a
/// paint-only world: unchanged tree, unchanged geometry, unchanged ancestor
/// transforms. The whole command stream is reassembled in traversal order,
/// copying each node's recorded segments verbatim and re-encoding only the
/// dirty ones. Copying is a memcpy per command; encoding is the expensive
/// part, so the frame cost is O(total commands) memcpy + O(dirty) encode.
/// Returns false when a dirty node cannot be re-encoded in isolation —
/// callers rebuild fully instead.
pub fn build_scene_incremental(
    scene: &mut Scene,
    roots: &[&[PlacedNode]],
    previous: &Scene,
    ranges: &SceneRanges,
    dirty: &[taffy::NodeId],
    tcx: &mut TextContext,
    device_scale: f64,
) -> bool {
    if dirty.is_empty() {
        return true;
    }
    let device = Affine::scale(device_scale);
    let viewport = Rect::new(
        0.,
        0.,
        f64::from(ranges.viewport.0),
        f64::from(ranges.viewport.1),
    );
    let dirty: std::collections::HashSet<taffy::NodeId> = dirty.iter().copied().collect();
    // A placed entry per node id for ancestor walks and dirty resolution.
    let index: std::collections::HashMap<taffy::NodeId, &PlacedNode> = roots
        .iter()
        .flat_map(|n| n.iter())
        .map(|n| (n.node_id, n))
        .collect();
    for &id in &dirty {
        let Some(node) = index.get(&id) else {
            return false;
        };
        if !ranges.splicable(node) {
            return false;
        }
    }
    let old = previous.commands.as_slice();
    let mut next = Scene::new();
    // The base fill the full pass emitted before any node.
    if ranges.base_fill.is_some() {
        if old.is_empty() {
            return false;
        }
        next.commands.push(old[0].clone());
    }
    for nodes in roots {
        for event in subtree_events(nodes) {
            let (node, is_enter) = match event {
                SubtreeEvent::Enter(n) => (n, true),
                SubtreeEvent::Exit(n) => (n, false),
            };
            let Some(segments) = ranges.ranges.get(&node.node_id) else {
                return false;
            };
            if is_enter {
                if dirty.contains(&node.node_id) {
                    // Re-encode this node in isolation: the ancestor transform
                    // is unchanged on a paint-only frame, so recomputing it
                    // from the parent chain reproduces the recorded one.
                    let mut parent_transform = Affine::IDENTITY;
                    let mut chain: Vec<&PlacedNode> = Vec::new();
                    let mut current = node.parent_node_id;
                    while let Some(id) = current
                        && let Some(parent) = index.get(&id)
                    {
                        chain.push(parent);
                        current = parent.parent_node_id;
                    }
                    for parent in chain.iter().rev() {
                        parent_transform = resolve_node_transform(parent, parent_transform);
                    }
                    let css_transform = resolve_node_transform(node, parent_transform);
                    let node_transform = device * css_transform;
                    let mut scratch = Scene::new();
                    let enc = EncodeCtx {
                        node,
                        node_transform,
                        paints_node: paints_viewport(node, css_transform, viewport),
                        has_rounded_corners: node.paint.border_radii.max() > 0.0,
                        device,
                        bg: &viewport,
                        viewport,
                        device_scale,
                    };
                    let mut layers = Vec::new();
                    encode_node(&mut scratch, tcx, &enc, &mut layers);
                    while layers.pop().is_some() {
                        scratch.pop_layer();
                    }
                    next.commands.extend(scratch.commands.iter().cloned());
                } else {
                    // Enter segment is the first recorded range.
                    let segment = &segments[0];
                    if segment.end > old.len() {
                        return false;
                    }
                    next.commands
                        .extend_from_slice(&old[segment.start..segment.end]);
                }
            } else if !dirty.contains(&node.node_id) {
                // Exit-tail segments (scrollbars) follow all descendants.
                // The first segment was already consumed at Enter.
                for segment in segments.iter().skip(1) {
                    if segment.end > old.len() {
                        return false;
                    }
                    next.commands
                        .extend_from_slice(&old[segment.start..segment.end]);
                }
            } else {
                // Dirty nodes re-emit their own Exit tail (scrollbars) with
                // the node state just re-encoded.
                let mut parent_transform = Affine::IDENTITY;
                let mut chain: Vec<&PlacedNode> = Vec::new();
                let mut current = node.parent_node_id;
                while let Some(id) = current
                    && let Some(parent) = index.get(&id)
                {
                    chain.push(parent);
                    current = parent.parent_node_id;
                }
                for parent in chain.iter().rev() {
                    parent_transform = resolve_node_transform(parent, parent_transform);
                }
                let transform = resolve_node_transform(node, parent_transform);
                draw_scrollbars(&mut next, node, device * transform);
            }
        }
    }
    *scene = next;
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::style::Shadow;

    #[test]
    fn native_motion_replaces_the_authored_target_transform() {
        let mut tree = taffy::TaffyTree::<()>::new();
        let node_id = tree.new_leaf(taffy::Style::default()).unwrap();
        let mut node = PlacedNode {
            node_id,
            parent_node_id: None,
            depth: 0,
            rect: [0.0; 4],
            content_origin: [0.0; 2],
            content_size: [0.0; 2],
            clip: None,
            clip_radius: 0.0,
            clip_radii: CornerRadii::default(),
            clip_depth: None,
            own_clip: None,
            own_clip_radius: 0.0,
            own_clip_radii: CornerRadii::default(),
            border_widths: [0.0; 4],
            scroll: Default::default(),
            paint: Default::default(),
        };
        node.paint.runtime_transform = Some([1.0, 0.0, 0.0, 1.0, 16.0, 0.0]);
        node.paint.motion_transform = Some([1.0, 0.0, 0.0, 1.0, 8.0, 0.0]);

        let (_, presented) = resolve_local_transforms(&node);

        assert_eq!(presented, Affine::translate((8.0, 0.0)));
    }

    #[test]
    fn shadow_geometry_preserves_vello_parameters() {
        let shadow = Shadow {
            offset_x: 6.0,
            offset_y: -4.0,
            spread: 3.0,
            std_dev: 7.5,
            color: Color::BLACK,
            radius: None,
        };
        let (rect, radius, std_dev) =
            shadow_geometry(Rect::new(10.0, 20.0, 110.0, 120.0), 8.0, &shadow);

        assert_eq!(rect, Rect::new(13.0, 13.0, 119.0, 119.0));
        assert_eq!(radius, 11.0);
        assert_eq!(std_dev, 7.5);
    }
    #[test]
    fn shadow_radius_can_be_independent_from_node_radius_and_spread() {
        let shadow = Shadow {
            offset_x: 0.0,
            offset_y: 0.0,
            spread: -20.0,
            std_dev: 2.0,
            color: Color::BLACK,
            radius: Some(24.0),
        };
        let (_, radius, _) = shadow_geometry(Rect::new(0.0, 0.0, 100.0, 100.0), 8.0, &shadow);

        assert_eq!(radius, 24.0);
    }
}
