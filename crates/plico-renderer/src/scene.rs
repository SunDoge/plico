//! Build a retained Wabou paint scene from the flattened layout list.

use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use crate::{PaintScene, Scene};
use kurbo::{Affine, Rect, Stroke};
use peniko::{BlendMode, Color, Fill};

use crate::layout::PlacedNode;
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
    let origin = kurbo::Vec2::new(
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

// A transformed clip's bounding box is conservative: rounded corners and
// rotated edges may leave extra work, but cannot hide visible descendants.
fn clip_bounds(clip: [f32; 4], transform: Affine) -> Option<Rect> {
    let rect = Rect::new(
        clip[0] as f64,
        clip[1] as f64,
        clip[2] as f64,
        clip[3] as f64,
    );
    if clip.iter().all(|v| v.is_finite()) {
        return Some(transform.transform_rect_bbox(rect));
    }
    let [a, b, c, d, e, f] = transform.as_coeffs();
    if b != 0.0 || c != 0.0 {
        return None;
    }
    let axis = |lo: f64, hi: f64, scale: f64, offset: f64| {
        if scale == 0.0 {
            (offset, offset)
        } else {
            let x = lo * scale + offset;
            let y = hi * scale + offset;
            (x.min(y), x.max(y))
        }
    };
    let (x0, x1) = axis(rect.x0, rect.x1, a, e);
    let (y0, y1) = axis(rect.y0, rect.y1, d, f);
    Some(Rect::new(x0, y0, x1, y1))
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
            let line = kurbo::Line::new(
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
    if tcx.has_native_paragraphs() {
        let Some(request) = node_paragraph_request(tcx, node) else {
            return;
        };
        let origin = transform
            * Affine::translate((node.content_origin[0] as f64, node.content_origin[1] as f64));
        for &[x0, y0, x1, y1] in node.paint.selection_rects.iter() {
            scene.fill(
                Fill::NonZero,
                origin,
                Color::from_rgba8(59, 130, 246, 105),
                None,
                &Rect::new(x0 as f64, y0 as f64, x1 as f64, y1 as f64),
            );
        }
        // Measurements and hit-testing populate the host's shared paragraph cache.
        tcx.native_paragraph(request.clone());
        scene.draw_paragraph(request, origin);
        return;
    }
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

/// Declaration shared by host-native measurement, painting and selection.
pub fn node_paragraph_request(
    tcx: &TextContext,
    node: &PlacedNode,
) -> Option<Arc<crate::paragraph::ParagraphRequest>> {
    let width = (node.paint.wrap_text || node.paint.text_ellipsis || node.paint.text_max_lines > 0)
        .then_some(node.content_size[0].max(0.));
    Some(Arc::new(tcx.paragraph_request(&node.paint, width)?))
}

/// Ordinary text geometry from the selected backend.
pub fn layout_node_paragraph(
    tcx: &mut TextContext,
    node: &PlacedNode,
) -> Option<crate::paragraph::ParagraphLayout> {
    if tcx.has_native_paragraphs() {
        let request = node_paragraph_request(tcx, node)?;
        Some(crate::paragraph::ParagraphLayout::Native(
            tcx.native_paragraph(request)?,
        ))
    } else {
        Some(crate::paragraph::ParagraphLayout::Parley(layout_node_text(
            tcx, node,
        )?))
    }
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

/// Build a Parley layout for Vello or an explicit glyph-run consumer.
/// Host-native ordinary text uses [`layout_node_paragraph`] instead.
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
fn draw_backdrop(scene: &mut Scene, node: &PlacedNode, transform: Affine) {
    let sigma = node.paint.backdrop_blur;
    if !sigma.is_finite() || sigma <= 0. || scene.commands.is_empty() {
        return;
    }
    let mut background = scene.clone();
    // Complete the snapshot without closing the live scene. Current-node
    // layers are not the whole story: ancestor opacity and clips are still
    // open in the command stream and must be balanced in the cloned replay.
    let open_layers = scene.commands.iter().fold(0usize, |depth, command| {
        use crate::PaintCommand;
        match command {
            PaintCommand::PushLayer { .. } | PaintCommand::PushClip { .. } => depth + 1,
            PaintCommand::PopLayer => depth.saturating_sub(1),
            _ => depth,
        }
    });
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
    Clip,
    Opacity,
}

/// Paint `nodes` into `scene` over a `base_color` background. Text nodes are
/// use the host-selected paragraph engine.
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
    scene.reset();
    let device = Affine::scale(device_scale);
    let bg = Rect::new(0.0, 0.0, width as f64, height as f64);
    if base_color != Color::TRANSPARENT {
        scene.fill(Fill::NonZero, device, base_color, None, &bg);
    }
    let roots_for_cache = HashSet::new();
    let invalid = HashSet::new();
    let mut encoder = Encoder {
        tcx,
        device,
        bg,
        viewport: bg,
        device_scale,
        cache_roots: &roots_for_cache,
        invalid: &invalid,
        cache: None,
    };
    for nodes in roots {
        encoder.encode_nodes(scene, nodes, Affine::IDENTITY, bg);
    }
}

/// Reusable, independently encoded Region subtrees.
///
/// Unlike per-node command ranges, a clean subtree is appended as one retained
/// fragment. Reuse therefore does not copy its commands into a new flat Vec.
#[derive(Default)]
pub struct SceneCache {
    entries: HashMap<taffy::NodeId, CachedSubtree>,
    key: Option<(u32, u32, u64)>,
    /// Number of fragments appended without re-encoding this frame.
    pub reused: usize,
    /// Number of Region fragments encoded this frame.
    pub built: usize,
    /// Number of placed nodes whose own paint was encoded this frame.
    pub encoded_nodes: usize,
}
struct CachedSubtree {
    scene: Scene,
    parent_transform: Affine,
    inherited_clip: Rect,
}
impl SceneCache {
    /// Drop every retained fragment and its surface context.
    pub fn clear(&mut self) {
        self.entries.clear();
        self.key = None;
    }
    /// Drop one unmounted subtree without disturbing independent Regions.
    pub fn remove(&mut self, id: taffy::NodeId) {
        self.entries.remove(&id);
    }
    fn begin_frame(&mut self, width: u32, height: u32, scale: f64) {
        self.reused = 0;
        self.built = 0;
        self.encoded_nodes = 0;
        let key = (width, height, scale.to_bits());
        if self.key != Some(key) {
            self.entries.clear();
            self.key = Some(key);
        }
    }
}

/// Surface parameters shared by every fragment encoded in a frame.
#[derive(Clone, Copy)]
pub struct SceneTarget {
    /// Logical viewport dimensions used by the scene encoder.
    pub size: [u32; 2],
    /// Background beneath the root layers.
    pub base_color: Color,
    /// Logical-to-device coordinate scale.
    pub device_scale: f64,
}

/// Encode a scene while reusing clean Region fragments. `invalid` must include
/// every cache root whose own paint, geometry or descendant output changed.
pub fn build_scene_cached(
    scene: &mut Scene,
    roots: &[&[PlacedNode]],
    cache_roots: &HashSet<taffy::NodeId>,
    invalid: &HashSet<taffy::NodeId>,
    cache: &mut SceneCache,
    tcx: &mut TextContext,
    target: SceneTarget,
) {
    let SceneTarget {
        size: [width, height],
        base_color,
        device_scale,
    } = target;
    cache.begin_frame(width, height, device_scale);
    scene.reset();
    let device = Affine::scale(device_scale);
    let bg = Rect::new(0.0, 0.0, width as f64, height as f64);
    if base_color != Color::TRANSPARENT {
        scene.fill(Fill::NonZero, device, base_color, None, &bg);
    }
    let mut encoder = Encoder {
        tcx,
        device,
        bg,
        viewport: bg,
        device_scale,
        cache_roots,
        invalid,
        cache: Some(cache),
    };
    for nodes in roots {
        encoder.encode_nodes(scene, nodes, Affine::IDENTITY, bg);
    }
}

struct Encoder<'a> {
    tcx: &'a mut TextContext,
    device: Affine,
    bg: Rect,
    viewport: Rect,
    device_scale: f64,
    cache_roots: &'a HashSet<taffy::NodeId>,
    invalid: &'a HashSet<taffy::NodeId>,
    cache: Option<&'a mut SceneCache>,
}
impl Encoder<'_> {
    fn encode_nodes(
        &mut self,
        scene: &mut Scene,
        mut nodes: &[PlacedNode],
        parent_transform: Affine,
        inherited_clip: Rect,
    ) {
        while let Some(first) = nodes.first() {
            let len = nodes[1..]
                .iter()
                .position(|node| node.depth <= first.depth)
                .map_or(nodes.len(), |index| index + 1);
            self.encode_one(scene, &nodes[..len], parent_transform, inherited_clip);
            nodes = &nodes[len..];
        }
    }

    fn encode_one(
        &mut self,
        scene: &mut Scene,
        nodes: &[PlacedNode],
        parent_transform: Affine,
        inherited_clip: Rect,
    ) {
        let node = &nodes[0];
        if self.cache_roots.contains(&node.node_id) {
            let cached = self.cache.as_ref().and_then(|cache| {
                cache.entries.get(&node.node_id).and_then(|cached| {
                    (!self.invalid.contains(&node.node_id)
                        && cached.parent_transform == parent_transform
                        && cached.inherited_clip == inherited_clip)
                        .then(|| cached.scene.clone())
                })
            });
            if let Some(fragment) = cached {
                scene.append_cached_scene(fragment, Affine::IDENTITY);
                if let Some(cache) = self.cache.as_deref_mut() {
                    cache.reused += 1;
                }
                return;
            }
            // Backdrop sampling reads commands preceding this subtree. Encoding
            // it into an independent fragment would capture the wrong backdrop.
            let independent = !nodes.iter().any(|node| node.paint.backdrop_blur > 0.0);
            if independent {
                let mut fragment = Scene::new();
                self.encode_contents(&mut fragment, nodes, parent_transform, inherited_clip);
                scene.append_cached_scene(fragment.clone(), Affine::IDENTITY);
                if let Some(cache) = self.cache.as_deref_mut() {
                    cache.built += 1;
                    cache.entries.insert(
                        node.node_id,
                        CachedSubtree {
                            scene: fragment,
                            parent_transform,
                            inherited_clip,
                        },
                    );
                }
                return;
            }
        }
        self.encode_contents(scene, nodes, parent_transform, inherited_clip);
    }

    fn encode_contents(
        &mut self,
        scene: &mut Scene,
        nodes: &[PlacedNode],
        parent_transform: Affine,
        inherited_clip: Rect,
    ) {
        let n = &nodes[0];
        if let Some(cache) = self.cache.as_deref_mut() {
            cache.encoded_nodes += 1;
        }
        let opacity = n.paint.opacity * n.paint.motion_opacity.unwrap_or(1.0).clamp(0.0, 1.0);
        if opacity <= 0.0 || inherited_clip.width() <= 0.0 || inherited_clip.height() <= 0.0 {
            return;
        }
        let has_rounded_corners = n.paint.border_radii.max() > 0.0;
        let css_transform = resolve_node_transform(n, parent_transform);
        let node_transform = self.device * css_transform;
        let paints_node = paints_viewport(n, css_transform, inherited_clip);
        let child_clip = n
            .own_clip
            .and_then(|clip| clip_bounds(clip, css_transform))
            .map_or(inherited_clip, |clip| inherited_clip.intersect(clip));
        let enc = EncodeCtx {
            node: n,
            node_transform,
            paints_node,
            has_rounded_corners,
            device: self.device,
            bg: &self.bg,
            viewport: self.viewport,
            device_scale: self.device_scale,
        };
        let mut layers = Vec::new();
        encode_node(scene, self.tcx, &enc, &mut layers);
        self.encode_nodes(scene, &nodes[1..], css_transform, child_clip);
        if paints_viewport(n, css_transform, self.viewport) {
            draw_scrollbars(scene, n, node_transform);
        }
        while layers.pop().is_some() {
            scene.pop_layer();
        }
    }
}

/// Per-node inputs resolved by the full scene traversal.
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
        layers.push(Layer::Opacity);
    }

    if paints_node {
        draw_backdrop(scene, n, *node_transform);
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
        layers.push(Layer::Clip);
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
    fn cached_subtrees_match_full_transform_clip_and_layer_encoding() {
        let mut parent = test_node(0, [5., 7., 95., 107.]);
        parent.own_clip = Some([5., 7., 95., 107.]);
        parent.paint.opacity = 0.8;
        parent.paint.transform = vec![PaintTransform::Translate(
            IrLength::Px { value: 2. },
            IrLength::Px { value: 0. },
        )];
        let mut child = test_node(1, [10., 12., 30., 32.]);
        child.parent_node_id = Some(parent.node_id);
        child.paint.background = Some(Color::WHITE);
        let mut sibling = test_node(1, [40., 12., 60., 32.]);
        unique_node_ids(&mut [&mut parent, &mut child, &mut sibling]);
        sibling.parent_node_id = Some(parent.node_id);
        child.parent_node_id = Some(parent.node_id);
        let nodes = vec![parent.clone(), child.clone(), sibling];
        let roots = HashSet::from([parent.node_id]);
        let mut cache = SceneCache::default();
        let mut actual = Scene::new();
        let mut text = TextContext::new();
        for (iteration, invalid) in [HashSet::new(), HashSet::new()].into_iter().enumerate() {
            build_scene_cached(
                &mut actual,
                &[&nodes],
                &roots,
                &invalid,
                &mut cache,
                &mut text,
                SceneTarget {
                    size: [100, 100],
                    base_color: Color::TRANSPARENT,
                    device_scale: 1.,
                },
            );
            assert_eq!(
                actual.flattened().commands,
                encoded(&nodes).commands,
                "iteration {iteration}"
            );
            assert!(
                !actual
                    .flattened()
                    .commands
                    .iter()
                    .any(|command| { matches!(command, crate::PaintCommand::Fragment { .. }) })
            );
        }
        assert_eq!(cache.reused, 1);
        assert_eq!(cache.encoded_nodes, 0);

        let mut changed = vec![parent.clone(), child, test_node(1, [45., 20., 65., 40.])];
        changed[2].parent_node_id = Some(parent.node_id);
        build_scene_cached(
            &mut actual,
            &[&changed],
            &roots,
            &HashSet::from([parent.node_id]),
            &mut cache,
            &mut text,
            SceneTarget {
                size: [100, 100],
                base_color: Color::TRANSPARENT,
                device_scale: 1.,
            },
        );
        assert_eq!(actual.flattened().commands, encoded(&changed).commands);
    }

    #[test]
    fn cached_subtree_rejects_changed_inherited_context() {
        let mut parent = test_node(0, [0., 0., 100., 100.]);
        let mut child = test_node(1, [10., 10., 40., 40.]);
        unique_node_ids(&mut [&mut parent, &mut child]);
        child.parent_node_id = Some(parent.node_id);
        let nodes = [child];
        let roots = HashSet::from([nodes[0].node_id]);
        let mut cache = SceneCache::default();
        let mut actual = Scene::new();
        let mut text = TextContext::new();
        // Build the context through a non-cached parent, then remove that
        // parent without clearing the cache to exercise the context key.
        let with_parent = vec![parent.clone(), nodes[0].clone()];
        build_scene_cached(
            &mut actual,
            &[&with_parent],
            &roots,
            &HashSet::new(),
            &mut cache,
            &mut text,
            SceneTarget {
                size: [100, 100],
                base_color: Color::TRANSPARENT,
                device_scale: 1.,
            },
        );
        assert_eq!(actual.flattened().commands, encoded(&with_parent).commands);

        let mut transformed_parent = parent;
        transformed_parent.paint.transform = vec![PaintTransform::Translate(
            IrLength::Px { value: 8. },
            IrLength::Px { value: 0. },
        )];
        let transformed = vec![transformed_parent, nodes[0].clone()];
        build_scene_cached(
            &mut actual,
            &[&transformed],
            &roots,
            &HashSet::new(),
            &mut cache,
            &mut text,
            SceneTarget {
                size: [100, 100],
                base_color: Color::TRANSPARENT,
                device_scale: 1.,
            },
        );
        assert_eq!(cache.built, 1);
        assert_eq!(actual.flattened().commands, encoded(&transformed).commands);
    }

    #[test]
    fn scene_cache_key_includes_viewport_and_device_scale() {
        let mut node = test_node(0, [0., 0., 40., 40.]);
        unique_node_ids(&mut [&mut node]);
        let nodes = vec![node.clone()];
        let roots = HashSet::from([node.node_id]);
        let mut cache = SceneCache::default();
        let mut actual = Scene::new();
        let mut text = TextContext::new();
        for (width, scale) in [(100, 1.), (100, 2.), (200, 2.)] {
            build_scene_cached(
                &mut actual,
                &[&nodes],
                &roots,
                &HashSet::new(),
                &mut cache,
                &mut text,
                SceneTarget {
                    size: [width, 100],
                    base_color: Color::TRANSPARENT,
                    device_scale: scale,
                },
            );
            let mut expected = Scene::new();
            build_scene_scaled_layers(
                &mut expected,
                &[&nodes],
                &mut TextContext::new(),
                width,
                100,
                Color::TRANSPARENT,
                scale,
            );
            assert_eq!(cache.built, 1);
            assert_eq!(cache.reused, 0);
            assert_eq!(actual.flattened().commands, expected.commands);
        }
    }

    #[test]
    fn backdrop_subtrees_stay_inline_with_their_actual_background() {
        let mut parent = test_node(0, [0., 0., 100., 100.]);
        let mut child = test_node(1, [10., 10., 40., 40.]);
        unique_node_ids(&mut [&mut parent, &mut child]);
        child.parent_node_id = Some(parent.node_id);
        child.paint.backdrop_blur = 3.;
        parent.paint.opacity = 0.9;
        parent.own_clip = Some([0., 0., 100., 100.]);
        let nodes = vec![parent.clone(), child];
        let roots = HashSet::from([parent.node_id]);
        let mut cache = SceneCache::default();
        let mut actual = Scene::new();
        build_scene_cached(
            &mut actual,
            &[&nodes],
            &roots,
            &HashSet::new(),
            &mut cache,
            &mut TextContext::new(),
            SceneTarget {
                size: [100, 100],
                base_color: Color::TRANSPARENT,
                device_scale: 1.,
            },
        );
        assert_eq!(actual.commands, encoded(&nodes).commands);
        assert!(cache.entries.is_empty());
        let mut depth = 0usize;
        for command in &actual.commands {
            match command {
                crate::PaintCommand::PushLayer { .. } | crate::PaintCommand::PushClip { .. } => {
                    depth += 1;
                }
                crate::PaintCommand::PopLayer => {
                    depth = depth.checked_sub(1).expect("balanced backdrop replay");
                }
                _ => {}
            }
        }
        assert_eq!(depth, 0, "backdrop snapshot must close ancestor layers");
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
    fn unique_node_ids(nodes: &mut [&mut PlacedNode]) {
        let mut tree = taffy::TaffyTree::<()>::new();
        for node in nodes {
            node.node_id = tree.new_leaf(taffy::Style::default()).unwrap();
        }
    }

    fn test_node(depth: usize, rect: [f32; 4]) -> PlacedNode {
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
        node.depth = depth;
        node.rect = rect;
        node.paint.background = Some(Color::BLACK);
        node
    }

    fn encoded(nodes: &[PlacedNode]) -> Scene {
        let mut scene = Scene::new();
        build_scene_scaled_layers(
            &mut scene,
            &[nodes],
            &mut TextContext::new(),
            100,
            100,
            Color::TRANSPARENT,
            1.0,
        );
        scene
    }

    #[test]
    fn zero_opacity_skips_descendants_but_not_siblings() {
        let mut hidden = test_node(0, [0., 0., 50., 50.]);
        hidden.paint.opacity = 0.;
        let child = test_node(1, [0., 0., 20., 20.]);
        let sibling = test_node(0, [50., 50., 80., 80.]);
        assert_eq!(
            encoded(&[hidden, child, sibling.clone()]).commands,
            encoded(&[sibling]).commands
        );
    }

    #[test]
    fn ancestor_clip_culls_paint_inside_window() {
        let mut parent = test_node(0, [0., 0., 20., 20.]);
        parent.own_clip = Some([f32::NEG_INFINITY, 0., f32::INFINITY, 20.]);
        let child = test_node(1, [30., 30., 50., 50.]);
        assert_eq!(
            encoded(&[parent.clone(), child]).commands,
            encoded(&[parent]).commands
        );
    }

    #[test]
    fn offscreen_parent_does_not_hide_overflowing_or_transformed_children() {
        let parent = test_node(0, [200., 200., 220., 220.]);
        let child = test_node(1, [10., 10., 30., 30.]);
        assert!(!encoded(&[parent.clone(), child]).commands.is_empty());
        let mut child = test_node(1, [200., 200., 220., 220.]);
        child.paint.runtime_transform = Some([1., 0., 0., 1., -190., -190.]);
        assert!(!encoded(&[parent, child]).commands.is_empty());
    }

    #[test]
    fn rotated_unbounded_clip_is_not_used_for_culling() {
        assert!(
            clip_bounds(
                [f32::NEG_INFINITY, 0., f32::INFINITY, 20.],
                Affine::rotate(0.5)
            )
            .is_none()
        );
    }

    #[test]
    fn clipped_text_does_not_populate_scene_text_caches() {
        let mut parent = test_node(0, [0., 0., 20., 20.]);
        parent.own_clip = Some([0., 0., 20., 20.]);
        let mut child = test_node(1, [30., 30., 50., 50.]);
        child.paint.text = Some(Arc::from("Must not shape"));
        let mut text = TextContext::new();
        let mut scene = Scene::new();
        build_scene_scaled_layers(
            &mut scene,
            &[&[parent, child]],
            &mut text,
            100,
            100,
            Color::TRANSPARENT,
            1.,
        );
        assert_eq!(text.cache_entries(), (0, 0));
    }

    #[test]
    fn transformed_clip_keeps_transformed_child_visible() {
        let mut parent = test_node(0, [200., 200., 220., 220.]);
        parent.own_clip = Some(parent.rect);
        parent.paint.runtime_transform = Some([1., 0., 0., 1., -190., -190.]);
        let child = test_node(1, [200., 200., 220., 220.]);
        let baseline = encoded(&[parent.clone()]).commands.len();
        assert!(encoded(&[parent, child]).commands.len() > baseline);
    }
}
