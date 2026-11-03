//! High-level CPU capture for tests, documentation, and offline exports.
use plico_renderer::{FrameSource, Scene, TextContext};
use snafu::Snafu;

/// A rendered, premultiplied-RGBA frame in physical pixels.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapturedFrame {
    width: u32,
    height: u32,
    rgba: Vec<u8>,
}
impl CapturedFrame {
    /// PNG files use straight alpha; captured pixels remain premultiplied.
    pub fn write_png(&self, path: impl AsRef<std::path::Path>) -> Result<(), CaptureError> {
        let mut pixels = self.rgba.clone();
        for pixel in pixels.as_chunks_mut::<4>().0 {
            let alpha = u32::from(pixel[3]);
            if alpha > 0 && alpha < 255 {
                for c in &mut pixel[..3] {
                    *c = ((u32::from(*c) * 255 + alpha / 2) / alpha).min(255) as u8;
                }
            }
        }
        image::save_buffer_with_format(
            path,
            &pixels,
            self.width,
            self.height,
            image::ColorType::Rgba8,
            image::ImageFormat::Png,
        )
        .map_err(|source| CaptureError::Image { source })
    }
    pub fn width(&self) -> u32 {
        self.width
    }
    pub fn height(&self) -> u32 {
        self.height
    }
    pub fn rgba(&self) -> &[u8] {
        &self.rgba
    }
    pub fn into_rgba(self) -> Vec<u8> {
        self.rgba
    }
}

#[derive(Debug, Snafu)]
pub enum CaptureError {
    #[snafu(display("capture dimensions must be nonzero and scale must be finite and positive"))]
    InvalidOptions,
    #[snafu(display("PNG encoding failed: {source}"))]
    Image { source: image::ImageError },
    #[snafu(display("Skia capture failed: {source}"))]
    Skia { source: plico_skia::Error },
}
impl From<plico_skia::Error> for CaptureError {
    fn from(source: plico_skia::Error) -> Self {
        Self::Skia { source }
    }
}

/// Capture a frame source without exposing scene, text, or settle-pass setup.
///
/// Polling is bounded: `settle_passes(0)` captures the first frame, while the
/// default allows eight asynchronous layout/native-widget notifications.
pub fn capture(source: &mut dyn FrameSource) -> Capture<'_> {
    Capture {
        source,
        width: 800,
        height: 600,
        scale: 1.,
        settle_passes: 8,
    }
}

pub struct Capture<'a> {
    source: &'a mut dyn FrameSource,
    width: u32,
    height: u32,
    scale: f64,
    settle_passes: usize,
}
impl Capture<'_> {
    pub fn size(mut self, width: u32, height: u32) -> Self {
        self.width = width;
        self.height = height;
        self
    }
    pub fn scale(mut self, scale: f64) -> Self {
        self.scale = scale;
        self
    }
    pub fn settle_passes(mut self, passes: usize) -> Self {
        self.settle_passes = passes;
        self
    }
    pub fn render(self) -> Result<CapturedFrame, CaptureError> {
        let (mut surface, width, height) = self.render_surface()?;
        let rgba = surface.read_pixels()?;
        Ok(CapturedFrame {
            width,
            height,
            rgba,
        })
    }
    pub fn write_png(self, path: impl AsRef<std::path::Path>) -> Result<(), CaptureError> {
        let (mut surface, _, _) = self.render_surface()?;
        surface.write_png(path.as_ref())?;
        Ok(())
    }
    fn render_surface(self) -> Result<(plico_skia::SkiaSurface, u32, u32), CaptureError> {
        if self.width == 0 || self.height == 0 || !self.scale.is_finite() || self.scale <= 0. {
            return Err(CaptureError::InvalidOptions);
        }
        let width = (self.width as f64 * self.scale).round() as u32;
        let height = (self.height as f64 * self.scale).round() as u32;
        if width == 0 || height == 0 {
            return Err(CaptureError::InvalidOptions);
        }
        let mut surface = plico_skia::SkiaSurface::new(width, height)?;
        let mut text = TextContext::new();
        surface.configure_text(&mut text);
        for pass in 0..=self.settle_passes {
            let mut scene = Scene::new();
            if self
                .source
                .render_retained_frame(&mut scene, &mut text, (self.width, self.height), self.scale)
                .is_none()
            {
                self.source.build_and_paint(
                    &mut scene,
                    &mut text,
                    self.width,
                    self.height,
                    self.scale,
                );
            }
            if pass == self.settle_passes || !self.source.poll_async() {
                surface.draw_scene(&scene, self.source.base_color())?;
                break;
            }
        }
        Ok((surface, width, height))
    }
}
