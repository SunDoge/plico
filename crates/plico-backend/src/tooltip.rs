//! Host-clock tooltip presentation; no application state or retained callbacks.
use super::*;
use plico_renderer::Point;
use std::time::Duration;
const DELAY: Duration = Duration::from_millis(600);
const WARM: Duration = Duration::from_millis(400);
type VisualKey = (Entity, Arc<str>, Theme, [f32; 4], u32, u32);
pub(super) struct Tooltips {
    pointer: Option<Point>,
    window_active: bool,
    hover: Option<Entity>,
    since: Duration,
    focus: Option<Entity>,
    pub prefer_hover: bool,
    shown: Option<Entity>,
    dismissed: Option<Entity>,
    last_visible: Option<Duration>,
    deadline: Option<Duration>,
    dirty: bool,
    ids: HashMap<Entity, u64>,
    layout_node: Option<NodeId>,
    visual: Option<VisualKey>,
    layer: Vec<PlacedNode>,
}
impl Default for Tooltips {
    fn default() -> Self {
        Self {
            pointer: None,
            window_active: true,
            hover: None,
            since: Duration::ZERO,
            focus: None,
            prefer_hover: false,
            shown: None,
            dismissed: None,
            last_visible: None,
            deadline: None,
            dirty: false,
            ids: HashMap::new(),
            layout_node: None,
            visual: None,
            layer: Vec::new(),
        }
    }
}
impl Tooltips {
    pub(super) fn layer(&self) -> &[PlacedNode] {
        &self.layer
    }
    pub(super) fn needs_frame(&self) -> bool {
        self.dirty || self.deadline.is_some()
    }
}
impl Adapter {
    pub(super) fn presented(&mut self) -> Vec<PlacedNode> {
        self.stats.placed_node_clones += self.placed.len() + self.tips.layer.len();
        let mut nodes = self.placed.clone();
        nodes.extend_from_slice(&self.tips.layer);
        nodes
    }
    fn tooltip_owner(&self, engine: &Engine, entity: Entity) -> Option<Entity> {
        if engine.node(entity)?.style.disabled || !self.modal_allows(engine, entity) {
            return None;
        }
        let start = self.nodes.get(&entity)?.id;
        for placed in self.placed_chain(start) {
            let entity = *self.reverse.get(&placed.node_id)?;
            let node = engine.node(entity)?;
            if node.kind == Kind::TooltipAnchor && !node.text.is_empty() && !node.style.disabled {
                return Some(entity);
            }
        }
        None
    }
    fn tooltip_anchor_visible(&self, entity: Entity) -> bool {
        let Some((width, height)) = self.viewport else {
            return false;
        };
        let Some(p) = self
            .nodes
            .get(&entity)
            .and_then(|n| self.placed_indices.get(&n.id))
            .map(|i| &self.placed[*i])
        else {
            return false;
        };
        self.visible_bounds(p.node_id).is_some_and(|rect| {
            rect[0].max(0.) < rect[2].min(width as f32)
                && rect[1].max(0.) < rect[3].min(height as f32)
        })
    }
    fn tooltip_hover(&self, engine: &Engine) -> Option<Entity> {
        let point = self.tips.pointer?;
        let node = self
            .placed
            .iter()
            .rev()
            .find(|p| self.visual_contains(p.node_id, point.x, point.y))?;
        self.tooltip_owner(engine, *self.reverse.get(&node.node_id)?)
    }
    pub(super) fn dismiss_tooltip(&mut self) {
        self.tips.dismissed = self.tips.shown.or(self.tips.hover).or(self.tips.focus);
        self.tips.dirty = true;
    }
    pub(super) fn tooltip_event(&mut self, engine: &Engine, event: &UiEvent) -> bool {
        if self.tips.ids.is_empty() {
            return false;
        }
        match event {
            UiEvent::Pointer(pointer) => {
                self.tips.pointer =
                    if matches!(pointer.phase, PointerPhase::Leave | PointerPhase::Cancel) {
                        None
                    } else {
                        Some(pointer.position)
                    };
                self.tips.prefer_hover = true;
                let hover = self.tooltip_hover(engine);
                if hover != self.tips.hover {
                    self.tips.hover = hover;
                    self.tips.since = engine.time().now;
                }
                if pointer.phase == PointerPhase::Down {
                    self.dismiss_tooltip();
                }
                self.tips.dirty = true;
            }
            UiEvent::Key(key) if key.phase == KeyPhase::Down => {
                self.tips.prefer_hover = false;
                if key.key == "Escape" {
                    if self
                        .focused
                        .and_then(|e| engine.input(e))
                        .is_some_and(|(buffer, _)| buffer.is_composing())
                    {
                        return false;
                    }
                    let visible = self.tips.shown.is_some();
                    self.dismiss_tooltip();
                    return visible;
                }
                if matches!(key.key.as_str(), "Enter" | " " | "Space") {
                    self.dismiss_tooltip();
                }
                self.tips.dirty = true;
            }
            UiEvent::Focus(active) => {
                self.tips.window_active = *active;
                if !active {
                    self.tips.pointer = None;
                }
                self.tips.dirty = true;
            }
            _ => {}
        }
        false
    }
    pub(super) fn update_tooltip(
        &mut self,
        engine: &Engine,
        tcx: &mut TextContext,
        width: u32,
        height: u32,
    ) {
        let now = engine.time().now;
        // Description ids are stable for surviving entities and disjoint from all
        // actual semantic ids, including on future arena generation rollover.
        self.tips.ids.retain(|e, _| {
            engine
                .node(*e)
                .is_some_and(|n| n.kind == Kind::TooltipAnchor && !n.text.is_empty())
        });
        let missing = self
            .placed
            .iter()
            .filter_map(|p| self.reverse.get(&p.node_id))
            .any(|e| {
                engine
                    .node(*e)
                    .is_some_and(|n| n.kind == Kind::TooltipAnchor && !n.text.is_empty())
                    && !self.tips.ids.contains_key(e)
            });
        if missing || (self.stats.created > 0 && !self.tips.ids.is_empty()) {
            let mut occupied: HashSet<_> = self.nodes.keys().map(|e| semantic_id(*e)).collect();
            let old_ids = std::mem::take(&mut self.tips.ids);
            for (e, id) in old_ids {
                if occupied.insert(id) {
                    self.tips.ids.insert(e, id);
                }
            }
            let mut next = u64::MAX;
            for e in self
                .placed
                .iter()
                .filter_map(|p| self.reverse.get(&p.node_id).copied())
            {
                if engine
                    .node(e)
                    .is_some_and(|n| n.kind == Kind::TooltipAnchor && !n.text.is_empty())
                    && !self.tips.ids.contains_key(&e)
                {
                    while occupied.contains(&next) {
                        next = next.checked_sub(1).expect("semantic id space exhausted");
                    }
                    self.tips.ids.insert(e, next);
                    occupied.insert(next);
                }
            }
        }
        if self.tips.ids.is_empty() {
            if self.tips.shown.take().is_some() {
                self.stats.paint_writes += 1;
                self.stats.changes |= plico_core::Changes::PAINT | plico_core::Changes::SEMANTICS;
            }
            self.tips.layer.clear();
            self.tips.visual = None;
            self.tips.deadline = None;
            self.tips.dirty = false;
            self.tips.hover = None;
            self.tips.focus = None;
            return;
        }
        let hover = if self.tips.window_active {
            self.tooltip_hover(engine)
        } else {
            None
        };
        if hover != self.tips.hover {
            self.tips.hover = hover;
            self.tips.since = now;
        }
        let focus = if self.focus_visible && self.tips.window_active {
            self.focused.and_then(|e| self.tooltip_owner(engine, e))
        } else {
            None
        };
        if focus != self.tips.focus {
            self.tips.focus = focus;
            self.tips.prefer_hover = false;
        }
        if self
            .tips
            .dismissed
            .is_some_and(|e| Some(e) != hover && Some(e) != focus)
        {
            self.tips.dismissed = None;
        }
        let keyboard = focus.filter(|e| Some(*e) != self.tips.dismissed);
        let hover = hover.filter(|e| Some(*e) != self.tips.dismissed);
        let warm = self.tips.shown.is_some()
            || self
                .tips
                .last_visible
                .is_some_and(|last| now.saturating_sub(last) <= WARM);
        let ready = warm || now.saturating_sub(self.tips.since) >= DELAY;
        let shown = (if !self.tips.prefer_hover && keyboard.is_some() {
            keyboard
        } else {
            hover.filter(|_| ready).or(keyboard)
        })
        .filter(|e| self.tooltip_anchor_visible(*e));
        self.tips.deadline =
            (shown.is_none() && hover.is_some()).then(|| self.tips.since.saturating_add(DELAY));
        if self.tips.shown.is_some() {
            self.tips.last_visible = Some(now);
        }
        if shown != self.tips.shown {
            self.stats.paint_writes += 1;
            self.stats.changes |= plico_core::Changes::PAINT | plico_core::Changes::SEMANTICS;
        }
        self.tips.shown = shown;
        self.tips.dirty = false;
        let Some(entity) = shown else {
            self.tips.layer.clear();
            self.tips.visual = None;
            return;
        };
        let anchor = &self.placed[self.placed_indices[&self.nodes[&entity].id]];
        let rect = self.visual_bounds(anchor.node_id).unwrap_or(anchor.rect);
        let text = self.nodes[&entity].text.clone();
        let key = (entity, text.clone(), engine.theme(), rect, width, height);
        if self.tips.visual.as_ref() == Some(&key) {
            return;
        }
        let metrics = engine.theme().metrics;
        let style = plico_core::Style {
            font_size: metrics.font_size.saturating_sub(1).max(1),
            foreground: Token::Literal(engine.theme().surface),
            background: Some(Token::Literal(engine.theme().foreground)),
            radius: 6.,
            ..Default::default()
        };
        let paint = make_paint(
            Kind::Text,
            style,
            text,
            engine.theme(),
            (false, false, false, 0.),
            self.font_family.clone(),
        );
        let size = taffy::Style {
            align_self: Some(AlignItems::START),
            padding: Rect {
                left: length(8.),
                right: length(8.),
                top: length(5.),
                bottom: length(5.),
            },
            max_size: Size {
                width: length(320f32.min((width as f32 - 16.).max(0.))),
                height: length((height as f32 - 16.).max(0.)),
            },
            ..Default::default()
        };
        let id = if let Some(id) = self.tips.layout_node {
            self.tree.set_style(id, size).unwrap();
            self.tree.set_node_context(id, Some(paint)).unwrap();
            self.tree.mark_dirty(id).unwrap();
            id
        } else {
            let id = self.tree.new_leaf_with_context(size, paint).unwrap();
            self.tips.layout_node = Some(id);
            id
        };
        self.stats.tooltip_layout_runs += 1;
        let mut layer = compute_and_walk_with_scroll(
            &mut self.tree,
            id,
            width as f32,
            height as f32,
            tcx,
            &HashMap::new(),
        );
        if let Some(p) = layer.first_mut() {
            let w = p.rect[2] - p.rect[0];
            let h = p.rect[3] - p.rect[1];
            let margin = 8f32.min(width as f32 / 2.);
            let x = ((rect[0] + rect[2] - w) / 2.)
                .clamp(margin, (width as f32 - margin - w).max(margin));
            let y = if rect[1] >= h + 14. {
                rect[1] - h - 6.
            } else {
                rect[3] + 6.
            };
            let y = y.clamp(0., (height as f32 - h).max(0.));
            p.rect = [x, y, x + w, y + h];
            p.content_origin[0] += x;
            p.content_origin[1] += y;
            p.clip = Some([0., 0., width as f32, height as f32]);
        }
        self.tips.layer = layer;
        self.tips.visual = Some(key);
        self.stats.paint_writes += 1;
        self.stats.changes |= plico_core::Changes::PAINT | plico_core::Changes::SEMANTICS;
    }
    pub(super) fn tooltip_description(&self, engine: &Engine, e: Entity) -> Option<u64> {
        let n = engine.node(e)?;
        if !n.kind.interactive() || n.kind == Kind::Overlay {
            return None;
        }
        self.tips.ids.get(&self.tooltip_owner(engine, e)?).copied()
    }
    pub(super) fn tooltip_semantics(
        &self,
        engine: &Engine,
        nodes: &mut Vec<plico_renderer::SemanticNode>,
    ) {
        use plico_renderer::{SemanticNode, SemanticRole, SemanticStates};
        let mut descriptions = Vec::new();
        for (index, p) in self.placed.iter().enumerate() {
            let Some(e) = self.reverse.get(&p.node_id).copied() else {
                continue;
            };
            let Some(id) = self.tips.ids.get(&e).copied() else {
                continue;
            };
            let label = engine.node(e).unwrap().text.to_owned();
            if let Some(group) = nodes.get_mut(index).filter(|n| n.id == semantic_id(e)) {
                group.children.push(id);
            }
            descriptions.push(SemanticNode {
                id,
                role: SemanticRole::Tooltip,
                label: Some(label),
                value: None,
                numeric_value: None,
                min_numeric_value: None,
                max_numeric_value: None,
                bounds: if self.tips.shown == Some(e) {
                    self.tips.layer.first().map_or([0.; 4], |p| p.rect)
                } else {
                    [0.; 4]
                },
                children: Vec::new(),
                controls: Vec::new(),
                described_by: Vec::new(),
                active_descendant: None,
                disabled: false,
                states: SemanticStates::default(),
            });
        }
        nodes.extend(descriptions);
    }
}
