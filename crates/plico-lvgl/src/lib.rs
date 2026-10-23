//! A small backend experiment: Plico declarations → native oxivgl widgets.
//!
//! Enable `native` on nightly Rust and provide oxivgl's `DEP_LV_CONFIG_PATH`.
//! Only Root/Column/Row/Text/Button/ButtonBase are implemented; this is not a complete
//! renderer or an embedded port. See `docs/lvgl.md` in the repository.
#![forbid(unsafe_code)]

#[cfg(feature = "external")]
mod native;
#[cfg(feature = "external")]
pub use native::{Adapter, Error, Stats};
#[cfg(feature = "external")]
pub use oxivgl;
