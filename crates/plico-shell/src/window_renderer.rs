//! Window-only renderer dispatch; event/IME/layout code stays in the shared shell.
use peniko::Color;
use plico_renderer::{RenderBackend, Scene};
#[cfg(feature = "skia")]
use snafu::ResultExt;
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
    #[cfg(feature = "skia")]
    Skia {
        active: Option<Box<plico_skia::SkiaWindow>>,
        transparent: bool,
    },
}
impl WindowRenderer {
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
        #[cfg(feature = "skia")]
        if let Self::Skia {
            active: Some(renderer),
            ..
        } = self
        {
            renderer.configure_text(_text);
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
            #[cfg(feature = "skia")]
            Self::Skia {
                active: Some(renderer),
                ..
            } => Some(renderer.last_damage_coverage()),
            #[cfg(feature = "skia")]
            Self::Skia { active: None, .. } => None,
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
                })
            }
            #[cfg(feature = "wgpu")]
            Self::Wgpu { active: None, .. } => None,
            #[cfg(feature = "skia")]
            Self::Skia { .. } => None,
        }
    }
    #[cfg_attr(not(any(feature = "skia", feature = "wgpu")), allow(unused_variables))]
    pub(crate) fn new(backend: RenderBackend, transparent: bool) -> crate::Result<Self> {
        match backend {
            RenderBackend::Auto => {
                #[cfg(feature = "wgpu")]
                {
                    Self::new(RenderBackend::Wgpu, transparent)
                }
                #[cfg(all(not(feature = "wgpu"), feature = "skia"))]
                {
                    Self::new(RenderBackend::Skia, transparent)
                }
                #[cfg(not(any(feature = "skia", feature = "wgpu")))]
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
    #[cfg_attr(not(any(feature = "skia", feature = "wgpu")), allow(unused_variables))]
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
    #[cfg_attr(not(any(feature = "skia", feature = "wgpu")), allow(unused_variables))]
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
            #[cfg(feature = "skia")]
            Self::Skia {
                active: Some(renderer),
                ..
            } => renderer.resize(width, height),
            #[cfg(feature = "skia")]
            Self::Skia { active: None, .. } => {}
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
        #[cfg(feature = "wgpu")]
        if let Self::Wgpu {
            active: Some(renderer),
            ..
        } = self
        {
            renderer.invalidate_presentation();
        }
        #[cfg(feature = "skia")]
        if let Self::Skia {
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
        #[cfg(feature = "skia")]
        if let Self::Skia {
            active: Some(renderer),
            ..
        } = self
        {
            renderer.trim_gpu_cache();
        }
    }
    #[cfg_attr(not(any(feature = "skia", feature = "wgpu")), allow(unused_variables))]
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
        #[cfg(all(not(feature = "wgpu"), feature = "skia"))]
        assert!(matches!(result, Ok(WindowRenderer::Skia { .. })));
        #[cfg(feature = "wgpu")]
        assert!(matches!(result, Ok(WindowRenderer::Wgpu { .. })));
        #[cfg(not(any(feature = "skia", feature = "wgpu")))]
        assert!(result.is_err());
    }

    #[test]
    #[cfg(not(feature = "wgpu"))]
    fn explicit_wgpu_reports_missing_feature() {
        assert!(WindowRenderer::new(RenderBackend::Wgpu, false).is_err());
    }

    #[test]
    #[cfg(not(feature = "skia"))]
    fn explicit_skia_reports_missing_feature() {
        assert!(WindowRenderer::new(RenderBackend::Skia, false).is_err());
    }
}
