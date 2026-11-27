//! Modal focus scopes and event isolation, shared by native and replayed views.
use super::*;
pub(super) fn contains(engine: &Engine, root: Entity, target: Entity) -> bool {
    std::iter::successors(Some(target), |entity| engine.parent(*entity))
        .any(|entity| entity == root)
}
impl Adapter {
    pub(super) fn is_popup_panel(&self, engine: &Engine, entity: Entity) -> bool {
        engine
            .parent(entity)
            .and_then(|parent| engine.node(parent))
            .is_some_and(|node| node.kind == Kind::Overlay && node.style.popup_anchor.is_some())
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
        self.modal_stack
            .iter()
            .rev()
            .find_map(|&(root, _)| (self.departing_popup != Some(root)).then_some(root))
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
        if self
            .inactive_overlays
            .iter()
            .copied()
            .chain(self.departing_popup)
            .any(|root| contains(engine, root, entity))
        {
            return false;
        }
        self.modal_root().is_none_or(|_| {
            self.modal_dialog(engine)
                .is_some_and(|dialog| contains(engine, dialog, entity))
        })
    }
    pub(super) fn sync_modal_focus(&mut self, engine: &Engine) {
        self.inactive_overlays.clear();
        let roots: Vec<_> = engine
            .node(engine.root())
            .unwrap()
            .children
            .iter()
            .copied()
            .filter(|&entity| {
                let Some(node) = engine
                    .node(entity)
                    .filter(|node| node.kind == Kind::Overlay)
                else {
                    return false;
                };
                if node.style.modal_open && !node.style.popup_preview {
                    true
                } else {
                    self.inactive_overlays.push(entity);
                    false
                }
            })
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
        // A frozen exit can still paint after its input scope has ended.
        // MyGO ui/scope.go (MIT), revisions 16efab09 and e2674dac.
        self.departing_popup = None;
        if let Some(dialog) = self.modal_dialog(engine)
            && self.focused.is_none_or(|e| !contains(engine, dialog, e))
        {
            fn first(engine: &Engine, root: Entity, input_only: bool) -> Option<Entity> {
                let n = engine.node(root)?;
                if !n.style.disabled
                    && (if input_only {
                        n.kind == Kind::TextInput
                    } else {
                        n.kind != Kind::Dialog
                            && n.kind.capabilities().contains(Capabilities::FOCUS)
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
                .or_else(|| first(engine, dialog, false));
            self.reveal_focus = true;
        }
    }
    pub(super) fn modal_event(
        &mut self,
        engine: &mut Engine,
        event: &UiEvent,
    ) -> Option<EventResponse> {
        let root = self.modal_root()?;
        let style = engine.node(root)?.style;
        let consume = || EventResponse {
            handled: true,
            request_redraw: true,
            consume_key_text: true,
            ..EventResponse::IGNORED
        };
        if let UiEvent::Key(key) = event {
            let panel = self.modal_dialog(engine);
            let role = panel.and_then(|e| engine.node(e)?.style.role);
            let input = self.focused.and_then(|e| engine.input(e));
            let leave_filter = role == Some(plico::WidgetRole::ListBox)
                && key.modifiers.is_empty()
                && matches!(key.key.as_str(), "ArrowDown" | "ArrowUp")
                && input
                    .is_some_and(|(buffer, _)| !buffer.is_composing() && !buffer.is_multiline())
                && self
                    .focused
                    .and_then(|e| engine.node(e))
                    .is_some_and(|n| n.style.input_navigation);
            if matches!(
                role,
                Some(plico::WidgetRole::ListBox | plico::WidgetRole::Menu)
            ) && key.phase == KeyPhase::Down
                && (input.is_none() || leave_filter)
            {
                let eligible = |e: &Entity| {
                    engine.node(*e).is_some_and(|n| {
                        matches!(
                            n.style.role,
                            Some(plico::WidgetRole::Option | plico::WidgetRole::MenuItem)
                        )
                    }) && self.modal_allows(engine, *e)
                };
                let order = &self.tab_order;
                // Start just beyond the focused item, wrapping at either edge.
                let split = || {
                    self.focused
                        .filter(eligible)
                        .and_then(|focus| order.iter().position(|&e| e == focus))
                };
                let forward = || {
                    let (before, after) = order.split_at(split().map_or(0, |i| i + 1));
                    after.iter().chain(before).copied().filter(eligible)
                };
                let name = key.key.as_str();
                let next = match name {
                    "ArrowDown" => forward().next(),
                    "ArrowUp" => {
                        let (before, after) = order.split_at(split().unwrap_or(0));
                        after.iter().chain(before).rev().copied().find(eligible)
                    }
                    "Home" => order.iter().copied().find(eligible),
                    "End" => order.iter().rev().copied().find(eligible),
                    value
                        if value.chars().count() == 1
                            && value != " "
                            && key.modifiers.is_empty() =>
                    {
                        let prefix = value.to_lowercase();
                        forward().find(|&id| {
                            engine
                                .accessible_name(id)
                                .is_some_and(|name| name.to_lowercase().starts_with(&prefix))
                        })
                    }
                    _ => None,
                };
                if let Some(next) = next {
                    self.focused = Some(next);
                    self.focus_visible = true;
                    self.reveal_focus = true;
                    return Some(EventResponse {
                        text_input: leave_filter.then_some(false),
                        ..consume()
                    });
                }
                if matches!(
                    name,
                    "ArrowDown" | "ArrowUp" | "Home" | "End" | "Enter" | " " | "Space"
                ) && !order.iter().any(eligible)
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
                .is_some_and(|node| self.visual_contains(node.id, point.x, point.y));
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
