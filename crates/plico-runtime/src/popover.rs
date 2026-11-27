//! Lay out anchored panels at their destination, then present the current pose.
use super::*;
use crate::popup_motion::{PopupMotion, smoothstep};
use crate::shader::{PopupOptics, SurfacePose};
use plico_renderer::layout::PlacedNode;
use plico_renderer::style::CornerRadii;
// MyGO's ui/runtime.go repaintFrame reuses laid-out elements when only
// their pose or draw callbacks change (reference revision f651b106).
pub(super) struct PopupPlacement {
    root: Entity,
    anchor: NodeId,
    panel: NodeId,
    face: Option<Entity>,
    content: Option<PopupContent>,
    flip: bool,
}

struct PopupContent {
    id: NodeId,
    clip: Option<[f32; 4]>,
    radii: CornerRadii,
}

impl Adapter {
    pub(super) fn position_popovers(
        &mut self,
        engine: &Engine,
        tcx: &mut TextContext,
        width: u32,
        height: u32,
    ) {
        self.popup_placements.clear();
        let roots: Vec<_> = engine
            .node(engine.root())
            .unwrap()
            .children
            .iter()
            .copied()
            .filter(|e| {
                engine
                    .node(*e)
                    .is_some_and(|n| n.style.popup_anchor.is_some())
            })
            .collect();
        if roots.is_empty() {
            return;
        }
        // Measure at viewport constraints first; a previously flipped/clamped panel must
        // be allowed to grow again when the window or anchor moves.
        for root in &roots {
            if let Some(panel) = engine
                .node(*root)
                .unwrap()
                .children
                .iter()
                .find(|e| engine.node(**e).is_some_and(|n| n.kind == Kind::Dialog))
            {
                let id = self.nodes[panel].id;
                let mut style = self.tree.style(id).unwrap().clone();
                style.position = Position::Absolute;
                style.max_size.width = length((width as f32 - 16.).max(0.));
                style.max_size.height = length(
                    engine
                        .node(*panel)
                        .unwrap()
                        .style
                        .max_height
                        .unwrap_or(f32::MAX)
                        .min((height as f32 - 16.).max(0.)),
                );
                self.tree.set_style(id, style).unwrap();
            }
        }
        self.relayout_popovers(engine, tcx, width, height);
        for root in roots {
            let token = engine.node(root).unwrap().style.popup_anchor.unwrap();
            let anchor = self
                .placed
                .iter()
                .find(|p| {
                    self.reverse
                        .get(&p.node_id)
                        .and_then(|e| engine.node(*e))
                        .is_some_and(|n| n.style.anchor_id == Some(token))
                })
                .and_then(|p| {
                    self.visual_bounds(p.node_id)
                        .map(|bounds| (p.node_id, bounds))
                });
            let Some((anchor_id, anchor)) = anchor else {
                continue;
            };
            let Some(panel) = engine
                .node(root)
                .unwrap()
                .children
                .iter()
                .find(|e| engine.node(**e).is_some_and(|n| n.kind == Kind::Dialog))
            else {
                continue;
            };
            let id = self.nodes[panel].id;
            let rect = self.placed[self.placed_indices[&id]].rect;
            let panel_width = rect[2] - rect[0];
            let panel_height = rect[3] - rect[1];
            let liquid = engine.node(root).unwrap().style.liquid_popup;
            let centered = engine.node(root).unwrap().style.popup_over_anchor;
            let gap = if liquid { 12. } else { 6. };
            let below = (height as f32 - 8. - anchor[3] - gap).max(0.);
            let above = (anchor[1] - 8. - gap).max(0.);
            let flip = panel_height > below && above > below;
            let available = if centered {
                (height as f32 - 16.).max(0.)
            } else if flip {
                above
            } else {
                below
            };
            let align_center = centered
                || liquid
                    && engine.node(*panel).unwrap().style.role == Some(plico::WidgetRole::Menu);
            let x = (if align_center {
                (anchor[0] + anchor[2] - panel_width) * 0.5
            } else {
                anchor[0]
            })
            .clamp(
                8f32.min(width as f32 / 2.),
                (width as f32 - 8. - panel_width).max(8f32.min(width as f32 / 2.)),
            );
            let y = if centered {
                ((anchor[1] + anchor[3] - panel_height) * 0.5)
                    .clamp(8., (height as f32 - 8. - panel_height).max(8.))
            } else if flip {
                anchor[1] - gap - panel_height.min(available)
            } else {
                anchor[3] + gap
            };
            let mut style = self.tree.style(id).unwrap().clone();
            style.inset.left = length(x);
            style.inset.top = length(y.clamp(0., height as f32));
            style.max_size.height = length(
                available.min(
                    engine
                        .node(*panel)
                        .unwrap()
                        .style
                        .max_height
                        .unwrap_or(f32::MAX),
                ),
            );
            self.tree.set_style(id, style).unwrap();
            let children = &engine.node(*panel).unwrap().children;
            let face = children
                .iter()
                .copied()
                .find(|node| engine.node(*node).is_some_and(|n| n.kind == Kind::Shader));
            let content = children
                .iter()
                .find(|node| engine.node(**node).is_some_and(|n| n.kind == Kind::Column))
                .map(|node| self.nodes[node].id);
            // A glass portal can paint a transient neck between the measured
            // anchor and panel. Keep it aligned when the panel flips or clamps.
            if let Some(bridge) = engine
                .node(root)
                .unwrap()
                .children
                .iter()
                .find(|e| engine.node(**e).is_some_and(|n| n.kind == Kind::Shader))
            {
                let bridge_id = self.nodes[bridge].id;
                let mut bridge_style = self.tree.style(bridge_id).unwrap().clone();
                let left = anchor[0].max(x);
                let right = anchor[2].min(x + panel_width);
                let bridge_width = (right - left).clamp(0., 72.);
                let edge = if flip { anchor[1] - gap } else { anchor[3] };
                bridge_style.inset.left = length((left + right - bridge_width) * 0.5);
                bridge_style.inset.top = length(edge - 2.);
                bridge_style.size.width = length(bridge_width);
                bridge_style.size.height = length(gap + 4.);
                self.tree.set_style(bridge_id, bridge_style).unwrap();
            }
            // Nested popovers can anchor into the panel just placed.
            self.relayout_popovers(engine, tcx, width, height);
            let content = content
                .and_then(|id| self.placed_indices.get(&id))
                .map(|&index| &self.placed[index])
                .map(|node| PopupContent {
                    id: node.node_id,
                    clip: node.own_clip,
                    radii: node.own_clip_radii,
                });
            self.popup_placements.push(PopupPlacement {
                root,
                anchor: anchor_id,
                panel: id,
                face,
                content,
                flip,
            });
        }
    }

    pub(super) fn present_popovers(&mut self, engine: &Engine) {
        self.effect_poses.clear();
        self.popup_motion_active = false;
        self.popup_motions.retain(|entity, _| {
            engine.node(*entity).is_some_and(|popup| {
                popup.style.popup_motion || popup.style.liquid_popup && popup.style.modal_open
            })
        });
        let mut changed = Vec::new();
        let mut paint_poses = Vec::new();
        for placement in &self.popup_placements {
            let Some(popup) = engine.node(placement.root) else {
                continue;
            };
            let popup_style = popup.style;
            let centered = popup_style.popup_over_anchor;
            if !popup_style.popup_motion {
                if popup_style.liquid_popup && popup_style.modal_open {
                    // The opaque/reduced-motion fallback is already visible at
                    // its destination. Switching back to glass must keep that pose.
                    self.popup_motions.insert(
                        placement.root,
                        PopupMotion::open_at_rest(centered, engine.time().now),
                    );
                }
                continue;
            }
            let Some(face) = placement.face else {
                continue;
            };
            let Some(anchor) = self.visual_bounds(placement.anchor) else {
                continue;
            };
            let Some(&index) = self.placed_indices.get(&placement.panel) else {
                continue;
            };
            let panel = &self.placed[index];
            let radius = panel.paint.border_radii.max();
            let [x, y, right, bottom] = panel.rect;
            let panel_width = right - x;
            let shown_height = bottom - y;
            let flip = placement.flip;
            let motion = self
                .popup_motions
                .entry(placement.root)
                .or_insert_with(|| PopupMotion::new(centered));
            if centered != matches!(motion, PopupMotion::Morph(_)) {
                *motion = PopupMotion::new(centered);
            }
            self.popup_motion_active |= motion.sample(engine.time().now, popup_style.modal_open);
            if let PopupMotion::Morph(morph) = motion {
                let [lead, cross, calm, throw, carry] = morph.frame(
                    self.reverse
                        .get(&placement.panel)
                        .and_then(|entity| engine.node(*entity))
                        .is_some_and(|panel| panel.style.role == Some(plico::WidgetRole::Menu)),
                );
                if let Some(&anchor_entity) = self.reverse.get(&placement.anchor)
                    && let Some(trigger) = engine.node(anchor_entity)
                {
                    let label_opacity = 1. - smoothstep(0., 0.02, lead);
                    for &child_entity in trigger.children {
                        let Some(child) = engine.node(child_entity) else {
                            continue;
                        };
                        let opacity = if child.kind == Kind::Shader {
                            f32::from(lead <= 0.)
                        } else {
                            label_opacity
                        };
                        paint_poses.push((self.nodes[&child_entity].id, opacity, throw));
                    }
                }
                let anchor_size = [anchor[2] - anchor[0], anchor[3] - anchor[1]];
                let panel_size = [panel_width, shown_height];
                let panel_center = [x + panel_width * 0.5, y + shown_height * 0.5];
                let anchor_center = [(anchor[0] + anchor[2]) * 0.5, (anchor[1] + anchor[3]) * 0.5];
                let tall = (panel_size[1] - anchor_size[1]).abs()
                    >= (panel_size[0] - anchor_size[0]).abs();
                let [fx, fy] = if tall { [cross, lead] } else { [lead, cross] };
                let lerp = |a, b, mix| a + (b - a) * mix;
                let size = [
                    lerp(anchor_size[0], panel_size[0], fx).max(2.),
                    lerp(anchor_size[1], panel_size[1], fy).max(2.),
                ];
                let center = [
                    lerp(anchor_center[0], panel_center[0], fx),
                    lerp(anchor_center[1] + throw, panel_center[1], fy) + carry,
                ];
                let round = size[0].min(size[1]) * 0.5;
                let t = ((calm - 0.3) / 0.7).clamp(0., 1.);
                let radius = lerp(round, radius.min(round), t * t * (3. - 2. * t));
                let [cx, cy] = center;
                let [hx, hy] = [size[0] * 0.5, size[1] * 0.5];
                // The shared uniform keeps the Select slice selector at zero.
                let mut geometry = [0.; 12];
                geometry[8] = radius;
                self.effect_poses.insert(
                    face,
                    SurfacePose {
                        bounds: Some([cx - hx, cy - hy, cx + hx, cy + hy]),
                        popup_geometry: Some(geometry),
                        popup_optics: Some(PopupOptics::Morph {
                            frost: smoothstep(0., 0.05, calm),
                            visible: lead > 0.,
                        }),
                    },
                );
                if let Some(content) = &placement.content
                    && let Some(&index) = self.placed_indices.get(&content.id)
                {
                    let scale = ((size[0] / panel_width.max(1.))
                        * (size[1] / shown_height.max(1.)))
                    .sqrt()
                    .clamp(0.1, 1.5);
                    let dx = cx - panel_center[0];
                    let dy = cy - panel_center[1];
                    let node = &mut self.placed[index];
                    let transform = (scale != 1. || dx != 0. || dy != 0.)
                        .then_some([scale, 0., 0., scale, dx, dy]);
                    let (clip, radii) = if transform.is_some() {
                        let [cx, cy] = rect_center(node.rect);
                        let hx = hx / scale;
                        let hy = hy / scale;
                        (
                            Some([cx - hx, cy - hy, cx + hx, cy + hy]),
                            CornerRadii::uniform(radius / scale),
                        )
                    } else {
                        (content.clip, content.radii)
                    };
                    let blur = 5.5 * (1. - smoothstep(0., 1., calm));
                    // Shape native paragraphs at their settled density. Reflowing
                    // at the live scale can change line breaks through hinting.
                    let raster_correction = scale.recip();
                    let paint_changed = node.paint.opacity != morph.content
                        || node.paint.foreground_blur != blur
                        || node.paint.raster_scale_correction != raster_correction;
                    node.paint.opacity = morph.content;
                    node.paint.foreground_blur = blur;
                    node.paint.raster_scale_correction = raster_correction;
                    if apply_content_pose(node, transform, clip, radii) || paint_changed {
                        let id = node.node_id;
                        changed.push(id);
                    }
                }
                continue;
            }
            let leading = if flip { y + shown_height } else { y };
            if let PopupMotion::List(list) = motion {
                let [opening, tension, reveal] = list.frame();
                // The joined glass shader owns the moving shadow. Fade in the
                // detached panel's elevation as the joined silhouette settles.
                let shadow_alpha = ((opening - 0.93) / 0.07).clamp(0., 1.);
                let mut shadows = self
                    .tree
                    .get_node_context(placement.panel)
                    .unwrap()
                    .shadows
                    .clone();
                for shadow in &mut shadows {
                    shadow.color.components[3] *= shadow_alpha;
                }
                let panel = &mut self.placed[index];
                if panel.paint.shadows != shadows {
                    panel.paint.shadows = shadows;
                    self.scene_dirty_nodes.insert(placement.panel);
                    self.stats.paint_writes += 1;
                }

                if let Some(content) = &placement.content
                    && let Some(&entity) = self.reverse.get(&content.id)
                    && let Some(viewport) = engine.node(entity)
                {
                    let items = viewport.children.iter().copied().filter(|&child| {
                        engine.node(child).is_some_and(|child| {
                            matches!(
                                child.style.role,
                                Some(
                                    plico::WidgetRole::Option
                                        | plico::WidgetRole::MenuItem
                                        | plico::WidgetRole::Separator
                                )
                            )
                        })
                    });
                    for (index, child) in items.enumerate() {
                        let entered = ((reveal - index as f32 * 0.06) / 0.22).clamp(0., 1.);
                        paint_poses.push((self.nodes[&child].id, entered, 6. * (1. - entered)));
                    }
                }
                let direction = if flip { -1. } else { 1. };
                let edge = if flip { anchor[1] } else { anchor[3] };
                let far = leading + direction * shown_height;
                let anchor_width = anchor[2] - anchor[0];
                let anchor_mid = (anchor[0] + anchor[2]) * 0.5;
                let center = anchor_mid + (x + panel_width * 0.5 - anchor_mid) * opening;
                let (top, morph_width, morph_height) = {
                    let near_progress = (opening / 0.3).clamp(0., 1.);
                    let ease = near_progress * near_progress * (3. - 2. * near_progress);
                    let near_start = edge - direction * 6.;
                    let leading =
                        near_start + (leading - direction * 11. * tension - near_start) * ease;
                    let far_start = edge - direction;
                    let trailing = far_start + (far - far_start) * opening;
                    (
                        leading.min(trailing),
                        0.6 * anchor_width + (panel_width - 0.6 * anchor_width) * opening,
                        (trailing - leading).abs().max(1.),
                    )
                };
                let left = center - morph_width * 0.5;
                let pose = self.effect_poses.entry(face).or_default();
                pose.bounds = Some([left, top, left + morph_width, top + morph_height]);
                pose.popup_optics = Some(PopupOptics::List {
                    opening,
                    radius: radius * (1. + 0.8 * tension),
                });

                if opening < 1.
                    && let Some(&anchor_entity) = self.reverse.get(&placement.anchor)
                    && let Some(trigger_face) = engine.node(anchor_entity).and_then(|anchor| {
                        anchor.children.iter().find(|node| {
                            engine.node(**node).is_some_and(|n| n.kind == Kind::Shader)
                        })
                    })
                {
                    let trigger = [anchor_mid, (anchor[1] + anchor[3]) * 0.5];
                    let panel = [center, top + morph_height * 0.5];
                    let reach = (direction * (if flip { top } else { top + morph_height } - edge)
                        / 90.)
                        .clamp(0., 1.);
                    let softness = 50. * tension * reach * reach * (3. - 2. * reach);
                    let neck = anchor_width * (0.2 + 0.4 * tension);
                    let radius = (24. * (1. + 0.8 * tension))
                        .min(morph_height * 0.5)
                        .min(morph_width * 0.5);
                    for (entity, origin, half) in [
                        (*trigger_face, trigger, -direction),
                        (face, panel, direction),
                    ] {
                        self.effect_poses.entry(entity).or_default().popup_geometry = Some([
                            trigger[0] - origin[0],
                            trigger[1] - origin[1],
                            anchor_width,
                            anchor[3] - anchor[1],
                            panel[0] - origin[0],
                            panel[1] - origin[1],
                            morph_width,
                            morph_height,
                            radius,
                            softness,
                            neck,
                            half,
                        ]);
                    }
                }
            }
        }
        let previous = std::mem::take(&mut self.popup_painted);
        for (id, opacity, dy) in paint_poses {
            self.popup_painted.insert(id);
            if self.paint_popup_child(id, opacity, dy) {
                changed.push(id);
            }
        }
        for id in previous {
            if !self.popup_painted.contains(&id) && self.paint_popup_child(id, 1., 0.) {
                changed.push(id);
            }
        }
        for id in changed {
            self.scene_dirty_nodes.insert(id);
            self.hit.get_mut().dirty = true;
            self.stats.changes |= plico::Changes::TRANSFORM | plico::Changes::HIT_TEST;
            self.update_presentation_subtree(id);
        }
    }

    fn paint_popup_child(&mut self, id: NodeId, opacity: f32, dy: f32) -> bool {
        let Some(&index) = self.placed_indices.get(&id) else {
            return false;
        };
        let declared = self.tree.get_node_context(id).unwrap();
        let opacity = declared.opacity * opacity;
        let transform = if dy == 0. {
            declared.runtime_transform
        } else {
            let mut transform = declared
                .runtime_transform
                .unwrap_or([1., 0., 0., 1., 0., 0.]);
            transform[5] += dy;
            Some(transform)
        };
        let paint = &mut self.placed[index].paint;
        if paint.opacity == opacity && paint.runtime_transform == transform {
            return false;
        }
        paint.opacity = opacity;
        paint.runtime_transform = transform;
        true
    }

    fn relayout_popovers(
        &mut self,
        engine: &Engine,
        tcx: &mut TextContext,
        width: u32,
        height: u32,
    ) {
        let root = self.nodes[&engine.root()].id;
        plico_renderer::layout::compute_layout_with_widgets(
            &mut self.tree,
            root,
            [width as f32, height as f32],
            tcx,
            1.,
            |_, _| None,
        );
        self.stats.layout_runs += 1;
        let composed = self.rebuild_placed(root);
        self.update_presentation(composed);
    }
}

fn rect_center([left, top, right, bottom]: [f32; 4]) -> [f32; 2] {
    [(left + right) * 0.5, (top + bottom) * 0.5]
}

fn apply_content_pose(
    node: &mut PlacedNode,
    transform: Option<[f32; 6]>,
    clip: Option<[f32; 4]>,
    radii: CornerRadii,
) -> bool {
    if node.paint.runtime_transform == transform
        && node.own_clip == clip
        && node.own_clip_radii == radii
    {
        return false;
    }
    node.paint.runtime_transform = transform;
    node.own_clip = clip;
    node.own_clip_radius = radii.max();
    node.own_clip_radii = radii;
    true
}
