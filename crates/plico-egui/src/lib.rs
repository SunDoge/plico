//! Plico declarations + Taffy geometry + egui controls, text and interaction.
//! This backend does not replay Scene commands or shape/rasterize glyphs itself.
#[cfg(feature = "widgets")]
mod adapter;
#[cfg(feature = "widgets")]
mod source;
#[cfg(feature = "window")]
mod window;
#[cfg(feature = "widgets")]
pub use adapter::{Adapter, Error};
#[cfg(feature = "widgets")]
pub use source::Source;
#[cfg(feature = "window")]
pub use window::EguiWindow;
