//! Host-owned native content in one explicitly sized Plico layout slot.
//! The widget is never serialized into a UI checkpoint.
use super::{Source, View};
use plico_core::Engine;
use plico_renderer::platform::effect::{EffectId, EffectPayload, EffectResult, EffectScope};
use plico_renderer::{
    EffectCompletion, EffectRequest, EventResponse, FrameSource, HostAction, ImeState,
    MeasureContext, PaintContext, Point, PointerPhase, Scene, TextContext, UiEvent, WakeCallback,
    Widget, WidgetAvailableSpace, WidgetGeometry, WidgetNodeEvent, layout::PlacedNode,
};
use std::{
    collections::{HashMap, HashSet, VecDeque},
    sync::Arc,
    time::Instant,
};

/// A single native widget anchored to a unique static local layout key.
///
/// The slot must have a size supplied by layout. Intrinsic measurement is not
/// fed back to Taffy. Native content is inserted at its node's paint position.
pub struct SurfaceSource<V: View, W: Widget> {
    pub source: Source<V>,
    pub widget: W,
    slot: &'static str,
    node: Option<taffy::NodeId>,
    rect: [f32; 4],
    local_to_window: [f64; 6],
    window_to_local: [f64; 6],
    focused: bool,
    window_focused: bool,
    captured: bool,
    visible: bool,
    mounted: bool,
    event_dirty: bool,
    entity: Option<plico_core::Entity>,
    scale: f64,
    actions: VecDeque<HostAction>,
    effects: VecDeque<EffectRequest>,
    reads: HashMap<EffectId, u64>,
    writes: HashSet<EffectId>,
    next_effect: u32,
    on_event: Option<fn(&mut V, WidgetNodeEvent) -> bool>,
}
impl<V: View, W: Widget> SurfaceSource<V, W> {
    pub fn new(view: V, widget: W, slot: &'static str) -> Self {
        Self {
            source: Source::new(view),
            widget,
            slot,
            node: None,
            rect: [0.; 4],
            local_to_window: [1., 0., 0., 1., 0., 0.],
            window_to_local: [1., 0., 0., 1., 0., 0.],
            focused: false,
            window_focused: true,
            captured: false,
            visible: false,
            mounted: false,
            event_dirty: false,
            entity: None,
            scale: 1.,
            actions: VecDeque::new(),
            effects: VecDeque::new(),
            reads: HashMap::new(),
            writes: HashSet::new(),
            next_effect: 0,
            on_event: None,
        }
    }
    pub fn with_events(mut self, handler: fn(&mut V, WidgetNodeEvent) -> bool) -> Self {
        self.on_event = Some(handler);
        self
    }
    fn locate(
        engine: &Engine,
        parent: plico_core::Entity,
        key: &'static str,
    ) -> Option<plico_core::Entity> {
        engine.find(parent, key).or_else(|| {
            engine
                .node(parent)?
                .children
                .iter()
                .find_map(|child| Self::locate(engine, *child, key))
        })
    }
    fn focus(&mut self, focused: bool) {
        if self.focused != focused {
            self.focused = focused;
            self.widget.focus_changed(focused && self.window_focused);
        }
    }
    fn drain(&mut self) {
        while let Some(event) = self.widget.take_node_event() {
            if let Some(handler) = self.on_event
                && handler(&mut self.source.view, event)
            {
                self.source.request_rebuild();
                self.event_dirty = true;
            }
        }
        while let Some(action) = self.widget.take_host_action() {
            let (payload, read) = match action {
                HostAction::SetClipboard(text) => (EffectPayload::ClipboardWrite { text }, None),
                HostAction::ReadClipboard { request_id } => {
                    (EffectPayload::ClipboardRead, Some(request_id))
                }
                action => {
                    self.actions.push_back(action);
                    continue;
                }
            };
            self.next_effect = self
                .next_effect
                .checked_add(1)
                .expect("surface effect IDs exhausted");
            let id = EffectId(self.next_effect);
            if let Some(request_id) = read {
                self.reads.insert(id, request_id);
            } else {
                self.writes.insert(id);
            }
            self.effects.push_back(EffectRequest {
                id,
                scope: EffectScope::Runtime,
                payload,
            });
        }
    }
    fn local(&self, p: Point) -> Point {
        let m = self.window_to_local;
        Point {
            x: m[0] * p.x + m[2] * p.y + m[4],
            y: m[1] * p.x + m[3] * p.y + m[5],
        }
    }
    fn contains(&self, p: Point) -> bool {
        let p = self.local(p);
        self.entity
            .is_some_and(|e| self.source.adapter.modal_allows(&self.source.engine, e))
            && self
                .source
                .adapter
                .hit(
                    &self.source.engine,
                    p.x * self.local_to_window[0] + self.rect[0] as f64,
                    p.y * self.local_to_window[3] + self.rect[1] as f64,
                )
                .is_none()
            && p.x >= 0.
            && p.y >= 0.
            && p.x < self.rect[2] as f64
            && p.y < self.rect[3] as f64
    }
}
impl<V: View, W: Widget> Drop for SurfaceSource<V, W> {
    fn drop(&mut self) {
        if self.mounted {
            self.widget.unmount();
        }
    }
}
impl<V: View, W: Widget> FrameSource for SurfaceSource<V, W> {
    fn set_device_scale(&mut self, scale: f64) {
        self.scale = scale;
        self.source.set_device_scale(scale);
    }
    fn set_wake_callback(&mut self, wake: WakeCallback) {
        self.source.set_wake_callback(wake.clone());
        self.widget.set_wake_callback(wake);
    }
    fn poll_async(&mut self) -> bool {
        let changed = self.widget.poll_async_changes();
        self.drain();
        self.source.poll_async() | !changed.is_empty() | std::mem::take(&mut self.event_dirty)
    }
    fn take_host_action(&mut self) -> Option<HostAction> {
        self.drain();
        self.actions
            .pop_front()
            .or_else(|| self.source.take_host_action())
    }
    fn take_effect(&mut self) -> Option<EffectRequest> {
        self.drain();
        self.effects
            .pop_front()
            .or_else(|| self.source.take_effect())
    }
    fn complete_effect(&mut self, completion: EffectCompletion) {
        if let Some(request_id) = self.reads.remove(&completion.id) {
            let text = match completion.result {
                EffectResult::ClipboardText(text) => text,
                _ => None,
            };
            self.widget.deliver_clipboard_text(request_id, text);
        } else if !self.writes.remove(&completion.id) {
            self.source.complete_effect(completion);
        }
    }
    fn prepare_for_event(&mut self, text: &mut TextContext) {
        self.source.prepare_for_event(text);
        self.widget.prepare_for_event(text);
    }
    fn build_frame(&mut self, text: &mut TextContext, width: u32, height: u32) -> Vec<PlacedNode> {
        self.event_dirty = false;
        let mut nodes = self.source.build_frame(text, width, height);
        self.entity = Self::locate(&self.source.engine, self.source.engine.root(), self.slot);
        self.node = self.entity.and_then(|e| self.source.adapter.backend_id(e));
        let slot = self
            .node
            .and_then(|id| nodes.iter_mut().find(|n| n.node_id == id));
        if let Some(slot) = slot {
            let [w, h] = slot.content_size;
            let visible = w > 0. && h > 0.;
            if visible != self.visible {
                self.widget.visibility_changed(visible);
                if visible && !self.mounted {
                    self.widget.mounted();
                    self.mounted = true;
                }
                self.visible = visible;
            }
            // This first embedding API supports ordinary layout slots;
            // transformed native surfaces need a separate geometry contract.
            let origin = slot.content_origin;
            self.rect = [origin[0], origin[1], w, h];
            self.local_to_window = [1., 0., 0., 1., origin[0] as f64, origin[1] as f64];
            self.window_to_local = [1., 0., 0., 1., -origin[0] as f64, -origin[1] as f64];
            let mut measure = MeasureContext::new(
                [Some(w), Some(h)],
                [
                    WidgetAvailableSpace::Definite(w),
                    WidgetAvailableSpace::Definite(h),
                ],
                self.scale,
                text,
            );
            self.widget.measure(&mut measure);
            self.widget.layout_changed(WidgetGeometry {
                content_size: [w, h],
                device_scale: self.scale,
                local_to_window: self.local_to_window,
                window_to_local: self.window_to_local,
            });
            // Actual device scale is supplied by paint_scene, below.
        } else {
            if self.visible {
                self.widget.visibility_changed(false);
            }
            self.visible = false;
            self.focus(false);
            self.captured = false;
        }
        nodes
    }
    fn paint_scene(
        &mut self,
        scene: &mut Scene,
        nodes: &[PlacedNode],
        text: &mut TextContext,
        size: (u32, u32),
        scale: f64,
    ) {
        self.scale = scale;
        let mut nodes = nodes.to_vec();
        if let Some(slot) = self
            .node
            .filter(|_| self.visible)
            .and_then(|id| nodes.iter_mut().find(|n| n.node_id == id))
        {
            let [w, h] = slot.content_size;
            self.widget.layout_changed(WidgetGeometry {
                content_size: [w, h],
                device_scale: scale,
                local_to_window: self.local_to_window,
                window_to_local: self.window_to_local,
            });
            let mut paint = PaintContext::new_clipped_with_radii_at(
                w,
                h,
                slot.paint.border_radii,
                scale,
                self.local_to_window,
                text,
            );
            self.widget.paint(&mut paint);
            slot.paint.widget = Some(Arc::new(paint.finish()));
        }
        plico_renderer::scene::build_scene_scaled(
            scene,
            &nodes,
            text,
            size.0,
            size.1,
            self.base_color(),
            scale,
        );
        self.drain();
    }
    fn handle_event(&mut self, mut event: UiEvent) -> EventResponse {
        if self.focused
            && self
                .entity
                .is_some_and(|e| !self.source.adapter.modal_allows(&self.source.engine, e))
        {
            self.focus(false);
            self.captured = false;
        }
        if let UiEvent::Focus(focused) = event {
            self.window_focused = focused;
            self.widget.focus_changed(focused && self.focused);
            return self.source.handle_event(event);
        }
        let route = match &mut event {
            UiEvent::Pointer(pointer) => {
                let inside = self.visible && self.contains(pointer.position);
                if pointer.phase == PointerPhase::Down {
                    self.focus(inside);
                    self.captured = inside;
                }
                let route = (inside || self.captured) && self.source.adapter.pressed.is_none();
                if matches!(pointer.phase, PointerPhase::Up | PointerPhase::Cancel) {
                    self.captured = false;
                }
                if route {
                    pointer.position = self.local(pointer.position);
                }
                route
            }
            UiEvent::Wheel(wheel) => {
                let inside = self.visible && self.contains(wheel.position);
                if inside {
                    wheel.position = self.local(wheel.position);
                }
                inside
            }
            UiEvent::Key(_) | UiEvent::TextInput(_) | UiEvent::Paste(_) | UiEvent::Ime(_) => {
                self.focused && self.visible
            }
            _ => false,
        };
        if route {
            let result = self.widget.handle_event(&event);
            self.drain();
            // Localized events belong to the surface even if the widget has
            // declined them; do not forward local coordinates to the outer UI.
            return EventResponse {
                handled: result.is_handled(),
                request_redraw: result.requests_redraw(),
                consume_key_text: result.consumes_key_text(),
                text_input: Some(self.focused && self.widget.accepts_text_input()),
                clipboard: result.clipboard_request().cloned(),
            };
        }
        self.source.handle_event(event)
    }
    fn ime_state(&self) -> Option<ImeState> {
        if !self.focused {
            return self.source.ime_state();
        }
        let state = self.widget.ime_state()?;
        let [x0, y0, x1, y1] = state.cursor_area;
        Some(ImeState {
            client_id: self.node.map_or(1, u64::from),
            cursor_area: [
                x0 as f64 + self.rect[0] as f64,
                y0 as f64 + self.rect[1] as f64,
                x1 as f64 + self.rect[0] as f64,
                y1 as f64 + self.rect[1] as f64,
            ],
            surrounding_text: state.surrounding_text,
            surrounding_cursor: state.surrounding_cursor,
            surrounding_anchor: state.surrounding_anchor,
            selection_utf16: state.selection_utf16,
            selection_reversed: state.selection_reversed,
            marked_range_utf16: state.marked_range_utf16,
            purpose: state.purpose,
            multiline: state.multiline,
            completion: state.completion,
            spellcheck: state.spellcheck,
        })
    }
    fn set_semantics_enabled(&mut self, enabled: bool) {
        self.source.set_semantics_enabled(enabled);
    }
    fn semantic_snapshot(&self) -> Option<Arc<plico_renderer::SemanticSnapshot>> {
        self.source.semantic_snapshot()
    }
    fn handle_semantic_action(&mut self, action: plico_renderer::SemanticAction) -> bool {
        let handled = self.source.handle_semantic_action(action);
        if handled {
            self.focus(false);
        }
        handled
    }
    fn base_color(&self) -> vello_common::peniko::Color {
        self.source.base_color()
    }
    fn has_anim(&self) -> bool {
        self.source.has_anim() || self.event_dirty
    }
    fn animation_deadline(&self) -> Option<Instant> {
        [
            self.source.animation_deadline(),
            self.widget.animation_deadline(),
        ]
        .into_iter()
        .flatten()
        .min()
    }
}
