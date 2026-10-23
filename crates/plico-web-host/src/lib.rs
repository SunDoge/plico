//! Browser host for the same retained Plico source used by native applications.
//! Build for wasm32-unknown-unknown. Native dynamic-library reload is not used.
#[cfg(target_arch = "wasm32")]
mod browser;
#[cfg(target_arch = "wasm32")]
pub use browser::*;

#[cfg(all(target_arch = "wasm32", feature = "split"))]
mod split;
