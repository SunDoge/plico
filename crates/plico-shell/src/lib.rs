//! plico-shell: render a UI tree with Winit, Taffy, Parley, and a GPU renderer.
//!
//! The shell is a reusable host: a [`plico_renderer::source::FrameSource`] produces a flattened
//! layout list each frame; [`shell::Shell`] owns the window and selected
//! GPU backend (wgpu by default). Plico supplies its immediate UI source through this boundary.

#![warn(missing_docs)]

extern crate self as plico_shell;

// Shell implementation vocabulary. Keep this private: applications consume
// these contracts from plico-renderer/plico-runtime instead of a second facade.
#[cfg(all(test, target_os = "linux", feature = "wgpu"))]
use plico_renderer::ScreenshotRequest;
use plico_renderer::{
    ColorScheme, ImeState, SemanticAction, SemanticCurrent, SemanticNode, SemanticPopup,
    SemanticRole, SemanticSnapshot, SemanticSortDirection, SemanticToggleState, WHEEL_LINE_DELTA,
};
#[cfg(test)]
use plico_renderer::{PaintContext, SemanticStates};

pub mod telemetry;

#[cfg(not(target_arch = "wasm32"))]
pub mod accessibility;
#[cfg(not(target_arch = "wasm32"))]
pub mod app;
#[cfg(not(target_arch = "wasm32"))]
pub mod app_dirs;
#[cfg(all(not(target_arch = "wasm32"), feature = "desktop-integration"))]
pub mod desktop;
pub mod effect;
#[cfg(not(target_arch = "wasm32"))]
pub mod error;
#[cfg(not(target_arch = "wasm32"))]
pub mod headless;
mod persisted_state;
pub use persisted_state::PersistedStatePersistence;
#[cfg(not(target_arch = "wasm32"))]
mod file_watch;
#[cfg(any(target_os = "macos", test))]
mod frame_pacing;
#[cfg(target_os = "linux")]
mod linux_preferences;
#[cfg(target_os = "macos")]
mod macos_frame_clock;
#[cfg(target_os = "macos")]
mod macos_input;
#[cfg(target_os = "macos")]
mod macos_preferences;
#[cfg(target_os = "macos")]
mod macos_window_frame;
#[cfg(not(target_arch = "wasm32"))]
mod performance_hud;
#[cfg(not(target_arch = "wasm32"))]
pub mod renderer;
#[cfg(not(target_arch = "wasm32"))]
pub mod shell;
#[cfg(not(target_arch = "wasm32"))]
mod single_instance;
#[cfg(not(target_arch = "wasm32"))]
mod system;
#[cfg(any(target_os = "macos", test))]
mod titlebar_action;
#[cfg(not(target_arch = "wasm32"))]
mod window_renderer;
#[cfg(not(target_arch = "wasm32"))]
mod window_state;
#[cfg(not(target_arch = "wasm32"))]
pub use file_watch::{FileWatchError, FileWatchEvent, FileWatchKind, FileWatcher};
#[cfg(not(target_arch = "wasm32"))]
pub use single_instance::{
    AppActivation, SingleInstance, SingleInstanceError, SingleInstanceOutcome,
};

#[cfg(not(target_arch = "wasm32"))]
pub use app::{
    ExtensionContext, FrameSourceFactory, RunOutcome, ShellExtension, run_window,
    run_window_with_options, run_window_with_size, run_windows, run_windows_with_factory,
    run_windows_with_factory_and_extensions,
};
#[cfg(not(target_arch = "wasm32"))]
pub use app_dirs::{AppDirectories, AppDirectoryConfig};
pub use effect::{
    CapabilityId, ClipboardContent, ClipboardImage, ContextMenuItem, ContextMenuRequest,
    DialogFilter, EFFECT_PROTOCOL_VERSION, EffectCompletion, EffectDispatch, EffectErrorCode,
    EffectExecutor, EffectId, EffectOp, EffectPayload, EffectRequest, EffectResult, EffectScope,
    EffectTapeEntry, MenuPosition, MessageDialogButtons, MessageDialogLevel, MessageDialogRequest,
    MethodId, NotificationRequest, OpenDialogRequest, PickDirectoryRequest,
    RecordingEffectExecutor, ReplayEffectExecutor, SaveDialogRequest, WindowResourceKey,
    initial_window_resource_key,
};
#[cfg(not(target_arch = "wasm32"))]
pub use error::{Error, Result};
#[cfg(not(target_arch = "wasm32"))]
pub use shell::Shell;
#[cfg(not(target_arch = "wasm32"))]
pub use window_state::{WindowSizePersistence, WindowStatePersistence};
#[cfg(not(target_arch = "wasm32"))]
pub use winit::raw_window_handle;

#[cfg(not(target_arch = "wasm32"))]
pub mod window_frame;

#[cfg(all(
    feature = "global-hotkeys",
    any(target_os = "linux", target_os = "windows", target_os = "macos")
))]
pub mod hotkeys;
