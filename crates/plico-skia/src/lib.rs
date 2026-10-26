//! Experimental Skia CPU and Vulkan consumers for Plico's retained paint commands.
//!
//! Enable `raster` for CPU drawing or `vulkan` for Ganesh GPU render targets.
//! Layout and shaping remain in Plico;
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
    #[snafu(display("damage-tracked pixels differ from a full redraw"))]
    DamageMismatch,
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
    #[cfg(feature = "vulkan")]
    #[snafu(display("Vulkan loader unavailable: {source}"))]
    VulkanLoader { source: ash::LoadingError },
    #[cfg(feature = "vulkan")]
    #[snafu(display("Vulkan {operation} failed: {source}"))]
    Vulkan {
        operation: &'static str,
        source: ash::vk::Result,
    },
    #[snafu(display("GPU initialization failed: {message}"))]
    Gpu { message: &'static str },
    #[snafu(display("Skia GPU submission failed"))]
    Submit,
    #[cfg(feature = "window")]
    #[snafu(display("native window handle is unavailable: {source}"))]
    Handle {
        source: raw_window_handle::HandleError,
    },
}

#[cfg(feature = "raster")]
mod paragraph;
#[cfg(feature = "raster")]
mod raster;
#[cfg(feature = "raster")]
mod svg;
#[cfg(feature = "raster")]
pub use raster::SkiaSurface;

#[cfg(feature = "vulkan")]
mod vulkan;
#[cfg(feature = "vulkan")]
pub use vulkan::{GpuCacheStats, SkiaGpuSurface};

#[cfg(feature = "window")]
pub use vulkan::SkiaWindow;
