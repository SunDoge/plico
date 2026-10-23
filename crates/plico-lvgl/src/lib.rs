//! A small backend experiment: Plico declarations → native LVGL widgets.
//!
//! Enable `native` on stable Rust and provide lightvgl-sys's
//! `DEP_LV_CONFIG_PATH` pointing at `crates/plico-lvgl/config`. Kinds without
//! a native widget degrade along `KindFamily`; this is not a complete
//! renderer or an embedded port. See `docs/lvgl.md` in the repository.
#![allow(unsafe_code)]

#[cfg(feature = "external")]
mod ffi;
#[cfg(feature = "external")]
mod native;
#[cfg(feature = "external")]
pub use ffi::{Driver, Snapshot};
#[cfg(feature = "external")]
pub use lightvgl_sys;
#[cfg(feature = "external")]
pub use native::{Adapter, Error, Stats};
