//! Desktop window host for the Plico runtime. The declaration runtime lives in
//! `plico-runtime`; this crate keeps the native launchers, the development CLI
//! binary, and desktop services through plico-shell.
pub use plico_app_host::{NativeApp, ServiceView, impl_host_view_for_app, tasks};
pub use plico_runtime::{
    BackendStats, BatchStats, FontAsset, FrameDiagnostics, HitTestDiagnostics, HostView,
    NativeView, Source, SourceError, testing,
};
#[cfg(not(target_arch = "wasm32"))]
pub use plico_runtime::{SurfaceSource, ZoomController, ZoomSource};

#[cfg(all(not(target_arch = "wasm32"), feature = "wgpu"))]
mod capture;
#[cfg(all(not(target_arch = "wasm32"), feature = "wgpu"))]
pub use capture::{
    CaptureError, CaptureOptions, CaptureProfile, CaptureSession, CapturedFrame, capture,
};

#[cfg(not(target_arch = "wasm32"))]
pub use plico_shell::Error as DesktopError;

pub use plico_renderer::platform::{
    ClipboardContent, ClipboardImage, EffectCompletion, EffectPayload, EffectQueue, EffectRequest,
    NotificationRequest, WindowResourceKey, initial_window_resource_key,
};

pub use plico_renderer::{
    FrameBudgetSample, KeyEvent, WakeCallback, WindowCommand, WindowLevel, WindowOptions,
    WindowResizeEdges,
};

/// Run a statically linked App with native window configuration.
#[cfg(not(target_arch = "wasm32"))]
pub fn run<A: plico_dev::App + 'static>(
    app: A,
    options: WindowOptions,
) -> Result<(), DesktopError> {
    run_source(Source::new(NativeApp(app)), options)
}

/// Launch an ordinary application view with no host integration.
#[cfg(not(target_arch = "wasm32"))]
pub fn run_view<V: plico::View + 'static>(
    view: V,
    options: WindowOptions,
) -> Result<(), DesktopError> {
    run_source(Source::new(NativeView(view)), options)
}

#[cfg(all(not(target_arch = "wasm32"), feature = "wgpu"))]
mod preview;
#[cfg(all(not(target_arch = "wasm32"), feature = "wgpu"))]
pub use preview::{PreviewError, PreviewScenario, PreviewSource, PreviewStep, PreviewViewport};

/// Versioned recipe plans with assertions, fixture events and checkpoint captures.
#[cfg(all(not(target_arch = "wasm32"), feature = "wgpu"))]
pub mod recipe;

/// Optional tray, native-menu and actionable-notification services.
#[cfg(all(not(target_arch = "wasm32"), feature = "desktop-integration"))]
pub use plico_shell::desktop;
#[cfg(not(target_arch = "wasm32"))]
pub use plico_shell::{
    AppActivation, FileWatchError, FileWatchEvent, FileWatchKind, FileWatcher,
    PersistedStatePersistence, SingleInstance, SingleInstanceError, SingleInstanceOutcome,
    WindowStatePersistence,
};
/// Native frame source launch and event-loop integrations.
#[cfg(not(target_arch = "wasm32"))]
pub use plico_shell::{
    EventLoopOptions, ExtensionContext, FrameSourceFactory, RunOutcome, ShellExtension,
    run_window as run_source, run_windows,
};
