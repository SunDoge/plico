//! Window-only renderer dispatch; event/IME/layout code stays in the shared shell.
use crate::{RenderBackend, Scene, renderer_backend};
#[cfg(feature = "skia")]
use snafu::ResultExt;
use std::sync::Arc;
use vello_common::peniko::Color;
use winit::window::Window;

pub(crate) enum WindowRenderer {
    Vello(Box<renderer_backend::WindowRenderer>),
    #[cfg(feature = "skia")]
    Skia {
        active: Option<Box<plico_skia::SkiaWindow>>,
        transparent: bool,
    },
}
impl WindowRenderer {
    pub(crate) fn damage_coverage(&self, _width: u32, _height: u32) -> Option<f64> {
        match self {
            Self::Vello(_) => None,
            #[cfg(feature = "skia")]
            Self::Skia {
                active: Some(renderer),
                ..
            } => Some(renderer.last_damage().coverage(_width, _height)),
            #[cfg(feature = "skia")]
            Self::Skia { active: None, .. } => None,
        }
    }
    pub(crate) fn new(backend: RenderBackend, transparent: bool) -> crate::Result<Self> {
        match backend {
            RenderBackend::Vello => renderer_backend::WindowRenderer::new(transparent)
                .map(|renderer| Self::Vello(Box::new(renderer))),
            RenderBackend::Skia => {
                #[cfg(feature = "skia")]
                {
                    Ok(Self::Skia {
                        active: None,
                        transparent,
                    })
                }
                #[cfg(not(feature = "skia"))]
                {
                    Err(crate::Error::Renderer {
                        message: "Skia requires plico-shell's skia feature".into(),
                    })
                }
            }
        }
    }
    pub(crate) fn resume(
        &mut self,
        window: Arc<dyn Window>,
        width: u32,
        height: u32,
    ) -> crate::Result<()> {
        match self {
            Self::Vello(renderer) => renderer.resume(window, width, height),
            #[cfg(feature = "skia")]
            Self::Skia {
                active,
                transparent,
            } => {
                let device = std::env::var("PLICO_SKIA_DEVICE").ok();
                let mut renderer =
                    plico_skia::SkiaWindow::new(window, *transparent, device.as_deref())
                        .context(crate::error::SkiaSnafu)?;
                renderer.resize(width, height);
                let damage_tracking = matches!(
                    std::env::var("PLICO_SKIA_DAMAGE").as_deref(),
                    Ok("1" | "true")
                );
                renderer.set_damage_tracking(damage_tracking);
                renderer.set_damage_validation(matches!(
                    std::env::var("PLICO_SKIA_DAMAGE_VALIDATE").as_deref(),
                    Ok("1" | "true")
                ));
                renderer.set_incremental_present(matches!(
                    std::env::var("PLICO_SKIA_INCREMENTAL_PRESENT").as_deref(),
                    Ok("1" | "true")
                ));
                tracing::info!(
                    device = renderer.device_name(),
                    damage_tracking,
                    incremental_present_supported = renderer.incremental_present_supported(),
                    "Skia Vulkan window renderer initialized"
                );
                *active = Some(Box::new(renderer));
            }
        }
        Ok(())
    }
    pub(crate) fn resize(&mut self, width: u32, height: u32) {
        match self {
            Self::Vello(renderer) => renderer.resize(width, height),
            #[cfg(feature = "skia")]
            Self::Skia {
                active: Some(renderer),
                ..
            } => renderer.resize(width, height),
            #[cfg(feature = "skia")]
            Self::Skia { active: None, .. } => {}
        }
    }
    pub(crate) fn render(
        &mut self,
        scene: &mut Scene,
        width: u32,
        height: u32,
        color: Color,
    ) -> crate::Result<bool> {
        match self {
            Self::Vello(renderer) => {
                renderer.render(scene, width, height, color);
                Ok(true)
            }
            #[cfg(feature = "skia")]
            Self::Skia {
                active: Some(renderer),
                ..
            } => {
                let presented = renderer
                    .present(scene, color)
                    .context(crate::error::SkiaSnafu)?;
                if presented {
                    tracing::trace!(target: "plico::perf", damage_coverage = renderer.last_damage().coverage(width, height),
                        present_damage_coverage = renderer.last_present_damage().coverage(width, height),
                        "Skia target damage");
                }
                Ok(presented)
            }
            #[cfg(feature = "skia")]
            Self::Skia { active: None, .. } => Ok(false),
        }
    }
}
