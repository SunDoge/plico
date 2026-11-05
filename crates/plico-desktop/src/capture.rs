//! Backend-selected offscreen capture for tests, documentation, and offline exports.
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
    #[snafu(display("capture backend {backend:?} is unavailable"))]
    Backend { backend: crate::RenderBackend },
    #[cfg(feature = "wgpu")]
    #[snafu(display("wgpu capture failed: {source}"))]
    Wgpu { source: plico_wgpu::Error },
    #[cfg(feature = "skia")]
    #[snafu(display("Skia capture failed: {source}"))]
    Skia { source: plico_skia::Error },
}
#[cfg(feature = "skia")]
impl From<plico_skia::Error> for CaptureError {
    fn from(source: plico_skia::Error) -> Self {
        Self::Skia { source }
    }
}

#[cfg(feature = "wgpu")]
impl From<plico_wgpu::Error> for CaptureError {
    fn from(source: plico_wgpu::Error) -> Self {
        Self::Wgpu { source }
    }
}

pub(crate) enum CaptureRenderer {
    #[cfg(feature = "skia")]
    Skia(plico_skia::SkiaSurface),
    #[cfg(feature = "wgpu")]
    Wgpu(Box<plico_wgpu::Renderer>),
}
impl CaptureRenderer {
    pub(crate) fn new(
        backend: crate::RenderBackend,
        width: u32,
        height: u32,
    ) -> Result<Self, CaptureError> {
        let backend = if matches!(backend, crate::RenderBackend::Auto) {
            if cfg!(feature = "skia") {
                crate::RenderBackend::Skia
            } else {
                crate::RenderBackend::Wgpu
            }
        } else {
            backend
        };
        match backend {
            #[cfg(feature = "skia")]
            crate::RenderBackend::Skia => {
                Ok(Self::Skia(plico_skia::SkiaSurface::new(width, height)?))
            }
            #[cfg(feature = "wgpu")]
            crate::RenderBackend::Wgpu => {
                let _ = (width, height);
                Ok(Self::Wgpu(Box::new(pollster::block_on(
                    plico_wgpu::Renderer::new(),
                )?)))
            }
            _ => Err(CaptureError::Backend { backend }),
        }
    }
    pub(crate) fn configure_text(
        &mut self,
        text: &mut TextContext,
        scale: f64,
    ) -> Result<(), CaptureError> {
        let _ = scale;
        match self {
            #[cfg(feature = "skia")]
            Self::Skia(surface) => {
                surface.configure_text(text);
                Ok(())
            }
            #[cfg(feature = "wgpu")]
            Self::Wgpu(renderer) => {
                #[cfg(any(target_os = "linux", target_os = "windows"))]
                {
                    renderer.configure_text(text, scale).map_err(Into::into)
                }
                #[cfg(not(any(target_os = "linux", target_os = "windows")))]
                {
                    let _ = (renderer, text);
                    Err(CaptureError::Backend {
                        backend: crate::RenderBackend::Wgpu,
                    })
                }
            }
        }
    }
    fn draw(
        &mut self,
        scene: &Scene,
        size: [u32; 2],
        background: peniko::Color,
    ) -> Result<Vec<u8>, CaptureError> {
        let _ = size;
        match self {
            #[cfg(feature = "skia")]
            Self::Skia(surface) => Ok(surface.render_scene(scene, background)?),
            #[cfg(feature = "wgpu")]
            Self::Wgpu(renderer) => {
                Ok(renderer.render_scene(scene, size, background.components, false)?)
            }
        }
    }
}
pub(crate) fn dimensions(width: u32, height: u32, scale: f64) -> Result<(u32, u32), CaptureError> {
    if width == 0 || height == 0 || !scale.is_finite() || scale <= 0. {
        return Err(CaptureError::InvalidOptions);
    }
    let width = f64::from(width) * scale;
    let height = f64::from(height) * scale;
    if !width.is_finite()
        || !height.is_finite()
        || width > u32::MAX as f64
        || height > u32::MAX as f64
    {
        return Err(CaptureError::InvalidOptions);
    }
    let (width, height) = (width.round() as u32, height.round() as u32);
    if width == 0 || height == 0 {
        return Err(CaptureError::InvalidOptions);
    }
    Ok((width, height))
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
        backend: crate::RenderBackend::Auto,
    }
}

pub struct Capture<'a> {
    source: &'a mut dyn FrameSource,
    width: u32,
    height: u32,
    scale: f64,
    settle_passes: usize,
    backend: crate::RenderBackend,
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
    /// Auto prefers Skia when enabled, otherwise wgpu. An unavailable explicit backend errors.
    pub fn renderer(mut self, backend: crate::RenderBackend) -> Self {
        self.backend = backend;
        self
    }
    pub fn render(self) -> Result<CapturedFrame, CaptureError> {
        let (width, height) = dimensions(self.width, self.height, self.scale)?;
        let mut renderer = CaptureRenderer::new(self.backend, width, height)?;
        let mut text = TextContext::new();
        renderer.configure_text(&mut text, self.scale)?;
        self.render_with(renderer, text)
    }
    pub fn write_png(self, path: impl AsRef<std::path::Path>) -> Result<(), CaptureError> {
        self.render()?.write_png(path)
    }
    pub(crate) fn render_with(
        self,
        mut renderer: CaptureRenderer,
        mut text: TextContext,
    ) -> Result<CapturedFrame, CaptureError> {
        let (width, height) = dimensions(self.width, self.height, self.scale)?;
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
                let rgba = renderer.draw(&scene, [width, height], self.source.base_color())?;
                return Ok(CapturedFrame {
                    width,
                    height,
                    rgba,
                });
            }
        }
        unreachable!("bounded settle loop always captures its final pass")
    }
}
