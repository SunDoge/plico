//! Window mode: a winit event loop driving a [`Shell`] from a [`FrameSource`].

#![warn(missing_docs)]

use futures_util::stream::{FuturesUnordered, Stream};
use slotmap::{Key as SlotMapKey, KeyData, SlotMap};
use snafu::ResultExt;
use std::cell::RefCell;
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, Wake, Waker};
use std::time::Instant;
use winit::application::ApplicationHandler;
use winit::cursor::{Cursor, CursorIcon};
use winit::data_transfer::TypeHint;
use winit::dpi::{LogicalSize, PhysicalPosition, PhysicalSize};
use winit::event::{ButtonSource, ElementState, MouseScrollDelta, TouchPhase, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
use winit::window::{
    ImeCapabilities, ImeEnableRequest, ImeRequest, ImeRequestData, ImeSurroundingText,
    ResizeDirection, UserAttentionType, WindowId,
};

use crate::WindowResourceKey;
use crate::performance_hud::{HudMetrics, HudSource, PerformanceHud};
use crate::shell::Shell;
use plico_renderer::platform::window_lifecycle::{
    WindowCapabilities, WindowEffect, WindowIntent, WindowLifecycle, WindowPresence,
};
use plico_renderer::source::{
    AppLifecycleEvent, ClipboardRequest, EventResponse, FileDropEvent, FileDropPhase, FrameSource,
    FrameStats, GestureEvent, ImeEvent, KeyEvent, KeyPhase, Modifiers, Point, PointerButton,
    PointerEvent, PointerId, PointerPhase, PointerProperties, PointerType, SemanticAction,
    SemanticRole, UiEvent, WakeCallback, WheelDeltaMode, WheelEvent, WindowCommand, WindowMetrics,
    WindowOptions,
};
use plico_renderer::style::CursorStyle;

#[cfg(target_os = "macos")]
mod appkit_menu;
mod effects;
mod event;
mod event_translate;
mod extension_context;
mod extensions;
mod ime;
mod input;
mod lifecycle;
mod multi_window;
#[cfg(any(target_os = "windows", target_os = "linux"))]
mod popup_window;
mod redraw;
mod window_event;

pub use extensions::ShellExtension;

use event_translate::{
    CUSTOM_DECORATION_RESIZE_BORDER, FrameWake, allowed_external_url, button_mask,
    button_source_properties, committed_key_text, display_wait_wake, gesture_phase,
    ime_client_changed, ime_cursor_rect, ime_hint_and_purpose, key_location, logical_key_name,
    modifiers as translate_modifiers, ms, namespaced_pointer_id, physical_key_name, pointer_button,
    pointer_kind_properties, pointer_navigation_key, pointer_source_properties, presentation_work,
    resize_direction_at, scheduled_frame_wake, unconsumed_key_text, update_present_retry,
    window_capabilities,
};

type ModalEffectFuture = Pin<Box<dyn Future<Output = crate::EffectCompletion>>>;

slotmap::new_key_type! {
    struct WindowSlotKey;
}

/// Convert a `file://` URI (as delivered by winit's uri-list data transfer)
/// into a native path, percent-decoding escapes. Non-file URIs are dropped.
fn file_uri_path(uri: &str) -> Option<std::path::PathBuf> {
    url::Url::parse(uri).ok()?.to_file_path().ok()
}

struct IncomingFileDrag {
    serial: winit::event_loop::AsyncRequestSerial,
    paths: Option<Vec<std::path::PathBuf>>,
    position: Option<Point>,
    dropped: bool,
}
impl IncomingFileDrag {
    fn drop_event(&mut self) -> Option<FileDropEvent> {
        if !self.dropped {
            return None;
        }
        Some(FileDropEvent {
            phase: FileDropPhase::Dropped,
            paths: self.paths.take()?,
            position: self.position,
        })
    }
}

struct PendingWindowEffect {
    id: crate::EffectId,
    op: crate::EffectOp,
    options: WindowOptions,
}

struct CallbackWaker(WakeCallback);

impl Wake for CallbackWaker {
    fn wake(self: Arc<Self>) {
        (self.0)();
    }

    fn wake_by_ref(self: &Arc<Self>) {
        (self.0)();
    }
}

/// The winit application. Owns a [`FrameSource`] (the frame producer) and a
/// lazily-created [`Shell`] (window + GPU surface + renderer). Created in
/// `can_create_surfaces` once the event loop is ready.
pub struct App {
    window_key: WindowResourceKey,
    source: Box<dyn FrameSource>,
    state: Option<Shell>,
    window_options: WindowOptions,
    window_metrics: WindowMetrics,
    modifiers: Modifiers,
    pointer_buttons: u32,
    pointer_position: Point,
    pointer_states: HashMap<PointerId, (Point, u32, PointerProperties)>,
    ime_requested: bool,
    ime_enabled: bool,
    /// Keep dropped transfers until their asynchronous URI data arrives.
    file_drags: HashMap<winit::data_transfer::DataTransferId, IncomingFileDrag>,
    ime_composing: bool,
    ime_state: Option<crate::ImeState>,
    startup_error: Arc<Mutex<Option<crate::Error>>>,
    /// EMA per-frame stage timings, reported to the source for a perf overlay.
    frame_stats: FrameStats,
    /// A configure can make the first acquire transiently fail. Static apps do
    /// not otherwise produce another frame, so allow exactly one recovery
    /// redraw without spinning forever while a window is occluded.
    present_retry_pending: bool,
    clipboard: Option<arboard::Clipboard>,
    pending_windows: Vec<PendingWindowEffect>,
    pending_window_commands: Vec<(crate::WindowResourceKey, WindowCommand)>,
    pending_extension_effects: Vec<crate::EffectRequest>,
    pending_modal_effects: FuturesUnordered<ModalEffectFuture>,
    effect_completion_tx: Sender<crate::EffectCompletion>,
    effect_completion_rx: Receiver<crate::EffectCompletion>,
    wake_callback: Option<WakeCallback>,
    close_requested: bool,
    application_exit_requested: bool,
    application_relaunch_requested: bool,
    lifecycle: WindowLifecycle,
    force_semantics: bool,
    scheduled_frame_deadline: Option<Instant>,
    skipped_present_deadline: Option<Instant>,
    #[cfg(target_os = "linux")]
    wayland_clock: crate::frame_pacing::WaylandFrameClock,
    multi_click_settings: Option<plico_renderer::source::MultiClickSettings>,
    hud_metrics: Option<Rc<RefCell<HudMetrics>>>,
    inline_hud: Option<PerformanceHud>,
    #[cfg(feature = "recording")]
    recording: crate::window_recording::WindowRecording,
}

impl ApplicationHandler for App {
    fn resumed(&mut self, _event_loop: &dyn ActiveEventLoop) {
        self.skipped_present_deadline = None;
        #[cfg(target_os = "linux")]
        self.wayland_clock.reset();
        if let Some(shell) = self.state.as_mut() {
            shell.invalidate_presentation();
        }
        self.dispatch_event(UiEvent::AppLifecycle(AppLifecycleEvent::Resumed));
    }

    fn suspended(&mut self, _event_loop: &dyn ActiveEventLoop) {
        if let Some(shell) = self.state.as_mut() {
            shell.trim_gpu_cache();
        }
        self.dispatch_event(UiEvent::AppLifecycle(AppLifecycleEvent::Suspended));
    }

    fn memory_warning(&mut self, _event_loop: &dyn ActiveEventLoop) {
        if let Some(shell) = self.state.as_mut() {
            shell.tcx.clear_caches();
            shell.trim_gpu_cache();
        }
        self.dispatch_event(UiEvent::AppLifecycle(AppLifecycleEvent::MemoryWarning));
    }

    fn can_create_surfaces(&mut self, event_loop: &dyn ActiveEventLoop) {
        if self.state.is_some() {
            return;
        }
        // Initial size: prefer the source's notion of viewport if it exposes one;
        // otherwise default. For now we read none from the source, so use a sane
        // default that the first resize will correct.
        if let Err(error) = self.create_surface(event_loop) {
            *self
                .startup_error
                .lock()
                .expect("startup error mutex poisoned") = Some(error);
            event_loop.exit();
        }
    }

    fn destroy_surfaces(&mut self, _event_loop: &dyn ActiveEventLoop) {
        self.source.set_gpu_context(None);
        self.state = None;
    }

    fn proxy_wake_up(&mut self, _event_loop: &dyn ActiveEventLoop) {
        let _ = self.drain_proxy_wake_up();
    }

    fn window_event(
        &mut self,
        event_loop: &dyn ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        if self.handle_window_event(event_loop, window_id, event) {
            self.redraw();
        }
    }

    fn about_to_wait(&mut self, event_loop: &dyn ActiveEventLoop) {
        if std::mem::take(&mut self.close_requested) && !self.source.close_requested() {
            self.state = None;
            event_loop.exit();
            return;
        }
        if self.application_exit_requested || self.application_relaunch_requested {
            event_loop.exit();
            return;
        }
        // A reactive source keeps the redraw loop spinning at vsync while it has
        // pending rAF work; a static source idles (ControlFlow::Wait) until a
        // resize/close.
        match self.service_frame_wake(Instant::now()) {
            FrameWake::Deadline(deadline) => {
                event_loop.set_control_flow(ControlFlow::WaitUntil(deadline));
            }
            _ => event_loop.set_control_flow(ControlFlow::Wait),
        }
    }
}

/// One winit event loop routing events to independent native windows.
/// Each window owns its own frame source, input state, surface and renderer.
struct MultiWindowApp {
    #[cfg(any(target_os = "windows", target_os = "linux"))]
    popup: Option<popup_window::Popup>,
    pending: Vec<App>,
    windows: HashMap<WindowId, App>,
    /// Windows intentionally closed to the tray on platforms such as Wayland,
    /// where an existing native window cannot be made invisible. Their frame
    /// sources stay alive and a fresh native surface is created when shown.
    hidden_windows: HashMap<WindowResourceKey, App>,
    window_resources: SlotMap<WindowSlotKey, ()>,
    performance_hud: Option<(WindowResourceKey, WindowResourceKey)>,
    startup_errors: Vec<Arc<Mutex<Option<crate::Error>>>>,
    factory: Option<FrameSourceFactory>,
    wake: WakeCallback,
    extensions: Vec<Box<dyn ShellExtension>>,
    extensions_initialized: bool,
    extensions_shutdown: bool,
    extension_error: Arc<Mutex<Option<crate::Error>>>,
    force_semantics: bool,
    relaunch_requested: Arc<AtomicBool>,
}

/// Event-loop services exposed to optional shell extensions.
///
/// The API intentionally deals in typed Wabou window keys rather than winit
/// window handles or unbranded integers, keeping platform integration crates
/// independent of Wabou's window backend.
pub struct ExtensionContext<'a> {
    windows: &'a mut HashMap<WindowId, App>,
    hidden_windows: &'a mut HashMap<WindowResourceKey, App>,
    event_loop: &'a dyn ActiveEventLoop,
}
/// Factory used to create a frame source for dynamically requested windows.
pub type FrameSourceFactory =
    Rc<dyn Fn(WindowResourceKey, &WindowOptions) -> Result<Box<dyn FrameSource>, String>>;

fn find_window_by_key<'a>(
    windows: impl Iterator<Item = &'a mut App>,
    window_key: WindowResourceKey,
) -> Option<&'a mut App> {
    windows.into_iter().find(|app| app.window_key == window_key)
}

fn apply_window_command(app: &mut App, command: WindowCommand) {
    match command {
        WindowCommand::Close => app.close_requested = true,
        WindowCommand::Minimize => {
            if let Some(shell) = app.state.as_mut() {
                shell.prepare_hide();
                shell.window().set_minimized(true);
            }
        }
        WindowCommand::SetMaximized(maximized) => {
            if let Some(shell) = app.state.as_ref() {
                shell.window().set_maximized(maximized);
            }
        }
        WindowCommand::ToggleMaximized => {
            if let Some(shell) = app.state.as_ref() {
                shell.window().set_maximized(!shell.is_maximized());
            }
        }
        WindowCommand::SetFullscreen(fullscreen) => {
            if let Some(shell) = app.state.as_ref() {
                shell.set_fullscreen(fullscreen);
            }
        }
        WindowCommand::ToggleFullscreen => {
            if let Some(shell) = app.state.as_ref() {
                shell.set_fullscreen(shell.window().fullscreen().is_none());
            }
        }
        WindowCommand::SetTitle(title) => {
            if let Some(shell) = app.state.as_ref() {
                shell.window().set_title(&title);
            }
        }
        WindowCommand::RestoreTitle => {
            if let Some(shell) = app.state.as_ref() {
                shell.window().set_title(&app.window_options.title);
            }
        }
        WindowCommand::RequestAttention => {
            if let Some(shell) = app.state.as_ref() {
                shell
                    .window()
                    .request_user_attention(Some(UserAttentionType::Informational));
            }
        }
        WindowCommand::SetDecorations(decorations) => {
            app.window_options.decorations = decorations;
            if let Some(shell) = app.state.as_mut() {
                shell.set_decorations(decorations, app.window_options.corner_radius);
            }
        }
        WindowCommand::StartDragging => {
            if let Some(shell) = app.state.as_mut() {
                shell.start_window_drag();
            }
        }
        // Surface restoration is a MultiWindowApp operation; a visible
        // window's Show is already a no-op in the lifecycle machine.
        WindowCommand::Show => {}
    }
}

/// Native integrations for a group of windows sharing one event loop.
#[derive(Default)]
pub struct EventLoopOptions {
    /// Creates extra windows requested by frame sources.
    pub factory: Option<FrameSourceFactory>,
    /// Services started and shut down with the event loop.
    pub extensions: Vec<Box<dyn ShellExtension>>,
}

impl EventLoopOptions {
    /// Restore the given window's placement and attach app state persistence.
    /// Apps call this once before launch, passing the initial window's options.
    pub fn persist_app(
        &mut self,
        directories: &crate::AppDirectoryConfig,
        window: crate::WindowResourceKey,
        options: &mut WindowOptions,
    ) {
        if let Some(dirs) = crate::AppDirectories::resolve(directories, "") {
            self.extensions
                .push(Box::new(crate::PersistedStatePersistence::new(
                    dirs.data_dir.join("persisted-state.plico"),
                )));
            self.extensions
                .push(Box::new(crate::WindowStatePersistence::restore(
                    dirs.data_dir.join("window-state.json"),
                    window,
                    options,
                )));
        }
    }
}

/// Run one frame source with explicit native window options.
pub fn run_window(source: impl FrameSource + 'static, options: WindowOptions) -> crate::Result<()> {
    run_windows(
        vec![(Box::new(source), options)],
        EventLoopOptions::default(),
    )
    .map(|_| ())
}

/// Run independent windows on one native event loop. The factory and
/// extensions in `options` share that loop with the initial windows.
pub fn run_windows(
    windows: Vec<(Box<dyn FrameSource>, WindowOptions)>,
    options: EventLoopOptions,
) -> crate::Result<RunOutcome> {
    if windows.is_empty() {
        return Ok(RunOutcome::Exit);
    }
    let _tracing = crate::telemetry::init_tracing();
    let _telemetry_export = crate::telemetry::ExportOnDrop;
    let event_loop: EventLoop = EventLoop::new().context(crate::error::CreateEventLoopSnafu)?;
    event_loop.set_control_flow(ControlFlow::Wait);
    let proxy = event_loop.create_proxy();
    let wake: WakeCallback = std::sync::Arc::new(move || proxy.wake_up());
    let apps = windows
        .into_iter()
        .map(|(source, options)| {
            let mut app = App::with_options(source, options);
            app.set_wake_callback(wake.clone());
            app
        })
        .collect();
    let relaunch_requested = Arc::new(AtomicBool::new(false));
    let app = MultiWindowApp::new(
        apps,
        options.factory,
        wake,
        options.extensions,
        relaunch_requested.clone(),
    );
    let errors = app.startup_errors.clone();
    let extension_error = app.extension_error.clone();
    event_loop
        .run_app(app)
        .context(crate::error::RunEventLoopSnafu)?;
    for error in errors {
        if let Some(error) = error.lock().expect("startup error mutex poisoned").take() {
            return Err(error);
        }
    }
    if let Some(error) = extension_error
        .lock()
        .expect("extension error mutex poisoned")
        .take()
    {
        return Err(error);
    }
    Ok(if relaunch_requested.load(Ordering::Acquire) {
        RunOutcome::Relaunch
    } else {
        RunOutcome::Exit
    })
}

/// Reason the native multi-window event loop terminated.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RunOutcome {
    /// All windows closed or the application requested normal termination.
    Exit,
    /// The application requested shutdown followed by process relaunch.
    Relaunch,
}

#[cfg(test)]
mod tests;
