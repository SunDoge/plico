//! Backend-selected offscreen capture for tests, documentation, and offline exports.
use plico_renderer::{FrameSource, Scene, TextContext};
use snafu::Snafu;
use std::time::Instant;

/// Exact CPU and renderer stages for the final pass of an offscreen capture.
///
/// Unlike the window HUD this is a raw sample, not an exponential average.
/// PNG encoding is deliberately excluded.
#[derive(Debug, Clone, Copy, Default, PartialEq, serde::Serialize)]
pub struct CaptureProfile {
    /// Retained nodes participating in the captured frame.
    pub node_count: usize,
    /// Application declaration, retained update and layout time.
    pub build_frame_ms: f64,
    /// Application declaration time; zero when retained declarations were reused.
    pub declaration_ms: f64,
    /// Retained-node synchronization time.
    pub sync_ms: f64,
    /// Layout, flatten and retained presentation-update time.
    pub backend_ms: f64,
    /// Declaration settle passes in the captured frame.
    pub declaration_passes: u64,
    /// Taffy layout runs in the captured frame.
    pub layout_runs: u64,
    /// Retained nodes visited by flattening in the captured frame.
    pub flatten_visits: u64,
    /// Backend-neutral paint scene assembly time.
    pub scene_ms: f64,
    /// Commands in the backend-neutral captured scene.
    pub scene_commands: u64,
    /// Whether the complete backend-neutral Scene was reused.
    pub scene_reused: bool,
    /// Renderer draw and offscreen readback wall time.
    pub draw_ms: f64,
    /// Backend renderer stages when the selected backend exposes them.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub renderer: Option<plico_renderer::FrameBudgetSample>,
}

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
    #[snafu(display("capture frame failed: {source}"))]
    Frame {
        source: plico_renderer::FrameSourceError,
    },
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
    #[cfg(feature = "wgpu")]
    pub(crate) fn force_general_pipelines(&mut self, force: bool) {
        #[allow(irrefutable_let_patterns)]
        if let Self::Wgpu(renderer) = self {
            renderer.force_general_pipelines(force);
        }
    }
    #[cfg(feature = "wgpu")]
    pub(crate) fn force_no_occlusion_culling(&mut self, force: bool) {
        #[allow(irrefutable_let_patterns)]
        if let Self::Wgpu(renderer) = self {
            renderer.force_no_occlusion_culling(force);
        }
    }
    #[cfg(feature = "wgpu")]
    pub(crate) fn force_no_ring_splitting(&mut self, force: bool) {
        #[allow(irrefutable_let_patterns)]
        if let Self::Wgpu(renderer) = self {
            renderer.force_no_ring_splitting(force);
        }
    }
    #[cfg(feature = "wgpu")]
    pub(crate) fn batch_stats(&self) -> Option<plico_wgpu::FrameBatchStats> {
        match self {
            #[cfg(feature = "wgpu")]
            Self::Wgpu(renderer) => Some(renderer.batch_stats()),
            #[allow(unreachable_patterns)]
            _ => None,
        }
    }
    pub(crate) fn adapter_name(&self) -> Option<&str> {
        match self {
            #[cfg(feature = "skia")]
            Self::Skia(_) => None,
            #[cfg(feature = "wgpu")]
            Self::Wgpu(renderer) => Some(renderer.adapter_info().name.as_str()),
        }
    }

    pub(crate) fn new(
        backend: crate::RenderBackend,
        width: u32,
        height: u32,
    ) -> Result<Self, CaptureError> {
        let backend = if matches!(backend, crate::RenderBackend::Auto) {
            if cfg!(feature = "wgpu") {
                crate::RenderBackend::Wgpu
            } else {
                crate::RenderBackend::Skia
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
                #[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
                {
                    renderer.configure_text(text, scale).map_err(Into::into)
                }
                #[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
                {
                    let _ = (renderer, text);
                    Err(CaptureError::Backend {
                        backend: crate::RenderBackend::Wgpu,
                    })
                }
            }
        }
    }
    /// Renderer stages and GPU time for the last drawn frame, when reported.
    pub(crate) fn frame_budget(&self) -> Option<plico_renderer::FrameBudgetSample> {
        match self {
            #[cfg(feature = "skia")]
            Self::Skia(_) => None,
            #[cfg(feature = "wgpu")]
            Self::Wgpu(renderer) => {
                let budget = renderer.frame_budget();
                Some(plico_renderer::FrameBudgetSample {
                    prepare_ms: budget.prepare_ms,
                    upload_ms: budget.upload_ms,
                    encode_ms: budget.encode_ms,
                    submit_ms: budget.submit_ms,
                    submissions: budget.submissions,
                    command_buffers: budget.command_buffers,
                    wait_ms: budget.wait_ms,
                    gpu_ms: budget.gpu_ms,
                    upload_bytes: budget.upload_bytes,
                    instances: budget.instances,
                    draw_calls: budget.draw_calls,
                    passes: budget.passes,
                    layer_bytes: budget.layer_bytes,
                    effect_bytes: budget.effect_bytes,
                    backdrop_copies: budget.backdrop_copies,
                    backdrop_mip_passes: budget.backdrop_mip_passes,
                    backdrop_bytes: budget.backdrop_bytes,
                    backdrop_textures: budget.backdrop_textures,
                    backdrop_shared_reads: budget.backdrop_shared_reads,
                    backdrop_effect_pixels: budget.backdrop_effect_pixels,
                })
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
    /// Auto prefers wgpu when enabled, otherwise Skia. An unavailable explicit backend errors.
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
        self.render_with_borrowed(&mut renderer, &mut text)
    }
    pub(crate) fn render_with_borrowed(
        self,
        renderer: &mut CaptureRenderer,
        text: &mut TextContext,
    ) -> Result<CapturedFrame, CaptureError> {
        self.render_profiled_with_borrowed(renderer, text)
            .map(|(frame, _)| frame)
    }
    pub(crate) fn render_profiled_with_borrowed(
        self,
        renderer: &mut CaptureRenderer,
        text: &mut TextContext,
    ) -> Result<(CapturedFrame, CaptureProfile), CaptureError> {
        let (width, height) = dimensions(self.width, self.height, self.scale)?;
        for pass in 0..=self.settle_passes {
            let mut scene = Scene::new();
            let measured = if let Some(measured) = self
                .source
                .render_retained_frame(&mut scene, text, (self.width, self.height), self.scale)
                .map_err(|source| CaptureError::Frame { source })?
            {
                measured
            } else {
                self.source
                    .build_and_paint(&mut scene, text, self.width, self.height, self.scale)
                    .map_err(|source| CaptureError::Frame { source })?
            };
            if pass == self.settle_passes || !self.source.poll_async() {
                let draw_started = Instant::now();
                let rgba = renderer.draw(&scene, [width, height], self.source.base_color())?;
                let mut source_stats = plico_renderer::FrameStats::default();
                self.source.augment_frame_stats(&mut source_stats);
                let profile = CaptureProfile {
                    node_count: measured.0,
                    build_frame_ms: measured.1,
                    declaration_ms: source_stats.sample_declaration_ms,
                    sync_ms: source_stats.sample_sync_ms,
                    backend_ms: source_stats.sample_backend_ms,
                    declaration_passes: source_stats.declaration_passes,
                    layout_runs: source_stats.layout_runs,
                    flatten_visits: source_stats.flatten_visits,
                    scene_ms: measured.2,
                    scene_commands: scene.commands.len() as u64,
                    scene_reused: source_stats.scene_reused,
                    draw_ms: draw_started.elapsed().as_secs_f64() * 1_000.,
                    renderer: renderer.frame_budget(),
                };
                return Ok((
                    CapturedFrame {
                        width,
                        height,
                        rgba,
                    },
                    profile,
                ));
            }
        }
        unreachable!("bounded settle loop always captures its final pass")
    }
}
