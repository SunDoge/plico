//! Native event dispatch and interaction invalidation.
use super::*;
use plico_renderer::WheelDeltaMode;

enum TreeKeyAction {
    Focus(Entity),
    Toggle(Entity),
}

impl Adapter {
    fn role_container_of(
        &self,
        engine: &Engine,
        entity: Entity,
    ) -> Option<(NodeId, plico::WidgetRole)> {
        let node = self.nodes.get(&entity)?;
        let mut cursor = self.tree.parent(node.id);
        while let Some(id) = cursor {
            if let Some(&entity) = self.reverse.get(&id)
                && let Some(role) = engine.node(entity).and_then(|n| n.style.role)
                && matches!(
                    role,
                    plico::WidgetRole::TabList
                        | plico::WidgetRole::RadioGroup
                        | plico::WidgetRole::Toolbar
                        | plico::WidgetRole::Tree
                        | plico::WidgetRole::ListBox
                        | plico::WidgetRole::Menu
                )
            {
                return Some((id, role));
            }
            cursor = self.tree.parent(id);
        }
        None
    }

    fn focus_items_in(
        &self,
        engine: &Engine,
        container: NodeId,
        role: Option<plico::WidgetRole>,
    ) -> Vec<Entity> {
        self.placed
            .iter()
            .filter_map(|placed| self.reverse.get(&placed.node_id).copied())
            .filter(|entity| self.modal_allows(engine, *entity))
            .filter(|entity| {
                engine.node(*entity).is_some_and(|node| {
                    !node.style.disabled
                        && node.style.tab_stop
                        && node.kind.capabilities().contains(Capabilities::TAB_STOP)
                        && (role != Some(plico::WidgetRole::Tree)
                            || node.style.role == Some(plico::WidgetRole::TreeItem))
                })
            })
            .filter(|entity| {
                let Some(node) = self.nodes.get(entity) else {
                    return false;
                };
                let mut cursor = Some(node.id);
                while let Some(id) = cursor {
                    if id == container {
                        return true;
                    }
                    cursor = self.tree.parent(id);
                }
                false
            })
            .collect()
    }

    fn focus_group_of(
        &self,
        engine: &Engine,
        entity: Entity,
    ) -> Option<(NodeId, plico::FocusAxis)> {
        let node = self.nodes.get(&entity)?;
        let mut cursor = self.tree.parent(node.id);
        while let Some(id) = cursor {
            if let Some(&entity) = self.reverse.get(&id)
                && let Some(axis) = engine.node(entity).and_then(|node| node.style.focus_axis)
            {
                return Some((id, axis));
            }
            cursor = self.tree.parent(id);
        }
        None
    }

    fn selection_follows_focus(&self, engine: &Engine, entity: Entity) -> bool {
        self.role_container_of(engine, entity)
            .and_then(|(container, _)| self.reverse.get(&container).copied())
            .and_then(|container| engine.node(container))
            .is_some_and(|node| node.style.selection_follows_focus)
    }

    fn tree_toggle(&self, engine: &Engine, item: Entity) -> Option<Entity> {
        let parent = self.tree.parent(self.nodes.get(&item)?.id)?;
        self.tree.children(parent).ok()?.into_iter().find_map(|id| {
            let entity = *self.reverse.get(&id)?;
            let node = engine.node(entity)?;
            (entity != item
                && !node.style.tab_stop
                && node.kind.capabilities().contains(Capabilities::ACTIVATE))
            .then_some(entity)
        })
    }

    fn tree_key_action(&self, engine: &Engine, key: &str) -> Option<TreeKeyAction> {
        let focused = self.focused?;
        let focused_node = engine.node(focused)?;
        if focused_node.style.role != Some(plico::WidgetRole::TreeItem) {
            return None;
        }
        let (container, role) = self.role_container_of(engine, focused)?;
        if role != plico::WidgetRole::Tree {
            return None;
        }
        let items = self.focus_items_in(engine, container, Some(role));
        let at = items.iter().position(|entity| *entity == focused)?;
        let depth = focused_node.style.tree_depth.unwrap_or(0);
        let focus = |index| items.get(index).copied().map(TreeKeyAction::Focus);
        match key {
            "ArrowDown" => focus((at + 1).min(items.len() - 1)),
            "ArrowUp" => focus(at.saturating_sub(1)),
            "Home" => focus(0),
            "End" => focus(items.len() - 1),
            "ArrowRight" => match focused_node.style.expanded {
                Some(false) => self.tree_toggle(engine, focused).map(TreeKeyAction::Toggle),
                Some(true) => items
                    .get(at + 1)
                    .filter(|child| {
                        engine
                            .node(**child)
                            .and_then(|node| node.style.tree_depth)
                            .is_some_and(|child_depth| child_depth > depth)
                    })
                    .copied()
                    .map(TreeKeyAction::Focus),
                None => None,
            },
            "ArrowLeft" => match focused_node.style.expanded {
                Some(true) => self.tree_toggle(engine, focused).map(TreeKeyAction::Toggle),
                _ => items[..at]
                    .iter()
                    .rev()
                    .find(|parent| {
                        engine
                            .node(**parent)
                            .and_then(|node| node.style.tree_depth)
                            .is_some_and(|parent_depth| parent_depth < depth)
                    })
                    .copied()
                    .map(TreeKeyAction::Focus),
            },
            _ => None,
        }
    }

    fn tree_type_select(
        &mut self,
        engine: &Engine,
        key: &str,
        now: std::time::Duration,
    ) -> Option<Entity> {
        let mut chars = key.chars();
        let typed = chars.next()?.to_lowercase().collect::<String>();
        if chars.next().is_some() || typed.chars().all(char::is_whitespace) {
            return None;
        }
        let focused = self.focused?;
        if engine.node(focused)?.style.role != Some(plico::WidgetRole::TreeItem) {
            return None;
        }
        let (container, role) = self.role_container_of(engine, focused)?;
        if role != plico::WidgetRole::Tree {
            return None;
        }
        let items = self.focus_items_in(engine, container, Some(role));
        let at = items.iter().position(|entity| *entity == focused)?;
        if self.tree_typeahead.0 != Some(container)
            || now.saturating_sub(self.tree_typeahead.2) > std::time::Duration::from_secs(1)
        {
            self.tree_typeahead.1.clear();
        }
        self.tree_typeahead.0 = Some(container);
        self.tree_typeahead.1.push_str(&typed);
        self.tree_typeahead.2 = now;
        let first = self.tree_typeahead.1.chars().next()?;
        let repeated = self.tree_typeahead.1.chars().all(|ch| ch == first);
        let needle = if repeated {
            first.to_string()
        } else {
            self.tree_typeahead.1.clone()
        };
        let start = if repeated { at + 1 } else { at };
        (0..items.len()).find_map(|offset| {
            let entity = items[(start + offset) % items.len()];
            engine
                .node(entity)
                .filter(|node| node.text.to_lowercase().starts_with(&needle))
                .map(|_| entity)
        })
    }

    pub(super) fn roving_tab_order(&self, engine: &Engine, candidates: Vec<Entity>) -> Vec<Entity> {
        let mut choices: HashMap<NodeId, (u8, Entity)> = HashMap::new();
        for &entity in &candidates {
            let Some(node) = engine.node(entity) else {
                continue;
            };
            let Some((container, container_role)) = self.role_container_of(engine, entity) else {
                continue;
            };
            let roving = matches!(
                (container_role, node.style.role),
                (plico::WidgetRole::Tree, Some(plico::WidgetRole::TreeItem))
                    | (
                        plico::WidgetRole::RadioGroup,
                        Some(plico::WidgetRole::RadioButton)
                    )
                    | (plico::WidgetRole::TabList, Some(plico::WidgetRole::Tab))
            );
            if !roving {
                continue;
            }
            let priority = match container_role {
                // Tree focus and selection are independent. Radio groups and
                // tab lists instead enter at their externally controlled choice.
                plico::WidgetRole::Tree => {
                    if self.focused == Some(entity) {
                        2
                    } else if node.style.selected == Some(true) || node.style.checked {
                        1
                    } else {
                        0
                    }
                }
                _ => {
                    if node.style.selected == Some(true) || node.style.checked {
                        2
                    } else if self.focused == Some(entity) {
                        1
                    } else {
                        0
                    }
                }
            };
            let choice = choices.entry(container).or_insert((priority, entity));
            if priority > choice.0 {
                *choice = (priority, entity);
            }
        }
        candidates
            .into_iter()
            .filter(|entity| {
                let Some(node) = engine.node(*entity) else {
                    return false;
                };
                self.role_container_of(engine, *entity)
                    .and_then(|(container, container_role)| {
                        matches!(
                            (container_role, node.style.role),
                            (plico::WidgetRole::Tree, Some(plico::WidgetRole::TreeItem))
                                | (
                                    plico::WidgetRole::RadioGroup,
                                    Some(plico::WidgetRole::RadioButton)
                                )
                                | (plico::WidgetRole::TabList, Some(plico::WidgetRole::Tab))
                        )
                        .then_some(container)
                    })
                    .is_none_or(|container| {
                        choices
                            .get(&container)
                            .is_some_and(|choice| choice.1 == *entity)
                    })
            })
            .collect()
    }

    pub(crate) fn update_hover_regions(&self, engine: &Engine) -> bool {
        engine.update_hover_regions(|entity| {
            self.pointer_position.is_some_and(|(x, y)| {
                self.modal_allows(engine, entity)
                    && self
                        .nodes
                        .get(&entity)
                        .is_some_and(|node| self.visual_contains(node.id, x, y))
            })
        })
    }

    pub(crate) fn update_pointer_regions(&self, engine: &Engine) -> bool {
        engine.update_pointer_regions(|entity| {
            let (x, y) = self.pointer_position?;
            if !self.modal_allows(engine, entity) {
                return None;
            }
            let node = self.nodes.get(&entity)?;
            let placed = &self.placed[*self.placed_indices.get(&node.id)?];
            let point = self.layout_point(node.id, x, y)?;
            let width = placed.rect[2] - placed.rect[0];
            let height = placed.rect[3] - placed.rect[1];
            if width <= 0. || height <= 0. {
                return None;
            }
            Some([
                ((point.x - f64::from(placed.rect[0])) / f64::from(width)) as f32,
                ((point.y - f64::from(placed.rect[1])) / f64::from(height)) as f32,
            ])
        })
    }

    /// Arrow/Home/End traversal inside a role container such as a tab strip,
    /// radio group, toolbar, tree or menu. Outside such a container the key is
    /// left to the focused control or host.
    fn container_focus_target(&self, engine: &Engine, key: &str) -> Option<Entity> {
        use plico::WidgetRole;
        let focused = self.focused?;
        let group = self.focus_group_of(engine, focused);
        let role_group = self.role_container_of(engine, focused);
        let (container, role, axis) = if let Some((container, axis)) = group {
            (container, None, Some(axis))
        } else {
            let (container, role) = role_group?;
            (container, Some(role), None)
        };
        let along = match key {
            "Home" | "End" => true,
            "ArrowRight" | "ArrowLeft" => {
                axis == Some(plico::FocusAxis::Horizontal)
                    || matches!(
                        role,
                        Some(WidgetRole::TabList | WidgetRole::Toolbar | WidgetRole::RadioGroup)
                    )
            }
            "ArrowDown" | "ArrowUp" => {
                axis == Some(plico::FocusAxis::Vertical)
                    || matches!(
                        role,
                        Some(
                            WidgetRole::Tree
                                | WidgetRole::ListBox
                                | WidgetRole::Menu
                                | WidgetRole::RadioGroup
                        )
                    )
            }
            _ => false,
        };
        if !along {
            return None;
        }
        let items = self.focus_items_in(engine, container, role);
        if items.len() < 2 {
            return None;
        }
        let at = items.iter().position(|entity| *entity == focused)?;
        let len = items.len();
        let next = match key {
            "Home" => 0,
            "End" => len - 1,
            "ArrowRight" | "ArrowDown" if axis.is_some() => (at + 1).min(len - 1),
            "ArrowLeft" | "ArrowUp" if axis.is_some() => at.saturating_sub(1),
            "ArrowRight" | "ArrowDown" => (at + 1) % len,
            "ArrowLeft" | "ArrowUp" => (at + len - 1) % len,
            _ => return None,
        };
        items.get(next).copied()
    }

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
        self.scrollbars.event(&event);
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
        let previous_pointer = self.pointer_position;
        let mut declare = true;
        let mut response = self.event_inner(engine, event, &mut declare);
        engine.set_hovered(self.hovered);
        engine.set_pressed(self.pressed);
        engine.set_pointer_position(self.pointer_position);
        if previous_pointer != self.pointer_position && engine.observes_pointer_position() {
            response.request_redraw = true;
            declare = true;
        }
        if self.update_hover_regions(engine) {
            response.request_redraw = true;
            declare = true;
        }
        if self.update_pointer_regions(engine) {
            response.request_redraw = true;
            declare = true;
        }
        if self.scrollbar_pointer_changed(engine) && !response.request_redraw {
            response.request_redraw = true;
            declare = false;
        }
        response.request_redraw |= self.tips.needs_frame(engine.time().now);
        (response, declare)
    }
    pub(super) fn event_inner(
        &mut self,
        engine: &mut Engine,
        mut event: UiEvent,
        declare: &mut bool,
    ) -> EventResponse {
        if let Some(response) = self.input_pointer_event(engine, &event) {
            return response;
        }
        if let Some(response) = self.slider_event(engine, &event) {
            return response;
        }
        if let Some(response) = self.modal_event(engine, &event) {
            return response;
        }
        if let Some(response) = self.input_event(engine, &mut event) {
            return response;
        }
        let had_focus = self.focused.is_some();
        if let Some(response) = self.selection_event(engine, &event) {
            // Read-only selection is backend presentation state. Losing an
            // input focus can still require the application's normal settlement.
            *declare = had_focus;
            return response;
        }
        let old_focus = self.focused;
        let mut handled = false;
        match event {
            UiEvent::Pointer(p) => {
                use plico_renderer::scrollbar::{self, ScrollAxis, ScrollbarPart};
                // Persistent input snapshot for declaration-time queries;
                // pointer movement outside any control still redeclares.
                self.pointer_position = match p.phase {
                    PointerPhase::Leave | PointerPhase::Cancel => None,
                    _ => Some((p.position.x, p.position.y)),
                };
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
                    *declare = false;
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
                        let smooth = self.smooth_scroll_enabled;
                        *declare = engine.observes_scroll(self.reverse[&id])
                            && !(smooth && self.smooth_scroll_enabled)
                            && next != node.scroll.offset[1];
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
                    *declare = false;
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
                        *declare = next != node.scroll.offset[1]
                            && engine.observes_scroll(self.reverse[&id]);
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
                        self.input_pointer(engine, hit, p);
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
                    PointerPhase::Down if p.button == Some(PointerButton::Secondary) => {
                        self.hovered = hit;
                        self.secondary_pressed = hit;
                        handled = true;
                    }
                    PointerPhase::Up if p.button == Some(PointerButton::Secondary) => {
                        self.hovered = hit;
                        if let Some(e) = self.secondary_pressed.take().filter(|e| Some(*e) == hit)
                            && engine.node(e).is_some_and(|n| {
                                n.kind.capabilities().contains(Capabilities::ACTIVATE)
                            })
                        {
                            engine.queue_secondary_click(e);
                        }
                        handled = true;
                    }
                    PointerPhase::Cancel | PointerPhase::Leave => {
                        self.secondary_pressed = None;
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
                    self.scrollbars.reveal(start, engine.time().now);
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
                        let entity = self.reverse[&id];
                        // MyGO ui/input.go: precise deltas already follow the
                        // fingers (including native momentum). Interpolate only
                        // wheel notches; touching the trackpad takes over at the
                        // visible offset, never the old wheel destination.
                        // Smooth is the only wheel scroll (MyGO has no
                        // per-container opt-out; reduced motion is the only
                        // exit, through smooth_scroll_enabled).
                        let smooth =
                            w.delta_mode == WheelDeltaMode::Line && self.smooth_scroll_enabled;
                        let destination = if smooth {
                            self.scroll_motions.get(&id).map_or(old[1], |m| m.target)
                        } else {
                            old[1]
                        };
                        let next = (destination + remaining).clamp(0., scroll.range[1]);
                        remaining -= next - destination;
                        if next != destination || (!smooth && self.scroll_motions.contains_key(&id))
                        {
                            *declare |= engine.observes_scroll(entity)
                                && !(smooth && self.smooth_scroll_enabled)
                                && next != old[1];
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
                self.secondary_pressed = None;
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
                } else if matches!(
                    k.key.as_str(),
                    "ArrowLeft" | "ArrowRight" | "ArrowUp" | "ArrowDown" | "Home" | "End"
                ) && let Some(action) = self.tree_key_action(engine, k.key.as_str())
                {
                    match action {
                        TreeKeyAction::Focus(entity) => {
                            if self.focused != Some(entity)
                                && self.selection_follows_focus(engine, entity)
                            {
                                engine.queue_click(entity);
                            }
                            self.focused = Some(entity);
                            self.reveal_focus = true;
                        }
                        TreeKeyAction::Toggle(entity) => engine.queue_click(entity),
                    }
                    self.focus_visible = true;
                    handled = true;
                } else if let Some(next) =
                    self.tree_type_select(engine, k.key.as_str(), engine.time().now)
                {
                    if self.focused != Some(next) && self.selection_follows_focus(engine, next) {
                        engine.queue_click(next);
                    }
                    self.focused = Some(next);
                    self.focus_visible = true;
                    self.reveal_focus = true;
                    handled = true;
                } else if matches!(
                    k.key.as_str(),
                    "ArrowLeft" | "ArrowRight" | "ArrowUp" | "ArrowDown" | "Home" | "End"
                ) && let Some(next) = self.container_focus_target(engine, k.key.as_str())
                {
                    if self.focused != Some(next) && self.selection_follows_focus(engine, next) {
                        engine.queue_click(next);
                    }
                    self.focused = Some(next);
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
