//! Host-side declarations. egui controls enqueue IDs/values, never capture app state.
use plico::{Clock, DeclareView, Engine, FrameTime, UiContext};
use plico_renderer::{
    EventResponse, FrameSource, HostAction, Scene, TextContext, UiEvent, WakeCallback,
    layout::PlacedNode,
};
use plico_runtime::View;
use std::time::Duration;

pub struct Source<V> {
    pub view: V,
    pub engine: Engine,
    clock: Box<dyn Clock>,
    previous: Duration,
    dirty: bool,
    deadline: Option<web_time::Instant>,
}
impl<V: View> Source<V> {
    pub fn new(view: V) -> Self {
        Self::with_clock(view, plico::SystemClock::default())
    }
    pub fn with_clock(view: V, clock: impl Clock + 'static) -> Self {
        Self {
            view,
            engine: Engine::default(),
            clock: Box::new(clock),
            previous: Duration::ZERO,
            dirty: true,
            deadline: None,
        }
    }
}
struct Declaration<'a, V>(&'a mut V);
impl<V: View> DeclareView for Declaration<'_, V> {
    fn show(&mut self, cx: &mut UiContext) {
        self.0.show(cx);
    }
    fn theme(&self) -> plico::Theme {
        self.0.theme()
    }
}
impl<V: View> FrameSource for Source<V> {
    fn native_widget_frame(&mut self, size: [f32; 2]) -> Option<&mut Engine> {
        let now = self.clock.now();
        let time = FrameTime {
            now,
            delta: now.saturating_sub(self.previous),
        };
        self.previous = now;
        self.engine.set_time(time);
        self.view.set_frame_time(time);
        if let Err(error) = self.engine.set_viewport(size[0], size[1]).and_then(|()| {
            self.engine
                .declare_until_settled(&mut Declaration(&mut self.view))
                .map(|_| ())
        }) {
            tracing::error!(%error, "egui declarations failed");
            return None;
        }
        self.engine.drain_removed().for_each(drop);
        self.dirty = false;
        self.deadline = None;
        Some(&mut self.engine)
    }
    fn native_widget_input_changed(&mut self) {
        self.dirty = true;
    }
    fn native_widget_repaint_after(&mut self, delay: Duration) {
        if delay.is_zero() {
            self.dirty = true;
        } else if delay != Duration::MAX {
            self.deadline = web_time::Instant::now().checked_add(delay);
        }
    }
    fn animation_deadline(&self) -> Option<web_time::Instant> {
        self.deadline
    }
    fn build_frame(&mut self, _: &mut TextContext, _: u32, _: u32) -> Vec<PlacedNode> {
        Vec::new()
    }
    fn paint_scene(
        &mut self,
        _: &mut Scene,
        _: &[PlacedNode],
        _: &mut TextContext,
        _: (u32, u32),
        _: f64,
    ) {
    }
    fn has_anim(&self) -> bool {
        self.dirty || self.engine.changes().contains(plico::Changes::ANIMATE)
    }
    fn poll_async(&mut self) -> bool {
        let changed = self.view.poll_reload();
        self.dirty |= changed;
        changed
    }
    fn set_wake_callback(&mut self, wake: WakeCallback) {
        self.view.set_wake_callback(wake);
    }
    fn take_host_action(&mut self) -> Option<HostAction> {
        self.view.take_host_action()
    }
    fn base_color(&self) -> peniko::Color {
        peniko::Color::from_rgb8(
            ((self.view.theme().surface >> 16) & 255) as u8,
            ((self.view.theme().surface >> 8) & 255) as u8,
            (self.view.theme().surface & 255) as u8,
        )
    }
    fn handle_event(&mut self, event: UiEvent) -> EventResponse {
        let handled = match event {
            UiEvent::Key(key) => self.view.on_unhandled_key(&key),
            _ => false,
        };
        self.dirty |= handled;
        EventResponse {
            handled,
            request_redraw: handled,
            ..Default::default()
        }
    }
}
