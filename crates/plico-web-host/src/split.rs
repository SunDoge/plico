use crate::WebHost;
use js_sys::Function;
use plico::{FrameTime, Key, PathKey, Theme, UiContext, ViewNode};
use plico_runtime::HostView;
use serde_json::json;
use std::collections::HashMap;
use wasm_bindgen::prelude::*;
use web_demo::protocol::{CAPACITY, Input as Request, Output as Frame, Reply, ServiceSnapshot};

struct ReplayKey {
    id: u64,
    visited: bool,
}

struct RemoteView {
    module: Function,
    frame: Frame,
    keys: HashMap<Vec<PathKey>, ReplayKey>,
    next_key: u64,
    pending: Vec<Vec<PathKey>>,
    pending_values: Vec<(Vec<PathKey>, f64)>,
    scroll_viewports: Vec<(Vec<PathKey>, [f32; 2])>,
    time: FrameTime,
    rendered_time: FrameTime,
    viewport: [f32; 2],
    rendered_scrolls: Vec<(Vec<PathKey>, [f32; 2])>,
    hovered: Option<Vec<PathKey>>,
    rendered_hovered: Option<Vec<PathKey>>,
    services: ServiceSnapshot,
    dirty: bool,
    reloads: u64,
    calls: u64,
    input_bytes: u64,
    output_bytes: u64,
    module_error: Option<String>,
}
impl RemoteView {
    fn new(module: Function) -> Result<Self, JsValue> {
        let services = ServiceSnapshot {
            instance: 1,
            progress: 0,
        };
        let (frame, initial_input, initial_output) = call(
            &module,
            &Request {
                services: services.clone(),
                ..Default::default()
            },
        )?;
        let frame = frame.resolve(None, &mut Vec::new()).map_err(err)?;
        Ok(Self {
            module,
            frame,
            services,
            keys: HashMap::new(),
            next_key: 0,
            pending: vec![],
            pending_values: vec![],
            scroll_viewports: vec![],
            time: FrameTime::default(),
            rendered_time: FrameTime::default(),
            viewport: [720., 820.],
            rendered_scrolls: vec![],
            hovered: None,
            rendered_hovered: None,
            dirty: false,
            reloads: 0,
            calls: 1,
            input_bytes: initial_input as u64,
            output_bytes: initial_output as u64,
            module_error: None,
        })
    }
    fn request(&self, clicks: Vec<Vec<PathKey>>, values: Vec<(Vec<PathKey>, f64)>) -> Request {
        Request {
            state: self.frame.state.clone(),
            services: self.services.clone(),
            viewport: Some(self.viewport),
            hovered: self.hovered.clone(),
            scroll_viewports: self.scroll_viewports.clone(),
            time: self.time,
            clicks,
            values,
            ..Default::default()
        }
    }
    fn invoke(&mut self, module: &Function, request: &Request) -> Result<Reply, JsValue> {
        let (frame, input, output) = call(module, request)?;
        self.calls += 1;
        self.input_bytes += input as u64;
        self.output_bytes += output as u64;
        Ok(frame)
    }
    fn activate(&mut self, module: Function) -> Result<(), JsValue> {
        // No host/UI state changes until the whole candidate frame validates.
        let request = self.request(vec![], vec![]);
        let frame = self
            .invoke(&module, &request)?
            .resolve(None, &mut Vec::new())
            .map_err(err)?;
        self.module = module;
        self.accept(frame);
        self.reloads += 1;
        Ok(())
    }
    fn accept(&mut self, frame: Frame) {
        self.frame = frame;
        self.rendered_time = self.time;
        self.rendered_scrolls = self.scroll_viewports.clone();
        self.rendered_hovered = self.hovered.clone();
        self.dirty = false;
        self.module_error = None;
    }
    fn dispatch(
        &mut self,
        clicks: Vec<Vec<PathKey>>,
        values: Vec<(Vec<PathKey>, f64)>,
    ) -> Result<(), JsValue> {
        let mut request = self.request(clicks, values);
        self.update(&mut request)?;
        Ok(())
    }
    fn update(&mut self, request: &mut Request) -> Result<bool, JsValue> {
        request.base = Some(self.frame.revision);
        let module = self.module.clone();
        let reply = self.invoke(&module, request)?;
        if !request.key_events.is_empty() && !reply.key_handled {
            return Ok(false);
        }
        let frame = reply
            .resolve(request.base, &mut self.frame.nodes)
            .map_err(err)?;
        self.accept(frame);
        Ok(true)
    }
    fn replay(&mut self, ui: &mut UiContext<'_>) {
        let mut hovered = None;
        struct Replay<'a> {
            hovered: &'a mut Option<Vec<PathKey>>,
            keys: &'a mut HashMap<Vec<PathKey>, ReplayKey>,
            next: &'a mut u64,
            clicks: &'a mut Vec<Vec<PathKey>>,
            values: &'a mut Vec<(Vec<PathKey>, f64)>,
            scrolls: &'a mut Vec<(Vec<PathKey>, [f32; 2])>,
        }
        impl Replay<'_> {
            fn nodes(
                &mut self,
                ui: &mut UiContext<'_>,
                nodes: &[ViewNode],
                path: &mut Vec<PathKey>,
            ) {
                for n in nodes {
                    path.push(n.key.clone());
                    let key = if let Some(entry) = self.keys.get_mut(path.as_slice()) {
                        entry.visited = true;
                        entry.id
                    } else {
                        *self.next += 1;
                        let id = *self.next;
                        self.keys
                            .insert(path.clone(), ReplayKey { id, visited: true });
                        id
                    };
                    if n.kind == plico::Kind::Shader {
                        if let Some(params) = &n.shader {
                            ui.shader_element(Key::Number(key), n.style, &n.text, params);
                        }
                    } else if n.kind == plico::Kind::Text {
                        ui.rich_text_element(Key::Number(key), n.style, &n.text, &n.spans);
                    } else if n.kind == plico::Kind::Slider {
                        if let Some(mut range) = n.numeric
                            && let Some(value) =
                                ui.slider_element(Key::Number(key), n.style, &n.text, &mut range)
                        {
                            self.values.push((path.clone(), value));
                        }
                    } else {
                        let (_, clicked) = ui.element_with_grid(
                            Key::Number(key),
                            n.kind,
                            n.style,
                            &n.text,
                            n.grid.as_ref(),
                            |ui| {
                                if ui.hovered() {
                                    *self.hovered = Some(path.clone());
                                }
                                if n.style.scroll_y
                                    && let Some(viewport) = ui.scroll_viewport()
                                {
                                    if let Some((_, old)) =
                                        self.scrolls.iter_mut().find(|(p, _)| p == path)
                                    {
                                        if *old != viewport {
                                            *old = viewport;
                                            ui.request_rebuild();
                                        }
                                    } else {
                                        self.scrolls.push((path.clone(), viewport));
                                        ui.request_rebuild();
                                    }
                                }
                                self.nodes(ui, &n.children, path);
                            },
                        );
                        if clicked {
                            self.clicks.push(path.clone());
                        }
                    }
                    path.pop();
                }
            }
        }
        Replay {
            hovered: &mut hovered,
            keys: &mut self.keys,
            next: &mut self.next_key,
            clicks: &mut self.pending,
            values: &mut self.pending_values,
            scrolls: &mut self.scroll_viewports,
        }
        .nodes(ui, &self.frame.nodes, &mut Vec::new());
        self.hovered = hovered;
        // Virtualized children are unmounted. Do not retain a path-to-ID entry
        // for every row ever visited during a long session.
        self.keys.retain(|_, key| std::mem::take(&mut key.visited));
        self.scroll_viewports
            .retain(|(path, _)| self.keys.contains_key(path));
    }
}
impl HostView for RemoteView {
    fn set_frame_time(&mut self, time: FrameTime) {
        self.time = time;
    }
    fn theme(&self) -> Theme {
        self.frame.theme
    }
    fn on_unhandled_key(&mut self, key: &plico_renderer::KeyEvent) -> bool {
        let mut request = self.request(vec![], vec![]);
        request.key_events.push(key.clone());
        match self.update(&mut request) {
            Ok(handled) => handled,
            Err(e) => {
                self.module_error = Some(format!("{e:?}"));
                false
            }
        }
    }
    fn show(&mut self, ui: &mut UiContext<'_>) {
        let resized = self.viewport != ui.viewport_size();
        self.viewport = ui.viewport_size();
        let clicks = std::mem::take(&mut self.pending);
        let values = std::mem::take(&mut self.pending_values);
        let input = !clicks.is_empty() || !values.is_empty();
        let ticking = (self.frame.requests & plico::Changes::ANIMATE.bits() != 0)
            && self.time != self.rendered_time;
        if (self.dirty
            || resized
            || input
            || ticking
            || self.rendered_scrolls != self.scroll_viewports
            || self.rendered_hovered != self.hovered)
            && let Err(e) = self.dispatch(clicks, values)
        {
            self.module_error = Some(format!("{e:?}"));
            self.dirty = false;
            self.frame.requests &= !plico::Changes::ANIMATE.bits();
        }
        let previous_hover = self.hovered.clone();
        self.replay(ui);
        if previous_hover != self.hovered {
            ui.request_rebuild();
        }
        if self.frame.requests & plico::Changes::ANIMATE.bits() != 0 {
            ui.request_animation_frame();
        }
        if input || !self.pending.is_empty() || !self.pending_values.is_empty() {
            ui.request_rebuild();
        }
    }
}
fn call(module: &Function, request: &Request) -> Result<(Reply, usize, usize), JsValue> {
    let input = rkyv::to_bytes::<rkyv::rancor::Error>(request).map_err(err)?;
    if input.len() > CAPACITY {
        return Err(err("UI request exceeds buffer capacity"));
    }
    let value = module.call1(&JsValue::NULL, &js_sys::Uint8Array::from(input.as_slice()))?;
    let output = value
        .dyn_into::<js_sys::Uint8Array>()
        .map_err(|_| err("UI module must return bytes"))?;
    if output.length() as usize > CAPACITY {
        return Err(err("UI reply exceeds buffer capacity"));
    }
    let bytes = output.to_vec();
    let frame = rkyv::from_bytes::<Reply, rkyv::rancor::Error>(&bytes).map_err(err)?;
    Ok((frame, input.len(), bytes.len()))
}
fn err(message: impl std::fmt::Display) -> JsValue {
    JsValue::from_str(&message.to_string())
}

/// Experimental independently instantiated UI module + resident browser host.
#[wasm_bindgen]
pub struct SplitHost {
    host: WebHost<RemoteView>,
    font_loads: u64,
    service_ticks: u64,
}
#[wasm_bindgen]
impl SplitHost {
    pub fn abi_version() -> u32 {
        web_demo::protocol::ABI
    }
    pub fn buffer_capacity() -> usize {
        CAPACITY
    }
    pub async fn create(
        canvas: web_sys::HtmlCanvasElement,
        module: Function,
    ) -> Result<Self, JsValue> {
        Ok(Self {
            host: WebHost::new_wgpu(
                canvas,
                RemoteView::new(module)?,
                plico_wgpu::web::BrowserBackend::Auto,
            )
            .await?,
            font_loads: 0,
            service_ticks: 0,
        })
    }
    pub fn load_font(&mut self, bytes: Vec<u8>) {
        self.host.load_font(bytes);
        self.font_loads += 1;
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
    pub fn next_frame_delay_ms(&self) -> f64 {
        self.host.next_frame_delay_ms()
    }
    pub fn pointer(&mut self, time: f64, event: web_sys::PointerEvent) -> Result<(), JsValue> {
        self.host.pointer(time, &event)
    }
    pub fn selected_text(&self) -> Option<String> {
        self.host.selected_text()
    }
    pub fn paste(&mut self, time: f64, text: String) -> Result<bool, JsValue> {
        self.host.paste(time, text)
    }
    pub fn cut_selection(&mut self, time: f64) -> Result<(), JsValue> {
        self.host.cut_selection(time)
    }
    pub fn focus(&mut self, time: f64, focused: bool) -> Result<(), JsValue> {
        self.host.focus(time, focused)
    }
    pub fn set_reduced_motion(&mut self, reduced: bool) {
        self.host.source.set_reduced_motion(reduced);
    }
    pub fn wheel(&mut self, time: f64, event: web_sys::WheelEvent) -> Result<bool, JsValue> {
        self.host.wheel(time, &event)
    }
    pub fn key(&mut self, time: f64, event: web_sys::KeyboardEvent) -> Result<bool, JsValue> {
        self.host.key(time, &event)
    }
    pub fn activate(&mut self, module: Function) -> Result<(), JsValue> {
        self.host
            .source
            .try_prepare_for_event(&mut self.host.text)
            .map_err(err)?;
        self.host.source.update(|view| view.activate(module))?;
        Ok(())
    }
    pub fn advance_service(&mut self) {
        self.service_ticks += 1;
        self.host.source.update(|view| {
            view.services.progress = self.service_ticks;
            view.dirty = true;
        });
    }
    pub fn stats(&self) -> String {
        let view = self.host.source.view();
        let diagnostics = self.host.source.frame_diagnostics();
        serde_json::to_string(&json!({
            "state":view.frame.state.decode::<serde_json::Value>().ok(),"state_bytes":view.frame.state,"reset":view.frame.reset,"error":view.module_error,
            "reloads":view.reloads,"calls":view.calls,
            "input_bytes":view.input_bytes,"output_bytes":view.output_bytes,
            "font_loads":self.font_loads, "text_cache":self.host.text.cache_entries(),"service":view.services,
            "nodes":view.keys.len(),"svg_parses":diagnostics.backend.svg_parses,
            "created":diagnostics.backend.created
        }))
        .unwrap()
    }
    pub fn semantics(&self) -> String {
        let nodes = self.host.source.semantics().nodes;
        serde_json::to_string(
            &nodes
                .iter()
                .map(|n| {
                    json!({
                        "label":n.label,"bounds":n.bounds,"role":format!("{:?}",n.role)
                    })
                })
                .collect::<Vec<_>>(),
        )
        .unwrap()
    }
}
