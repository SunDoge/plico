use plico_core::{Clock, VirtualClock};
use plico_renderer::{FrameSource, Scene, TextContext, UiEvent, web::WebRenderer};
use plico_vello::{Source, View};
use std::time::Duration;
use wasm_bindgen::prelude::*;

/// Owns the layout, scene and WebGL renderer. The browser drives time and events.
pub struct WebHost<V: View> {
    pub source: Source<V>,
    canvas: web_sys::HtmlCanvasElement,
    renderer: WebRenderer,
    pub(crate) text: TextContext,
    scene: Scene,
    clock: VirtualClock,
}
impl<V: View> WebHost<V> {
    pub fn new(canvas: web_sys::HtmlCanvasElement, view: V) -> Result<Self, JsValue> {
        let renderer = WebRenderer::new(canvas.clone()).map_err(error)?;
        let clock = VirtualClock::default();
        let mut source = Source::with_clock(view, clock.clone());
        source.set_wake_callback(std::sync::Arc::new(|| {
            if let Some(window) = web_sys::window()
                && let Ok(event) = web_sys::Event::new("plico-wake")
            {
                let _ = window.dispatch_event(&event);
            }
        }));
        Ok(Self {
            source,
            canvas,
            renderer,
            text: TextContext::new(),
            scene: Scene::new(),
            clock,
        })
    }
    pub fn load_font(&mut self, bytes: Vec<u8>) {
        self.text.load_font(bytes);
        self.source.request_rebuild();
    }
    fn set_time(&self, milliseconds: f64) -> Result<(), JsValue> {
        let time = Duration::try_from_secs_f64(milliseconds / 1000.)
            .map_err(|_| error("invalid browser clock"))?;
        self.clock
            .set(time.max(self.clock.now()))
            .map_err(|e| error(e.to_string()))
    }
    /// Logical CSS size plus device-pixel ratio. Returns whether animation needs another frame.
    pub fn frame(
        &mut self,
        milliseconds: f64,
        width: u32,
        height: u32,
        scale: f64,
    ) -> Result<bool, JsValue> {
        self.set_time(milliseconds)?;
        if width == 0
            || height == 0
            || !scale.is_finite()
            || scale <= 0.
            || f64::from(width) * scale > f64::from(u16::MAX)
            || f64::from(height) * scale > f64::from(u16::MAX)
        {
            return Err(error("invalid canvas viewport"));
        }
        self.source.poll_async();
        self.source
            .render_retained_frame(&mut self.scene, &mut self.text, (width, height), scale);
        if let Some(e) = &self.source.last_error {
            return Err(error(e.to_string()));
        }
        self.renderer
            .render(
                &self.scene,
                (f64::from(width) * scale).ceil() as u32,
                (f64::from(height) * scale).ceil() as u32,
            )
            .map_err(error)?;
        Ok(self.source.has_anim())
    }
    /// Flush the previous discrete input before routing the next one.
    pub fn dispatch(
        &mut self,
        milliseconds: f64,
        event: UiEvent,
    ) -> Result<plico_renderer::EventResponse, JsValue> {
        self.set_time(milliseconds)?;
        self.source.prepare_for_event(&mut self.text);
        Ok(self.source.handle_event(event))
    }
    pub fn selected_text(&self) -> Option<String> {
        self.source.adapter.selected_text()
    }
    pub fn pointer(&mut self, time: f64, event: &web_sys::PointerEvent) -> Result<(), JsValue> {
        use plico_renderer::*;
        let rect = self.canvas.get_bounding_client_rect();
        let phase = match event.type_().as_str() {
            "pointerdown" => PointerPhase::Down,
            "pointerup" => PointerPhase::Up,
            "pointercancel" => PointerPhase::Cancel,
            "pointerleave" => PointerPhase::Leave,
            _ => PointerPhase::Move,
        };
        let modifiers = modifiers(
            event.shift_key(),
            event.ctrl_key(),
            event.alt_key(),
            event.meta_key(),
        );
        // Native layout: primary=1, secondary=2, auxiliary=4, matching DOM buttons.
        self.dispatch(
            time,
            UiEvent::Pointer(PointerEvent {
                phase,
                position: Point {
                    x: event.client_x() as f64 - rect.left(),
                    y: event.client_y() as f64 - rect.top(),
                },
                button: match event.button() {
                    0 => Some(PointerButton::Primary),
                    1 => Some(PointerButton::Auxiliary),
                    2 => Some(PointerButton::Secondary),
                    _ => None,
                },
                buttons: event.buttons() as u32,
                modifiers,
                properties: PointerProperties {
                    id: PointerId {
                        lo: event.pointer_id() as u32,
                        hi: 0,
                    },
                    pointer_type: match event.pointer_type().as_str() {
                        "mouse" => PointerType::Mouse,
                        "touch" => PointerType::Touch,
                        "pen" => PointerType::Pen,
                        _ => PointerType::Unknown,
                    },
                    primary: event.is_primary(),
                    pressure: Some(event.pressure() as f64),
                    ..Default::default()
                },
            }),
        )?;
        Ok(())
    }
    pub fn key(&mut self, time: f64, event: &web_sys::KeyboardEvent) -> Result<bool, JsValue> {
        use plico_renderer::*;
        let key = event.key();
        let response = self.dispatch(
            time,
            UiEvent::Key(KeyEvent {
                phase: if event.type_() == "keyup" {
                    KeyPhase::Up
                } else {
                    KeyPhase::Down
                },
                key: key.clone(),
                key_without_modifiers: key,
                code: event.code(),
                text: None,
                text_with_all_modifiers: None,
                location: KeyLocation::Standard,
                modifiers: modifiers(
                    event.shift_key(),
                    event.ctrl_key(),
                    event.alt_key(),
                    event.meta_key(),
                ),
                repeat: event.repeat(),
                synthetic: false,
            }),
        )?;
        Ok(response.handled)
    }
    pub fn wheel(&mut self, time: f64, event: &web_sys::WheelEvent) -> Result<bool, JsValue> {
        use plico_renderer::*;
        let rect = self.canvas.get_bounding_client_rect();
        let factor = match event.delta_mode() {
            1 => WHEEL_LINE_DELTA,
            2 => rect.height(),
            _ => 1.,
        };
        let response = self.dispatch(
            time,
            UiEvent::Wheel(WheelEvent {
                position: Point {
                    x: event.client_x() as f64 - rect.left(),
                    y: event.client_y() as f64 - rect.top(),
                },
                delta_x: event.delta_x() * factor,
                delta_y: event.delta_y() * factor,
                delta_mode: WheelDeltaMode::Pixel,
                phase: GesturePhase::Changed,
                modifiers: modifiers(
                    event.shift_key(),
                    event.ctrl_key(),
                    event.alt_key(),
                    event.meta_key(),
                ),
            }),
        )?;
        Ok(response.handled)
    }
}
fn modifiers(shift: bool, ctrl: bool, alt: bool, meta: bool) -> plico_renderer::Modifiers {
    use plico_renderer::Modifiers as M;
    let mut m = M::empty();
    if shift {
        m |= M::SHIFT;
    }
    if ctrl {
        m |= M::CONTROL;
    }
    if alt {
        m |= M::ALT;
    }
    if meta {
        m |= M::META;
    }
    m
}
fn error(message: impl AsRef<str>) -> JsValue {
    JsValue::from_str(message.as_ref())
}

/// Optional gallery binding; applications can wrap WebHost with their own wasm-bindgen API.
#[cfg(feature = "gallery")]
#[wasm_bindgen]
pub struct Gallery {
    host: WebHost<plico_vello::NativeApp<plico_gallery::Gallery>>,
}
#[cfg(feature = "gallery")]
#[wasm_bindgen]
impl Gallery {
    #[wasm_bindgen(constructor)]
    pub fn new(canvas: web_sys::HtmlCanvasElement) -> Result<Self, JsValue> {
        Ok(Self {
            host: WebHost::new(
                canvas,
                plico_vello::NativeApp(plico_gallery::Gallery::default()),
            )?,
        })
    }
    pub fn load_font(&mut self, bytes: Vec<u8>) {
        self.host.load_font(bytes);
    }
    pub fn frame(
        &mut self,
        time: f64,
        width: u32,
        height: u32,
        scale: f64,
    ) -> Result<bool, JsValue> {
        self.host.frame(time, width, height, scale)
    }
    pub fn selected_text(&self) -> Option<String> {
        self.host.selected_text()
    }
    pub fn pointer(&mut self, time: f64, event: web_sys::PointerEvent) -> Result<(), JsValue> {
        self.host.pointer(time, &event)
    }
    pub fn key(&mut self, time: f64, event: web_sys::KeyboardEvent) -> Result<bool, JsValue> {
        self.host.key(time, &event)
    }
    pub fn wheel(&mut self, time: f64, event: web_sys::WheelEvent) -> Result<bool, JsValue> {
        self.host.wheel(time, &event)
    }
    pub fn text(&mut self, time: f64, text: String) -> Result<(), JsValue> {
        self.host.dispatch(time, UiEvent::TextInput(text))?;
        Ok(())
    }
    pub fn focus(&mut self, time: f64, focused: bool) -> Result<(), JsValue> {
        self.host.dispatch(time, UiEvent::Focus(focused))?;
        Ok(())
    }
    pub fn debug_state(&self) -> String {
        serde_json::to_string(&self.host.source.view.0).unwrap()
    }
    pub fn semantics(&self) -> String {
        let snapshot = self
            .host
            .source
            .adapter
            .semantics(&self.host.source.engine, 0);
        let nodes: Vec<_> = snapshot
            .nodes
            .iter()
            .map(|n| {
                serde_json::json!({
                    "id":n.id, "role":format!("{:?}",n.role), "label":n.label,
                    "bounds":n.bounds, "disabled":n.disabled
                })
            })
            .collect();
        serde_json::to_string(&nodes).unwrap()
    }
}
