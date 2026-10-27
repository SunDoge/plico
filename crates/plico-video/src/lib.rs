//! Experimental video playback. The default build does not link libmpv.
//! Enable `mpv` on desktop to use the software-frame reference backend.
#[cfg(all(feature = "mpv", not(target_arch = "wasm32")))]
mod mpv;
#[cfg(all(feature = "mpv", not(target_arch = "wasm32")))]
pub use mpv::*;
