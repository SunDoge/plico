//! Deterministic native-UI compositions rendered by Plico's wgpu backend.
//!
//! Frame and composition semantics are adapted from MyGO's MIT-licensed
//! `plugins/render`; see `SOURCES.md` and `licenses/mygo-MIT.txt`.

mod frame;
mod output;

pub use frame::{Extrapolate, Frame, Interpolation, interpolate};
pub use output::{
    Codec, Composition, Error, MotionBlur, Options, Progress, frames, images, still, video,
};
