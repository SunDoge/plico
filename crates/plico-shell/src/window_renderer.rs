//! Window-only renderer dispatch; event/IME/layout code stays in the shared shell.
use peniko::Color;
use plico_renderer::{RenderBackend, Scene};
use std::sync::Arc;
use winit::window::Window;

pub(crate) enum WindowRenderer {
    #[cfg(feature = "wgpu")]
    Wgpu {
        active: Option<Box<plico_wgpu::WgpuWindow>>,
        transparent: bool,
    },
    #[allow(dead_code)]
    Unavailable,
}
impl WindowRenderer {
    pub(crate) fn gpu_context(&self) -> Option<plico_renderer::source::GpuContext> {
        #[cfg(feature = "wgpu")]
        if let Self::Wgpu {
            active: Some(renderer),
            ..
        } = self
        {
            return Some(renderer.gpu_context());
        }
        None
    }

    pub(crate) fn defers_initial_show(&self) -> bool {
        #[cfg(all(target_os = "macos", feature = "wgpu"))]
        {
            matches!(
                self,
                Self::Wgpu {
                    active: Some(_),
                    ..
                }
            )
        }
        #[cfg(not(all(target_os = "macos", feature = "wgpu")))]
        {
            false
        }
    }
    pub(crate) fn render_initial(
        &mut self,
        scene: &mut Scene,
        width: u32,
        height: u32,
        color: Color,
    ) -> crate::Result<bool> {
        #[cfg(feature = "wgpu")]
        if let Self::Wgpu {
            active: Some(renderer),
            ..
        } = self
        {
            renderer.resize(width, height);
            return renderer
                .present_initial(scene, color)
                .map_err(|e| crate::Error::Renderer {
                    message: e.to_string(),
                });
        }
        self.render(scene, width, height, color)
    }
    pub(crate) fn configure_text(
        &mut self,
        _text: &mut plico_renderer::TextContext,
    ) -> crate::Result<()> {
        #[cfg(feature = "wgpu")]
        if let Self::Wgpu {
            active: Some(renderer),
            ..
        } = self
        {
            renderer
                .configure_text(_text)
                .map_err(|e| crate::Error::Renderer {
                    message: e.to_string(),
                })?;
        }
        Ok(())
    }
    pub(crate) fn damage_coverage(&self, _width: u32, _height: u32) -> Option<f64> {
        match self {
            Self::Unavailable => None,
            #[cfg(feature = "wgpu")]
            Self::Wgpu {
                active: Some(renderer),
                ..
            } => Some(renderer.last_damage_coverage()),
            #[cfg(feature = "wgpu")]
            Self::Wgpu { active: None, .. } => None,
        }
    }
    /// Renderer CPU stages and GPU time for the last submitted frame.
    pub(crate) fn frame_budget(&self) -> Option<plico_renderer::FrameBudgetSample> {
        match self {
            Self::Unavailable => None,
            #[cfg(feature = "wgpu")]
            Self::Wgpu {
                active: Some(renderer),
                ..
            } => {
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
            #[cfg(feature = "wgpu")]
            Self::Wgpu { active: None, .. } => None,
        }
    }
    #[cfg_attr(not(feature = "wgpu"), allow(unused_variables))]
    pub(crate) fn new(backend: RenderBackend, transparent: bool) -> crate::Result<Self> {
        match backend {
            RenderBackend::Auto => {
                #[cfg(feature = "wgpu")]
                {
                    Self::new(RenderBackend::Wgpu, transparent)
                }
                #[cfg(not(feature = "wgpu"))]
                {
                    Err(crate::Error::Renderer {
                        message: "no renderer enabled".into(),
                    })
                }
            }
            RenderBackend::Wgpu => {
                #[cfg(feature = "wgpu")]
                {
                    Ok(Self::Wgpu {
                        active: None,
                        transparent,
                    })
                }
                #[cfg(not(feature = "wgpu"))]
                {
                    Err(crate::Error::Renderer {
                        message: "wgpu requires plico-shell's wgpu feature".into(),
                    })
                }
            }
        }
    }
    #[cfg_attr(not(feature = "wgpu"), allow(unused_variables))]
    pub(crate) fn resume(
        &mut self,
        window: Arc<dyn Window>,
        width: u32,
        height: u32,
    ) -> crate::Result<()> {
        match self {
            Self::Unavailable => {}
            #[cfg(feature = "wgpu")]
            Self::Wgpu {
                active,
                transparent,
            } => {
                let mut renderer =
                    plico_wgpu::WgpuWindow::new(window, *transparent).map_err(|e| {
                        crate::Error::Renderer {
                            message: e.to_string(),
                        }
                    })?;
                renderer.resize(width, height);
                *active = Some(Box::new(renderer));
            }
        }
        Ok(())
    }
    #[cfg_attr(not(feature = "wgpu"), allow(unused_variables))]
    pub(crate) fn resize(&mut self, width: u32, height: u32) {
        match self {
            Self::Unavailable => {}
            #[cfg(feature = "wgpu")]
            Self::Wgpu {
                active: Some(renderer),
                ..
            } => renderer.resize(width, height),
            #[cfg(feature = "wgpu")]
            Self::Wgpu { active: None, .. } => {}
        }
    }
    #[cfg(target_os = "macos")]
    pub(crate) fn set_presentation_clock(
        &mut self,
        tick: Option<(u64, Option<std::time::Instant>)>,
        interval: std::time::Duration,
    ) {
        #[cfg(feature = "wgpu")]
        if let Self::Wgpu {
            active: Some(renderer),
            ..
        } = self
        {
            renderer.set_presentation_clock(
                tick.map(|(serial, observed_at)| plico_wgpu::PresentationTick {
                    serial,
                    observed_at,
                }),
                interval,
            );
        }
        #[cfg(not(feature = "wgpu"))]
        let _ = (tick, interval);
    }
    pub(crate) fn presentation_deferred(&self) -> bool {
        #[cfg(feature = "wgpu")]
        if let Self::Wgpu {
            active: Some(renderer),
            ..
        } = self
        {
            return renderer.presentation_deferred();
        }
        false
    }
    pub(crate) fn presentation_deadline(&self) -> Option<std::time::Instant> {
        #[cfg(feature = "wgpu")]
        if let Self::Wgpu {
            active: Some(renderer),
            ..
        } = self
        {
            return renderer.presentation_deadline();
        }
        None
    }
    pub(crate) fn surface_acquire_pending(&self) -> bool {
        #[cfg(feature = "wgpu")]
        if let Self::Wgpu {
            active: Some(renderer),
            ..
        } = self
        {
            return renderer.surface_acquire_pending();
        }
        false
    }
    pub(crate) fn present_skipped(&self) -> bool {
        #[cfg(feature = "wgpu")]
        if let Self::Wgpu {
            active: Some(renderer),
            ..
        } = self
        {
            return renderer.last_present_skipped();
        }
        false
    }
    pub(crate) fn invalidate_presentation(&mut self) {
        #[cfg(feature = "wgpu")]
        if let Self::Wgpu {
            active: Some(renderer),
            ..
        } = self
        {
            renderer.invalidate_presentation();
        }
    }
    pub(crate) fn set_surface_wake(&mut self, wake: plico_renderer::WakeCallback) {
        #[cfg(feature = "wgpu")]
        if let Self::Wgpu {
            active: Some(renderer),
            ..
        } = self
        {
            renderer.set_surface_wake(wake);
            return;
        }
        let _ = wake;
    }
    pub(crate) fn take_surface_ready(&self) -> bool {
        #[cfg(feature = "wgpu")]
        if let Self::Wgpu {
            active: Some(renderer),
            ..
        } = self
        {
            return renderer.take_surface_ready();
        }
        false
    }
    pub(crate) fn prepare_hide(&mut self) {
        #[cfg(feature = "wgpu")]
        if let Self::Wgpu {
            active: Some(renderer),
            ..
        } = self
        {
            renderer.prepare_hide();
        }
    }
    pub(crate) fn maintain(&mut self, now: std::time::Instant) -> Option<std::time::Instant> {
        #[cfg(feature = "wgpu")]
        if let Self::Wgpu {
            active: Some(renderer),
            ..
        } = self
        {
            renderer.maintain(now);
            return renderer.maintenance_deadline();
        }
        let _ = now;
        None
    }
    pub(crate) fn trim_gpu_cache(&mut self) {
        #[cfg(feature = "wgpu")]
        if let Self::Wgpu {
            active: Some(renderer),
            ..
        } = self
        {
            renderer.trim_gpu_cache();
        }
    }
    #[cfg_attr(not(feature = "wgpu"), allow(unused_variables))]
    pub(crate) fn render(
        &mut self,
        scene: &mut Scene,
        width: u32,
        height: u32,
        color: Color,
    ) -> crate::Result<bool> {
        match self {
            #[cfg(feature = "wgpu")]
            Self::Wgpu {
                active: Some(renderer),
                ..
            } => {
                renderer.resize(width, height);
                renderer
                    .present(scene, color)
                    .map_err(|e| crate::Error::Renderer {
                        message: e.to_string(),
                    })
            }
            #[cfg(feature = "wgpu")]
            Self::Wgpu { active: None, .. } => Ok(false),
            Self::Unavailable => Err(crate::Error::Renderer {
                message: "no renderer enabled".into(),
            }),
        }
    }
}

impl WindowRenderer {
    pub(crate) fn capture(
        &mut self,
        scene: &Scene,
        width: u32,
        height: u32,
        color: Color,
    ) -> crate::Result<image::RgbaImage> {
        #[cfg(feature = "wgpu")]
        if let Self::Wgpu {
            active: Some(renderer),
            ..
        } = self
        {
            let mut pixels = renderer
                .capture_rgba(scene, [width, height], color)
                .map_err(|e| crate::Error::Renderer {
                    message: e.to_string(),
                })?;
            crate::renderer::unpremultiply(&mut pixels);
            return image::RgbaImage::from_raw(width, height, pixels)
                .ok_or(crate::Error::InvalidImageBuffer { width, height });
        }
        crate::renderer::render_to_image(scene, width, height, color)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn automatic_backend_prefers_enabled_renderer() {
        let result = WindowRenderer::new(RenderBackend::Auto, true);
        #[cfg(feature = "wgpu")]
        assert!(matches!(result, Ok(WindowRenderer::Wgpu { .. })));
        #[cfg(not(feature = "wgpu"))]
        assert!(result.is_err());
    }

    #[test]
    #[cfg(not(feature = "wgpu"))]
    fn explicit_wgpu_reports_missing_feature() {
        assert!(WindowRenderer::new(RenderBackend::Wgpu, false).is_err());
    }
}
