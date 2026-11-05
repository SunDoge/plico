//! Accessibility projection and semantic actions.
use super::*;

impl Adapter {
    /// Invalid SVG sources remain cached while referenced, avoiding parse retries every frame.
    pub fn svg_diagnostics(&self) -> impl Iterator<Item = &str> {
        self.svg_cache
            .values()
            .flat_map(|variants| variants.values())
            .filter_map(|v| v.as_ref().err().map(|e| e.as_ref()))
    }
    pub fn semantics(&self, engine: &Engine, revision: u64) -> plico_renderer::SemanticSnapshot {
        (*self.semantics_shared(engine, revision)).clone()
    }
    /// Share an immutable semantic projection until the next backend commit or input event.
    pub fn semantics_shared(
        &self,
        engine: &Engine,
        revision: u64,
    ) -> Arc<plico_renderer::SemanticSnapshot> {
        let epoch = self.semantic.borrow().epoch;
        if self.experiment_force_full_semantics {
            self.semantic.borrow_mut().nodes.clear();
            self.semantic.borrow_mut().builds = 0;
            self.semantic.borrow_mut().reuses = 0;
            return Arc::new(self.build_semantics(engine, revision));
        }
        let cached = self.semantic.borrow().snapshot.clone();
        if let Some((cached_epoch, cached_revision, snapshot)) = cached
            && cached_epoch == epoch
            && cached_revision == revision
        {
            self.semantic.borrow_mut().snapshot_reuses += 1;
            return snapshot;
        }
        self.semantic.borrow_mut().builds = 0;
        self.semantic.borrow_mut().reuses = 0;
        let snapshot = Arc::new(self.build_semantics(engine, revision));
        self.semantic.borrow_mut().snapshot = Some((epoch, revision, snapshot.clone()));
        snapshot
    }
    #[doc(hidden)]
    pub fn experiment_semantic_cache_stats(&self) -> (usize, usize, usize) {
        let cache = self.semantic.borrow();
        (cache.builds, cache.reuses, cache.snapshot_reuses)
    }
    pub(super) fn build_semantics(
        &self,
        engine: &Engine,
        revision: u64,
    ) -> plico_renderer::SemanticSnapshot {
        use plico_renderer::{
            SemanticNode, SemanticRole as Role, SemanticStates, SemanticToggleState,
        };
        let mut nodes: Vec<SemanticNode> = self
            .placed
            .iter()
            .filter_map(|p| {
                let e = *self.reverse.get(&p.node_id)?;
                let n = engine.node(e)?;
                let cacheable =
                    n.kind != Kind::ButtonBase && n.style.role != Some(plico::WidgetRole::ComboBox);
                let cached = cacheable
                    .then(|| self.semantic.borrow().nodes.get(&e).cloned())
                    .flatten();
                let mut semantic = if let Some(cached) = cached {
                    self.semantic.borrow_mut().reuses += 1;
                    cached
                } else {
                    self.semantic.borrow_mut().builds += 1;
                    SemanticNode {
                        id: semantic_id(e),
                        role: match n.style.role {
                            Some(plico::WidgetRole::ComboBox) => Role::ComboBox,
                            Some(plico::WidgetRole::ListBox) => Role::ListBox,
                            Some(plico::WidgetRole::Option) => Role::Option,
                            Some(plico::WidgetRole::Menu) => Role::Menu,
                            Some(plico::WidgetRole::MenuButton) => Role::Button,
                            Some(plico::WidgetRole::MenuItem) => Role::MenuItem,
                            Some(plico::WidgetRole::TabList) => Role::TabList,
                            Some(plico::WidgetRole::Tab) => Role::Tab,
                            Some(plico::WidgetRole::Navigation) => Role::Navigation,
                            Some(plico::WidgetRole::Link) => Role::Link,
                            Some(plico::WidgetRole::Meter) => Role::Meter,
                            Some(plico::WidgetRole::ProgressBar) => Role::ProgressBar,
                            None => match n.kind {
                                Kind::Button | Kind::ButtonBase => Role::Button,
                                Kind::Slider => Role::Slider,
                                Kind::Dialog => Role::Dialog,
                                Kind::TextInput => Role::TextInput,
                                Kind::Switch => Role::Switch,
                                Kind::Checkbox => Role::CheckBox,
                                Kind::Text => Role::Label,
                                _ => Role::Group,
                            },
                        },
                        label: engine.accessible_name(e).map(|name| name.into_owned()),
                        value: if n.style.role == Some(plico::WidgetRole::ComboBox) {
                            n.children
                                .iter()
                                .filter_map(|e| engine.node(*e))
                                .find(|n| n.kind == Kind::Text)
                                .map(|n| n.text.to_owned())
                        } else {
                            n.numeric.map(|range| range.value.to_string())
                        },
                        numeric_value: n.numeric.map(|range| range.value),
                        min_numeric_value: n.numeric.map(|range| range.min),
                        max_numeric_value: n.numeric.map(|range| range.max),
                        bounds: p.rect,
                        children: n.children.iter().map(|e| semantic_id(*e)).collect(),
                        controls: Vec::new(),
                        described_by: Vec::new(),
                        active_descendant: None,
                        disabled: n.style.disabled,
                        states: SemanticStates {
                            selected: n.style.selected,
                            expanded: n.style.expanded,
                            checked: matches!(n.kind, Kind::Switch | Kind::Checkbox).then_some(
                                if n.style.checked {
                                    SemanticToggleState::On
                                } else {
                                    SemanticToggleState::Off
                                },
                            ),
                            ..Default::default()
                        },
                    }
                };
                if cacheable {
                    self.semantic.borrow_mut().nodes.insert(e, semantic.clone());
                }
                semantic.bounds = self.visual_bounds(p.node_id).unwrap_or(p.rect);
                semantic.described_by = self.tooltip_description(engine, e).into_iter().collect();
                semantic.disabled |=
                    interactive(n.kind) && n.kind != Kind::Overlay && !self.modal_allows(engine, e);
                semantic.states.modal = (n.kind == Kind::Dialog && n.style.role.is_none())
                    .then_some(!self.is_popup_panel(engine, e));
                Some(semantic)
            })
            .collect();
        // Portals remain independent in layout. In the accessibility tree, an
        // anchored popup belongs to the dialog containing its trigger, otherwise
        // modal projection would hide the newly opened panel from assistive tech.
        for (root, _) in &self.modal_stack {
            let Some(token) = engine.node(*root).and_then(|n| n.style.popup_anchor) else {
                continue;
            };
            let Some(anchor) = self
                .placed
                .iter()
                .filter_map(|p| self.reverse.get(&p.node_id).copied())
                .find(|e| {
                    engine
                        .node(*e)
                        .is_some_and(|n| n.style.anchor_id == Some(token))
                })
            else {
                continue;
            };
            let panel = engine
                .node(*root)
                .unwrap()
                .children
                .iter()
                .copied()
                .find(|e| engine.node(*e).is_some_and(|n| n.kind == Kind::Dialog));
            if let Some(trigger) = nodes.iter_mut().find(|n| n.id == semantic_id(anchor)) {
                trigger.controls = panel.map(semantic_id).into_iter().collect();
            }
            let enclosing = self
                .placed
                .iter()
                .rev()
                .filter_map(|p| self.reverse.get(&p.node_id).copied())
                .find(|e| {
                    engine.node(*e).is_some_and(|n| n.kind == Kind::Dialog)
                        && modal::contains(engine, *e, anchor)
                });
            if let Some(enclosing) = enclosing {
                let root_id = semantic_id(*root);
                if let Some(parent) = nodes
                    .iter_mut()
                    .find(|n| n.id == semantic_id(engine.root()))
                {
                    parent.children.retain(|id| *id != root_id);
                }
                if let Some(parent) = nodes.iter_mut().find(|n| n.id == semantic_id(enclosing)) {
                    parent.children.push(root_id);
                }
            }
        }
        self.tooltip_semantics(engine, &mut nodes);
        plico_renderer::SemanticSnapshot {
            revision,
            nodes,
            root_children: vec![semantic_id(engine.root())],
            focus: self.focused.map(semantic_id),
            modal_root: self.accessible_modal(engine).map(semantic_id),
        }
    }
    pub fn semantic_action(
        &mut self,
        engine: &mut Engine,
        action: plico_renderer::SemanticAction,
    ) -> bool {
        use plico_renderer::SemanticAction;
        self.semantic.get_mut().epoch = self.semantic.get_mut().epoch.wrapping_add(1);
        let target = match action {
            SemanticAction::Click { target }
            | SemanticAction::Focus { target }
            | SemanticAction::Blur { target }
            | SemanticAction::SetNumericValue { target, .. }
            | SemanticAction::Increment { target }
            | SemanticAction::Decrement { target } => target,
            _ => return false,
        };
        let Some(e) = self
            .tab_order
            .iter()
            .copied()
            .find(|e| semantic_id(*e) == target)
            .or_else(|| {
                self.modal_dialog(engine)
                    .filter(|e| semantic_id(*e) == target)
            })
        else {
            return false;
        };
        if !self.modal_allows(engine, e) || !engine.node(e).is_some_and(|n| !n.style.disabled) {
            return false;
        }
        let caps = engine.node(e).unwrap().kind.capabilities();
        let allowed = match action {
            SemanticAction::Focus { .. } | SemanticAction::Blur { .. } => {
                caps.contains(Capabilities::FOCUS)
            }
            SemanticAction::Click { .. } => {
                caps.intersects(Capabilities::ACTIVATE | Capabilities::FOCUS)
            }
            SemanticAction::SetNumericValue { .. }
            | SemanticAction::Increment { .. }
            | SemanticAction::Decrement { .. } => caps.contains(Capabilities::VALUE_EDIT),
            _ => false,
        };
        if !allowed {
            return false;
        }
        let action = if matches!(action, SemanticAction::Click { .. })
            && engine.node(e).is_some_and(|n| {
                n.kind.capabilities().contains(Capabilities::FOCUS)
                    && !n.kind.capabilities().contains(Capabilities::ACTIVATE)
            }) {
            SemanticAction::Focus { target }
        } else {
            action
        };
        let old_focus = self.focused;
        if let Some(range) = engine.node(e).and_then(|n| n.numeric) {
            let current = engine.pending_value(e).unwrap_or(range.value);
            let value = match action {
                SemanticAction::SetNumericValue { value, .. } => Some(value),
                SemanticAction::Increment { .. } => Some(current + range.increment()),
                SemanticAction::Decrement { .. } => Some(current - range.increment()),
                _ => None,
            };
            if let Some(value) = value {
                if !value.is_finite() {
                    return false;
                }
                engine.queue_value(e, range.normalize(value));
                return true;
            }
        } else if matches!(
            action,
            SemanticAction::SetNumericValue { .. }
                | SemanticAction::Increment { .. }
                | SemanticAction::Decrement { .. }
        ) {
            return false;
        }
        match action {
            SemanticAction::Click { .. } => {
                if !engine
                    .node(e)
                    .is_some_and(|n| n.kind.capabilities().contains(Capabilities::ACTIVATE))
                {
                    return false;
                }
                engine.queue_click(e);
            }
            SemanticAction::Focus { .. } => {
                self.focused = Some(e);
                self.focus_visible = true;
                self.reveal_focus = true;
            }
            SemanticAction::Blur { .. } if self.focused == Some(e) => {
                self.focused = None;
            }
            _ => {}
        }
        if old_focus != self.focused
            && let Some((buffer, _)) = old_focus.and_then(|e| engine.input(e))
        {
            buffer.edit(plico::InputEdit::ClearPreedit);
        }
        true
    }
    pub fn cursor(&self) -> CursorStyle {
        if self.selection.hovered {
            return CursorStyle::Text;
        }
        if self
            .hovered
            .is_some_and(|e| self.input_layouts.contains_key(&e))
        {
            CursorStyle::Text
        } else if self.hovered.is_some() {
            CursorStyle::Pointer
        } else {
            CursorStyle::Default
        }
    }
}
pub(crate) fn semantic_id(e: Entity) -> u64 {
    e.raw()
}
