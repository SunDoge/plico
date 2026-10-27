//! Desktop window host for the Plico runtime. The declaration runtime lives in
//! `plico-runtime`; this crate keeps the native launchers, the development CLI
//! binary, and the Skia/Vello backend selection through plico-shell.
pub use plico_runtime::*;

#[cfg(not(target_arch = "wasm32"))]
pub use plico_shell::Error as DesktopError;

pub use plico_renderer::{
    HostAction, KeyEvent, RenderBackend, WakeCallback, WindowCommand, WindowLevel, WindowOptions,
    WindowResizeEdges,
};

/// Run an application directly, without dynamic loading or the development CLI.
#[cfg(not(target_arch = "wasm32"))]
pub fn run<A: plico_dev::App + 'static>(app: A, title: &str) -> Result<(), DesktopError> {
    run_with_options(app, WindowOptions::new().title(title))
}

/// Launch a view without requiring serializable application state.
/// Useful for services, channels and data that must not enter reload snapshots.
#[cfg(not(target_arch = "wasm32"))]
pub fn run_view_with_options<V: View + 'static>(
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
        #[cfg(feature = "hot-reload")]
        #[snafu(display("application module failed: {source}"))]
        Module { source: plico_dev::host::Error },
        #[cfg(not(feature = "hot-reload"))]
        #[snafu(display("enable plico-vello/hot-reload to run a development module"))]
        HotReloadDisabled,
    }

    /// Normal `main.rs` entry point. In plico dev the executable remains resident and
    /// only the module is replaced; outside development this uses direct typed calls.
    pub fn run_service_app_with_options<A, S>(
        app: A,
        services: S,
        options: WindowOptions,
    ) -> Result<(), ServiceDesktopError>
    where
        A: ServiceApp + 'static,
        S: HostService<Snapshot = A::Snapshot, Command = A::Command> + 'static,
    {
        if let Some(descriptor) = std::env::var_os("PLICO_DEV_MODULE") {
            #[cfg(feature = "hot-reload")]
            {
                let view =
                    plico_runtime::reload::ReloadView::with_services(descriptor.into(), services)
                        .map_err(|source| ServiceDesktopError::Module { source })?;
                return crate::run_view_with_options(view, options)
                    .map_err(|source| ServiceDesktopError::Desktop { source });
            }
            #[cfg(not(feature = "hot-reload"))]
            {
                let _ = descriptor;
                return Err(ServiceDesktopError::HotReloadDisabled);
            }
        }
        crate::run_view_with_options(ServiceView::new(app, services), options)
            .map_err(|source| ServiceDesktopError::Desktop { source })
    }
}
#[cfg(not(target_arch = "wasm32"))]
pub use services_host::{ServiceDesktopError, run_service_app_with_options};
