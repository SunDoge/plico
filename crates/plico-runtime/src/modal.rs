//! Modal focus scopes and event isolation, shared by native and replayed views.
use super::*;
pub(super) fn contains(engine: &Engine, root: Entity, target: Entity) -> bool {
    root == target
        || engine.node(root).is_some_and(|n| {
            n.children
                .iter()
                .any(|child| contains(engine, *child, target))
        })
}
impl Adapter {
    pub(super) fn is_popup_panel(&self, engine: &Engine, entity: Entity) -> bool {
        self.modal_stack.iter().any(|(root, _)| {
            engine
                .node(*root)
                .is_some_and(|n| n.style.popup_anchor.is_some() && n.children.contains(&entity))
        })
    }
    pub(super) fn accessible_modal(&self, engine: &Engine) -> Option<Entity> {
        self.modal_stack.iter().rev().find_map(|(root, _)| {
            let node = engine.node(*root)?;
            if node.style.popup_anchor.is_some() {
                return None;
            }
            node.children
                .iter()
                .copied()
                .find(|e| engine.node(*e).is_some_and(|n| n.kind == Kind::Dialog))
        })
    }

    fn modal_root(&self) -> Option<Entity> {
        self.modal_stack.last().map(|(entity, _)| *entity)
    }
    pub(super) fn modal_dialog(&self, engine: &Engine) -> Option<Entity> {
        engine
            .node(self.modal_root()?)?
            .children
            .iter()
            .copied()
            .find(|e| engine.node(*e).is_some_and(|n| n.kind == Kind::Dialog))
    }
    pub(super) fn modal_allows(&self, engine: &Engine, entity: Entity) -> bool {
        let active = self.modal_stack.iter().rev().find_map(|(root, _)| {
            let n = engine.node(*root)?;
            (!(n.style.popup_anchor.is_some()
                && (!n.style.modal_open || self.departing_popup == Some(*root))))
            .then_some((*root, n))
        });
        if self.modal_stack.iter().any(|(root, _)| {
            engine.node(*root).is_some_and(|n| {
                n.style.popup_anchor.is_some()
                    && (!n.style.modal_open || self.departing_popup == Some(*root))
            }) && contains(engine, *root, entity)
        }) {
            return false;
        }
        active.is_none_or(|(_, node)| {
            node.style.modal_open
                && node.children.iter().any(|child| {
                    engine.node(*child).is_some_and(|n| n.kind == Kind::Dialog)
                        && contains(engine, *child, entity)
                })
        })
    }
    pub(super) fn sync_modal_focus(&mut self, engine: &Engine) {
        let roots: Vec<_> = engine
            .node(engine.root())
            .unwrap()
            .children
            .iter()
            .copied()
            .filter(|e| engine.node(*e).is_some_and(|n| n.kind == Kind::Overlay))
            .collect();
        let shared = self
            .modal_stack
            .iter()
            .zip(&roots)
            .take_while(|((old, _), new)| old == *new)
            .count();
        while self.modal_stack.len() > shared {
            let (removed, restore) = self.modal_stack.pop().unwrap();
            if self.departing_popup != Some(removed) {
                self.focused =
                    restore.filter(|e| engine.node(*e).is_some_and(|n| !n.style.disabled));
            } else {
                self.departing_popup = None;
            }
        }
        for root in roots.iter().skip(shared) {
            fn find_anchor(engine: &Engine, e: Entity, token: u64) -> Option<Entity> {
                let node = engine.node(e)?;
                if node.style.anchor_id == Some(token) {
                    return Some(e);
                }
                node.children
                    .iter()
                    .find_map(|child| find_anchor(engine, *child, token))
            }
            let restore = engine
                .node(*root)
                .and_then(|n| n.style.popup_anchor)
                .and_then(|token| find_anchor(engine, engine.root(), token))
                .or(self.focused);
            self.modal_stack.push((*root, restore));
            self.focused = None;
            self.pressed = None;
            self.hovered = None;
            self.scrollbar_drag = None;
        }
        if self
            .modal_root()
            .and_then(|r| engine.node(r))
            .is_some_and(|n| n.style.popup_anchor.is_some() && !n.style.modal_open)
        {
            if self.focused.is_none_or(|e| {
                self.modal_dialog(engine)
                    .is_some_and(|d| contains(engine, d, e))
            }) {
                self.focused = self.modal_stack.last().and_then(|(_, restore)| *restore);
            }
            self.departing_popup = self.modal_root();
            return;
        }
        if let Some(dialog) = self.modal_dialog(engine) {
            if self.focused.is_none_or(|e| !contains(engine, dialog, e)) {
                fn first(engine: &Engine, root: Entity, input_only: bool) -> Option<Entity> {
                    let n = engine.node(root)?;
                    if !n.style.disabled
                        && (if input_only {
                            n.kind == Kind::TextInput
                        } else {
                            interactive(n.kind) && n.kind != Kind::Overlay
                        })
                    {
                        return Some(root);
                    }
                    n.children
                        .iter()
                        .find_map(|child| first(engine, *child, input_only))
                }
                fn selected(engine: &Engine, e: Entity) -> Option<Entity> {
                    let n = engine.node(e)?;
                    if n.style.role == Some(plico::WidgetRole::Option)
                        && n.style.selected == Some(true)
                        && !n.style.disabled
                    {
                        return Some(e);
                    }
                    n.children.iter().find_map(|child| selected(engine, *child))
                }
                fn last_item(engine: &Engine, e: Entity) -> Option<Entity> {
                    let n = engine.node(e)?;
                    n.children
                        .iter()
                        .rev()
                        .find_map(|child| last_item(engine, *child))
                        .or_else(|| {
                            (!n.style.disabled && n.style.role == Some(plico::WidgetRole::MenuItem))
                                .then_some(e)
                        })
                }
                let open_last = self
                    .modal_stack
                    .last()
                    .and_then(|(_, restore)| *restore)
                    .is_some_and(|restore| self.popup_open_last == Some(restore));
                self.popup_open_last = None;
                self.focused = open_last
                    .then(|| last_item(engine, dialog))
                    .flatten()
                    .or_else(|| selected(engine, dialog))
                    .or_else(|| first(engine, dialog, true))
                    .or_else(|| first(engine, dialog, false))
                    .or(Some(dialog));
                self.reveal_focus = true;
            }
        } else if self.focused.is_none() && !self.modal_stack.is_empty() {
            self.focused = self.modal_root();
        }
    }
    pub(super) fn modal_event(
        &mut self,
        engine: &mut Engine,
        event: &UiEvent,
    ) -> Option<EventResponse> {
        let root = self.modal_root()?;
        let style = engine.node(root)?.style;
        if style.popup_anchor.is_some() && !style.modal_open {
            return None;
        }
        let consume = || EventResponse {
            handled: true,
            request_redraw: true,
            consume_key_text: true,
            ..EventResponse::IGNORED
        };
        if let UiEvent::Key(key) = event {
            let panel = self.modal_dialog(engine);
            let specialized = panel.and_then(|e| engine.node(e)).is_some_and(|n| {
                matches!(
                    n.style.role,
                    Some(plico::WidgetRole::ListBox | plico::WidgetRole::Menu)
                )
            });
            if specialized
                && key.phase == KeyPhase::Down
                && self.focused.and_then(|e| engine.input(e)).is_none()
            {
                let items: Vec<_> = self
                    .tab_order
                    .iter()
                    .copied()
                    .filter(|e| {
                        self.modal_allows(engine, *e)
                            && engine.node(*e).is_some_and(|n| {
                                matches!(
                                    n.style.role,
                                    Some(plico::WidgetRole::Option | plico::WidgetRole::MenuItem)
                                )
                            })
                    })
                    .collect();
                let at = self
                    .focused
                    .and_then(|e| items.iter().position(|item| *item == e));
                let next = if items.is_empty() {
                    None
                } else {
                    match key.key.as_str() {
                        "ArrowDown" => Some(at.map_or(0, |i| (i + 1) % items.len())),
                        "ArrowUp" => Some(
                            at.map_or(items.len() - 1, |i| (i + items.len() - 1) % items.len()),
                        ),
                        "Home" => Some(0),
                        "End" => Some(items.len() - 1),
                        value
                            if value.chars().count() == 1
                                && value != " "
                                && key.modifiers.is_empty() =>
                        {
                            let prefix = value.to_lowercase();
                            (1..=items.len())
                                .map(|offset| {
                                    (at.unwrap_or(items.len() - 1) + offset) % items.len()
                                })
                                .find(|i| {
                                    engine
                                        .node(items[*i])
                                        .is_some_and(|n| n.text.to_lowercase().starts_with(&prefix))
                                })
                        }
                        _ => None,
                    }
                };
                if let Some(next) = next {
                    self.focused = Some(items[next]);
                    self.focus_visible = true;
                    self.reveal_focus = true;
                    return Some(consume());
                }
                if items.is_empty()
                    && matches!(
                        key.key.as_str(),
                        "ArrowDown" | "ArrowUp" | "Home" | "End" | "Enter" | " " | "Space"
                    )
                {
                    return Some(consume());
                }
            }
            if key.key == "Escape" && key.phase == KeyPhase::Down {
                if self
                    .focused
                    .and_then(|e| engine.input(e))
                    .is_some_and(|(buffer, _)| buffer.is_composing())
                {
                    return None;
                }
                if style.modal_open && style.dismiss_on_escape && !key.repeat {
                    engine.queue_click(root);
                }
                return Some(consume());
            }
            if key.key == "Tab" && key.phase == KeyPhase::Down && style.popup_anchor.is_some() {
                engine.queue_click(root);
                self.departing_popup = Some(root);
                // Restore the trigger before ordinary traversal. The closing popup
                // remains mounted for animation but must not trap focus.
                self.focused = self.modal_stack.last().and_then(|(_, restore)| *restore);
                self.reveal_focus = true;
                return None;
            }
            if !style.modal_open {
                return Some(consume());
            }
            if key.key == "Tab" && !self.tab_order.iter().any(|e| self.modal_allows(engine, *e)) {
                return Some(consume());
            }
        }
        let point = match event {
            UiEvent::Pointer(pointer) => Some(pointer.position),
            UiEvent::Wheel(wheel) => Some(wheel.position),
            _ => None,
        };
        if let Some(point) = point {
            let inside = self
                .modal_dialog(engine)
                .and_then(|e| self.nodes.get(&e))
                .and_then(|n| self.placed_indices.get(&n.id))
                .is_some_and(|index| {
                    let r = self.placed[*index].rect;
                    point.x >= r[0] as f64
                        && point.y >= r[1] as f64
                        && point.x < r[2] as f64
                        && point.y < r[3] as f64
                });
            if let UiEvent::Pointer(pointer) = event {
                if matches!(pointer.phase, PointerPhase::Cancel | PointerPhase::Leave) {
                    self.backdrop_pressed = None;
                }
                if pointer.phase == PointerPhase::Down
                    && pointer.button == Some(PointerButton::Primary)
                {
                    self.backdrop_pressed = (!inside).then_some(root);
                }
                if pointer.phase == PointerPhase::Up
                    && pointer.button == Some(PointerButton::Primary)
                {
                    let backdrop = self.backdrop_pressed.take() == Some(root);
                    if backdrop && !inside && style.modal_open && style.dismiss_on_backdrop {
                        engine.queue_click(root);
                    }
                }
            }
            if !inside || !style.modal_open {
                self.hovered = None;
                self.pressed = None;
                return Some(consume());
            }
        }
        None
    }
}
