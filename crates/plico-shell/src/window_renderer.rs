//! Window-only renderer dispatch; event/IME/layout code stays in the shared shell.
use crate::{RenderBackend, Scene};
use peniko::Color;
#[cfg(feature = "vello")]
use plico_vello as renderer_backend;
#[cfg(feature = "skia")]
use snafu::ResultExt;
use std::sync::Arc;
use winit::window::Window;

pub(crate) enum WindowRenderer {
    #[cfg(feature = "vello")]
    Vello(Box<renderer_backend::WindowRenderer>),
    #[allow(dead_code)]
    Unavailable,
    #[cfg(feature = "skia")]
    Skia {
        active: Option<Box<plico_skia::SkiaWindow>>,
        transparent: bool,
    },
    #[cfg(feature = "egui")]
    Egui {
        active: Option<Box<plico_egui::EguiWindow>>,
        transparent: bool,
    },
}
impl WindowRenderer {
    pub(crate) fn configure_text(&self, _text: &mut plico_renderer::TextContext) {
        #[cfg(feature = "skia")]
        if let Self::Skia {
            active: Some(renderer),
            ..
        } = self
        {
            renderer.configure_text(_text);
        }
    }
    pub(crate) fn build_native(
        &mut self,
        _source: &mut dyn plico_renderer::FrameSource,
        _size: [u32; 2],
        _scale: f64,
    ) -> Option<(usize, f64, f64)> {
        #[cfg(feature = "egui")]
        if let Self::Egui {
            active: Some(renderer),
            ..
        } = self
        {
            return Some(renderer.build(_source, _size, _scale).unwrap_or_default());
        }
        None
    }
    pub(crate) fn event(
        &mut self,
        _event: &plico_renderer::UiEvent,
    ) -> Option<plico_renderer::EventResponse> {
        #[cfg(feature = "egui")]
        if let Self::Egui {
            active: Some(renderer),
            ..
        } = self
        {
            return renderer.event(_event);
        }
        None
    }
    pub(crate) fn take_host_action(&mut self) -> Option<plico_renderer::HostAction> {
        #[cfg(feature = "egui")]
        if let Self::Egui {
            active: Some(renderer),
            ..
        } = self
        {
            return renderer.take_host_action();
        }
        None
    }
    pub(crate) fn ime_state(&self) -> Option<plico_renderer::ImeState> {
        #[cfg(feature = "egui")]
        if let Self::Egui {
            active: Some(renderer),
            ..
        } = self
        {
            return renderer.ime_state();
        }
        None
    }
    pub(crate) fn pointer_cursor(&self) -> Option<plico_renderer::style::CursorStyle> {
        #[cfg(feature = "egui")]
        if let Self::Egui {
            active: Some(renderer),
            ..
        } = self
        {
            return Some(renderer.pointer_cursor());
        }
        None
    }
    pub(crate) fn damage_coverage(&self, _width: u32, _height: u32) -> Option<f64> {
        match self {
            #[cfg(feature = "vello")]
            Self::Vello(_) => None,
            Self::Unavailable => None,
            #[cfg(feature = "skia")]
            Self::Skia {
                active: Some(renderer),
                ..
            } => Some(renderer.last_damage_coverage()),
            #[cfg(feature = "skia")]
            Self::Skia { active: None, .. } => None,
            #[cfg(feature = "egui")]
            Self::Egui { .. } => None,
        }
    }
    #[cfg_attr(
        not(any(feature = "vello", feature = "skia", feature = "egui")),
        allow(unused_variables)
    )]
    pub(crate) fn new(backend: RenderBackend, transparent: bool) -> crate::Result<Self> {
        match backend {
            RenderBackend::Auto => {
                #[cfg(feature = "skia")]
                {
                    Self::new(RenderBackend::Skia, transparent)
                }
                #[cfg(all(not(feature = "skia"), feature = "vello"))]
                {
                    Self::new(RenderBackend::Vello, transparent)
                }
                #[cfg(all(not(feature = "skia"), not(feature = "vello"), feature = "egui"))]
                {
                    Self::new(RenderBackend::Egui, transparent)
                }
                #[cfg(not(any(feature = "skia", feature = "vello", feature = "egui")))]
                {
                    Err(crate::Error::Renderer {
                        message: "no renderer enabled".into(),
                    })
                }
            }
            RenderBackend::Vello => {
                #[cfg(feature = "vello")]
                {
                    renderer_backend::WindowRenderer::new(transparent)
                        .map(|renderer| Self::Vello(Box::new(renderer)))
                        .map_err(|error| crate::Error::Renderer {
                            message: error.to_string(),
                        })
                }
                #[cfg(not(feature = "vello"))]
                {
                    Err(crate::Error::Renderer {
                        message: "Vello requires plico-shell's vello feature".into(),
                    })
                }
            }
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
            RenderBackend::Egui => {
                #[cfg(feature = "egui")]
                {
                    Ok(Self::Egui {
                        active: None,
                        transparent,
                    })
                }
                #[cfg(not(feature = "egui"))]
                {
                    Err(crate::Error::Renderer {
                        message: "Egui requires plico-shell's egui feature".into(),
                    })
                }
            }
        }
    }
    #[cfg_attr(
        not(any(feature = "vello", feature = "skia", feature = "egui")),
        allow(unused_variables)
    )]
    pub(crate) fn resume(
        &mut self,
        window: Arc<dyn Window>,
        width: u32,
        height: u32,
    ) -> crate::Result<()> {
        match self {
            #[cfg(feature = "vello")]
            Self::Vello(renderer) => renderer.resume(window, width, height),
            Self::Unavailable => {}
            #[cfg(feature = "egui")]
            Self::Egui {
                active,
                transparent,
            } => {
                let mut renderer =
                    plico_egui::EguiWindow::new(window, *transparent).map_err(|error| {
                        crate::Error::Renderer {
                            message: error.to_string(),
                        }
                    })?;
                renderer.resize(width, height);
                tracing::info!("egui native-widget window renderer initialized");
                *active = Some(Box::new(renderer));
            }
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
                let damage_tracking = !matches!(
                    std::env::var("PLICO_SKIA_DAMAGE").as_deref(),
                    Ok("0" | "false")
                );
                renderer.set_damage_tracking(damage_tracking);
                let multi_rect_damage = matches!(
                    std::env::var("PLICO_SKIA_MULTI_RECT").as_deref(),
                    Ok("1" | "true")
                );
                let present_skip = !matches!(
                    std::env::var("PLICO_SKIA_PRESENT_SKIP").as_deref(),
                    Ok("0" | "false")
                );
                renderer.set_multi_rect_damage(multi_rect_damage);
                renderer.set_present_skip(present_skip);
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
                    gpu_cache_limit_bytes = renderer.gpu_cache_stats().limit_bytes,
                    text_mode = if std::env::var("PLICO_SKIA_TEXT_MODE").as_deref() == Ok("mask") {
                        "mask"
                    } else {
                        "distance-field"
                    },
                    damage_tracking,
                    multi_rect_damage,
                    present_skip,
                    incremental_present_supported = renderer.incremental_present_supported(),
                    "Skia Vulkan window renderer initialized"
                );
                *active = Some(Box::new(renderer));
            }
        }
        Ok(())
    }
    #[cfg_attr(
        not(any(feature = "vello", feature = "skia", feature = "egui")),
        allow(unused_variables)
    )]
    pub(crate) fn resize(&mut self, width: u32, height: u32) {
        match self {
            #[cfg(feature = "vello")]
            Self::Vello(renderer) => renderer.resize(width, height),
            Self::Unavailable => {}
            #[cfg(feature = "skia")]
            Self::Skia {
                active: Some(renderer),
                ..
            } => renderer.resize(width, height),
            #[cfg(feature = "skia")]
            Self::Skia { active: None, .. } => {}
            #[cfg(feature = "egui")]
            Self::Egui {
                active: Some(renderer),
                ..
            } => renderer.resize(width, height),
            #[cfg(feature = "egui")]
            Self::Egui { active: None, .. } => {}
        }
    }
    pub(crate) fn present_skipped(&self) -> bool {
        #[cfg(feature = "skia")]
        if let Self::Skia {
            active: Some(renderer),
            ..
        } = self
        {
            return renderer.last_present_skipped();
        }
        false
    }
    pub(crate) fn invalidate_presentation(&mut self) {
        #[cfg(feature = "skia")]
        if let Self::Skia {
            active: Some(renderer),
            ..
        } = self
        {
            renderer.invalidate_presentation();
        }
    }
    pub(crate) fn trim_gpu_cache(&mut self) {
        #[cfg(feature = "skia")]
        if let Self::Skia {
            active: Some(renderer),
            ..
        } = self
        {
            renderer.trim_gpu_cache();
        }
    }
    #[cfg_attr(
        not(any(feature = "vello", feature = "skia", feature = "egui")),
        allow(unused_variables)
    )]
    pub(crate) fn render(
        &mut self,
        scene: &mut Scene,
        width: u32,
        height: u32,
        color: Color,
    ) -> crate::Result<bool> {
        match self {
            #[cfg(feature = "vello")]
            Self::Vello(renderer) => {
                renderer.render(scene, width, height, color);
                Ok(true)
            }
            Self::Unavailable => Err(crate::Error::Renderer {
                message: "no renderer enabled".into(),
            }),
            #[cfg(feature = "skia")]
            Self::Skia {
                active: Some(renderer),
                ..
            } => {
                let presented = renderer
                    .present(scene, color)
                    .context(crate::error::SkiaSnafu)?;
                if presented {
                    tracing::trace!(target: "plico::perf", damage_coverage = renderer.last_damage_coverage(),
                        damage_rect_count = renderer.last_damage_rect_count(),
                        present_damage_coverage = renderer.last_present_damage().coverage(width, height),
                        present_skipped = renderer.last_present_skipped(),
                        gpu_cache_bytes = renderer.gpu_cache_stats().bytes,
                        gpu_cache_purgeable_bytes = renderer.gpu_cache_stats().purgeable_bytes,
                        gpu_cache_limit_bytes = renderer.gpu_cache_stats().limit_bytes,
                        "Skia target damage");
                }
                Ok(presented)
            }
            #[cfg(feature = "skia")]
            Self::Skia { active: None, .. } => Ok(false),
            #[cfg(feature = "egui")]
            Self::Egui {
                active: Some(renderer),
                ..
            } => Ok(renderer.present(scene, color)),
            #[cfg(feature = "egui")]
            Self::Egui { active: None, .. } => Ok(false),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn automatic_backend_prefers_enabled_renderer() {
        let result = WindowRenderer::new(RenderBackend::Auto, true);
        #[cfg(feature = "skia")]
        assert!(matches!(result, Ok(WindowRenderer::Skia { .. })));
        #[cfg(all(not(feature = "skia"), feature = "vello"))]
        assert!(matches!(result, Ok(WindowRenderer::Vello(_))));
        #[cfg(all(not(feature = "skia"), not(feature = "vello"), feature = "egui"))]
        assert!(matches!(result, Ok(WindowRenderer::Egui { .. })));
        #[cfg(not(any(feature = "skia", feature = "vello", feature = "egui")))]
        assert!(result.is_err());
    }

    #[test]
    #[cfg(not(feature = "vello"))]
    fn explicit_vello_reports_missing_feature() {
        assert!(WindowRenderer::new(RenderBackend::Vello, false).is_err());
    }

    #[test]
    #[cfg(not(feature = "skia"))]
    fn explicit_skia_reports_missing_feature() {
        assert!(WindowRenderer::new(RenderBackend::Skia, false).is_err());
    }
}
