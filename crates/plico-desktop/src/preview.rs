//! Repeatable application previews: typed fixture state, scripted input, virtual time.
use crate::{CapturedFrame, HostView, Source};
use plico::{ClockError, VirtualClock};
use plico_renderer::{FrameSource, TextContext};
use plico_runtime::testing::{Command, dispatch};
use serde::{Deserialize, Serialize};
use snafu::Snafu;
use std::{path::Path, time::Duration};

#[derive(Debug, Snafu)]
pub enum PreviewError {
    #[snafu(display("cannot read preview scenario: {source}"))]
    Io { source: std::io::Error },
    #[snafu(display("invalid preview scenario: {source}"))]
    Json { source: serde_json::Error },
    #[snafu(display("unsupported preview scenario version {version}"))]
    Version { version: u32 },
    #[snafu(display("preview input or declaration failed: {source}"))]
    Input {
        source: plico_runtime::testing::Error,
    },
    #[snafu(display("preview capture failed: {source}"))]
    Capture { source: crate::CaptureError },
    #[snafu(display("recording FPS must be between 1 and 120, got {fps}"))]
    RecordingFps { fps: u32 },
    #[snafu(display("recording output failed: {source}"))]
    RecordingIo { source: std::io::Error },
    #[snafu(display("FFmpeg recording failed with {status}"))]
    Encoder { status: std::process::ExitStatus },
}
/// App-defined fixture data. `S` can be the app itself or a small configuration
/// used to construct it; runtime handles and credentials need not be serialized.
#[derive(Debug, Serialize, Deserialize)]
#[serde(
    deny_unknown_fields,
    bound(deserialize = "S: Deserialize<'de> + Default")
)]
pub struct PreviewScenario<S> {
    #[serde(default = "version")]
    pub version: u32,
    #[serde(default)]
    pub viewport: PreviewViewport,
    #[serde(default)]
    pub state: S,
    #[serde(default)]
    pub steps: Vec<PreviewStep>,
    /// Advance by this duration after the last action, without sleeping.
    #[serde(default)]
    pub time_ms: u64,
}
fn version() -> u32 {
    1
}
impl<S: Default> Default for PreviewScenario<S> {
    fn default() -> Self {
        Self {
            version: 1,
            viewport: Default::default(),
            state: S::default(),
            steps: vec![],
            time_ms: 0,
        }
    }
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct PreviewViewport {
    pub width: u32,
    pub height: u32,
    pub scale: f64,
}
impl Default for PreviewViewport {
    fn default() -> Self {
        Self {
            width: 1120,
            height: 820,
            scale: 1.,
        }
    }
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PreviewStep {
    #[serde(default)]
    pub after_ms: u64,
    pub command: Command,
}
impl<S: serde::de::DeserializeOwned + Default> PreviewScenario<S> {
    /// Fixture asset paths are interpreted by the application's factory. The
    /// factory chooses whether they are project-relative or scenario-relative.
    pub fn load(path: impl AsRef<Path>) -> Result<Self, PreviewError> {
        let bytes = std::fs::read(path).map_err(|source| PreviewError::Io { source })?;
        serde_json::from_slice(&bytes).map_err(|source| PreviewError::Json { source })
    }
}
/// A statically linked UI source, optionally hosting a native widget surface.
pub trait PreviewSource: FrameSource {
    type View: HostView;
    fn ui_source(&mut self) -> &mut Source<Self::View>;
}
impl<V: HostView> PreviewSource for Source<V> {
    type View = V;
    fn ui_source(&mut self) -> &mut Source<V> {
        self
    }
}
impl<V: HostView, W: plico_renderer::Widget> PreviewSource for plico_runtime::SurfaceSource<V, W> {
    type View = V;
    fn ui_source(&mut self) -> &mut Source<V> {
        &mut self.source
    }
}
/// Reuses backend resources across recipes, never application or input state.
/// Each render still constructs a fresh Source, TextContext and VirtualClock.
/// This is a bounded batch helper, not a live application session.
pub struct PreviewSession {
    backend: crate::RenderBackend,
    renderer: Option<crate::capture::CaptureRenderer>,
    #[cfg(feature = "wgpu")]
    force_general_pipelines: bool,
    #[cfg(feature = "wgpu")]
    force_no_occlusion_culling: bool,
    #[cfg(feature = "wgpu")]
    force_no_ring_splitting: bool,
}
impl PreviewSession {
    pub fn new(backend: crate::RenderBackend) -> Self {
        Self {
            backend,
            renderer: None,
            #[cfg(feature = "wgpu")]
            force_general_pipelines: false,
            #[cfg(feature = "wgpu")]
            force_no_occlusion_culling: false,
            #[cfg(feature = "wgpu")]
            force_no_ring_splitting: false,
        }
    }
    /// Compare captures with every quad class drawn by the general shader.
    #[cfg(feature = "wgpu")]
    pub fn force_general_pipelines(&mut self, force: bool) {
        self.force_general_pipelines = force;
        if let Some(renderer) = &mut self.renderer {
            renderer.force_general_pipelines(force);
        }
    }
    /// Disable conservative painter-order culling for pixel/performance A/Bs.
    #[cfg(feature = "wgpu")]
    pub fn force_no_occlusion_culling(&mut self, force: bool) {
        self.force_no_occlusion_culling = force;
        if let Some(renderer) = &mut self.renderer {
            renderer.force_no_occlusion_culling(force);
        }
    }
    /// Disable large border-only scissor splitting for pixel/performance A/Bs.
    #[cfg(feature = "wgpu")]
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
            .and_then(|renderer| renderer.frame_budget())
    }
    /// Renderer batching and specialized-pipeline counters from the latest capture.
    #[cfg(feature = "wgpu")]
    pub fn last_batch_stats(&self) -> Option<plico_wgpu::FrameBatchStats> {
        self.renderer
            .as_ref()
            .and_then(|renderer| renderer.batch_stats())
    }
    /// Name reported by the active GPU adapter, for reproducible profiling.
    pub fn adapter_name(&self) -> Option<&str> {
        self.renderer
            .as_ref()
            .and_then(|renderer| renderer.adapter_name())
    }
    pub(crate) fn renderer(
        &mut self,
        width: u32,
        height: u32,
    ) -> Result<&mut crate::capture::CaptureRenderer, crate::CaptureError> {
        if self.renderer.is_none() {
            let renderer = crate::capture::CaptureRenderer::new(self.backend, width, height)?;
            #[cfg(feature = "wgpu")]
            let mut renderer = renderer;
            #[cfg(feature = "wgpu")]
            renderer.force_general_pipelines(self.force_general_pipelines);
            #[cfg(feature = "wgpu")]
            renderer.force_no_occlusion_culling(self.force_no_occlusion_culling);
            #[cfg(feature = "wgpu")]
            renderer.force_no_ring_splitting(self.force_no_ring_splitting);
            self.renderer = Some(renderer);
        }
        Ok(self.renderer.as_mut().unwrap())
    }
}
impl<S> PreviewScenario<S> {
    pub fn render<V: HostView>(
        self,
        create: impl FnOnce(S) -> V,
    ) -> Result<CapturedFrame, PreviewError> {
        self.render_with_backend(crate::RenderBackend::Auto, create)
    }
    /// Select the same backend for scenario layout, input geometry and final capture.
    pub fn render_with_backend<V: HostView>(
        self,
        backend: crate::RenderBackend,
        create: impl FnOnce(S) -> V,
    ) -> Result<CapturedFrame, PreviewError> {
        self.render_source_with_backend(backend, |state, clock| {
            Source::with_clock(create(state), clock)
        })
    }
    /// Native surfaces use the supplied clock when constructing their source.
    /// Scripted commands target Plico controls; native widget internals remain
    /// the widget's own testing contract.
    pub fn render_source<F: PreviewSource>(
        self,
        create: impl FnOnce(S, VirtualClock) -> F,
    ) -> Result<CapturedFrame, PreviewError> {
        self.render_source_with_backend(crate::RenderBackend::Auto, create)
    }
    pub fn render_source_with_backend<F: PreviewSource>(
        self,
        backend: crate::RenderBackend,
        create: impl FnOnce(S, VirtualClock) -> F,
    ) -> Result<CapturedFrame, PreviewError> {
        self.render_source_in(&mut PreviewSession::new(backend), create)
    }
    pub fn render_in<V: HostView>(
        self,
        session: &mut PreviewSession,
        create: impl FnOnce(S) -> V,
    ) -> Result<CapturedFrame, PreviewError> {
        self.render_source_in(session, |state, clock| {
            Source::with_clock(create(state), clock)
        })
    }
    pub fn render_source_in<F: PreviewSource>(
        self,
        session: &mut PreviewSession,
        create: impl FnOnce(S, VirtualClock) -> F,
    ) -> Result<CapturedFrame, PreviewError> {
        let (width, height) = self.physical_size()?;
        let v = self.viewport;
        let clock = VirtualClock::default();
        let mut source = create(self.state, clock.clone());
        let mut text = TextContext::new();
        let renderer = session
            .renderer(width, height)
            .map_err(|source| PreviewError::Capture { source })?;
        renderer
            .configure_text(&mut text, v.scale)
            .map_err(|source| PreviewError::Capture { source })?;
        build_input_frame(&mut source, &mut text, v)?;
        for step in self.steps {
            clock
                .advance(Duration::from_millis(step.after_ms))
                .map_err(clock_error)?;
            apply_step(&mut source, &mut text, v, &step.command)?;
        }
        clock
            .advance(Duration::from_millis(self.time_ms))
            .map_err(clock_error)?;
        build_input_frame(&mut source, &mut text, v)?;
        let image = crate::capture(&mut source)
            .size(v.width, v.height)
            .scale(v.scale)
            .render_with_borrowed(renderer, &mut text)
            .map_err(|source| PreviewError::Capture { source })?;
        Ok(image)
    }
    /// Sample a script at a fixed frame rate and send its premultiplied frames
    /// to a caller-owned sink. Commands run on their exact virtual timestamps.
    pub fn record_frames<V: HostView>(
        self,
        fps: u32,
        create: impl FnOnce(S) -> V,
        emit: impl FnMut(Duration, CapturedFrame) -> Result<(), PreviewError>,
    ) -> Result<(), PreviewError> {
        self.record_source_frames_in(
            &mut PreviewSession::new(crate::RenderBackend::Auto),
            fps,
            |state, clock| Source::with_clock(create(state), clock),
            emit,
        )
    }

    /// The fixed-rate recorder for callers that own a renderer session or native surface.
    pub fn record_source_frames_in<F: PreviewSource>(
        self,
        session: &mut PreviewSession,
        fps: u32,
        create: impl FnOnce(S, VirtualClock) -> F,
        mut emit: impl FnMut(Duration, CapturedFrame) -> Result<(), PreviewError>,
    ) -> Result<(), PreviewError> {
        validate_fps(fps)?;
        let (width, height) = self.physical_size()?;
        let viewport = self.viewport;
        let end_ms = self
            .steps
            .iter()
            .try_fold(self.time_ms, |elapsed, step| {
                elapsed.checked_add(step.after_ms)
            })
            .ok_or_else(timeline_overflow)?;
        let last_frame = (u128::from(end_ms) * u128::from(fps)).div_ceil(1000);
        let clock = VirtualClock::default();
        let mut source = create(self.state, clock.clone());
        let mut text = TextContext::new();
        let renderer = session
            .renderer(width, height)
            .map_err(|source| PreviewError::Capture { source })?;
        renderer
            .configure_text(&mut text, viewport.scale)
            .map_err(|source| PreviewError::Capture { source })?;
        let mut steps = self.steps.into_iter().peekable();
        let mut step_time_ms = 0;
        for index in 0..=last_frame {
            let ns = index * 1_000_000_000 / u128::from(fps);
            let timestamp = Duration::new((ns / 1_000_000_000) as u64, (ns % 1_000_000_000) as u32);
            while let Some(step) = steps.peek() {
                let next = step_time_ms + step.after_ms;
                let action_time = Duration::from_millis(next);
                if action_time > timestamp {
                    break;
                }
                clock.set(action_time).map_err(clock_error)?;
                let step = steps.next().unwrap();
                apply_step(&mut source, &mut text, viewport, &step.command)?;
                step_time_ms = next;
            }
            // Keep the final frame exactly at the scenario's end, even if
            // the encoder's final frame index falls just beyond it.
            let time = timestamp.min(Duration::from_millis(end_ms));
            clock.set(time).map_err(clock_error)?;
            let image = crate::capture(&mut source)
                .size(viewport.width, viewport.height)
                .scale(viewport.scale)
                .render_with_borrowed(renderer, &mut text)
                .map_err(|source| PreviewError::Capture { source })?;
            emit(time, image)?;
        }
        Ok(())
    }

    /// Encode a scripted recording to H.264 MP4 with the installed FFmpeg.
    /// A failed script or encoder leaves an existing destination intact.
    pub fn record_mp4<V: HostView>(
        self,
        output: impl AsRef<Path>,
        fps: u32,
        create: impl FnOnce(S) -> V,
    ) -> Result<(), PreviewError> {
        use std::io::Write;
        use std::process::{Command as Process, Stdio};
        validate_fps(fps)?;
        let (width, height) = self.physical_size()?;
        let output = output.as_ref();
        let directory = output
            .parent()
            .filter(|path| !path.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        let temporary = tempfile::Builder::new()
            .suffix(".mp4")
            .tempfile_in(directory)
            .map_err(recording_io)?;
        let mut encoder = Process::new("ffmpeg")
            .args([
                "-hide_banner",
                "-loglevel",
                "error",
                "-y",
                "-f",
                "rawvideo",
                "-pixel_format",
                "rgba",
            ])
            .arg("-video_size")
            .arg(format!("{width}x{height}"))
            .arg("-framerate")
            .arg(fps.to_string())
            .args([
                "-i",
                "pipe:0",
                "-vf",
                "pad=ceil(iw/2)*2:ceil(ih/2)*2",
                "-c:v",
                "libx264",
                "-preset",
                "fast",
                "-crf",
                "18",
                "-pix_fmt",
                "yuv420p",
                "-movflags",
                "+faststart",
            ])
            .arg(temporary.path())
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .spawn()
            .map_err(recording_io)?;
        let mut stdin = encoder.stdin.take().expect("FFmpeg has piped stdin");
        let result = self.record_frames(fps, create, |_, frame| {
            stdin.write_all(frame.rgba()).map_err(recording_io)
        });
        drop(stdin);
        if let Err(error) = result {
            let _ = encoder.kill();
            let _ = encoder.wait();
            return Err(error);
        }
        let status = encoder.wait().map_err(recording_io)?;
        if !status.success() {
            return Err(PreviewError::Encoder { status });
        }
        temporary
            .persist(output)
            .map_err(|error| recording_io(error.error))?;
        Ok(())
    }

    fn physical_size(&self) -> Result<(u32, u32), PreviewError> {
        if self.version != 1 {
            return Err(PreviewError::Version {
                version: self.version,
            });
        }
        let v = self.viewport;
        if v.width == 0
            || v.height == 0
            || !v.scale.is_finite()
            || v.scale <= 0.
            || v.width as f64 * v.scale > 8192.
            || v.height as f64 * v.scale > 8192.
        {
            return Err(PreviewError::Capture {
                source: crate::CaptureError::InvalidOptions,
            });
        }
        crate::capture::dimensions(v.width, v.height, v.scale)
            .map_err(|source| PreviewError::Capture { source })
    }
}

fn validate_fps(fps: u32) -> Result<(), PreviewError> {
    if !(1..=120).contains(&fps) {
        return Err(PreviewError::RecordingFps { fps });
    }
    Ok(())
}

fn recording_io(source: std::io::Error) -> PreviewError {
    PreviewError::RecordingIo { source }
}

fn clock_error(_: ClockError) -> PreviewError {
    timeline_overflow()
}

fn timeline_overflow() -> PreviewError {
    PreviewError::Input {
        source: plico_runtime::testing::Error::Clock,
    }
}

fn build_input_frame<F: PreviewSource>(
    source: &mut F,
    text: &mut TextContext,
    viewport: PreviewViewport,
) -> Result<(), PreviewError> {
    source
        .ui_source()
        .try_build_frame(text, viewport.width, viewport.height)
        .map_err(|error| PreviewError::Input {
            source: plico_runtime::testing::Error::Declaration {
                message: error.to_string(),
            },
        })?;
    Ok(())
}

fn apply_step<F: PreviewSource>(
    source: &mut F,
    text: &mut TextContext,
    viewport: PreviewViewport,
    command: &Command,
) -> Result<(), PreviewError> {
    build_input_frame(source, text, viewport)?;
    dispatch(source.ui_source(), text, command).map_err(|source| PreviewError::Input { source })?;
    build_input_frame(source, text, viewport)
}
