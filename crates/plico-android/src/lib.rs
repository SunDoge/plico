//! Android Activity host for a Plico frame source.
#![cfg(target_os = "android")]

use plico_renderer::{
    FrameSource, Modifiers, Point, PointerButton, PointerEvent, PointerId, PointerPhase,
    PointerProperties, PointerType, Scene, TextContext, UiEvent,
};
use plico_wgpu::WgpuWindow;
use std::sync::Arc;
pub use winit::platform::android::activity::AndroidApp;
use winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, EventLoop},
    platform::android::EventLoopBuilderExtAndroid,
    window::{
        ImeCapabilities, ImeEnableRequest, ImeRequest, ImeRequestData, Window, WindowAttributes,
        WindowId,
    },
};

struct Host<S> {
    editor: ime::Editor,
    legacy_ime: bool,
    modifiers: Modifiers,
    app: AndroidApp,
    viewport: Option<[u32; 4]>,
    source: S,
    touches: std::collections::HashMap<PointerId, PointerEvent>,
    text: TextContext,
    surface: Option<(WgpuWindow, Arc<dyn Window>)>,
}

mod clipboard;
mod ime;
mod links;

fn content_viewport(window: &dyn Window) -> [u32; 4] {
    use winit::platform::android::WindowExtAndroid;
    let size = window.surface_size();
    let rect = window.content_rect();
    let left = (rect.left.max(0) as u32).min(size.width);
    let top = (rect.top.max(0) as u32).min(size.height);
    let width = (rect.right.max(0) as u32)
        .min(size.width)
        .saturating_sub(left);
    let height = (rect.bottom.max(0) as u32)
        .min(size.height)
        .saturating_sub(top);
    if width > 0 && height > 0 {
        [left, top, width, height]
    } else {
        [0, 0, size.width, size.height]
    }
}

impl<S: FrameSource> ApplicationHandler for Host<S> {
    fn about_to_wait(&mut self, events: &dyn ActiveEventLoop) {
        use winit::event_loop::ControlFlow;
        let mut flow = ControlFlow::Wait;
        let mut completed = false;
        while let Some(action) = self.source.take_host_action() {
            use plico_renderer::HostAction;
            match action {
                HostAction::OpenUrl(raw) => {
                    if let Some(url) = plico_renderer::platform::allowed_external_url(&raw)
                        && links::open(&self.app, url.as_str()).is_err()
                    {
                        eprintln!("Android could not open the browser");
                    }
                }
                HostAction::SetClipboard(text) => {
                    let _ = clipboard::text(&self.app, Some(&text));
                }
                HostAction::Window(plico_renderer::WindowCommand::Close) => events.exit(),
                _ => {}
            }
        }
        while let Some(request) = self.source.take_effect() {
            use plico_renderer::platform::{
                EffectCompletion, EffectErrorCode, EffectPayload, EffectResult,
            };
            let op = request.payload.op();
            let result = match request.payload {
                EffectPayload::ClipboardRead => clipboard::text(&self.app, None)
                    .map(EffectResult::ClipboardText)
                    .unwrap_or_else(clipboard::failure),
                EffectPayload::ClipboardWrite { text } => clipboard::text(&self.app, Some(&text))
                    .map(|_| EffectResult::Unit)
                    .unwrap_or_else(clipboard::failure),
                EffectPayload::ApplicationExit => {
                    events.exit();
                    EffectResult::Unit
                }
                EffectPayload::AppDirsResolve(dirs) => EffectResult::AppDirectories(dirs),
                EffectPayload::Invalid { message, .. } => EffectResult::Error {
                    code: EffectErrorCode::InvalidRequest,
                    message,
                },
                _ => EffectResult::Error {
                    code: EffectErrorCode::Unsupported,
                    message: format!("Android host does not implement {op:?}"),
                },
            };
            self.source.complete_effect(EffectCompletion {
                id: request.id,
                op,
                result,
            });
            completed = true;
        }
        if let Some((_, window)) = &self.surface
            && (completed || Some(content_viewport(window.as_ref())) != self.viewport)
        {
            window.request_redraw();
        }
        if let Some((_, window)) = &self.surface
            && let Some(deadline) = self.source.animation_deadline()
        {
            if deadline <= std::time::Instant::now() {
                window.request_redraw();
            } else {
                flow = ControlFlow::WaitUntil(deadline);
            }
        }
        events.set_control_flow(flow);
    }

    fn resumed(&mut self, _: &dyn ActiveEventLoop) {
        self.source.handle_event(UiEvent::AppLifecycle(
            plico_renderer::AppLifecycleEvent::Resumed,
        ));
    }

    fn suspended(&mut self, _: &dyn ActiveEventLoop) {
        self.source.handle_event(UiEvent::AppLifecycle(
            plico_renderer::AppLifecycleEvent::Suspended,
        ));
    }

    fn memory_warning(&mut self, _: &dyn ActiveEventLoop) {
        self.text.clear_caches();
        if let Some((renderer, _)) = &mut self.surface {
            renderer.trim_gpu_cache();
        }
        self.source.handle_event(UiEvent::AppLifecycle(
            plico_renderer::AppLifecycleEvent::MemoryWarning,
        ));
    }

    fn proxy_wake_up(&mut self, _: &dyn ActiveEventLoop) {
        let mut edited = false;
        for event in self.editor.drain() {
            self.source.handle_event(UiEvent::Ime(event));
            edited = true;
        }
        if (self.source.poll_async() || edited)
            && let Some((_, window)) = &self.surface
        {
            window.request_redraw();
        }
    }
    fn can_create_surfaces(&mut self, events: &dyn ActiveEventLoop) {
        if self.surface.is_some() {
            return;
        }
        let window: Arc<dyn Window> = events
            .create_window(WindowAttributes::default())
            .expect("create Android window")
            .into();
        let renderer = WgpuWindow::new(window.clone(), false).expect("create Android renderer");
        self.source.set_gpu_context(Some(renderer.gpu_context()));
        window.request_redraw();
        self.surface = Some((renderer, window));
    }

    fn destroy_surfaces(&mut self, _: &dyn ActiveEventLoop) {
        self.source.set_gpu_context(None);
        for (_, mut contact) in self.touches.drain() {
            contact.phase = PointerPhase::Cancel;
            contact.buttons = 0;
            contact.button = None;
            self.source.handle_event(UiEvent::Pointer(contact));
        }
        self.surface = None;
    }

    fn window_event(&mut self, events: &dyn ActiveEventLoop, _: WindowId, event: WindowEvent) {
        let Some((renderer, window)) = &mut self.surface else {
            return;
        };
        match event {
            WindowEvent::CloseRequested => events.exit(),
            WindowEvent::Focused(false) => self.modifiers = Modifiers::empty(),
            WindowEvent::KeyboardInput { event, .. } => {
                use winit::keyboard::{Key, KeyCode, NamedKey, PhysicalKey};
                let pressed = event.state == winit::event::ElementState::Pressed;
                let modifier = match &event.logical_key {
                    Key::Named(NamedKey::Control) => Some(Modifiers::CONTROL),
                    Key::Named(NamedKey::Shift) => Some(Modifiers::SHIFT),
                    Key::Named(NamedKey::Alt) => Some(Modifiers::ALT),
                    Key::Named(NamedKey::Meta) => Some(Modifiers::META),
                    _ => None,
                };
                if let Some(flag) = modifier {
                    self.modifiers.set(flag, pressed);
                }
                let shortcut = self.modifiers.primary_shortcut();
                let logical = match (&event.logical_key, event.physical_key) {
                    (_, PhysicalKey::Code(KeyCode::KeyA)) if shortcut => "a".into(),
                    (_, PhysicalKey::Code(KeyCode::KeyC)) if shortcut => "c".into(),
                    (_, PhysicalKey::Code(KeyCode::KeyV)) if shortcut => "v".into(),
                    (_, PhysicalKey::Code(KeyCode::KeyX)) if shortcut => "x".into(),
                    (Key::Named(key), _) => format!("{key:?}"),
                    (Key::Character(ch), _) => ch.to_string(),
                    _ => return,
                };
                let phase = if pressed {
                    plico_renderer::KeyPhase::Down
                } else {
                    plico_renderer::KeyPhase::Up
                };
                let mut key = plico_renderer::KeyEvent::logical(phase, logical, self.modifiers);
                key.repeat = event.repeat;
                let response = self.source.handle_event(UiEvent::Key(key));
                match response.clipboard {
                    Some(plico_renderer::ClipboardRequest::Write(value)) => {
                        let _ = clipboard::text(&self.app, Some(&value));
                    }
                    Some(plico_renderer::ClipboardRequest::Read) => {
                        if let Ok(Some(value)) = clipboard::text(&self.app, None) {
                            self.source.handle_event(UiEvent::Paste(value));
                        }
                    }
                    None => {}
                }
                if pressed
                    && !shortcut
                    && !response.consume_key_text
                    && let Some(text) = event
                        .text
                        .filter(|text| !text.is_empty() && !text.chars().any(char::is_control))
                {
                    self.source
                        .handle_event(UiEvent::TextInput(text.to_string()));
                }
                window.request_redraw();
            }
            WindowEvent::Ime(event) => {
                use plico_renderer::ImeEvent;
                use winit::event::Ime;
                let event = match event {
                    Ime::Commit(text) => ImeEvent::Commit(text),
                    Ime::Preedit(text, cursor) => ImeEvent::Preedit { text, cursor },
                    Ime::Enabled => ImeEvent::Enabled,
                    Ime::Disabled => ImeEvent::Disabled,
                    Ime::DeleteSurrounding {
                        before_bytes,
                        after_bytes,
                    } => ImeEvent::DeleteSurrounding {
                        before_bytes,
                        after_bytes,
                    },
                    _ => return,
                };
                self.source.handle_event(UiEvent::Ime(event));
                window.request_redraw();
            }
            WindowEvent::PointerButton {
                button: winit::event::ButtonSource::Touch { finger_id, force },
                position,
                primary,
                ..
            }
            | WindowEvent::PointerMoved {
                source: winit::event::PointerSource::Touch { finger_id, force },
                position,
                primary,
                ..
            } => {
                let phase = match &event {
                    WindowEvent::PointerButton {
                        state: winit::event::ElementState::Pressed,
                        ..
                    } => PointerPhase::Down,
                    WindowEvent::PointerButton { .. } => PointerPhase::Up,
                    _ => PointerPhase::Move,
                };
                let id = PointerId {
                    lo: finger_id.into_raw() as u32,
                    hi: 1,
                };
                let [left, top, _, _] = content_viewport(window.as_ref());
                let scale = window.scale_factor();
                let contact = PointerEvent {
                    phase,
                    position: Point {
                        x: (position.x - f64::from(left)) / scale,
                        y: (position.y - f64::from(top)) / scale,
                    },
                    button: (phase != PointerPhase::Move).then_some(PointerButton::Primary),
                    buttons: u32::from(phase != PointerPhase::Up),
                    modifiers: Modifiers::empty(),
                    properties: PointerProperties {
                        id,
                        primary,
                        pointer_type: PointerType::Touch,
                        pressure: force.map(|force| force.normalized(None)),
                        ..Default::default()
                    },
                };
                if phase == PointerPhase::Up {
                    self.touches.remove(&id);
                } else {
                    self.touches.insert(id, contact);
                }
                self.source.handle_event(UiEvent::Pointer(contact));
                window.request_redraw();
            }
            WindowEvent::PointerLeft {
                kind: winit::event::PointerKind::Touch(finger),
                ..
            } => {
                // Android sends Up then Left on release, and only Left on cancellation.
                if let Some(mut contact) = self.touches.remove(&PointerId {
                    lo: finger.into_raw() as u32,
                    hi: 1,
                }) {
                    contact.phase = PointerPhase::Cancel;
                    contact.buttons = 0;
                    contact.button = None;
                    self.source.handle_event(UiEvent::Pointer(contact));
                    window.request_redraw();
                }
            }
            WindowEvent::SurfaceResized(size) => {
                renderer.resize(size.width, size.height);
                window.request_redraw();
            }
            WindowEvent::RedrawRequested => {
                let viewport @ [left, top, width, height] = content_viewport(window.as_ref());
                self.viewport = Some(viewport);
                let size = winit::dpi::PhysicalSize::new(width, height);
                let scale = window.scale_factor();
                let logical = size.to_logical::<u32>(scale);
                let mut scene = Scene::new();
                self.source
                    .build_and_paint(
                        &mut scene,
                        &mut self.text,
                        logical.width,
                        logical.height,
                        scale,
                    )
                    .expect("build Android frame");
                if left != 0 || top != 0 {
                    let application = std::mem::replace(&mut scene, Scene::new());
                    scene.commands.push(plico_renderer::PaintCommand::Fragment {
                        scene: application,
                        transform: kurbo::Affine::translate((f64::from(left), f64::from(top))),
                    });
                }
                renderer
                    .present(&scene, self.source.base_color())
                    .expect("present Android frame");
                let state = self.source.ime_state();
                let wants_keyboard = state.is_some();
                if !self.legacy_ime && self.editor.update(&self.app, state).is_err() {
                    self.legacy_ime = true;
                }
                if self.legacy_ime && wants_keyboard != window.ime_capabilities().is_some() {
                    let request = if wants_keyboard {
                        ImeRequest::Enable(
                            ImeEnableRequest::new(
                                ImeCapabilities::new(),
                                ImeRequestData::default(),
                            )
                            .expect("empty IME capabilities"),
                        )
                    } else {
                        ImeRequest::Disable
                    };
                    let _ = window.request_ime_update(request);
                }
                if self.source.has_anim() {
                    window.request_redraw();
                }
            }
            _ => {}
        }
    }
}

pub fn run(
    app: AndroidApp,
    mut source: impl FrameSource + 'static,
) -> Result<(), winit::error::EventLoopError> {
    let events = EventLoop::builder().with_android_app(app.clone()).build()?;
    let proxy = events.create_proxy();
    let wake: Arc<dyn Fn() + Send + Sync> = Arc::new(move || proxy.wake_up());
    source.set_wake_callback(wake.clone());
    events.run_app(Host {
        editor: ime::Editor::new(wake),
        legacy_ime: false,
        modifiers: Modifiers::empty(),
        app,
        source,
        viewport: None,
        touches: Default::default(),
        text: TextContext::new(),
        surface: None,
    })
}
