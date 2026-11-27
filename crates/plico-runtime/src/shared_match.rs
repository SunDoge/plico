// Match identity handoff and position sampling follow MyGO ui/match.go (MIT,
// revision 74c3f26d). See licenses/MyGo-MIT.txt.
use super::*;
use kurbo::{Affine, Rect, Vec2};
use plico::{Animation, SharedElementGeometry, SharedElementMotion};
use plico_renderer::scene::PresentedFlight;
use std::collections::HashMap;

#[derive(Default)]
pub(crate) struct SharedMatches {
    records: HashMap<plico::Key, Match>,
    candidates: HashMap<plico::Key, (usize, Animation)>,
    pub(crate) active: bool,
    pub(crate) flights: Vec<PresentedFlight>,
    ordered: Vec<(plico::Key, (usize, Animation))>,
    pub(crate) reduced: bool,
}

struct Match {
    owner: NodeId,
    last: SharedElementGeometry,
    motion: SharedElementMotion,
    flying: bool,
}

impl Adapter {
    pub(super) fn present_shared_matches(&mut self, engine: &Engine) {
        let declarations = engine.shared_elements();
        if declarations.len() == 0 && self.shared_matches.records.is_empty() {
            return;
        }
        let mut shared = std::mem::take(&mut self.shared_matches);
        for (entity, key, animation) in declarations {
            let Some(index) = self
                .backend_id(entity)
                .and_then(|id| self.placed_indices.get(&id))
                .copied()
            else {
                continue;
            };
            let node = &self.placed[index];
            if node.world_opacity <= 0. {
                continue;
            }
            // A retained owner wins while both endpoints are present. The
            // lowest preorder index resolves fresh duplicates deterministically.
            let previous = shared.records.get(&key).map(|record| record.owner);
            let candidate = shared.candidates.entry(key).or_insert((index, animation));
            if previous == Some(node.node_id)
                || (previous != Some(self.placed[candidate.0].node_id) && index < candidate.0)
            {
                *candidate = (index, animation);
            }
        }

        // A node may stop declaring its match before it leaves the tree.
        shared.records.retain(|key, record| {
            if shared.candidates.contains_key(key) {
                return true;
            }
            if record.flying {
                self.present_shared_box(record.owner, None);
            }
            false
        });
        shared.active = false;
        shared.flights.clear();
        shared.ordered.extend(shared.candidates.drain());
        shared
            .ordered
            .sort_unstable_by_key(|(_, (index, _))| *index);
        for (key, (index, animation)) in shared.ordered.drain(..) {
            let node = &self.placed[index];
            let id = node.node_id;
            let parent = node
                .parent_node_id
                .and_then(|id| self.placed_indices.get(&id))
                .map_or(Affine::IDENTITY, |&i| self.placed[i].world_transform);
            let [left, top, right, bottom] = node.rect.map(f64::from);
            let bounds = plico_renderer::scene::resolve_node_transform(node, parent)
                .transform_rect_bbox(Rect::new(left, top, right, bottom));
            let target = SharedElementGeometry::rect([
                bounds.x0 as f32,
                bounds.y0 as f32,
                bounds.width() as f32,
                bounds.height() as f32,
            ]);
            let record = shared.records.entry(key).or_insert_with(|| Match {
                owner: id,
                last: target,
                motion: SharedElementMotion::default(),
                flying: false,
            });
            let changed_owner = record.owner != id;
            if changed_owner && record.flying {
                self.present_shared_box(record.owner, None);
            }

            let mut geometry = target;
            if changed_owner || record.flying {
                let frame = record.motion.sample_at(
                    engine.time().now,
                    shared.reduced,
                    key,
                    record.last,
                    target,
                    animation,
                );
                if let Some(frame) = frame.filter(|f| f.active) {
                    geometry = frame.geometry;
                    if let Some(transform) = fitting_transform(bounds, geometry.clip) {
                        let end = self.present_shared_box(id, Some(transform));
                        let layer_end = self.shared_layer_end(engine, id);
                        let [x, y, w, h] = geometry.clip;
                        let [scale, _, _, _, dx, dy] = transform.as_coeffs();
                        shared.flights.push(PresentedFlight {
                            start: index,
                            end,
                            layer_end,
                            clip: [x, y, x + w, y + h],
                            raster_correction: scale.recip(),
                            text_phase: if scale == 1. { [dx, dy] } else { [0.; 2] },
                        });
                    }
                    record.flying = true;
                    shared.active = true;
                } else {
                    self.present_shared_box(id, None);
                    record.flying = false;
                }
            } else {
                record.motion.place(key, target, engine.time().now);
            }
            record.owner = id;
            record.last = geometry;
        }
        self.shared_matches = shared;
    }

    fn shared_layer_end(&self, engine: &Engine, id: NodeId) -> usize {
        if let Some(overlay) = self.placed_ancestors(id).find(|node| {
            self.reverse
                .get(&node.node_id)
                .and_then(|&entity| engine.node(entity))
                .is_some_and(|node| node.kind == Kind::Overlay)
        }) {
            let start = self.placed_indices[&overlay.node_id];
            return start
                + 1
                + self.placed[start + 1..]
                    .iter()
                    .take_while(|node| node.depth > overlay.depth)
                    .count();
        }
        engine
            .node(engine.root())
            .into_iter()
            .flat_map(|root| root.children.iter())
            .copied()
            .filter(|&e| {
                engine
                    .node(e)
                    .is_some_and(|node| node.kind == Kind::Overlay)
            })
            .filter_map(|e| {
                self.backend_id(e)
                    .and_then(|id| self.placed_indices.get(&id))
                    .copied()
            })
            .min()
            .unwrap_or(self.placed.len())
    }

    fn present_shared_box(&mut self, id: NodeId, transform: Option<Affine>) -> usize {
        let Some(&start) = self.placed_indices.get(&id) else {
            return 0;
        };
        self.update_presentation_node(start);
        if let Some(transform) = transform {
            self.placed[start].world_transform = transform * self.placed[start].world_transform;
        }
        let depth = self.placed[start].depth;
        let mut index = start + 1;
        while index < self.placed.len() && self.placed[index].depth > depth {
            self.update_presentation_node(index);
            index += 1;
        }
        self.scene_dirty_nodes.insert(id);
        self.hit.get_mut().dirty = true;
        self.stats.changes |= plico::Changes::TRANSFORM | plico::Changes::HIT_TEST;
        index
    }
}

fn fitting_transform(rest: Rect, [x, y, width, height]: [f32; 4]) -> Option<Affine> {
    let [x, y, width, height] = [x, y, width, height].map(f64::from);
    let scale = (width / rest.width()).max(height / rest.height());
    if !scale.is_finite() || scale <= 0. {
        return None;
    }
    Some(
        Affine::translate(Vec2::new(x + width / 2., y + height / 2.))
            * Affine::scale(scale)
            * Affine::translate(-rest.center().to_vec2()),
    )
}
