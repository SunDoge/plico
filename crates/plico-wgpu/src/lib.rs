//! Plico wgpu renderer with shared Scene support and physical-pixel quads.
//!
//! Shared Scene rendering is always available; system text and window hosting
//! remain separate features. The rendering path does not invoke Vello.
mod backdrop_batch;
mod backdrop_composite;
mod backdrop_prefilter;
mod blit;
mod blur;
#[cfg(all(target_os = "linux", feature = "system-text"))]
mod glyphs;
mod mipmap;
mod renderer;
#[cfg(all(target_arch = "wasm32", feature = "web"))]
pub mod web;
#[cfg(all(target_arch = "wasm32", feature = "web"))]
mod web_text;
#[cfg(feature = "window")]
mod window;
#[cfg(feature = "window")]
pub use window::WgpuWindow;
pub mod image;
#[cfg(all(target_os = "macos", feature = "system-text"))]
mod macos_paragraph;
pub mod mask;
#[cfg(all(target_os = "linux", feature = "system-text"))]
mod paragraph;
#[cfg(all(target_os = "windows", feature = "system-text"))]
mod windows_paragraph;
#[cfg(all(target_os = "macos", feature = "system-text"))]
pub use macos_paragraph::MacosTextCacheStats;
#[cfg(all(target_os = "linux", feature = "system-text"))]
pub use paragraph::ParagraphCacheStats;
#[cfg(all(target_os = "windows", feature = "system-text"))]
pub use windows_paragraph::WindowsTextCacheStats;
mod path;
pub mod scene;
mod svg;
#[cfg(all(target_os = "linux", feature = "system-text"))]
pub mod text;
mod texture;
pub use renderer::LayerPoolStats;
pub use renderer::{FrameBatchStats, Renderer};

use snafu::Snafu;

/// Errors are returned rather than silently omitting unsupported/invalid input.
#[derive(Debug, Snafu)]
pub enum Error {
    #[snafu(display("window surface failed: {message}"))]
    Surface { message: String },
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
/// The border is inside the rectangle.
#[derive(Clone, Copy, Debug)]
pub struct Quad {
    /// x, y, width, height; negative positions are allowed.
    pub rect: [f32; 4],
    /// Circular corner radii: top-left, top-right, bottom-right, bottom-left.
    /// The renderer shrinks overlapping adjacent corners proportionally.
    pub radii: [f32; 4],
    /// Gaussian shadow sigma. Positive values blur the inset rounded rectangle
    /// (the quad includes a four-sigma margin on every side).
    pub shadow_sigma: f32,
    /// Continuous-corner blend for shadow geometry only.
    pub shadow_smoothing: f32,
    /// Optional rounded hole relative to the quad origin, for an inset shadow.
    pub shadow_hole: Option<[f32; 4]>,
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
            radii: [0.; 4],
            shadow_sigma: 0.,
            shadow_smoothing: 0.,
            shadow_hole: None,
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
            || (self.shadow_hole.is_some() && self.material)
            || !self.shadow_smoothing.is_finite()
            || !(0. ..=1.).contains(&self.shadow_smoothing)
            || !self.shadow_sigma.is_finite()
            || self.shadow_sigma < 0.
            || self
                .shadow_hole
                .is_some_and(|h| !h.iter().all(|v| v.is_finite()))
            || (self.shadow_sigma > 0.
                && self.shadow_hole.is_none()
                && (self.material
                    || self.rect[2] < 8. * self.shadow_sigma
                    || self.rect[3] < 8. * self.shadow_sigma))
            || !self.radii.iter().all(|r| r.is_finite() && *r >= 0.)
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
    Image(&'a image::ImageQuad),
    Layer(&'a scene::PreparedLayer),
    Effect(&'a plico_renderer::ShaderEffect, &'a Quad),
    /// Replace the active coverage mask; masks do not themselves paint pixels.
    SetMask(&'a mask::CoverageMask),
    ResetMask,
    #[cfg(all(target_os = "linux", feature = "system-text"))]
    Text(text::TextRun<'a>),
}

// Prefer native primary APIs. Creating an unused EGL backend in a headless,
// multithreaded process can bind a thread-owned context and fail with BadAccess.
// WGPU_BACKEND still permits an explicit legacy-GL override for experiments.
pub(crate) fn instance_descriptor() -> wgpu::InstanceDescriptor {
    let mut descriptor = wgpu::InstanceDescriptor::new_without_display_handle();
    #[cfg(not(target_arch = "wasm32"))]
    {
        descriptor.backends = wgpu::Backends::PRIMARY;
    }
    #[cfg(target_arch = "wasm32")]
    {
        descriptor.backends = wgpu::Backends::BROWSER_WEBGPU | wgpu::Backends::GL;
    }
    descriptor.with_env()
}
