//! The main application-facing entry point for Plico.
//!
//! Disable default features for declarations without a desktop host.
extern crate self as plico;

#[doc(hidden)]
pub use plico_core as __core;
pub use plico_core::*;
pub use plico_dev::{App, services, tasks};
pub use serde;

/// Common widgets, layout primitives, macros and application trait.
pub mod prelude {
    pub use plico_core::prelude::*;
    pub use plico_dev::App;
}

/// Desktop window creation, backed by the Plico desktop host.
#[cfg(all(feature = "desktop", not(target_arch = "wasm32")))]
pub use plico_vello::{DesktopError, RenderBackend, WindowOptions, run, run_with_options};

/// Export the application ABI for the development host.
#[cfg(all(feature = "abi", not(target_arch = "wasm32")))]
pub use plico_dev::export_app;
