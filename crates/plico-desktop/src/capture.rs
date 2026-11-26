//! Offscreen capture for tests, documentation, and offline exports.
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

    /// Construct a frame from renderer-native premultiplied RGBA8 pixels.
    pub fn from_premultiplied_rgba(
        width: u32,
        height: u32,
        rgba: Vec<u8>,
    ) -> Result<Self, CaptureError> {
        let expected = usize::try_from(width)
            .ok()
            .and_then(|width| {
                usize::try_from(height)
                    .ok()
                    .and_then(|height| width.checked_mul(height))
            })
            .and_then(|pixels| pixels.checked_mul(4));
        if width == 0 || height == 0 || expected != Some(rgba.len()) {
            return Err(CaptureError::InvalidOptions);
        }
        Ok(Self {
            width,
            height,
            rgba,
        })
    }
}

#[derive(Debug, Snafu)]
pub enum CaptureError {
    #[snafu(display("capture dimensions must be nonzero and scale must be finite and positive"))]
    InvalidOptions,
    #[snafu(display("PNG encoding failed: {source}"))]
    Image { source: image::ImageError },
    #[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
    #[snafu(display("wgpu capture is unavailable on this platform"))]
    UnsupportedPlatform,
    #[snafu(display("capture frame failed: {source}"))]
    Frame {
        source: plico_renderer::FrameSourceError,
    },
    #[snafu(display("wgpu capture failed: {source}"))]
    Wgpu { source: plico_wgpu::Error },
}

impl From<plico_wgpu::Error> for CaptureError {
    fn from(source: plico_wgpu::Error) -> Self {
        Self::Wgpu { source }
    }
}

pub(crate) fn configure_text(
    renderer: &mut plico_wgpu::Renderer,
    text: &mut TextContext,
    scale: f64,
) -> Result<(), CaptureError> {
    #[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
    {
        renderer.configure_text(text, scale).map_err(Into::into)
    }
    #[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
    {
        let _ = (renderer, text, scale);
        Err(CaptureError::UnsupportedPlatform)
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

/// Logical capture extent, render scale and bounded async polling budget.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CaptureOptions {
    pub width: u32,
    pub height: u32,
    pub scale: f64,
    /// Extra passes after the initial frame. Zero captures the first frame.
    pub settle_passes: usize,
}

impl CaptureOptions {
    pub const fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            scale: 1.,
            settle_passes: 8,
        }
    }
}

impl Default for CaptureOptions {
    fn default() -> Self {
        Self::new(800, 600)
    }
}

/// Render one offscreen frame. For a sequence, reuse a [`CaptureSession`].
pub fn capture(
    source: &mut dyn FrameSource,
    options: CaptureOptions,
) -> Result<CapturedFrame, CaptureError> {
    CaptureSession::default().render(source, options)
}

/// Reuse GPU pipelines and offscreen resources across frames. Low-level
/// captures also retain text caches. Scripted previews create their own
/// source, clock and text context for each scenario.
#[derive(Default)]
pub struct CaptureSession {
    renderer: Option<Box<plico_wgpu::Renderer>>,
    text: Option<(f64, TextContext)>,
    force_general_pipelines: bool,
    force_no_occlusion_culling: bool,
    force_no_ring_splitting: bool,
}
impl CaptureSession {
    /// Compare captures with every quad class drawn by the general shader.
    pub fn force_general_pipelines(&mut self, force: bool) {
        self.force_general_pipelines = force;
        if let Some(renderer) = &mut self.renderer {
            renderer.force_general_pipelines(force);
        }
    }
    /// Disable conservative painter-order culling for pixel/performance A/Bs.
    pub fn force_no_occlusion_culling(&mut self, force: bool) {
        self.force_no_occlusion_culling = force;
        if let Some(renderer) = &mut self.renderer {
            renderer.force_no_occlusion_culling(force);
        }
    }
    /// Disable large border-only scissor splitting for pixel/performance A/Bs.
    pub fn force_no_ring_splitting(&mut self, force: bool) {
        self.force_no_ring_splitting = force;
        if let Some(renderer) = &mut self.renderer {
            renderer.force_no_ring_splitting(force);
        }
    }
    /// Renderer CPU stages and GPU time from the most recent capture.
    pub fn last_frame_budget(&self) -> Option<plico_renderer::FrameBudgetSample> {
        self.renderer
            .as_ref()
            .map(|renderer| renderer.frame_budget())
    }
    /// Renderer batching and specialized-pipeline counters from the latest capture.
    pub fn last_batch_stats(&self) -> Option<plico_wgpu::FrameBatchStats> {
        self.renderer
            .as_ref()
            .map(|renderer| renderer.batch_stats())
    }
    /// Name reported by the active GPU adapter, for reproducible profiling.
    pub fn adapter_name(&self) -> Option<&str> {
        self.renderer
            .as_ref()
            .map(|renderer| renderer.adapter_info().name.as_str())
    }
    pub(crate) fn renderer(&mut self) -> Result<&mut plico_wgpu::Renderer, CaptureError> {
        if self.renderer.is_none() {
            let mut renderer = Box::new(pollster::block_on(plico_wgpu::Renderer::new())?);
            renderer.force_general_pipelines(self.force_general_pipelines);
            renderer.force_no_occlusion_culling(self.force_no_occlusion_culling);
            renderer.force_no_ring_splitting(self.force_no_ring_splitting);
            self.renderer = Some(renderer);
        }
        Ok(self.renderer.as_deref_mut().unwrap())
    }
    pub fn render(
        &mut self,
        source: &mut dyn FrameSource,
        options: CaptureOptions,
    ) -> Result<CapturedFrame, CaptureError> {
        self.render_profiled(source, options)
            .map(|(frame, _)| frame)
    }

    pub fn render_profiled(
        &mut self,
        source: &mut dyn FrameSource,
        options: CaptureOptions,
    ) -> Result<(CapturedFrame, CaptureProfile), CaptureError> {
        dimensions(options.width, options.height, options.scale)?;
        self.renderer()?;
        let renderer = self.renderer.as_deref_mut().unwrap();
        let (scale, text) = self.text.get_or_insert_with(|| (0., TextContext::new()));
        if *scale != options.scale {
            configure_text(renderer, text, options.scale)?;
            *scale = options.scale;
        }
        render_with_text(source, renderer, text, options)
    }
}

pub(crate) fn render_with_text(
    source: &mut dyn FrameSource,
    renderer: &mut plico_wgpu::Renderer,
    text: &mut TextContext,
    options: CaptureOptions,
) -> Result<(CapturedFrame, CaptureProfile), CaptureError> {
    let (width, height) = dimensions(options.width, options.height, options.scale)?;
    for pass in 0..=options.settle_passes {
        let mut scene = Scene::new();
        let measured = source
            .render_frame(
                &mut scene,
                text,
                (options.width, options.height),
                options.scale,
            )
            .map_err(|source| CaptureError::Frame { source })?;
        if pass == options.settle_passes || !source.poll_async() {
            let draw_started = Instant::now();
            let rgba = renderer.render_scene(
                &scene,
                [width, height],
                source.base_color().components,
                false,
            )?;
            let mut source_stats = plico_renderer::FrameStats::default();
            source.augment_frame_stats(&mut source_stats);
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
                renderer: Some(renderer.frame_budget()),
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
