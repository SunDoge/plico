//! Read-only paragraph selection uses the exact shaped layouts used for painting.
use super::*;
use parley::{Affinity, Cursor, Selection};
use plico_renderer::{ClipboardRequest, Point};

struct Paragraph {
    entity: Entity,
    group: Entity,
    text: Arc<str>,
    layout: Arc<parley::Layout<[u8; 4]>>,
}
#[derive(Clone, Copy)]
struct Endpoint {
    entity: Entity,
    byte: usize,
}
#[derive(Default)]
pub(super) struct TextSelection {
    paragraphs: Vec<Paragraph>,
    anchor: Option<Endpoint>,
    head: Option<Endpoint>,
    dragging: bool,
    pub(super) hovered: bool,
}
/// Shared empty selection rects: `Arc::from([])` allocates a header on every
/// construction, and the no-selection path builds one per placed node per
/// refresh.
fn empty_rects() -> Arc<[[f32; 4]]> {
    static EMPTY: std::sync::OnceLock<Arc<[[f32; 4]]>> = std::sync::OnceLock::new();
    EMPTY.get_or_init(|| Vec::new().into()).clone()
}
impl Adapter {
    fn selection_group(&self, engine: &Engine, id: NodeId) -> Option<Entity> {
        for placed in self.placed_chain(id) {
            let entity = self.reverse[&placed.node_id];
            let n = engine.node(entity)?;
            if n.style.disabled || n.kind.interactive() {
                return None;
            }
            if n.style.selectable {
                return Some(entity);
            }
        }
        None
    }
    pub(super) fn refresh_selection(&mut self, engine: &Engine, tcx: &mut TextContext) {
        let mut paragraphs = Vec::new();
        for p in &self.placed {
            let entity = self.reverse[&p.node_id];
            if engine.node(entity).is_none_or(|n| {
                let caps = n.kind.capabilities();
                !caps.contains(Capabilities::TEXT_SELECT) || caps.contains(Capabilities::TEXT_EDIT)
            }) || !self.modal_allows(engine, entity)
            {
                continue;
            }
            if let Some(group) = self.selection_group(engine, p.node_id)
                && let Some(layout) = plico_renderer::scene::layout_node_text(tcx, p)
                && let Some(text) = &p.paint.text
            {
                paragraphs.push(Paragraph {
                    entity,
                    group,
                    text: text.clone(),
                    layout,
                });
            }
        }
        let valid = [self.selection.anchor, self.selection.head]
            .into_iter()
            .flatten()
            .all(|end| {
                paragraphs
                    .iter()
                    .find(|p| p.entity == end.entity)
                    .is_some_and(|p| {
                        p.text.is_char_boundary(end.byte)
                            && self
                                .selection
                                .paragraphs
                                .iter()
                                .find(|old| old.entity == p.entity)
                                .is_none_or(|old| p.text.starts_with(old.text.as_ref()))
                    })
            });
        if !valid {
            self.selection.anchor = None;
            self.selection.head = None;
            self.selection.dragging = false;
        }
        self.selection.paragraphs = paragraphs;
        let ranges = self.selected_ranges();
        if ranges.is_empty() {
            // No selection anywhere: only stale rects need clearing, and the
            // common case is that there are none to clear.
            for p in &mut self.placed {
                if p.paint.selection_rects.is_empty() {
                    continue;
                }
                p.paint.selection_rects = empty_rects();
                self.tree
                    .get_node_context_mut(p.node_id)
                    .unwrap()
                    .selection_rects = empty_rects();
                self.stats.paint_writes += 1;
            }
        } else {
            for p in &mut self.placed {
                let entity = self.reverse[&p.node_id];
                let rects: Arc<[[f32; 4]]> = if let Some((_, start, end)) =
                    ranges.iter().find(|(e, _, _)| *e == entity)
                    && let Some(para) = self
                        .selection
                        .paragraphs
                        .iter()
                        .find(|n| n.entity == entity)
                {
                    let a = Cursor::from_byte_index(&para.layout, *start, Affinity::Downstream);
                    let b = Cursor::from_byte_index(&para.layout, *end, Affinity::Upstream);
                    Selection::new(a, b)
                        .geometry(&para.layout)
                        .into_iter()
                        .map(|(r, _)| [r.x0 as f32, r.y0 as f32, r.x1 as f32, r.y1 as f32])
                        .collect()
                } else {
                    empty_rects()
                };
                if p.paint.selection_rects != rects {
                    self.tree
                        .get_node_context_mut(p.node_id)
                        .unwrap()
                        .selection_rects = rects.clone();
                    p.paint.selection_rects = rects;
                    self.stats.paint_writes += 1;
                }
            }
        }
    }
    fn selected_ranges(&self) -> Vec<(Entity, usize, usize)> {
        let Some(a) = self.selection.anchor else {
            return Vec::new();
        };
        let Some(b) = self.selection.head else {
            return Vec::new();
        };
        let paras = &self.selection.paragraphs;
        let Some(ai) = paras.iter().position(|p| p.entity == a.entity) else {
            return Vec::new();
        };
        let Some(bi) = paras.iter().position(|p| p.entity == b.entity) else {
            return Vec::new();
        };
        let ((ai, a), (bi, b)) = if (ai, a.byte) <= (bi, b.byte) {
            ((ai, a), (bi, b))
        } else {
            ((bi, b), (ai, a))
        };
        paras
            .iter()
            .enumerate()
            .skip(ai)
            .take(bi - ai + 1)
            .filter(|(_, p)| p.group == paras[ai].group)
            .map(|(i, p)| {
                (
                    p.entity,
                    if i == ai { a.byte } else { 0 },
                    if i == bi { b.byte } else { p.text.len() },
                )
            })
            .collect()
    }
    /// Plain displayed text, with paragraph separators; never Markdown syntax.
    pub fn selected_text(&self) -> Option<String> {
        let parts: Vec<_> = self
            .selected_ranges()
            .iter()
            .filter_map(|(e, a, b)| {
                self.selection
                    .paragraphs
                    .iter()
                    .find(|p| p.entity == *e)
                    .map(|p| p.text[*a..*b].to_string())
            })
            .collect();
        let text = parts.join("\n\n");
        (!text.is_empty()).then_some(text)
    }
    fn selection_point(&self, engine: &Engine, point: Point, dragging: bool) -> Option<Endpoint> {
        let group = self.selection.anchor.and_then(|a| {
            self.selection
                .paragraphs
                .iter()
                .find(|p| p.entity == a.entity)
                .map(|p| p.group)
        });
        let mut best = None;
        let mut distance = f64::INFINITY;
        for para in self.selection.paragraphs.iter().rev() {
            if !self.modal_allows(engine, para.entity) || dragging && Some(para.group) != group {
                continue;
            }
            let node = &self.placed[self.placed_indices[&self.nodes[&para.entity].id]];
            let Some(bounds) = self.visible_bounds(node.node_id) else {
                continue;
            };
            let inside = self.visual_contains(node.node_id, point.x, point.y);
            if !dragging && !inside {
                continue;
            }
            let d = if inside {
                0.
            } else {
                (point.y - f64::from(bounds[1])).min(0.).abs()
                    + (point.y - f64::from(bounds[3])).max(0.)
                    + (point.x - f64::from(bounds[0])).min(0.).abs()
                    + (point.x - f64::from(bounds[2])).max(0.)
            };
            if d >= distance {
                continue;
            }
            let local = self.layout_point(node.node_id, point.x, point.y)?;
            let c = Cursor::from_point(
                &para.layout,
                local.x as f32 - node.content_origin[0],
                local.y as f32 - node.content_origin[1],
            );
            best = Some(Endpoint {
                entity: para.entity,
                byte: c.index(),
            });
            distance = d;
        }
        best
    }
    pub(super) fn selection_event(
        &mut self,
        engine: &Engine,
        event: &UiEvent,
    ) -> Option<EventResponse> {
        let mut handled = false;
        let mut clipboard = None;
        match event {
            UiEvent::Pointer(p) => {
                let hit = self.hit(engine, p.position.x, p.position.y);
                let point = self.selection_point(engine, p.position, self.selection.dragging);
                self.selection.hovered = point.is_some() && hit.is_none();
                match p.phase {
                    PointerPhase::Down if p.button == Some(PointerButton::Primary) => {
                        if let Some(point) = point.filter(|_| hit.is_none()) {
                            let same_group = self.selection.anchor.is_some_and(|a| {
                                let group = |entity| {
                                    self.selection
                                        .paragraphs
                                        .iter()
                                        .find(|p| p.entity == entity)
                                        .map(|p| p.group)
                                };
                                group(a.entity) == group(point.entity)
                            });
                            if !p.modifiers.shift() || !same_group {
                                self.selection.anchor = Some(point);
                            }
                            self.selection.head = Some(point);
                            self.selection.dragging = true;
                            if let Some((buffer, _)) = self.focused.and_then(|e| engine.input(e)) {
                                buffer.edit(plico_core::InputEdit::ClearPreedit);
                            }
                            self.focused = None;
                            self.pressed = None;
                            handled = true;
                        } else {
                            // Let the pointer event continue to its target, but
                            // invalidate highlights even if no backend node changes.
                            self.selection_dirty |=
                                self.selection.anchor.is_some() || self.selection.head.is_some();
                            self.selection.anchor = None;
                            self.selection.head = None;
                        }
                    }
                    PointerPhase::Move if self.selection.dragging => {
                        if point.is_some() {
                            self.selection.head = point;
                        }
                        handled = true;
                    }
                    PointerPhase::Up
                        if self.selection.dragging && p.button == Some(PointerButton::Primary) =>
                    {
                        if point.is_some() {
                            self.selection.head = point;
                        }
                        self.selection.dragging = false;
                        handled = true;
                    }
                    PointerPhase::Cancel => {
                        self.selection.dragging = false;
                    }
                    PointerPhase::Leave => {
                        self.selection.hovered = false;
                    }
                    _ => {}
                }
            }
            UiEvent::Key(k) if k.phase == KeyPhase::Down && self.selection.anchor.is_some() => {
                if k.modifiers.primary_shortcut() && matches!(k.key.as_str(), "c" | "C") {
                    clipboard = self.selected_text().map(ClipboardRequest::Write);
                    handled = true;
                } else if k.modifiers.primary_shortcut() && matches!(k.key.as_str(), "a" | "A") {
                    let group = self.selection.anchor.and_then(|a| {
                        self.selection
                            .paragraphs
                            .iter()
                            .find(|p| p.entity == a.entity)
                            .map(|p| p.group)
                    });
                    let mut paras = self
                        .selection
                        .paragraphs
                        .iter()
                        .filter(|p| Some(p.group) == group);
                    if let Some(first) = paras.next() {
                        let last = paras.next_back().unwrap_or(first);
                        self.selection.anchor = Some(Endpoint {
                            entity: first.entity,
                            byte: 0,
                        });
                        self.selection.head = Some(Endpoint {
                            entity: last.entity,
                            byte: last.text.len(),
                        });
                    }
                    handled = true;
                } else if k.key == "Escape" {
                    self.selection.anchor = None;
                    self.selection.head = None;
                    handled = true;
                }
            }
            UiEvent::Focus(false) => {
                self.selection.dragging = false;
            }
            _ => {}
        }
        if handled {
            self.selection_dirty = true;
        }
        handled.then_some(EventResponse {
            handled: true,
            request_redraw: true,
            consume_key_text: true,
            text_input: Some(false),
            clipboard,
        })
    }
}
