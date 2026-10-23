use crate::WebHost;
use js_sys::Function;
use plico_backend::View;
use plico_core::{FrameTime, Key, PathKey, Theme, Ui, ViewNode};
use plico_dev::{Frame, PROTOCOL_VERSION, Request};
use plico_renderer::FrameSource;
use serde_json::{Value, json};
use std::collections::HashMap;
use wasm_bindgen::prelude::*;

struct RemoteView {
    module: Function,
    frame: Frame,
    keys: HashMap<Vec<PathKey>, u64>,
    next_key: u64,
    pending: Vec<Vec<PathKey>>,
    pending_values: Vec<(Vec<PathKey>, f64)>,
    scroll_viewports: Vec<(Vec<PathKey>, [f32; 2])>,
    time: FrameTime,
    rendered_time: FrameTime,
    viewport: [f32; 2],
    rendered_scrolls: Vec<(Vec<PathKey>, [f32; 2])>,
    services: Value,
    dirty: bool,
    reloads: u64,
    calls: u64,
    input_bytes: u64,
    output_bytes: u64,
    last_error: Option<String>,
}
impl RemoteView {
    fn new(module: Function) -> Result<Self, JsValue> {
        let services = json!({"instance":1,"progress":0});
        let (frame, initial_input, initial_output) = call(
            &module,
            &Request {
                services: services.clone(),
                ..Default::default()
            },
        )?;
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
            dirty: false,
            reloads: 0,
            calls: 1,
            input_bytes: initial_input as u64,
            output_bytes: initial_output as u64,
            last_error: None,
        })
    }
    fn request(&self, clicks: Vec<Vec<PathKey>>, values: Vec<(Vec<PathKey>, f64)>) -> Request {
        Request {
            state: self.frame.state.clone(),
            services: self.services.clone(),
            viewport: Some(self.viewport),
            scroll_viewports: self.scroll_viewports.clone(),
            time: self.time,
            clicks,
            values,
            ..Default::default()
        }
    }
    fn invoke(&mut self, module: &Function, request: &Request) -> Result<Frame, JsValue> {
        let (frame, input, output) = call(module, request)?;
        self.calls += 1;
        self.input_bytes += input as u64;
        self.output_bytes += output as u64;
        Ok(frame)
    }
    fn activate(&mut self, module: Function) -> Result<(), JsValue> {
        // No host/UI state changes until the whole candidate frame validates.
        let request = self.request(vec![], vec![]);
        let frame = self.invoke(&module, &request)?;
        self.module = module;
        self.accept(frame);
        self.reloads += 1;
        Ok(())
    }
    fn accept(&mut self, frame: Frame) {
        self.frame = frame;
        self.rendered_time = self.time;
        self.rendered_scrolls = self.scroll_viewports.clone();
        self.dirty = false;
        self.last_error = None;
    }
    fn dispatch(
        &mut self,
        clicks: Vec<Vec<PathKey>>,
        values: Vec<(Vec<PathKey>, f64)>,
    ) -> Result<(), JsValue> {
        let request = self.request(clicks, values);
        let module = self.module.clone();
        let frame = self.invoke(&module, &request)?;
        self.accept(frame);
        Ok(())
    }
    fn replay(&mut self, ui: &mut Ui<'_>) {
        struct Replay<'a> {
            keys: &'a mut HashMap<Vec<PathKey>, u64>,
            next: &'a mut u64,
            clicks: &'a mut Vec<Vec<PathKey>>,
            values: &'a mut Vec<(Vec<PathKey>, f64)>,
            scrolls: &'a mut Vec<(Vec<PathKey>, [f32; 2])>,
        }
        impl Replay<'_> {
            fn nodes(&mut self, ui: &mut Ui<'_>, nodes: &[ViewNode], path: &mut Vec<PathKey>) {
                for n in nodes {
                    path.push(n.key.clone());
                    let key = *self.keys.entry(path.clone()).or_insert_with(|| {
                        *self.next += 1;
                        *self.next
                    });
                    if n.kind == plico_core::Kind::Shader {
                        if let Some(params) = &n.shader {
                            ui.shader_element(Key::Number(key), n.style, &n.text, params);
                        }
                    } else if n.kind == plico_core::Kind::Text {
                        ui.rich_text_element(Key::Number(key), n.style, &n.text, &n.spans);
                    } else if n.kind == plico_core::Kind::Slider {
                        if let Some(value) = n.numeric.and_then(|range| {
                            ui.slider_element(Key::Number(key), n.style, &n.text, range)
                        }) {
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
            keys: &mut self.keys,
            next: &mut self.next_key,
            clicks: &mut self.pending,
            values: &mut self.pending_values,
            scrolls: &mut self.scroll_viewports,
        }
        .nodes(ui, &self.frame.nodes, &mut Vec::new());
        // Virtualized children are unmounted. Do not retain a path-to-ID entry
        // for every row ever visited during a long session.
        fn active_paths(
            nodes: &[ViewNode],
            path: &mut Vec<PathKey>,
            active: &mut std::collections::HashSet<Vec<PathKey>>,
        ) {
            for node in nodes {
                path.push(node.key.clone());
                active.insert(path.clone());
                active_paths(&node.children, path, active);
                path.pop();
            }
        }
        let mut active = std::collections::HashSet::new();
        active_paths(&self.frame.nodes, &mut Vec::new(), &mut active);
        self.keys.retain(|path, _| active.contains(path));
        self.scroll_viewports
            .retain(|(path, _)| active.contains(path));
    }
}
impl View for RemoteView {
    fn set_frame_time(&mut self, time: FrameTime) {
        self.time = time;
    }
    fn theme(&self) -> Theme {
        self.frame.theme
    }
    fn on_unhandled_key(&mut self, key: &plico_renderer::KeyEvent) -> bool {
        if !key.modifiers.is_empty() {
            return false;
        }
        let mut request = self.request(vec![], vec![]);
        request.keys.push(key.key.clone());
        let module = self.module.clone();
        match self.invoke(&module, &request) {
            Ok(frame) if frame.key_handled => {
                self.accept(frame);
                true
            }
            Ok(_) => false,
            Err(e) => {
                self.last_error = Some(format!("{e:?}"));
                false
            }
        }
    }
    fn show(&mut self, ui: &mut Ui<'_>) {
        let resized = self.viewport != ui.viewport_size();
        self.viewport = ui.viewport_size();
        let clicks = std::mem::take(&mut self.pending);
        let values = std::mem::take(&mut self.pending_values);
        let input = !clicks.is_empty() || !values.is_empty();
        let ticking = self.frame.requests.contains(plico_core::Changes::ANIMATE)
            && self.time != self.rendered_time;
        if (self.dirty
            || resized
            || input
            || ticking
            || self.rendered_scrolls != self.scroll_viewports)
            && let Err(e) = self.dispatch(clicks, values)
        {
            self.last_error = Some(format!("{e:?}"));
            self.dirty = false;
            self.frame.requests.remove(plico_core::Changes::ANIMATE);
        }
        self.replay(ui);
        if self.frame.requests.contains(plico_core::Changes::ANIMATE) {
            ui.request_animation_frame();
        }
        if input || !self.pending.is_empty() || !self.pending_values.is_empty() {
            ui.request_rebuild();
        }
    }
}
fn call(module: &Function, request: &Request) -> Result<(Frame, usize, usize), JsValue> {
    let input = serde_json::to_string(request).map_err(err)?;
    let output = module
        .call1(&JsValue::NULL, &JsValue::from_str(&input))?
        .as_string()
        .ok_or_else(|| err("UI module must return JSON text"))?;
    let frame: Frame = serde_json::from_str(&output).map_err(err)?;
    if frame.version != PROTOCOL_VERSION {
        return Err(err("UI protocol version mismatch"));
    }
    if !frame.commands.is_empty() {
        return Err(err("split experiment has no command executor"));
    }
    plico_dev::validate_nodes(&frame.nodes).map_err(err)?;
    fn web_nodes(nodes: &[ViewNode]) -> bool {
        nodes.iter().all(|n| {
            !matches!(
                n.kind,
                plico_core::Kind::Shader | plico_core::Kind::TextInput
            ) && web_nodes(&n.children)
        })
    }
    if !web_nodes(&frame.nodes) {
        return Err(err("unsupported portable web node"));
    }
    Ok((frame, input.len(), output.len()))
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
    #[wasm_bindgen(constructor)]
    pub fn new(canvas: web_sys::HtmlCanvasElement, module: Function) -> Result<Self, JsValue> {
        Ok(Self {
            host: WebHost::new(canvas, RemoteView::new(module)?)?,
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
    pub fn pointer(&mut self, time: f64, event: web_sys::PointerEvent) -> Result<(), JsValue> {
        self.host.pointer(time, &event)
    }
    pub fn key(&mut self, time: f64, event: web_sys::KeyboardEvent) -> Result<bool, JsValue> {
        self.host.key(time, &event)
    }
    pub fn activate(&mut self, module: Function) -> Result<(), JsValue> {
        self.host.source.prepare_for_event(&mut self.host.text);
        self.host.source.view.activate(module)?;
        self.host.source.request_rebuild();
        Ok(())
    }
    pub fn advance_service(&mut self) {
        self.service_ticks += 1;
        self.host.source.view.services = json!({"instance":1,"progress":self.service_ticks});
        self.host.source.view.dirty = true;
        self.host.source.request_rebuild();
    }
    pub fn stats(&self) -> String {
        let view = &self.host.source.view;
        serde_json::to_string(&json!({
            "state":view.frame.state,"reset":view.frame.reset,"error":view.last_error,
            "reloads":view.reloads,"calls":view.calls,
            "input_bytes":view.input_bytes,"output_bytes":view.output_bytes,
            "font_loads":self.font_loads, "text_cache":self.host.text.cache_entries(),"service":view.services,
            "nodes":view.keys.len(),"svg_parses":self.host.source.adapter.stats.svg_parses,
            "created":self.host.source.adapter.stats.created
        }))
        .unwrap()
    }
    pub fn semantics(&self) -> String {
        let nodes = self
            .host
            .source
            .adapter
            .semantics(&self.host.source.engine, 0)
            .nodes;
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
