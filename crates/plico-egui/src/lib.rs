//! egui/epaint consumer for the Plico command stream.
//!
//! This backend verifies the renderer independence of the Plico front half:
//! declarations, the engine, Taffy layout and Parley shaping stay untouched on
//! the shared runtime - this crate only replays the resulting [`Scene`]
//! commands onto `epaint` shapes. Glyph identifiers and positions arrive
//! pre-shaped in `PaintCommand::GlyphRun`; the only typography done here is
//! rasterizing those identifiers into alpha coverage, the same role epaint's
//! own font atlas plays for its labels.
//!
//! Unsupported features return explicit errors instead of drawing nothing:
//! WGSL shader effects have no epaint equivalent.

#[cfg(feature = "replay")]
mod glyph;
#[cfg(feature = "replay")]
mod replay;
#[cfg(feature = "replay")]
pub use glyph::GlyphCache;
#[cfg(feature = "replay")]
pub use replay::{ReplayError, replay_scene, replay_scene_at};
