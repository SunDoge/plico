//! Desktop window host for the Plico runtime. The declaration runtime lives in
//! `plico-runtime`; this crate keeps the native launchers, the development CLI
//! binary, and desktop renderer selection through plico-shell.
pub use plico_runtime::*;

#[cfg(all(not(target_arch = "wasm32"), any(feature = "skia", feature = "wgpu")))]
mod capture;
#[cfg(all(not(target_arch = "wasm32"), any(feature = "skia", feature = "wgpu")))]
pub use capture::{Capture, CaptureError, CapturedFrame, capture};

#[cfg(not(target_arch = "wasm32"))]
pub use plico_shell::Error as DesktopError;

pub use plico_renderer::platform::{
    NotificationRequest, WindowResourceKey, initial_window_resource_key,
};

pub use plico_renderer::{
    HostAction, KeyEvent, RenderBackend, WakeCallback, WindowCommand, WindowLevel, WindowOptions,
    WindowResizeEdges,
};

/// Run an application directly, without dynamic loading or the development CLI.
#[cfg(not(target_arch = "wasm32"))]
pub fn run<A: plico_dev::App + 'static>(app: A, title: &str) -> Result<(), DesktopError> {
    run_with_options(app, WindowOptions::new().title(title))
}

/// Launch an ordinary application view without serializable application state.
#[cfg(not(target_arch = "wasm32"))]
pub fn run_view_with_options<V: plico::View + 'static>(
    view: V,
    options: WindowOptions,
) -> Result<(), DesktopError> {
    run_host_view_with_options(NativeView(view), options)
}

/// Launch a view that implements native host capabilities directly.
/// Useful for services, Regions, and application-owned wakeups.
#[cfg(not(target_arch = "wasm32"))]
pub fn run_host_view_with_options<V: HostView + 'static>(
    view: V,
    options: WindowOptions,
) -> Result<(), DesktopError> {
    plico_shell::run_window_with_options(Box::new(Source::new(view)), options)
}

/// Run a statically linked App with native window configuration.
#[cfg(not(target_arch = "wasm32"))]
pub fn run_with_options<A: plico_dev::App + 'static>(
    app: A,
    options: WindowOptions,
) -> Result<(), DesktopError> {
    plico_shell::run_window_with_options(Box::new(Source::new(NativeApp(app))), options)
}

/// Launch a custom native frame source, including host-owned surfaces.
#[cfg(not(target_arch = "wasm32"))]
pub fn run_source_with_options(
    source: impl plico_renderer::FrameSource + 'static,
    options: WindowOptions,
) -> Result<(), DesktopError> {
    plico_shell::run_window_with_options(Box::new(source), options)
}

#[cfg(not(target_arch = "wasm32"))]
mod services_host {
    use plico_dev::services::{HostService, ServiceApp};
    use plico_runtime::{ServiceView, WindowOptions};
    use snafu::Snafu;

    #[derive(Debug, Snafu)]
    pub enum ServiceDesktopError {
        #[snafu(display("desktop host failed: {source}"))]
        Desktop { source: crate::DesktopError },
    }

    /// Launch an application and its typed host services.
    pub fn run_service_app_with_options<A, S>(
        app: A,
        services: S,
        options: WindowOptions,
    ) -> Result<(), ServiceDesktopError>
    where
        A: ServiceApp + 'static,
        S: HostService<Snapshot = A::Snapshot, Command = A::Command> + 'static,
    {
        crate::run_host_view_with_options(ServiceView::new(app, services), options)
            .map_err(|source| ServiceDesktopError::Desktop { source })
    }
}
#[cfg(not(target_arch = "wasm32"))]
pub use services_host::{ServiceDesktopError, run_service_app_with_options};

#[cfg(all(not(target_arch = "wasm32"), any(feature = "skia", feature = "wgpu")))]
mod preview;
#[cfg(all(not(target_arch = "wasm32"), any(feature = "skia", feature = "wgpu")))]
pub use preview::{
    PreviewError, PreviewScenario, PreviewSession, PreviewSource, PreviewStep, PreviewViewport,
};

/// Versioned recipe plans with assertions, fixture events and checkpoint captures.
#[cfg(all(not(target_arch = "wasm32"), any(feature = "skia", feature = "wgpu")))]
pub mod recipe;

/// Optional tray, native-menu and actionable-notification services.
#[cfg(all(not(target_arch = "wasm32"), feature = "desktop-integration"))]
pub use plico_shell::desktop;
/// Native window services available to desktop integrations.
#[cfg(not(target_arch = "wasm32"))]
pub use plico_shell::{ExtensionContext, RunOutcome, ShellExtension};

/// Launch a source and native integrations on one event loop.
#[cfg(not(target_arch = "wasm32"))]
pub fn run_source_with_extensions(
    source: impl plico_renderer::FrameSource + 'static,
    options: WindowOptions,
    extensions: Vec<Box<dyn ShellExtension>>,
) -> Result<RunOutcome, DesktopError> {
    plico_shell::run_windows_with_factory_and_extensions(
        vec![(Box::new(source), options)],
        None,
        extensions,
    )
}
