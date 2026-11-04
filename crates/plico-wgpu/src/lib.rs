//! Experimental physical-pixel, axis-aligned quad renderer.
//!
//! This is an offscreen backend prototype, not a complete `plico_renderer::Scene`
//! implementation. It deliberately has no Vello dependency in its rendering path.
mod blit;
#[cfg(feature = "paint-scene")]
mod blur;
#[cfg(all(feature = "paint-scene", target_os = "linux", feature = "system-text"))]
mod glyphs;
mod renderer;
#[cfg(feature = "window")]
mod window;
#[cfg(feature = "window")]
pub use window::WgpuWindow;
#[cfg(feature = "paint-scene")]
pub mod image;
#[cfg(feature = "paint-scene")]
pub mod mask;
#[cfg(all(feature = "paint-scene", target_os = "linux", feature = "system-text"))]
mod paragraph;
#[cfg(all(
    feature = "paint-scene",
    target_os = "windows",
    feature = "system-text"
))]
mod windows_paragraph;
#[cfg(all(feature = "paint-scene", target_os = "linux", feature = "system-text"))]
pub use paragraph::ParagraphCacheStats;
#[cfg(all(
    feature = "paint-scene",
    target_os = "windows",
    feature = "system-text"
))]
pub use windows_paragraph::WindowsTextCacheStats;
#[cfg(feature = "paint-scene")]
mod path;
#[cfg(feature = "paint-scene")]
pub mod scene;
#[cfg(feature = "paint-scene")]
mod svg;
#[cfg(all(target_os = "linux", feature = "system-text"))]
pub mod text;
mod texture;
#[cfg(feature = "paint-scene")]
pub use renderer::LayerPoolStats;
pub use renderer::{FrameBatchStats, Renderer};

use snafu::Snafu;

/// Errors are returned rather than silently omitting unsupported/invalid input.
#[derive(Debug, Snafu)]
pub enum Error {
    #[snafu(display("window surface failed: {message}"))]
    Surface { message: String },
    #[cfg(feature = "paint-scene")]
    #[snafu(display("paragraph preparation failed: {source}"))]
    Paragraph {
        source: plico_renderer::paragraph::ParagraphError,
    },
    #[snafu(display("unsupported paint command {command}: {reason}"))]
    Unsupported { command: usize, reason: String },
    #[cfg(all(target_os = "linux", feature = "system-text"))]
    #[snafu(display("system glyph processing failed: {source}"))]
    Text { source: plico_native::text::Error },
    #[snafu(display("could not acquire a GPU adapter: {source}"))]
    Adapter { source: wgpu::RequestAdapterError },
    #[snafu(display("could not create a GPU device: {source}"))]
    Device { source: wgpu::RequestDeviceError },
    #[snafu(display("invalid quad or frame: {reason}"))]
    InvalidInput { reason: String },
    #[snafu(display("invalid WGSL material: {message}"))]
    Shader { message: String },
    #[snafu(display("GPU wait failed: {source}"))]
    Poll { source: wgpu::PollError },
    #[snafu(display("GPU readback failed: {message}"))]
    Readback { message: String },
}
pub type Result<T> = std::result::Result<T, Error>;

/// Physical-pixel quad. Colors are straight sRGB RGBA, channels in 0..=1.
/// The border is inside the rectangle. All corners share a circular radius.
#[derive(Clone, Copy, Debug)]
pub struct Quad {
    /// x, y, width, height; negative positions are allowed.
    pub rect: [f32; 4],
    pub radius: f32,
    /// Gaussian shadow sigma. Positive values blur the inset rounded rectangle
    /// (the quad includes a four-sigma margin on every side).
    pub shadow_sigma: f32,
    /// Uniform inside border width in physical pixels.
    pub border_width: f32,
    pub border_color: [f32; 4],
    /// Top and bottom gradient colors; equal colors produce a solid fill.
    pub colors: [[f32; 4]; 2],
    /// Gradient start/end in normalized rectangle coordinates.
    pub gradient: [f32; 4],
    /// Interpolate colors premultiplied (CSS); false preserves Canvas-style straight interpolation.
    pub gradient_premultiplied: bool,
    /// Optional hard clip: x, y, width, height.
    pub clip: Option<[f32; 4]>,
    /// Apply pixel-area coverage at fractional clip edges instead of a hard scissor.
    pub clip_antialias: bool,
    /// Use the installed WGSL material instead of the built-in vertical gradient.
    pub material: bool,
    /// Scalar passed to the installed material (e.g. progress).
    pub parameter: f32,
}
impl Quad {
    pub fn new(rect: [f32; 4], color: [f32; 4]) -> Self {
        Self {
            rect,
            radius: 0.,
            shadow_sigma: 0.,
            border_width: 0.,
            border_color: [0.; 4],
            colors: [color; 2],
            gradient: [0., 0., 0., 1.],
            gradient_premultiplied: false,
            clip: None,
            clip_antialias: false,
            material: false,
            parameter: 0.,
        }
    }
    fn validate(&self) -> Result<()> {
        let valid_rect = |r: [f32; 4]| r.iter().all(|v| v.is_finite()) && r[2] >= 0. && r[3] >= 0.;
        let valid_color = |c: [f32; 4]| c.iter().all(|v| v.is_finite() && (0. ..=1.).contains(v));
        if !valid_rect(self.rect)
            || self.clip.is_some_and(|r| !valid_rect(r))
            || !self.shadow_sigma.is_finite()
            || self.shadow_sigma < 0.
            || (self.shadow_sigma > 0.
                && (self.material
                    || self.rect[2] < 8. * self.shadow_sigma
                    || self.rect[3] < 8. * self.shadow_sigma))
            || !self.radius.is_finite()
            || self.radius < 0.
            || !self.border_width.is_finite()
            || self.border_width < 0.
            || !self.gradient.iter().all(|v| v.is_finite())
            || !self.parameter.is_finite()
            || !self.colors.into_iter().all(valid_color)
            || !valid_color(self.border_color)
        {
            return Err(Error::InvalidInput {reason:"quad coordinates must be finite, extents/radii/borders nonnegative and colors in 0..=1".into()});
        }
        Ok(())
    }
}
/// Public WGSL ABI: define `fn plico_material(material: Material) -> vec4f`.
/// `Material` supplies uv, physical size, two straight colors, time and parameter.
/// Output is straight RGBA; the renderer applies border, clip and premultiplication.
pub const DEFAULT_MATERIAL: &str = include_str!("default_material.wgsl");

/// Shared WGSL material data definition, usable by other shader backends.
pub const MATERIAL_INTERFACE: &str = include_str!("material_interface.wgsl");

/// Ordered physical-pixel draws. Text and shapes share the same compositing pass.
#[derive(Clone, Copy)]
pub enum Draw<'a> {
    Quad(&'a Quad),
    #[cfg(feature = "paint-scene")]
    Image(&'a image::ImageQuad),
    #[cfg(feature = "paint-scene")]
    Layer(&'a scene::PreparedLayer),
    #[cfg(feature = "paint-scene")]
    Effect(&'a plico_renderer::ShaderEffect, &'a Quad),
    /// Replace the active coverage mask; masks do not themselves paint pixels.
    #[cfg(feature = "paint-scene")]
    SetMask(&'a mask::CoverageMask),
    #[cfg(feature = "paint-scene")]
    ResetMask,
    #[cfg(all(target_os = "linux", feature = "system-text"))]
    Text(text::TextRun<'a>),
}

// Prefer native primary APIs. Creating an unused EGL backend in a headless,
// multithreaded process can bind a thread-owned context and fail with BadAccess.
// WGPU_BACKEND still permits an explicit legacy-GL override for experiments.
pub(crate) fn instance_descriptor() -> wgpu::InstanceDescriptor {
    let mut descriptor = wgpu::InstanceDescriptor::new_without_display_handle();
    descriptor.backends = wgpu::Backends::PRIMARY;
    descriptor.with_env()
}
