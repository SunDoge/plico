//! Native event dispatch and interaction invalidation.
use super::*;

impl Adapter {
    pub fn event(&mut self, engine: &mut Engine, event: UiEvent) -> EventResponse {
        self.event_with_invalidation(engine, event).0
    }
    /// A repaint does not always require evaluating application declarations.
    /// Keep this distinction internal; the public event response still tells
    /// every host when it needs to present another frame.
    pub(crate) fn event_with_invalidation(
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
    pub(super) fn event_inner(
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
}
