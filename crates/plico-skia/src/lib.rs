//! Experimental Skia CPU consumer for Plico's retained paint commands.
//!
//! Enable `raster` to replay a scene. Layout and shaping remain in Plico;
//! unsupported commands return errors rather than producing incomplete images.

/// Failure to replay or export a scene.
#[derive(Debug, snafu::Snafu)]
pub enum Error {
    #[snafu(display("unsupported Skia command or property: {command}"))]
    Unsupported { command: &'static str },
    #[snafu(display("invalid raster surface size: {width} x {height}"))]
    InvalidSize { width: u32, height: u32 },
    #[snafu(display("Skia surface allocation failed"))]
    Surface,
    #[snafu(display("no successfully rendered frame is available"))]
    NoFrame,
    #[snafu(display("Skia pixel readback failed"))]
    ReadPixels,
    #[snafu(display("Skia could not load the command's font"))]
    Font,
    #[snafu(display("glyph ID exceeds Skia's 16-bit glyph range"))]
    GlyphId,
    #[snafu(display("unbalanced clip stack"))]
    ClipStack,
    #[snafu(display("Skia PNG encoding failed"))]
    Encode,
    #[snafu(display("PNG write failed: {source}"))]
    Write { source: std::io::Error },
}

#[cfg(feature = "raster")]
mod raster;
#[cfg(feature = "raster")]
pub use raster::SkiaSurface;
