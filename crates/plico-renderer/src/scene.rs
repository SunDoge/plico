//! Build a retained Wabou paint scene from the flattened layout list.

use std::sync::Arc;

use crate::corner_shape::corner_shape;
use crate::{PaintScene, Scene};
use kurbo::{Affine, Rect, Shape, Stroke};
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
    // Most nodes carry no authored or presentation transform. As in MyGO's
    // Painter.add identity fast path, keep those nodes on the parent's matrix
    // without building and multiplying two origin translations plus identities.
    if node.paint.transform.is_empty()
        && node.paint.motion_transform.is_none()
        && node.paint.runtime_transform.is_none()
    {
        return parent_transform;
    }
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

fn snap_box(rect: Rect, device_scale: f64) -> Rect {
    let scale = device_scale.max(f64::EPSILON);
    Rect::new(
        (rect.x0 * scale).round() / scale,
        (rect.y0 * scale).round() / scale,
        (rect.x1 * scale).round() / scale,
        (rect.y1 * scale).round() / scale,
    )
}

fn snap_border_width(width: f32, device_scale: f64) -> f32 {
    if width <= 0. {
        return 0.;
    }
    let scale = device_scale.max(f64::EPSILON);
    ((f64::from(width) * scale).round().max(1.) / scale) as f32
}

fn snap_offset(offset: f32, device_scale: f64) -> f64 {
    let scale = device_scale.max(f64::EPSILON);
    (f64::from(offset) * scale).round() / scale
}

fn snap_local_box(rect: Rect, origin: [f32; 2], device_scale: f64) -> Rect {
    let offset = kurbo::Vec2::new(f64::from(origin[0]), f64::from(origin[1]));
    snap_box(rect + offset, device_scale) - offset
}

fn visual_bounds(node: &PlacedNode) -> Rect {
    let [x0, y0, x1, y1] = node.rect;
    let border_box = Rect::new(x0 as f64, y0 as f64, x1 as f64, y1 as f64);
    let outline = f64::from((node.paint.outline_width + node.paint.outline_offset.abs()).max(0.0));
    let mut bounds = border_box.inflate(outline, outline);
    if let Some(widget) = &node.paint.widget
        && let Some(crate::PaintCommand::PushClip { transform, clip }) = widget.commands.first()
    {
        let local = transform.transform_rect_bbox(clip.bounding_box());
        bounds = bounds.union(
            local
                + kurbo::Vec2::new(
                    f64::from(node.content_origin[0]),
                    f64::from(node.content_origin[1]),
                ),
        );
    }
    for shadow in node.paint.shadows.iter().filter(|s| !s.inset) {
        let (shadow_rect, _, std_dev) =
            shadow_geometry(border_box, f64::from(node.paint.border_radii.max()), shadow);
        let blur_extent = std_dev * 3.0;
        bounds = bounds.union(shadow_rect.inflate(blur_extent, blur_extent));
    }
    let sigma = node.paint.foreground_blur;
    if sigma.is_finite() && sigma > 0. {
        bounds = bounds.inflate(
            f64::from(sigma.min(64.)) * 3.,
            f64::from(sigma.min(64.)) * 3.,
        );
    }
    bounds
}

fn paint_is_cullable(node: &PlacedNode) -> bool {
    node.paint.vector_path.is_none()
        && node.paint.widget.as_ref().is_none_or(|widget| {
            matches!(
                widget.commands.first(),
                Some(crate::PaintCommand::PushClip { .. })
            )
        })
}

fn paints_viewport(node: &PlacedNode, transform: Affine, viewport: Rect) -> bool {
    if !paint_is_cullable(node) {
        return true;
    }
    let bounds = transform.transform_rect_bbox(visual_bounds(node));
    bounds.width() > 0.
        && bounds.height() > 0.
        && bounds.x1 >= viewport.x0
        && bounds.x0 <= viewport.x1
        && bounds.y1 >= viewport.y0
        && bounds.y0 <= viewport.y1
}

fn rect_intersects(bounds: Rect, clip: Rect) -> bool {
    bounds.width() > 0.
        && bounds.height() > 0.
        && bounds.x1 >= clip.x0
        && bounds.x0 <= clip.x1
        && bounds.y1 >= clip.y0
        && bounds.y0 <= clip.y1
}

#[derive(Clone, Copy)]
struct SubtreeInk {
    own: Rect,
    ink: Rect,
    end: usize,
    has_ink: bool,
    own_cullable: bool,
    cullable: bool,
}

/// Reusable allocation for presented scene encoding.
///
/// Hosts should keep one per retained scene source. The contents are transient
/// and never part of a cached scene snapshot.
#[derive(Default)]
pub struct SceneScratch {
    subtree_ink: Vec<SubtreeInk>,
    subtree_stack: Vec<usize>,
}

fn prepare_subtree_ink(nodes: &[PlacedNode], out: &mut Vec<SubtreeInk>, stack: &mut Vec<usize>) {
    fn close(nodes: &[PlacedNode], out: &mut [SubtreeInk], stack: &mut Vec<usize>, end: usize) {
        let index = stack.pop().unwrap();
        out[index].end = end;
        let sigma = nodes[index].paint.foreground_blur;
        if out[index].has_ink && sigma.is_finite() && sigma > 0. {
            let extent = f64::from(sigma.min(64.)) * 3.;
            out[index].ink = out[index].ink.inflate(extent, extent);
        }
        if let Some(&parent) = stack.last() {
            let child = out[index];
            out[parent].cullable &= child.cullable;
            if child.has_ink {
                if out[parent].has_ink {
                    out[parent].ink = out[parent].ink.union(child.ink);
                } else {
                    out[parent].ink = child.ink;
                    out[parent].has_ink = true;
                }
            }
        }
    }

    out.clear();
    stack.clear();
    out.reserve(nodes.len().saturating_sub(out.capacity()));
    for (index, node) in nodes.iter().enumerate() {
        while stack
            .last()
            .is_some_and(|&open| nodes[open].depth >= node.depth)
        {
            close(nodes, out, stack, index);
        }
        let own = node
            .world_transform
            .transform_rect_bbox(visual_bounds(node));
        let finite = [own.x0, own.y0, own.x1, own.y1]
            .into_iter()
            .all(f64::is_finite);
        // Typed vector paths and unclipped widget fragments are intentionally
        // unconstrained by the layout box. Until their exact bounds are
        // retained, treat the node and its ancestors like MyGO's noCull
        // elements.
        let own_cullable = finite && paint_is_cullable(node);
        let visible = finite
            && own.width() > 0.
            && own.height() > 0.
            && node.world_opacity > 0.
            && node.paint.motion_opacity.unwrap_or(1.).clamp(0., 1.) > 0.;
        out.push(SubtreeInk {
            own,
            ink: own,
            end: nodes.len(),
            has_ink: visible,
            own_cullable,
            cullable: own_cullable,
        });
        stack.push(index);
    }
    while !stack.is_empty() {
        close(nodes, out, stack, nodes.len());
    }
}

fn should_prepare_subtree_ink(nodes: &[PlacedNode], viewport: Rect) -> bool {
    const SAMPLES: usize = 32;
    if nodes.len() < 64 || !nodes.windows(2).any(|pair| pair[1].depth > pair[0].depth) {
        return false;
    }
    let samples = nodes.len().min(SAMPLES);
    let mut outside = 0;
    for sample in 0..samples {
        let index = sample * (nodes.len() - 1) / (samples - 1);
        let node = &nodes[index];
        outside += usize::from(
            node.world_opacity <= 0. || !paints_viewport(node, node.world_transform, viewport),
        );
    }
    // The prepass pays for itself when it can skip a material fraction of
    // nested entries. Near-fully-visible scenes keep the direct encoder.
    outside * 4 >= samples
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

fn draw_scrollbars(scene: &mut Scene, node: &PlacedNode, transform: Affine, device_scale: f64) {
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
            let track = snap_box(track, device_scale);
            scene.fill(
                Fill::NonZero,
                transform,
                fade(style.track_color),
                None,
                &track,
            );
        }
        let Some(raw_rect) = scrollbar_thumb(node, axis) else {
            continue;
        };
        let radius = if style.radius < 0.0 {
            match axis {
                ScrollAxis::Horizontal => raw_rect.height() * 0.5,
                ScrollAxis::Vertical => raw_rect.width() * 0.5,
            }
        } else {
            f64::from(style.radius)
        };
        let rect = snap_box(raw_rect, device_scale);
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

fn draw_node_box(scene: &mut Scene, node: &PlacedNode, transform: Affine, device_scale: f64) {
    let [x0, y0, x1, y1] = node.rect;
    // MyGO snaps the two physical edges independently, preserving layout in
    // logical coordinates while keeping static UI chrome off half pixels.
    let rect = snap_box(
        Rect::new(f64::from(x0), f64::from(y0), f64::from(x1), f64::from(y1)),
        device_scale,
    );
    let radii = node.paint.border_radii;
    for shadow in node.paint.shadows.iter().filter(|s| !s.inset) {
        let (shadow_rect, radius, std_dev) = shadow_geometry(rect, f64::from(radii.max()), shadow);
        let shadow_rect = snap_box(shadow_rect, device_scale);
        if (shadow_rect.width() <= 0. || shadow_rect.height() <= 0.)
            || shadow.color.components[3] <= 0.
        {
            continue;
        }
        // CSS/MyGO outer shadows never darken translucent content. A reversed
        // inner contour makes a nonzero-winding hole, following continuous corners.
        let extent = shadow_rect
            .inflate(4. * std_dev + 1., 4. * std_dev + 1.)
            .union(rect.inflate(1., 1.));
        let mut outside = extent.to_path(0.1);
        let hole = corner_shape(rect, radii, node.paint.corner_smoothing).to_path(0.1);
        let hole = if hole.area().signum() == outside.area().signum() {
            hole.reverse_subpaths()
        } else {
            hole
        };
        outside.extend(hole.elements().iter().copied());
        scene.push_clip_layer(transform, &outside);
        scene.draw_box_shadow_profile(
            transform,
            shadow_rect,
            shadow.color,
            crate::ShadowProfile {
                radius,
                std_dev,
                corner_smoothing: node.paint.corner_smoothing,
            },
        );
        scene.pop_layer();
    }
    if let Some(background) = node.paint.background {
        let [a, b, c, d, _, _] = transform.as_coeffs();
        let uniform = radii.top_left == radii.top_right
            && radii.top_left == radii.bottom_right
            && radii.top_left == radii.bottom_left;
        if uniform
            && node.paint.corner_smoothing > 0.
            && radii.max() > 0.
            && a > 0.
            && (a - d).abs() < 1e-9
            && b.abs() < 1e-9
            && c.abs() < 1e-9
        {
            // A zero-sigma contour is a solid fill. The analytical contour
            // backend batches these without a separate raster/texture per tile;
            // other backends retain the same compatible continuous geometry.
            scene.draw_box_shadow_profile(
                transform,
                rect,
                background,
                crate::ShadowProfile {
                    radius: f64::from(radii.max()),
                    std_dev: 0.,
                    corner_smoothing: node.paint.corner_smoothing,
                },
            );
        } else {
            scene.fill_paint_shape(
                Fill::NonZero,
                transform,
                background,
                None,
                corner_shape(rect, radii, node.paint.corner_smoothing),
            );
        }
    }
    for shadow in node
        .paint
        .shadows
        .iter()
        .filter(|s| s.inset && s.color.components[3] > 0.)
    {
        // MyGO/CSS inset: positive spread contracts the hole, then blur its
        // complement. Clip to the same contour as the background.
        let spread = f64::from(shadow.spread);
        let offset_x = snap_offset(shadow.offset_x, device_scale);
        let offset_y = snap_offset(shadow.offset_y, device_scale);
        let hole = Rect::new(
            rect.x0 + offset_x + spread,
            rect.y0 + offset_y + spread,
            rect.x1 + offset_x - spread,
            rect.y1 + offset_y - spread,
        );
        let radius = shadow
            .radius
            .map(f64::from)
            .unwrap_or_else(|| (f64::from(radii.max()) - spread).max(0.));
        scene.push_clip_layer(
            transform,
            &corner_shape(rect, radii, node.paint.corner_smoothing),
        );
        scene.draw_inset_box_shadow_profile(
            transform,
            rect,
            hole,
            shadow.color,
            crate::ShadowProfile {
                radius,
                std_dev: f64::from(shadow.std_dev),
                corner_smoothing: node.paint.corner_smoothing,
            },
        );
        scene.pop_layer();
    }
    if node.paint.outline_width > 0.0
        && let Some(color) = node.paint.outline_color
    {
        let width = f64::from(snap_border_width(node.paint.outline_width, device_scale));
        let expansion = f64::from(node.paint.outline_offset + node.paint.outline_width);
        let outer = snap_box(rect.inflate(expansion, expansion), device_scale);
        if outer.width() > 0. && outer.height() > 0. {
            let outer_radii = radii.expand(expansion as f32);
            let mut ring =
                corner_shape(outer, outer_radii, node.paint.corner_smoothing).into_bez_path();
            let inner = Rect::new(
                outer.x0 + width,
                outer.y0 + width,
                outer.x1 - width,
                outer.y1 - width,
            );
            if inner.width() > 0. && inner.height() > 0. {
                ring.extend(
                    corner_shape(
                        inner,
                        outer_radii.inset(width as f32),
                        node.paint.corner_smoothing,
                    )
                    .into_bez_path()
                    .elements()
                    .iter()
                    .copied(),
                );
            }
            scene.fill(Fill::EvenOdd, transform, color, None, &ring);
        }
    }
    let Some((_, border_color)) = node.paint.border else {
        return;
    };
    let [top, right, bottom, left] = node
        .border_widths
        .map(|width| snap_border_width(width, device_scale));
    if top > 0.0 && top == right && top == bottom && top == left {
        let width = f64::from(top);
        if node.paint.corner_smoothing > 0. {
            // An even-odd ring keeps the outer contour identical to the fill,
            // and the inner contour identical to the content clip.
            let mut ring = corner_shape(rect, radii, node.paint.corner_smoothing).into_bez_path();
            let inner = Rect::new(
                rect.x0 + width,
                rect.y0 + width,
                (rect.x1 - width).max(rect.x0 + width),
                (rect.y1 - width).max(rect.y0 + width),
            );
            if inner.width() > 0. && inner.height() > 0. {
                let shape = corner_shape(inner, radii.inset(top), node.paint.corner_smoothing);
                ring.extend(shape.into_bez_path().elements().iter().copied());
            }
            scene.fill(Fill::EvenOdd, transform, border_color, None, &ring);
            return;
        }
        let half = width / 2.0;
        let inset = Rect::new(
            rect.x0 + half,
            rect.y0 + half,
            rect.x1 - half,
            rect.y1 - half,
        );
        scene.stroke(
            &Stroke::new(width),
            transform,
            border_color,
            None,
            &corner_shape(inset, radii.inset(half as f32), node.paint.corner_smoothing),
        );
        return;
    }
    // Side-specific utility borders must not turn into a uniform rectangle.
    // Clip their straight center lines to the outer rounded border box. Without
    // this, a single `border-t`/`border-r` reaches the rectangular corner even
    // though the node background and contents follow `border-radius`.
    let clips_to_rounded_border = radii.max() > 0.0;
    if clips_to_rounded_border {
        scene.push_clip_layer(
            transform,
            &corner_shape(rect, radii, node.paint.corner_smoothing),
        );
    }
    let (x0, y0, x1, y1) = (
        rect.x0 as f32,
        rect.y0 as f32,
        rect.x1 as f32,
        rect.y1 as f32,
    );
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

fn draw_image(scene: &mut Scene, node: &PlacedNode, transform: Affine, device_scale: f64) {
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
    let destination = snap_box(
        Rect::new(
            dx,
            dy,
            dx + f64::from(image_width) * scale,
            dy + f64::from(image_height) * scale,
        ),
        device_scale,
    );
    if destination.width() <= 0. || destination.height() <= 0. {
        return;
    }
    scene.draw_image(
        image.brush().into(),
        transform
            * Affine::translate((destination.x0, destination.y0))
            * Affine::scale_non_uniform(
                destination.width() / f64::from(image_width),
                destination.height() / f64::from(image_height),
            ),
    );
}

fn draw_text_at_raster_scale(
    scene: &mut Scene,
    node: &PlacedNode,
    tcx: &mut TextContext,
    transform: Affine,
    device_scale: f64,
    raster_factor: f64,
) {
    if tcx.has_native_paragraphs() {
        let Some(request) = node_paragraph_request(tcx, node) else {
            return;
        };
        let origin = transform
            * Affine::translate((node.content_origin[0] as f64, node.content_origin[1] as f64));
        for &[x0, y0, x1, y1] in node.paint.selection_rects.iter() {
            let rect = snap_local_box(
                Rect::new(x0 as f64, y0 as f64, x1 as f64, y1 as f64),
                node.content_origin,
                device_scale,
            );
            scene.fill(
                Fill::NonZero,
                origin,
                Color::from_rgba8(59, 130, 246, 105),
                None,
                &rect,
            );
        }
        // Measurements and hit-testing populate the host's shared paragraph cache.
        tcx.native_paragraph(request.clone());
        let request = if raster_factor != 1. {
            let [a, b, c, d, _, _] = origin.as_coeffs();
            if a > 0. && (a - d).abs() < 1e-9 && b.abs() < 1e-9 && c.abs() < 1e-9 {
                let mut request = (*request).clone();
                request.raster_scale_hint = Some((a * raster_factor * 10000.).round() / 10000.);
                Arc::new(request)
            } else {
                request
            }
        } else {
            request
        };
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
        let rect = snap_local_box(
            Rect::new(f64::from(x0), f64::from(y0), f64::from(x1), f64::from(y1)),
            node.content_origin,
            device_scale,
        );
        scene.fill(
            Fill::NonZero,
            transform * origin,
            Color::from_rgba8(59, 130, 246, 105),
            None,
            &rect,
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
    let glyphs = tcx.native_weight_glyph_scene_scaled(layout, device_scale);
    scene.append_scene((*glyphs).clone(), transform);
}

/// Replay the prior application scene into a GPU-filtered layer, then clip its
/// result to the glass panel. Foreground commands are recorded afterwards.
/// This prototype trades replay work for a backend-neutral command stream;
/// it does not claim a persistent blurred-texture cache.
fn draw_backdrop(scene: &mut Scene, node: &PlacedNode, transform: Affine, device_scale: f64) {
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
    let clip = corner_shape(
        snap_box(Rect::new(x0, y0, x1, y1), device_scale),
        node.paint.border_radii,
        node.paint.corner_smoothing,
    );
    scene.push_layer(BlendMode::default(), 1., transform, &clip, None, None);
    let filter =
        crate::filter::Filter::from_primitive(crate::filter::FilterPrimitive::GaussianBlur {
            std_deviation: sigma.min(64.),
            edge_mode: crate::filter::EdgeMode::None,
        });
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
    build_scene_scaled_layers_impl(
        scene,
        roots,
        tcx,
        width,
        height,
        base_color,
        device_scale,
        false,
        None,
    );
}

/// Encode layers whose [`PlacedNode`] presentation fields were refreshed by
/// the host. This keeps hit testing and painting on one composed transform.
#[doc(hidden)]
pub fn build_presented_scene_scaled_layers(
    scene: &mut Scene,
    roots: &[&[PlacedNode]],
    tcx: &mut TextContext,
    width: u32,
    height: u32,
    base_color: Color,
    device_scale: f64,
) {
    let mut scratch = SceneScratch::default();
    build_presented_scene_scaled_layers_with_scratch(
        scene,
        roots,
        tcx,
        width,
        height,
        base_color,
        device_scale,
        &mut scratch,
    );
}

/// Encode host-resolved layers while reusing subtree-culling scratch storage.
#[doc(hidden)]
#[allow(clippy::too_many_arguments)]
pub fn build_presented_scene_scaled_layers_with_scratch(
    scene: &mut Scene,
    roots: &[&[PlacedNode]],
    tcx: &mut TextContext,
    width: u32,
    height: u32,
    base_color: Color,
    device_scale: f64,
    scratch: &mut SceneScratch,
) {
    build_scene_scaled_layers_impl(
        scene,
        roots,
        tcx,
        width,
        height,
        base_color,
        device_scale,
        true,
        Some(scratch),
    );
}

#[allow(clippy::too_many_arguments)]
fn build_scene_scaled_layers_impl(
    scene: &mut Scene,
    roots: &[&[PlacedNode]],
    tcx: &mut TextContext,
    width: u32,
    height: u32,
    base_color: Color,
    device_scale: f64,
    use_presentations: bool,
    scratch: Option<&mut SceneScratch>,
) {
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
        use_presentations,
    };
    let mut local_scratch = SceneScratch::default();
    let scratch = scratch.unwrap_or(&mut local_scratch);
    for nodes in roots {
        let subtree_ink = (use_presentations && should_prepare_subtree_ink(nodes, bg)).then(|| {
            prepare_subtree_ink(nodes, &mut scratch.subtree_ink, &mut scratch.subtree_stack);
            scratch.subtree_ink.as_slice()
        });
        if let Some(subtree_ink) = subtree_ink {
            encoder.encode_nodes::<true>(scene, nodes, subtree_ink, Affine::IDENTITY, bg, 1.);
        } else {
            // Keep the common all-visible and small-scene path monomorphized
            // without one optional-ink branch per node.
            encoder.encode_nodes::<false>(scene, nodes, &[], Affine::IDENTITY, bg, 1.);
        }
    }
}

struct Encoder<'a> {
    tcx: &'a mut TextContext,
    device: Affine,
    bg: Rect,
    viewport: Rect,
    device_scale: f64,
    use_presentations: bool,
}

#[derive(Clone, Copy)]
struct NodeBatch<'a> {
    nodes: &'a [PlacedNode],
    subtree_ink: &'a [SubtreeInk],
}

impl Encoder<'_> {
    fn encode_nodes<const SUBTREE_CULL: bool>(
        &mut self,
        scene: &mut Scene,
        nodes: &[PlacedNode],
        subtree_ink: &[SubtreeInk],
        parent_transform: Affine,
        inherited_clip: Rect,
        raster_factor: f64,
    ) {
        let batch = NodeBatch { nodes, subtree_ink };
        let mut cursor = 0;
        while cursor < nodes.len() {
            self.encode_subtree::<SUBTREE_CULL>(
                scene,
                batch,
                &mut cursor,
                parent_transform,
                inherited_clip,
                raster_factor,
            );
        }
    }

    /// Encode one preorder subtree and leave `cursor` at its next sibling.
    ///
    /// The old slice-based walk searched forward for the end of every
    /// subtree before recursively doing the same work again. A deep tree
    /// therefore revisited the tail at every level. The cursor consumes each
    /// entry once, matching the single DFS used by MyGO's linked node tree.
    fn encode_subtree<const SUBTREE_CULL: bool>(
        &mut self,
        scene: &mut Scene,
        batch: NodeBatch<'_>,
        cursor: &mut usize,
        parent_transform: Affine,
        inherited_clip: Rect,
        raster_factor: f64,
    ) {
        let index = *cursor;
        if SUBTREE_CULL {
            let ink = batch.subtree_ink[index];
            if ink.cullable && (!ink.has_ink || !rect_intersects(ink.ink, inherited_clip)) {
                *cursor = ink.end;
                return;
            }
        }
        let n = &batch.nodes[index];
        let depth = n.depth;
        let opacity = n.paint.opacity * n.paint.motion_opacity.unwrap_or(1.0).clamp(0.0, 1.0);
        if opacity <= 0.0 || inherited_clip.width() <= 0.0 || inherited_clip.height() <= 0.0 {
            *cursor += 1;
            while *cursor < batch.nodes.len() && batch.nodes[*cursor].depth > depth {
                *cursor += 1;
            }
            return;
        }
        let has_rounded_corners = n.paint.border_radii.max() > 0.0;
        let css_transform = if self.use_presentations {
            n.world_transform
        } else {
            resolve_node_transform(n, parent_transform)
        };
        let node_transform = self.device * css_transform;
        let raster_factor = raster_factor
            * f64::from(if n.paint.raster_scale_correction > 0. {
                n.paint.raster_scale_correction
            } else {
                1.
            });
        let paints_node = if SUBTREE_CULL {
            let ink = batch.subtree_ink[index];
            !ink.own_cullable || (ink.has_ink && rect_intersects(ink.own, inherited_clip))
        } else {
            paints_viewport(n, css_transform, inherited_clip)
        };
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
            raster_factor,
        };
        let layers = encode_node(scene, self.tcx, &enc);
        *cursor += 1;
        while *cursor < batch.nodes.len() && batch.nodes[*cursor].depth > depth {
            self.encode_subtree::<SUBTREE_CULL>(
                scene,
                batch,
                cursor,
                css_transform,
                child_clip,
                raster_factor,
            );
        }
        // The scrollbar lives inside the node's border box, so the node's
        // existing visibility result also covers it. Most nodes have no live
        // overlay scrollbar; do not recompute visual bounds and transform
        // them a second time for those nodes.
        if paints_node && n.scroll.opacity > 0.0 {
            draw_scrollbars(scene, n, node_transform, self.device_scale);
        }
        for _ in 0..layers {
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
    raster_factor: f64,
}
fn encode_node(scene: &mut Scene, tcx: &mut TextContext, enc: &EncodeCtx<'_>) -> usize {
    let EncodeCtx {
        node: n,
        node_transform,
        paints_node,
        has_rounded_corners,
        device,
        bg,
        viewport,
        device_scale,
        raster_factor,
    } = enc;
    let n: &PlacedNode = n;
    let device = *device;
    let bg = *bg;
    let viewport = *viewport;
    let device_scale = *device_scale;
    let paints_node = *paints_node;
    let has_rounded_corners = *has_rounded_corners;
    let mut layers = 0;

    // Keep ancestor transform, opacity, and clip state even when this
    // node's own visual bounds are offscreen: a transformed descendant can
    // still enter the viewport. Only node-owned paint is culled here.
    let opacity = n.paint.opacity * n.paint.motion_opacity.unwrap_or(1.0).clamp(0.0, 1.0);
    let sigma = n.paint.foreground_blur;
    if sigma.is_finite() && sigma > 0. {
        let filter = crate::filter::GaussianBlur {
            sigma: sigma.min(64.),
            edge: crate::filter::BlurEdge::Transparent,
        }
        .into_filter();
        // Filtering and opacity share one isolated layer. Expanded bounds keep
        // the halo outside the source box instead of clipping it at the glyph edge.
        scene.push_layer(
            BlendMode::default(),
            opacity,
            *node_transform,
            &visual_bounds(n),
            Some(Arc::new(filter)),
            None,
        );
        layers += 1;
    } else if opacity < 1.0 {
        scene.push_layer(BlendMode::default(), opacity, device, &bg, None, None);
        layers += 1;
    }

    if paints_node {
        draw_backdrop(scene, n, *node_transform, device_scale);
        draw_node_box(scene, n, *node_transform, device_scale);
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
        // Keep the scissor contour on the same physical edges as the painted
        // box. Otherwise a snapped one-pixel border can still be softened by
        // an adjacent fractional clip edge.
        let clip_rect = snap_box(
            Rect::new(finite(cx0), finite(cy0), finite(cx1), finite(cy1)),
            device_scale,
        );
        scene.push_clip_layer(
            *node_transform,
            &corner_shape(clip_rect, n.own_clip_radii, n.paint.corner_smoothing),
        );
        layers += 1;
    }

    if paints_node {
        draw_svg(scene, n, *node_transform);
        draw_vector_path(scene, n, *node_transform);
        draw_image(scene, n, *node_transform, device_scale);
    }

    if paints_node && let Some(ws) = &n.paint.widget {
        // Keep rounded widget clipping inside the fragment itself. Some
        // GPU backends do not reliably carry a parent clip across an
        // appended scene at HiDPI; local encoding also avoids mixing
        // absolute logical and fragment coordinates.
        let outer_widget_clip = (!has_rounded_corners || n.paint.corner_smoothing > 0.)
            .then(|| widget_clip(n))
            .flatten();
        if let Some(([cx0, cy0, cx1, cy1], radii)) = outer_widget_clip {
            let widget_clip = snap_box(
                Rect::new(
                    cx0.max(0.0) as f64,
                    cy0.max(0.0) as f64,
                    cx1.min(viewport.width() as f32) as f64,
                    cy1.min(viewport.height() as f32) as f64,
                ),
                device_scale,
            );
            scene.push_clip_layer(
                *node_transform,
                &corner_shape(widget_clip, radii, n.paint.corner_smoothing),
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
        draw_text_at_raster_scale(scene, n, tcx, *node_transform, device_scale, *raster_factor);
    }
    layers
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::style::Shadow;

    #[test]
    fn static_node_boxes_and_continuous_borders_snap_at_fractional_dpi() {
        let mut node = test_node(0, [0.5, 1., 16.5, 17.]);
        node.paint.background = Some(Color::WHITE);
        node.paint.border = Some((1., Color::BLACK));
        node.paint.border_radii = CornerRadii::uniform(4.);
        node.paint.corner_smoothing = 1.;
        node.border_widths = [1.; 4];

        let mut scene = Scene::new();
        draw_node_box(&mut scene, &node, Affine::scale(1.25), 1.25);

        let crate::PaintCommand::BoxShadow { rect, .. } = scene.commands[0] else {
            panic!("continuous background should use the analytical box path");
        };
        assert_eq!(rect, Rect::new(0.8, 0.8, 16.8, 16.8));
        let crate::PaintCommand::Fill { ref shape, .. } = scene.commands[1] else {
            panic!("continuous border should remain an even-odd ring");
        };
        assert_eq!(shape.bounding_box(), rect);
        let mut expected = corner_shape(rect, CornerRadii::uniform(4.), 1.).into_bez_path();
        let border_width = f64::from(snap_border_width(1., 1.25));
        expected.extend(
            corner_shape(
                Rect::new(
                    rect.x0 + border_width,
                    rect.y0 + border_width,
                    rect.x1 - border_width,
                    rect.y1 - border_width,
                ),
                CornerRadii::uniform(4.).inset(border_width as f32),
                1.,
            )
            .into_bez_path()
            .elements()
            .iter()
            .copied(),
        );
        assert_eq!(shape.to_path(0.1), expected);
    }

    #[test]
    fn outlines_scrollbars_and_local_highlights_share_physical_pixel_snapping() {
        let assert_rect_near = |actual: Rect, expected: Rect| {
            for (actual, expected) in [
                (actual.x0, expected.x0),
                (actual.y0, expected.y0),
                (actual.x1, expected.x1),
                (actual.y1, expected.y1),
            ] {
                assert!((actual - expected).abs() < 1e-9, "{actual} != {expected}");
            }
        };
        let mut outlined = test_node(0, [0.5, 1., 16.5, 17.]);
        outlined.paint.background = None;
        outlined.paint.outline_width = 1.;
        outlined.paint.outline_offset = 1.;
        outlined.paint.outline_color = Some(Color::BLACK);
        outlined.paint.border_radii = CornerRadii::uniform(4.);
        outlined.paint.corner_smoothing = 1.;
        let mut scene = Scene::new();
        draw_node_box(&mut scene, &outlined, Affine::scale(1.25), 1.25);
        let crate::PaintCommand::Fill { fill, shape, .. } = &scene.commands[0] else {
            panic!("outline should be an exact filled ring");
        };
        assert_eq!(*fill, Fill::EvenOdd);
        assert_eq!(shape.bounding_box(), Rect::new(-1.6, -1.6, 19.2, 19.2));

        let mut scrolling = test_node(0, [0., 0., 101., 101.]);
        scrolling.paint.background = None;
        scrolling.scroll = crate::layout::ScrollMetrics {
            port: [0.5, 0.5, 100.5, 100.5],
            scrollable: [false, true],
            range: [0., 100.],
            offset: [0., 50.],
            opacity: 1.,
            interaction: 0,
        };
        scene.reset();
        draw_scrollbars(&mut scene, &scrolling, Affine::scale(1.25), 1.25);
        let expected_track = snap_box(
            scrollbar_track(&scrolling, ScrollAxis::Vertical).unwrap(),
            1.25,
        );
        let expected_thumb = snap_box(
            scrollbar_thumb(&scrolling, ScrollAxis::Vertical).unwrap(),
            1.25,
        );
        let crate::PaintCommand::Fill { shape: track, .. } = &scene.commands[0] else {
            panic!("scrollbar track should be filled");
        };
        let crate::PaintCommand::Fill { shape: thumb, .. } = &scene.commands[1] else {
            panic!("scrollbar thumb should be filled");
        };
        assert_rect_near(track.bounding_box(), expected_track);
        assert_rect_near(thumb.bounding_box(), expected_thumb);

        assert_rect_near(
            snap_local_box(Rect::new(0.25, 0.25, 8.25, 5.25), [10.25, 20.25], 1.25),
            Rect::new(0.15, 0.55, 8.15, 5.35),
        );
    }

    #[test]
    fn node_owned_clip_uses_the_same_snapped_edges_as_its_box() {
        let mut node = test_node(0, [0.5, 0.5, 20.5, 20.5]);
        node.paint.background = None;
        node.own_clip = Some([0.5, 0.5, 20.5, 20.5]);
        let mut scene = Scene::new();
        let mut text = TextContext::new();
        build_scene_scaled(
            &mut scene,
            &[node],
            &mut text,
            40,
            40,
            Color::TRANSPARENT,
            1.25,
        );
        let crate::PaintCommand::PushClip { clip, .. } = &scene.commands[0] else {
            panic!("node clip should be encoded");
        };
        assert_eq!(clip.bounding_box(), Rect::new(0.8, 0.8, 20.8, 20.8));
    }

    #[test]
    fn uniform_continuous_fills_are_analytical_without_changing_other_geometry() {
        let mut node = test_node(0, [0., 0., 40., 30.]);
        node.paint.background = Some(Color::WHITE);
        node.paint.border_radii = CornerRadii::uniform(6.);
        node.paint.corner_smoothing = 1.;
        let mut scene = Scene::new();
        draw_node_box(&mut scene, &node, Affine::scale(1.5), 1.5);
        assert!(matches!(
            scene.commands[0],
            crate::PaintCommand::BoxShadow {
                radius: 6.,
                std_dev: 0.,
                corner_smoothing: 1.,
                ..
            }
        ));
        for transform in [Affine::scale_non_uniform(1., 2.), Affine::rotate(0.1)] {
            let mut scene = Scene::new();
            draw_node_box(&mut scene, &node, transform, 1.);
            assert!(matches!(
                scene.commands[0],
                crate::PaintCommand::Fill { .. }
            ));
        }
        node.paint.border_radii.top_left = 2.;
        let mut scene = Scene::new();
        draw_node_box(&mut scene, &node, Affine::IDENTITY, 1.);
        assert!(matches!(
            scene.commands[0],
            crate::PaintCommand::Fill { .. }
        ));
    }

    #[test]
    fn inset_shadow_follows_background_and_continuous_clip_without_expanding_bounds() {
        let mut node = test_node(0, [20., 20., 80., 80.]);
        node.paint.background = Some(Color::WHITE);
        node.paint.border_radii = CornerRadii::uniform(16.);
        node.paint.corner_smoothing = 1.;
        node.paint.shadows.push(Shadow {
            inset: true,
            offset_x: 2.,
            offset_y: 3.,
            spread: 4.,
            std_dev: 5.,
            color: Color::BLACK,
            radius: None,
        });
        assert_eq!(visual_bounds(&node), Rect::new(20., 20., 80., 80.));
        let mut scene = Scene::new();
        draw_node_box(&mut scene, &node, Affine::IDENTITY, 1.);
        assert!(matches!(
            scene.commands[0],
            crate::PaintCommand::BoxShadow { std_dev: 0., .. }
        ));
        let crate::PaintCommand::PushClip { clip, .. } = &scene.commands[1] else {
            panic!("inset requires contour clip");
        };
        assert_eq!(
            clip.to_path(0.1),
            corner_shape(Rect::new(20., 20., 80., 80.), node.paint.border_radii, 1.).to_path(0.1)
        );
        let crate::PaintCommand::InsetBoxShadow {
            hole,
            std_dev,
            corner_smoothing,
            ..
        } = &scene.commands[2]
        else {
            panic!("missing inset");
        };
        assert_eq!(*hole, Rect::new(26., 27., 78., 79.));
        assert_eq!(*std_dev, 5.);
        assert_eq!(*corner_smoothing, 1.);
    }

    #[test]
    fn outer_shadow_excludes_translucent_continuous_panel_interior() {
        let mut node = test_node(0, [32., 32., 96., 96.]);
        node.paint.background = Some(Color::from_rgba8(255, 255, 255, 128));
        node.paint.border_radii = CornerRadii::uniform(16.);
        node.paint.corner_smoothing = 1.;
        node.paint.shadows.push(Shadow {
            inset: false,
            offset_x: 0.,
            offset_y: 4.,
            spread: -1.,
            std_dev: 6.,
            color: Color::BLACK,
            radius: None,
        });
        let mut scene = Scene::new();
        draw_node_box(&mut scene, &node, Affine::IDENTITY, 1.);
        let crate::PaintCommand::PushClip { clip, .. } = &scene.commands[0] else {
            panic!("outer shadow must be cut out");
        };
        assert_eq!(clip.winding((64., 64.).into()), 0);
        assert_ne!(clip.winding((64., 102.).into()), 0);
        assert_ne!(
            clip.winding((33., 33.).into()),
            0,
            "continuous corner cutout preserves the exposed corner shadow"
        );
        assert!(matches!(
            scene.commands[1],
            crate::PaintCommand::BoxShadow { .. }
        ));
        assert!(matches!(scene.commands[2], crate::PaintCommand::PopLayer));
        assert!(visual_bounds(&node).y1 > 96.);
        node.paint.shadows[0].spread = -40.;
        scene = Scene::new();
        draw_node_box(&mut scene, &node, Affine::IDENTITY, 1.);
        assert!(
            !scene.commands.iter().any(
                |c| matches!(c, crate::PaintCommand::BoxShadow { std_dev, .. } if *std_dev > 0.)
            )
        );
    }

    #[test]
    fn native_motion_replaces_the_authored_target_transform() {
        let mut tree = taffy::TaffyTree::<()>::new();
        let node_id = tree.new_leaf(taffy::Style::default()).unwrap();
        let mut node = PlacedNode {
            node_id,
            parent_node_id: None,
            depth: 0,
            world_transform: Affine::IDENTITY,
            world_opacity: 1.0,
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
    fn untransformed_node_reuses_the_parent_matrix_exactly() {
        let mut node = test_node(0, [20., 30., 140., 90.]);
        node.paint.transform_origin = [
            IrLength::Percent { value: 0.3 },
            IrLength::Percent { value: 0.7 },
        ];
        let parent = Affine::new([1.25, 0.1, -0.05, 0.9, 17., 23.]);
        assert_eq!(resolve_node_transform(&node, parent), parent);
    }

    #[test]
    #[ignore = "manual CPU profile"]
    fn profile_untransformed_node_resolution() {
        fn previous(node: &PlacedNode, parent_transform: Affine) -> Affine {
            let [x0, y0, x1, y1] = node.rect;
            let resolve = |length: &IrLength, size: f32| match length {
                IrLength::Px { value } => f64::from(*value),
                IrLength::Percent { value } => f64::from(*value * size),
                IrLength::Auto => 0.,
            };
            let (static_transform, runtime_transform) = resolve_local_transforms(node);
            let origin = kurbo::Vec2::new(
                f64::from(x0) + resolve(&node.paint.transform_origin[0], x1 - x0),
                f64::from(y0) + resolve(&node.paint.transform_origin[1], y1 - y0),
            );
            parent_transform
                * Affine::translate(origin)
                * static_transform
                * runtime_transform
                * Affine::translate(-origin)
        }
        fn median(values: &mut [f64]) -> f64 {
            values.sort_by(f64::total_cmp);
            values[values.len() / 2]
        }
        let node = test_node(0, [20., 30., 140., 90.]);
        let parent = Affine::new([1.25, 0.1, -0.05, 0.9, 17., 23.]);
        let mut fast = Vec::new();
        let mut old = Vec::new();
        for sample in 0..10 {
            let started = std::time::Instant::now();
            for _ in 0..1_000_000 {
                let result = if sample % 2 == 0 {
                    resolve_node_transform(
                        std::hint::black_box(&node),
                        std::hint::black_box(parent),
                    )
                } else {
                    previous(std::hint::black_box(&node), std::hint::black_box(parent))
                };
                std::hint::black_box(result);
            }
            let elapsed = started.elapsed().as_secs_f64() * 1e9 / 1_000_000.;
            if sample % 2 == 0 {
                fast.push(elapsed);
            } else {
                old.push(elapsed);
            }
        }
        eprintln!(
            "untransformed resolve ns/node: old={:.2} fast={:.2}",
            median(&mut old),
            median(&mut fast)
        );
    }

    #[test]
    #[ignore = "manual CPU profile"]
    fn profile_hidden_scrollbar_visibility() {
        fn median(values: &mut [f64]) -> f64 {
            values.sort_by(f64::total_cmp);
            values[values.len() / 2]
        }
        let node = test_node(0, [20., 30., 140., 90.]);
        let viewport = Rect::new(0., 0., 800., 600.);
        let mut fast = Vec::new();
        let mut old = Vec::new();
        for sample in 0..10 {
            let started = std::time::Instant::now();
            for _ in 0..1_000_000 {
                let result = if sample % 2 == 0 {
                    let node = std::hint::black_box(&node);
                    node.scroll.opacity > 0.0
                        && paints_viewport(
                            node,
                            std::hint::black_box(Affine::IDENTITY),
                            std::hint::black_box(viewport),
                        )
                } else {
                    paints_viewport(
                        std::hint::black_box(&node),
                        std::hint::black_box(Affine::IDENTITY),
                        std::hint::black_box(viewport),
                    )
                };
                std::hint::black_box(result);
            }
            let elapsed = started.elapsed().as_secs_f64() * 1e9 / 1_000_000.;
            if sample % 2 == 0 {
                fast.push(elapsed);
            } else {
                old.push(elapsed);
            }
        }
        eprintln!(
            "hidden scrollbar visibility ns/node: old={:.2} fast={:.2}",
            median(&mut old),
            median(&mut fast)
        );
    }

    #[test]
    #[ignore = "manual CPU profile"]
    fn profile_recorded_background_fill() {
        fn median(values: &mut [f64]) -> f64 {
            values.sort_by(f64::total_cmp);
            values[values.len() / 2]
        }
        const ITERATIONS: usize = 200_000;
        let rect = Rect::new(20., 30., 140., 90.);
        let radii = CornerRadii::default();
        let mut scene = Scene::new();
        let mut fast = Vec::new();
        let mut old = Vec::new();
        for sample in 0..10 {
            let started = std::time::Instant::now();
            for _ in 0..ITERATIONS {
                scene.reset();
                let shape =
                    corner_shape(std::hint::black_box(rect), std::hint::black_box(radii), 0.);
                if sample % 2 == 0 {
                    scene.fill_paint_shape(
                        Fill::NonZero,
                        Affine::IDENTITY,
                        Color::BLACK,
                        None,
                        shape,
                    );
                } else {
                    // The previous representation retained this fallback path
                    // beside every analytical rectangle, even though wgpu
                    // consumed only the primitive.
                    let fallback = shape.to_path(0.1);
                    std::hint::black_box(&fallback);
                    scene.fill_paint_shape(
                        Fill::NonZero,
                        Affine::IDENTITY,
                        Color::BLACK,
                        None,
                        shape,
                    );
                }
                std::hint::black_box(scene.commands.len());
            }
            let elapsed = started.elapsed().as_secs_f64() * 1e9 / ITERATIONS as f64;
            if sample % 2 == 0 {
                fast.push(elapsed);
            } else {
                old.push(elapsed);
            }
        }
        eprintln!(
            "background fill ns/op: old={:.2} fast={:.2}",
            median(&mut old),
            median(&mut fast)
        );
    }

    #[test]
    #[ignore = "manual CPU profile"]
    fn profile_preorder_subtree_walk() {
        fn previous(mut depths: &[usize]) -> usize {
            let mut visited = 0;
            while let Some(&depth) = depths.first() {
                let len = depths[1..]
                    .iter()
                    .position(|child| *child <= depth)
                    .map_or(depths.len(), |index| index + 1);
                visited += 1 + previous(&depths[1..len]);
                depths = &depths[len..];
            }
            visited
        }
        fn cursor(depths: &[usize], at: &mut usize) -> usize {
            let depth = depths[*at];
            *at += 1;
            let mut visited = 1;
            while *at < depths.len() && depths[*at] > depth {
                visited += cursor(depths, at);
            }
            visited
        }
        fn fast(depths: &[usize]) -> usize {
            let mut at = 0;
            let mut visited = 0;
            while at < depths.len() {
                visited += cursor(depths, &mut at);
            }
            visited
        }
        fn median(values: &mut [f64]) -> f64 {
            values.sort_by(f64::total_cmp);
            values[values.len() / 2]
        }

        const NODES: usize = 256;
        const ITERATIONS: usize = 2_000;
        let depths: Vec<_> = (0..NODES).collect();
        assert_eq!(previous(&depths), NODES);
        assert_eq!(fast(&depths), NODES);
        let mut old = Vec::new();
        let mut linear = Vec::new();
        for sample in 0..10 {
            let started = std::time::Instant::now();
            for _ in 0..ITERATIONS {
                let visited = if sample % 2 == 0 {
                    fast(std::hint::black_box(&depths))
                } else {
                    previous(std::hint::black_box(&depths))
                };
                std::hint::black_box(visited);
            }
            let elapsed = started.elapsed().as_secs_f64() * 1e9 / ITERATIONS as f64;
            if sample % 2 == 0 {
                linear.push(elapsed);
            } else {
                old.push(elapsed);
            }
        }
        eprintln!(
            "256-deep preorder walk ns/tree: old={:.2} linear={:.2}",
            median(&mut old),
            median(&mut linear)
        );
    }

    #[test]
    #[ignore = "manual CPU profile"]
    fn profile_subtree_ink_culling() {
        fn encode<const SUBTREE_CULL: bool>(
            nodes: &[PlacedNode],
            scene: &mut Scene,
            text: &mut TextContext,
            scratch: &mut SceneScratch,
            viewport: Rect,
        ) {
            scene.reset();
            if SUBTREE_CULL {
                prepare_subtree_ink(nodes, &mut scratch.subtree_ink, &mut scratch.subtree_stack);
            }
            Encoder {
                tcx: text,
                device: Affine::IDENTITY,
                bg: viewport,
                viewport,
                device_scale: 1.,
                use_presentations: true,
            }
            .encode_nodes::<SUBTREE_CULL>(
                scene,
                nodes,
                &scratch.subtree_ink,
                Affine::IDENTITY,
                viewport,
                1.,
            );
            std::hint::black_box(scene.commands.len());
        }
        fn median(values: &mut [f64]) -> f64 {
            values.sort_by(f64::total_cmp);
            values[values.len() / 2]
        }

        let viewport = Rect::new(0., 0., 800., 600.);
        let mut nodes = vec![test_node(0, [0., 0., 800., 96_000.])];
        for row in 0..4_000 {
            let y = row as f32 * 24.;
            nodes.push(test_node(1, [0., y, 800., y + 20.]));
            nodes.push(test_node(2, [8., y + 2., 300., y + 18.]));
        }
        crate::layout::refresh_presentations(&mut nodes);
        assert!(should_prepare_subtree_ink(&nodes, viewport));

        let mut scene = Scene::new();
        let mut text = TextContext::new();
        let mut scratch = SceneScratch::default();
        encode::<false>(&nodes, &mut scene, &mut text, &mut scratch, viewport);
        encode::<true>(&nodes, &mut scene, &mut text, &mut scratch, viewport);

        const ITERATIONS: usize = 100;
        let mut direct = Vec::new();
        let mut ink = Vec::new();
        for sample in 0..10 {
            let started = std::time::Instant::now();
            for _ in 0..ITERATIONS {
                if sample % 2 == 0 {
                    encode::<false>(&nodes, &mut scene, &mut text, &mut scratch, viewport);
                } else {
                    encode::<true>(&nodes, &mut scene, &mut text, &mut scratch, viewport);
                }
            }
            let elapsed = started.elapsed().as_secs_f64() * 1e6 / ITERATIONS as f64;
            if sample % 2 == 0 {
                direct.push(elapsed);
            } else {
                ink.push(elapsed);
            }
        }
        let tall_viewport = Rect::new(0., 0., 800., 96_000.);
        assert!(!should_prepare_subtree_ink(&nodes, tall_viewport));
        const HEURISTIC_ITERATIONS: usize = 10_000;
        let mut heuristic = Vec::new();
        for _ in 0..10 {
            let started = std::time::Instant::now();
            for _ in 0..HEURISTIC_ITERATIONS {
                std::hint::black_box(should_prepare_subtree_ink(
                    std::hint::black_box(&nodes),
                    std::hint::black_box(tall_viewport),
                ));
            }
            heuristic.push(started.elapsed().as_secs_f64() * 1e6 / HEURISTIC_ITERATIONS as f64);
        }
        eprintln!(
            "4000-row scene us/frame: direct={:.2} subtree-ink={:.2}; all-visible sampling us/frame={:.2}",
            median(&mut direct),
            median(&mut ink),
            median(&mut heuristic),
        );
    }

    #[test]
    fn shadow_geometry_preserves_vello_parameters() {
        let shadow = Shadow {
            inset: false,
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
            inset: false,
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
    fn test_node(depth: usize, rect: [f32; 4]) -> PlacedNode {
        let mut tree = taffy::TaffyTree::<()>::new();
        let node_id = tree.new_leaf(taffy::Style::default()).unwrap();
        let mut node = PlacedNode {
            node_id,
            parent_node_id: None,
            depth: 0,
            world_transform: Affine::IDENTITY,
            world_opacity: 1.0,
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
    fn presented_scene_reuses_the_transform_shared_with_hit_testing() {
        let mut parent = test_node(0, [0., 0., 40., 40.]);
        parent.paint.runtime_transform = Some([1., 0., 0., 1., 12., 3.]);
        parent.paint.opacity = 0.5;
        let mut child = test_node(1, [4., 5., 20., 25.]);
        child.paint.runtime_transform = Some([1., 0., 0., 1., 2., -1.]);
        child.paint.opacity = 0.5;
        let mut nodes = [parent, child];
        crate::layout::refresh_presentations(&mut nodes);

        assert_eq!(nodes[0].world_transform, Affine::translate((12., 3.)));
        assert_eq!(nodes[1].world_transform, Affine::translate((14., 2.)));
        assert_eq!(nodes[1].world_opacity, 0.25);

        let ordinary = encoded(&nodes);
        let mut presented = Scene::new();
        build_presented_scene_scaled_layers(
            &mut presented,
            &[&nodes],
            &mut TextContext::new(),
            100,
            100,
            Color::TRANSPARENT,
            1.,
        );
        assert_eq!(presented.commands, ordinary.commands);
    }

    #[test]
    fn presented_scene_culls_offscreen_subtrees_but_keeps_transformed_overflow() {
        let parent = test_node(0, [200., 200., 240., 240.]);
        let mut hidden_text = test_node(1, [204., 204., 230., 220.]);
        hidden_text.paint.text = Some(Arc::from("Must not shape"));
        let sibling = test_node(0, [10., 10., 30., 30.]);
        let mut nodes = [parent, hidden_text, sibling.clone()];
        crate::layout::refresh_presentations(&mut nodes);
        let mut scene = Scene::new();
        let mut text = TextContext::new();
        build_presented_scene_scaled_layers(
            &mut scene,
            &[&nodes],
            &mut text,
            100,
            100,
            Color::TRANSPARENT,
            1.,
        );
        assert_eq!(scene.commands, encoded(&[sibling]).commands);
        assert_eq!(text.cache_entries(), (0, 0));

        let parent = test_node(0, [200., 200., 240., 240.]);
        let mut overflowing = test_node(1, [204., 204., 230., 220.]);
        overflowing.paint.runtime_transform = Some([1., 0., 0., 1., -200., -200.]);
        let mut nodes = [parent.clone(), overflowing];
        crate::layout::refresh_presentations(&mut nodes);
        let mut scene = Scene::new();
        build_presented_scene_scaled_layers(
            &mut scene,
            &[&nodes],
            &mut TextContext::new(),
            100,
            100,
            Color::TRANSPARENT,
            1.,
        );
        assert!(scene.commands.len() > encoded(&[parent]).commands.len());
    }

    #[test]
    fn unclipped_widget_overflow_disables_layout_box_culling() {
        let mut fragment = Scene::new();
        fragment.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            Color::BLACK,
            None,
            &Rect::new(-200., -200., -180., -180.),
        );
        let mut node = test_node(0, [200., 200., 240., 240.]);
        node.paint.background = None;
        node.content_origin = [200., 200.];
        node.paint.widget = Some(Arc::new(fragment));
        crate::layout::refresh_presentations(std::slice::from_mut(&mut node));
        assert!(!paint_is_cullable(&node));

        let mut scene = Scene::new();
        build_presented_scene_scaled_layers(
            &mut scene,
            &[std::slice::from_ref(&node)],
            &mut TextContext::new(),
            100,
            100,
            Color::TRANSPARENT,
            1.,
        );
        let crate::PaintCommand::Fill {
            transform, shape, ..
        } = &scene.commands[0]
        else {
            panic!("unclipped widget overflow should remain in the scene");
        };
        assert!(rect_intersects(
            transform.transform_rect_bbox(shape.bounding_box()),
            Rect::new(0., 0., 100., 100.),
        ));
    }

    #[test]
    fn subtree_ink_prepass_is_selected_only_for_cullable_nested_work() {
        let root = test_node(0, [0., 0., 100., 100.]);
        let mut nodes = vec![root.clone()];
        for row in 0..64 {
            let y = 200. + row as f32 * 24.;
            nodes.push(test_node(1, [0., y, 100., y + 20.]));
            let mut text = test_node(2, [4., y + 2., 96., y + 18.]);
            text.paint.text = Some(Arc::from("Offscreen retained row"));
            nodes.push(text);
        }
        let visible = test_node(1, [8., 8., 32., 32.]);
        nodes.push(visible.clone());
        crate::layout::refresh_presentations(&mut nodes);
        let viewport = Rect::new(0., 0., 100., 100.);
        assert!(should_prepare_subtree_ink(&nodes, viewport));

        let mut scene = Scene::new();
        let mut text = TextContext::new();
        let mut scratch = SceneScratch::default();
        build_presented_scene_scaled_layers_with_scratch(
            &mut scene,
            &[&nodes],
            &mut text,
            100,
            100,
            Color::TRANSPARENT,
            1.,
            &mut scratch,
        );
        assert_eq!(scene.commands, encoded(&[root, visible]).commands);
        assert_eq!(text.cache_entries(), (0, 0));
        let capacity = scratch.subtree_ink.capacity();
        let allocation = scratch.subtree_ink.as_ptr();
        assert!(capacity >= nodes.len());
        build_presented_scene_scaled_layers_with_scratch(
            &mut scene,
            &[&nodes],
            &mut text,
            100,
            100,
            Color::TRANSPARENT,
            1.,
            &mut scratch,
        );
        assert_eq!(scratch.subtree_ink.capacity(), capacity);
        assert_eq!(scratch.subtree_ink.as_ptr(), allocation);

        let mut visible_nodes = vec![test_node(0, [0., 0., 100., 100.])];
        visible_nodes.extend((0..64).map(|_| test_node(1, [8., 8., 32., 32.])));
        crate::layout::refresh_presentations(&mut visible_nodes);
        assert!(!should_prepare_subtree_ink(&visible_nodes, viewport));
    }

    #[test]
    fn foreground_blur_expands_bounds_and_shares_opacity_layer() {
        let mut node = test_node(0, [20., 20., 40., 40.]);
        node.paint.foreground_blur = 3.;
        node.paint.opacity = 0.5;
        assert_eq!(visual_bounds(&node), Rect::new(11., 11., 49., 49.));
        let scene = encoded(&[node]);
        let filters: Vec<_> = scene
            .commands
            .iter()
            .filter_map(|command| match command {
                crate::PaintCommand::PushLayer {
                    alpha,
                    filter: Some(filter),
                    ..
                } => Some((*alpha, filter)),
                _ => None,
            })
            .collect();
        assert_eq!(filters.len(), 1);
        assert_eq!(filters[0].0, 0.5);
        assert_eq!(
            crate::filter::GaussianBlur::from_filter(filters[0].1)
                .unwrap()
                .sigma,
            3.
        );
        let mut edge = test_node(0, [-9., 20., -1., 40.]);
        assert!(!paints_viewport(
            &edge,
            Affine::IDENTITY,
            Rect::new(0., 0., 100., 100.)
        ));
        edge.paint.foreground_blur = 3.;
        assert!(paints_viewport(
            &edge,
            Affine::IDENTITY,
            Rect::new(0., 0., 100., 100.)
        ));
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
    fn ancestor_clip_culls_overlay_scrollbars_with_their_node() {
        let mut parent = test_node(0, [0., 0., 20., 20.]);
        parent.own_clip = Some([0., 0., 20., 20.]);
        let mut child = test_node(1, [30., 30., 50., 50.]);
        child.paint.background = None;
        child.scroll = crate::layout::ScrollMetrics {
            port: child.rect,
            scrollable: [false, true],
            range: [0., 100.],
            offset: [0., 50.],
            opacity: 1.,
            interaction: 0,
        };
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
