//! Repeatable application previews: typed fixture state, scripted input, virtual time.
use crate::{CapturedFrame, HostView, Source};
use plico::VirtualClock;
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
        let clock = VirtualClock::default();
        let mut source = create(self.state, clock.clone());
        let mut text = TextContext::new();
        let (width, height) = crate::capture::dimensions(v.width, v.height, v.scale)
            .map_err(|source| PreviewError::Capture { source })?;
        let mut renderer = crate::capture::CaptureRenderer::new(backend, width, height)
            .map_err(|source| PreviewError::Capture { source })?;
        renderer
            .configure_text(&mut text, v.scale)
            .map_err(|source| PreviewError::Capture { source })?;
        let frame = |source: &mut F, text: &mut TextContext| -> Result<(), PreviewError> {
            source.build_frame(text, v.width, v.height);
            if let Some(error) = &source.ui_source().last_error {
                return Err(PreviewError::Input {
                    source: plico_runtime::testing::Error::Declaration {
                        message: error.to_string(),
                    },
                });
            }
            Ok(())
        };
        frame(&mut source, &mut text)?;
        for step in self.steps {
            clock
                .advance(Duration::from_millis(step.after_ms))
                .map_err(|_| PreviewError::Input {
                    source: plico_runtime::testing::Error::Clock,
                })?;
            frame(&mut source, &mut text)?;
            dispatch(source.ui_source(), &mut text, &step.command)
                .map_err(|source| PreviewError::Input { source })?;
            frame(&mut source, &mut text)?;
        }
        clock
            .advance(Duration::from_millis(self.time_ms))
            .map_err(|_| PreviewError::Input {
                source: plico_runtime::testing::Error::Clock,
            })?;
        frame(&mut source, &mut text)?;
        let image = crate::capture(&mut source)
            .size(v.width, v.height)
            .scale(v.scale)
            .render_with(renderer, text)
            .map_err(|source| PreviewError::Capture { source })?;
        if let Some(error) = &source.ui_source().last_error {
            return Err(PreviewError::Input {
                source: plico_runtime::testing::Error::Declaration {
                    message: error.to_string(),
                },
            });
        }
        Ok(image)
    }
}
