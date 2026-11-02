//! Native application zoom, independent from OS pixel density.
use plico_renderer::layout::PlacedNode;
use plico_renderer::{
    EventResponse, FrameSource, KeyPhase, Scene, SemanticSnapshot, TextContext, UiEvent,
};
use std::sync::Arc;
struct ZoomState {
    factor: std::sync::atomic::AtomicU64,
    wake: std::sync::Mutex<Option<plico_renderer::WakeCallback>>,
}
/// One window's shared native zoom setting. Values outside 50–300% are rejected.
#[derive(Clone)]
pub struct ZoomController(Arc<ZoomState>);
impl Default for ZoomController {
    fn default() -> Self {
        Self(Arc::new(ZoomState {
            factor: std::sync::atomic::AtomicU64::new(1f64.to_bits()),
            wake: std::sync::Mutex::new(None),
        }))
    }
}
impl ZoomController {
    pub fn factor(&self) -> f64 {
        f64::from_bits(self.0.factor.load(std::sync::atomic::Ordering::Acquire))
    }
    /// Returns false for invalid/out-of-range input. Setting a valid factor wakes its host.
    pub fn set(&self, factor: f64) -> bool {
        if !factor.is_finite() || !(0.5..=3.).contains(&factor) {
            return false;
        }
        if self
            .0
            .factor
            .swap(factor.to_bits(), std::sync::atomic::Ordering::AcqRel)
            != factor.to_bits()
        {
            let wake = self.0.wake.lock().unwrap().clone();
            if let Some(wake) = wake {
                wake();
            }
        }
        true
    }
}
/// Scale viewport, rendering, input, semantics and IME geometry consistently.
pub struct ZoomSource {
    inner: Box<dyn FrameSource>,
    controller: ZoomController,
    zoom: f64,
}
impl ZoomSource {
    /// Wrap a frame source at 100% zoom with standard Ctrl +/-/0 shortcuts.
    pub fn new(inner: Box<dyn FrameSource>) -> Self {
        Self::with_controller(inner, ZoomController::default())
    }
    /// Share a controller with native host UI or a gallery demo.
    pub fn with_controller(inner: Box<dyn FrameSource>, controller: ZoomController) -> Self {
        Self {
            inner,
            controller,
            zoom: 1.,
        }
    }
    /// Clone the native, resident zoom controller (never a serialized UI resource).
    pub fn controller(&self) -> ZoomController {
        self.controller.clone()
    }
    fn sync(&mut self) {
        self.zoom = self.controller.factor();
    }
    fn viewport(&self, size: (u32, u32)) -> (u32, u32) {
        (
            (f64::from(size.0) / self.zoom).round().max(1.) as u32,
            (f64::from(size.1) / self.zoom).round().max(1.) as u32,
        )
    }
}
impl FrameSource for ZoomSource {
    fn set_device_scale(&mut self, scale: f64) {
        self.sync();
        self.inner.set_device_scale(scale * self.zoom);
    }
    fn prepare_for_event(&mut self, text: &mut TextContext) {
        self.inner.prepare_for_event(text)
    }
    fn build_frame(&mut self, text: &mut TextContext, width: u32, height: u32) -> Vec<PlacedNode> {
        self.sync();
        let (w, h) = self.viewport((width, height));
        self.inner.build_frame(text, w, h)
    }
    fn paint_scene(
        &mut self,
        scene: &mut Scene,
        nodes: &[PlacedNode],
        text: &mut TextContext,
        size: (u32, u32),
        scale: f64,
    ) {
        self.inner
            .paint_scene(scene, nodes, text, self.viewport(size), scale * self.zoom)
    }
    fn render_retained_frame(
        &mut self,
        scene: &mut Scene,
        text: &mut TextContext,
        size: (u32, u32),
        scale: f64,
    ) -> Option<(usize, f64, f64)> {
        self.sync();
        self.inner
            .render_retained_frame(scene, text, self.viewport(size), scale * self.zoom)
    }
    fn base_color(&self) -> peniko::Color {
        self.inner.base_color()
    }
    fn has_anim(&self) -> bool {
        self.inner.has_anim() || self.zoom != self.controller.factor()
    }
    fn animation_deadline(&self) -> Option<web_time::Instant> {
        self.inner.animation_deadline()
    }
    fn set_wake_callback(&mut self, w: plico_renderer::WakeCallback) {
        *self.controller.0.wake.lock().unwrap() = Some(w.clone());
        self.inner.set_wake_callback(w)
    }
    fn poll_async(&mut self) -> bool {
        self.inner.poll_async() || self.zoom != self.controller.factor()
    }
    fn take_host_action(&mut self) -> Option<plico_renderer::HostAction> {
        self.inner.take_host_action()
    }
    fn pointer_cursor(&self) -> plico_renderer::style::CursorStyle {
        self.inner.pointer_cursor()
    }
    fn set_semantics_enabled(&mut self, e: bool) {
        self.inner.set_semantics_enabled(e)
    }
    fn semantic_snapshot(&self) -> Option<Arc<SemanticSnapshot>> {
        let snapshot = self.inner.semantic_snapshot()?;
        if self.zoom == 1. {
            return Some(snapshot);
        }
        let mut snapshot = (*snapshot).clone();
        for node in &mut snapshot.nodes {
            for n in &mut node.bounds {
                *n *= self.zoom as f32;
            }
        }
        Some(Arc::new(snapshot))
    }
    fn handle_semantic_action(&mut self, a: plico_renderer::SemanticAction) -> bool {
        self.inner.handle_semantic_action(a)
    }
    fn handle_event(&mut self, mut event: UiEvent) -> EventResponse {
        if let UiEvent::Key(key) = &event
            && key.phase == KeyPhase::Down
            && (key.modifiers.control() || (cfg!(target_os = "macos") && key.modifiers.meta()))
        {
            let command = match key.key.as_str() {
                "+" | "=" => Some((self.controller.factor() + 0.25).min(3.)),
                "-" | "_" => Some((self.controller.factor() - 0.25).max(0.5)),
                "0" => Some(1.),
                _ => None,
            };
            if let Some(command) = command {
                self.controller.set(command);
                return EventResponse {
                    handled: true,
                    request_redraw: true,
                    ..Default::default()
                };
            }
        }
        match &mut event {
            UiEvent::Pointer(p) => {
                p.position.x /= self.zoom;
                p.position.y /= self.zoom;
            }
            UiEvent::Wheel(w) => {
                w.position.x /= self.zoom;
                w.position.y /= self.zoom;
                w.delta_x /= self.zoom;
                w.delta_y /= self.zoom;
            }
            UiEvent::WindowMetrics(m) => {
                let (w, h) = self.viewport((m.logical_width, m.logical_height));
                m.logical_width = w;
                m.logical_height = h;
                m.scale_factor *= self.zoom;
            }
            _ => {}
        }
        self.inner.handle_event(event)
    }
    fn ime_state(&self) -> Option<plico_renderer::ImeState> {
        let mut state = self.inner.ime_state()?;
        for x in &mut state.cursor_area {
            *x *= self.zoom;
        }
        Some(state)
    }
    fn ime_text_for_range(&self, range: std::ops::Range<usize>) -> Option<String> {
        self.inner.ime_text_for_range(range)
    }
    fn ime_bounds_for_range(&self, range: std::ops::Range<usize>) -> Option<[f64; 4]> {
        self.inner
            .ime_bounds_for_range(range)
            .map(|rect| rect.map(|v| v * self.zoom))
    }
    fn ime_character_index_for_point(&self, point: plico_renderer::Point) -> Option<usize> {
        self.inner
            .ime_character_index_for_point(plico_renderer::Point {
                x: point.x / self.zoom,
                y: point.y / self.zoom,
            })
    }
    fn paint_debug_overlay(
        &mut self,
        scene: &mut Scene,
        nodes: &[PlacedNode],
        text: &mut TextContext,
        scale: f64,
    ) {
        self.inner
            .paint_debug_overlay(scene, nodes, text, scale * self.zoom)
    }
    fn take_effect(&mut self) -> Option<plico_renderer::EffectRequest> {
        self.inner.take_effect()
    }
    fn complete_effect(&mut self, e: plico_renderer::EffectCompletion) {
        self.inner.complete_effect(e)
    }
    fn close_requested(&mut self) -> bool {
        self.inner.close_requested()
    }
    fn push_frame_stats(&mut self, s: &plico_renderer::FrameStats) {
        self.inner.push_frame_stats(s)
    }
    fn augment_frame_stats(&self, s: &mut plico_renderer::FrameStats) {
        self.inner.augment_frame_stats(s)
    }
    fn take_screenshot_request(&mut self) -> Option<plico_renderer::ScreenshotRequest> {
        self.inner.take_screenshot_request()
    }
    fn complete_screenshot(
        &mut self,
        path: &std::path::Path,
        result: Result<std::path::PathBuf, String>,
    ) {
        self.inner.complete_screenshot(path, result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{HostView, Source};
    use plico::prelude::*;
    use plico_renderer::{
        KeyEvent, KeyLocation, Modifiers, Point, PointerButton, PointerEvent, PointerPhase,
        SemanticAction, SemanticRole,
    };
    use std::sync::atomic::{AtomicUsize, Ordering};
    struct Demo {
        clicks: Arc<AtomicUsize>,
        buffer: plico::TextBuffer,
    }
    impl HostView for Demo {
        #[plico::component]
        fn show(&mut self, cx: &mut UiContext) {
            column(cx).padding(20).gap(16).children(|cx| {
                if button(cx).text("Counter").width(140.).clicked() {
                    self.clicks.fetch_add(1, Ordering::Relaxed);
                }
                text_input(cx, "Editor", &self.buffer);
            });
        }
    }
    fn setup() -> (ZoomSource, Arc<AtomicUsize>) {
        let clicks = Arc::new(AtomicUsize::new(0));
        let source = Source::new(Demo {
            clicks: clicks.clone(),
            buffer: {
                let buffer = plico::TextBuffer::new();
                buffer.set("abc".to_owned().into());
                buffer
            },
        });
        (ZoomSource::new(Box::new(source)), clicks)
    }
    #[test]
    fn viewport_semantic_bounds_and_pointer_hits_share_the_zoom_factor() {
        let (mut source, clicks) = setup();
        let mut text = TextContext::new();
        source.set_semantics_enabled(true);
        source.build_frame(&mut text, 900, 760);
        let initial = source.semantic_snapshot().unwrap();
        let bounds = initial
            .node_by_role(SemanticRole::Button, "Counter", None)
            .unwrap()
            .bounds;
        source.controller().set(1.5);
        source.poll_async();
        source.build_frame(&mut text, 900, 760);
        let projected = source.semantic_snapshot().unwrap();
        let button = projected
            .node_by_role(SemanticRole::Button, "Counter", None)
            .unwrap();
        assert!((button.bounds[2] - button.bounds[0]) > (bounds[2] - bounds[0]));
        let point = Point {
            x: f64::from((button.bounds[0] + button.bounds[2]) * 0.5),
            y: f64::from((button.bounds[1] + button.bounds[3]) * 0.5),
        };
        for phase in [PointerPhase::Down, PointerPhase::Up] {
            source.prepare_for_event(&mut text);
            assert!(
                source
                    .handle_event(UiEvent::Pointer(PointerEvent {
                        phase,
                        position: point,
                        button: Some(PointerButton::Primary),
                        buttons: if phase == PointerPhase::Down { 1 } else { 0 },
                        modifiers: Modifiers::empty(),
                        properties: Default::default()
                    }))
                    .handled
            );
        }
        source.build_frame(&mut text, 900, 760);
        assert_eq!(clicks.load(Ordering::Relaxed), 1);
    }
    #[test]
    fn ctrl_shortcuts_change_zoom_and_invalid_factors_are_rejected() {
        let (mut source, _) = setup();
        let controller = source.controller();
        assert!(!controller.set(f64::NAN));
        assert!(!controller.set(0.));
        assert!(!controller.set(4.));
        for (key, expected) in [("+", 1.25), ("=", 1.5), ("-", 1.25), ("0", 1.)] {
            assert!(
                source
                    .handle_event(UiEvent::Key(KeyEvent {
                        phase: KeyPhase::Down,
                        key: key.into(),
                        key_without_modifiers: key.into(),
                        code: String::new(),
                        text: None,
                        text_with_all_modifiers: None,
                        location: KeyLocation::Standard,
                        modifiers: Modifiers::CONTROL,
                        repeat: false,
                        synthetic: false
                    }))
                    .handled
            );
            assert_eq!(controller.factor(), expected);
        }
    }
    #[test]
    fn ime_anchor_and_range_geometry_scale_without_changing_editor_offsets() {
        let (mut source, _) = setup();
        let mut text = TextContext::new();
        source.set_semantics_enabled(true);
        source.controller().set(1.5);
        source.build_frame(&mut text, 900, 760);
        let snapshot = source.semantic_snapshot().unwrap();
        let target = snapshot
            .nodes
            .iter()
            .find(|n| n.label.as_deref() == Some("Editor"))
            .unwrap()
            .id;
        assert!(source.handle_semantic_action(SemanticAction::Focus { target }));
        source.build_frame(&mut text, 900, 760);
        let inner = source.inner.ime_state().expect("Editor must activate IME");
        let projected = source.ime_state().unwrap();
        assert_eq!(projected.cursor_area, inner.cursor_area.map(|n| n * 1.5));
        assert_eq!(projected.selection_utf16, inner.selection_utf16);
        assert_eq!(projected.surrounding_text, "abc");
        assert_eq!(
            source.ime_text_for_range(0..3),
            source.inner.ime_text_for_range(0..3)
        );
        assert_eq!(
            source.ime_bounds_for_range(0..2),
            source
                .inner
                .ime_bounds_for_range(0..2)
                .map(|rect| rect.map(|v| v * 1.5))
        );
    }
}
