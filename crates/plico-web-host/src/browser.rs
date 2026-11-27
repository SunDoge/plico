use plico::{Clock, VirtualClock};
use plico_renderer::{FrameSource, Scene, TextContext, UiEvent};
use plico_runtime::{HostView, Source};
use std::time::Duration;
use wasm_bindgen::prelude::*;

/// Owns layout, retained scene and a browser renderer; the browser drives time/events.
pub struct WebHost<V: HostView> {
    pub source: Source<V>,
    canvas: web_sys::HtmlCanvasElement,
    renderer: Box<plico_wgpu::web::WebRenderer>,
    pub(crate) text: TextContext,
    scene: Scene,
    clock: VirtualClock,
}
impl<V: HostView> WebHost<V> {
    #[cfg(feature = "wgpu")]
    pub async fn new_wgpu(
        canvas: web_sys::HtmlCanvasElement,
        view: V,
        backend: plico_wgpu::web::BrowserBackend,
    ) -> Result<Self, JsValue> {
        static HOOK: std::sync::Once = std::sync::Once::new();
        HOOK.call_once(|| {
            std::panic::set_hook(Box::new(|info| {
                web_sys::console::error_1(&JsValue::from_str(&info.to_string()));
            }))
        });
        let renderer = plico_wgpu::web::WebRenderer::new(canvas.clone(), backend)
            .await
            .map_err(|e| error(e.to_string()))?;
        Ok(Self::with_renderer(canvas, view, Box::new(renderer)))
    }
    fn with_renderer(
        canvas: web_sys::HtmlCanvasElement,
        view: V,
        renderer: Box<plico_wgpu::web::WebRenderer>,
    ) -> Self {
        let clock = VirtualClock::default();
        let mut source = Source::with_clock(view, clock.clone());
        source.set_wake_callback(std::sync::Arc::new(|| {
            if let Some(window) = web_sys::window()
                && let Ok(event) = web_sys::Event::new("plico-wake")
            {
                let _ = window.dispatch_event(&event);
            }
        }));
        Self {
            source,
            canvas,
            renderer,
            text: TextContext::new(),
            scene: Scene::new(),
            clock,
        }
    }
    pub fn renderer_name(&self) -> &'static str {
        self.renderer.backend_name()
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
        self.renderer
            .configure_text(&mut self.text)
            .map_err(|e| error(e.to_string()))?;
        self.source
            .render_frame(&mut self.scene, &mut self.text, (width, height), scale)
            .map_err(|e| error(e.to_string()))?;
        let size = [
            (f64::from(width) * scale).ceil() as u32,
            (f64::from(height) * scale).ceil() as u32,
        ];
        self.renderer
            .render(&self.scene, size, self.source.base_color().components)
            .map_err(|e| error(e.to_string()))?;
        Ok(self.source.has_anim() || self.source.animation_deadline().is_some())
    }
    pub fn next_frame_delay_ms(&self) -> f64 {
        if self.source.has_anim() {
            return 0.;
        }
        self.source.wake_delay().unwrap_or_default().as_secs_f64() * 1000.
    }
    /// Flush the previous discrete input before routing the next one.
    pub fn dispatch(
        &mut self,
        milliseconds: f64,
        event: UiEvent,
    ) -> Result<plico_renderer::EventResponse, JsValue> {
        self.set_time(milliseconds)?;
        self.source
            .try_prepare_for_event(&mut self.text)
            .map_err(|e| error(e.to_string()))?;
        Ok(self.source.handle_event(event))
    }
    pub fn selected_text(&self) -> Option<String> {
        self.source.selected_text()
    }
    /// The browser owns clipboard access during its paste event.
    pub fn paste(&mut self, time: f64, text: String) -> Result<bool, JsValue> {
        if text.is_empty() {
            return Ok(false);
        }
        Ok(self.dispatch(time, UiEvent::Paste(text))?.handled)
    }
    /// Called after a cut event has copied the editor selection to the clipboard.
    pub fn cut_selection(&mut self, time: f64) -> Result<(), JsValue> {
        if self.source.has_focused_text_input() {
            let key =
                plico::KeyEvent::logical(plico::KeyPhase::Down, "x", plico::Modifiers::CONTROL);
            self.dispatch(time, UiEvent::Key(key))?;
        }
        Ok(())
    }
    pub fn pointer(&mut self, time: f64, event: &web_sys::PointerEvent) -> Result<(), JsValue> {
        use plico_renderer::*;
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
        if event.pointer_type() == "mouse"
            && let Some(key) = match event.button() {
                3 => Some("BrowserBack"),
                4 => Some("BrowserForward"),
                _ => None,
            }
        {
            let response = self.dispatch(
                time,
                UiEvent::Key(KeyEvent::logical(
                    if phase == PointerPhase::Down {
                        KeyPhase::Down
                    } else {
                        KeyPhase::Up
                    },
                    key,
                    modifiers,
                )),
            )?;
            if response.handled {
                event.prevent_default();
            }
            return Ok(());
        }
        let rect = self.canvas.get_bounding_client_rect();
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
        // Composing keydowns belong to the IME. Allow releases through in case
        // a widget observed the press before composition started.
        if event.type_() == "keydown" && (event.is_composing() || event.key_code() == 229) {
            return Ok(false);
        }
        let key = event.key();
        // Let the browser deliver a trusted clipboard event with its DataTransfer.
        // In particular, cutting here would remove the selection before that event.
        if event.type_() == "keydown"
            && (event.ctrl_key() || event.meta_key())
            && !event.alt_key()
            && (key.eq_ignore_ascii_case("v")
                || (!event.shift_key() && matches!(key.as_str(), "c" | "C" | "x" | "X")))
            && (self.source.has_focused_text_input()
                || (key.eq_ignore_ascii_case("c") && self.selected_text().is_some()))
        {
            return Ok(false);
        }
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
                location: match event.location() {
                    1 => KeyLocation::Left,
                    2 => KeyLocation::Right,
                    3 => KeyLocation::Numpad,
                    _ => KeyLocation::Standard,
                },
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
    pub fn focus(&mut self, time: f64, focused: bool) -> Result<(), JsValue> {
        self.dispatch(time, UiEvent::Focus(focused))?;
        Ok(())
    }
    pub fn wheel(&mut self, time: f64, event: &web_sys::WheelEvent) -> Result<bool, JsValue> {
        use plico_renderer::*;
        let rect = self.canvas.get_bounding_client_rect();
        // MyGO internal/web/surface.go uses 16 CSS pixels per DOM line.
        // Pixel-mode notches already carry their distance. Only their motion
        // policy differs from the continuous deltas of a trackpad.
        let (x_scale, y_scale, delta_mode) = match event.delta_mode() {
            1 => (16., 16., WheelDeltaMode::Line),
            2 => (rect.width(), rect.height(), WheelDeltaMode::Line),
            0 if js_sys::Reflect::get(event, &JsValue::from_str("wheelDeltaY"))
                .ok()
                .and_then(|value| value.as_f64())
                .is_some_and(|delta| delta != 0. && delta % 120. == 0.) =>
            {
                (1., 1., WheelDeltaMode::Line)
            }
            _ => (1., 1., WheelDeltaMode::Pixel),
        };
        let response = self.dispatch(
            time,
            UiEvent::Wheel(WheelEvent {
                position: Point {
                    x: event.client_x() as f64 - rect.left(),
                    y: event.client_y() as f64 - rect.top(),
                },
                delta_x: event.delta_x() * x_scale,
                delta_y: event.delta_y() * y_scale,
                delta_mode,
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

/// Browser candidate-window position in canvas CSS pixels.
#[wasm_bindgen]
pub struct InputAnchor {
    pub client: u64,
    pub x: f64,
    pub y: f64,
    pub height: f64,
    pub secret: bool,
}

/// Optional gallery binding; applications can wrap WebHost with their own wasm-bindgen API.
#[cfg(feature = "gallery")]
#[wasm_bindgen]
pub struct Gallery {
    host: WebHost<plico_app_host::NativeApp<gallery::Gallery>>,
}
#[cfg(feature = "gallery")]
#[wasm_bindgen]
impl Gallery {
    #[cfg(feature = "wgpu")]
    pub async fn create_wgpu(
        canvas: web_sys::HtmlCanvasElement,
        backend: String,
    ) -> Result<Self, JsValue> {
        use plico_wgpu::web::BrowserBackend;
        let backend = match backend.as_str() {
            "auto" => BrowserBackend::Auto,
            "webgpu" => BrowserBackend::WebGpu,
            "webgl" | "webgl2" => BrowserBackend::WebGl,
            _ => return Err(error("unknown browser renderer")),
        };
        Ok(Self {
            host: WebHost::new_wgpu(
                canvas,
                plico_app_host::NativeApp(gallery::Gallery::default()),
                backend,
            )
            .await?,
        })
    }
    pub fn renderer_name(&self) -> String {
        self.host.renderer_name().into()
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
    pub fn next_frame_delay_ms(&self) -> f64 {
        self.host.next_frame_delay_ms()
    }
    pub fn pointer(&mut self, time: f64, event: web_sys::PointerEvent) -> Result<(), JsValue> {
        self.host.pointer(time, &event)
    }
    pub fn key(&mut self, time: f64, event: web_sys::KeyboardEvent) -> Result<bool, JsValue> {
        self.host.key(time, &event)
    }
    pub fn set_reduced_motion(&mut self, reduced: bool) {
        self.host.source.set_reduced_motion(reduced);
    }
    pub fn wheel(&mut self, time: f64, event: web_sys::WheelEvent) -> Result<bool, JsValue> {
        self.host.wheel(time, &event)
    }
    pub fn input_anchor(&self) -> Option<InputAnchor> {
        let (client, [x, y, _, bottom], secret) = self.host.source.input_anchor()?;
        Some(InputAnchor {
            client,
            x,
            y,
            height: (bottom - y).max(1.),
            secret,
        })
    }
    pub fn composition(
        &mut self,
        time: f64,
        client: u64,
        text: String,
        finished: bool,
    ) -> Result<(), JsValue> {
        use plico_renderer::ImeEvent;
        if self.host.source.input_anchor().map(|(id, _, _)| id) != Some(client) {
            return Ok(());
        }
        let event = match (finished, text.is_empty()) {
            (true, true) => ImeEvent::Disabled,
            (true, false) => ImeEvent::Commit(text),
            (false, _) => ImeEvent::Preedit { text, cursor: None },
        };
        self.host.dispatch(time, UiEvent::Ime(event))?;
        Ok(())
    }
    pub fn paste(&mut self, time: f64, text: String) -> Result<bool, JsValue> {
        self.host.paste(time, text)
    }
    pub fn cut_selection(&mut self, time: f64) -> Result<(), JsValue> {
        self.host.cut_selection(time)
    }
    pub fn text(&mut self, time: f64, text: String) -> Result<(), JsValue> {
        self.host.dispatch(time, UiEvent::TextInput(text))?;
        Ok(())
    }
    pub fn focus(&mut self, time: f64, focused: bool) -> Result<(), JsValue> {
        self.host.focus(time, focused)
    }
    pub fn debug_state(&self) -> String {
        serde_json::to_string(&self.host.source.view().0).unwrap()
    }
    pub fn semantics(&self) -> String {
        let snapshot = self.host.source.semantics();
        let nodes: Vec<_> = snapshot
            .nodes
            .iter()
            .map(|n| {
                serde_json::json!({
                    "id":n.id, "role":format!("{:?}",n.role), "label":n.label,
                    "bounds":n.bounds, "disabled":n.disabled, "value":n.value
                })
            })
            .collect();
        serde_json::to_string(&nodes).unwrap()
    }
}
