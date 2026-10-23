//! Resolve anchors in layout space so paint, hit testing and scroll share geometry.
use super::*;
impl Adapter {
    pub(super) fn position_popovers(
        &mut self,
        engine: &Engine,
        tcx: &mut TextContext,
        width: u32,
        height: u32,
    ) {
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
                .and_then(|p| self.visual_bounds(p.node_id));
            let Some(anchor) = anchor else { continue };
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
            let below = (height as f32 - 8. - anchor[3] - 6.).max(0.);
            let above = (anchor[1] - 8. - 6.).max(0.);
            let flip = panel_height > below && above > below;
            let available = if flip { above } else { below };
            let x = anchor[0].clamp(
                8f32.min(width as f32 / 2.),
                (width as f32 - 8. - panel_width).max(8f32.min(width as f32 / 2.)),
            );
            let y = if flip {
                anchor[1] - 6. - panel_height.min(available)
            } else {
                anchor[3] + 6.
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
            // Nested popovers can anchor into the panel just placed.
            self.relayout_popovers(engine, tcx, width, height);
        }
    }
    fn relayout_popovers(
        &mut self,
        engine: &Engine,
        tcx: &mut TextContext,
        width: u32,
        height: u32,
    ) {
        compute_and_walk_with_scroll_into(
            &mut self.tree,
            self.nodes[&engine.root()].id,
            [width as f32, height as f32],
            tcx,
            &self.scroll_offsets,
            &mut self.placed,
        );
        self.stats.layout_runs += 1;
        self.stats.flatten_visits += self.placed.len();
        self.placed_indices.clear();
        self.placed_indices
            .extend(self.placed.iter().enumerate().map(|(i, p)| (p.node_id, i)));
        self.update_presentation();
    }
}
