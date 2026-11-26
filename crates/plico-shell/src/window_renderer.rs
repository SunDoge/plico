//! Own the wgpu surface attached to a native window.
use peniko::Color;
use plico_renderer::{FrameBudgetSample, Scene, TextContext, WakeCallback, source::GpuContext};
use std::{sync::Arc, time::Instant};
use winit::window::Window;

pub(crate) struct WindowRenderer {
    #[cfg(feature = "wgpu")]
    surface: Box<plico_wgpu::WgpuWindow>,
}

#[cfg_attr(not(feature = "wgpu"), allow(unused_variables))]
impl WindowRenderer {
    pub(crate) fn new(
        window: Arc<dyn Window>,
        transparent: bool,
        width: u32,
        height: u32,
    ) -> crate::Result<Self> {
        #[cfg(feature = "wgpu")]
        {
            let mut surface =
                plico_wgpu::WgpuWindow::new(window, transparent).map_err(renderer_error)?;
            surface.resize(width, height);
            Ok(Self {
                surface: Box::new(surface),
            })
        }
        #[cfg(not(feature = "wgpu"))]
        Err(wgpu_required())
    }

    pub(crate) fn gpu_context(&self) -> Option<GpuContext> {
        #[cfg(feature = "wgpu")]
        {
            Some(self.surface.gpu_context())
        }
        #[cfg(not(feature = "wgpu"))]
        None
    }

    pub(crate) fn defers_initial_show(&self) -> bool {
        cfg!(all(target_os = "macos", feature = "wgpu"))
    }

    pub(crate) fn configure_text(&mut self, text: &mut TextContext) -> crate::Result<()> {
        #[cfg(feature = "wgpu")]
        {
            self.surface.configure_text(text).map_err(renderer_error)
        }
        #[cfg(not(feature = "wgpu"))]
        Err(wgpu_required())
    }

    pub(crate) fn damage_coverage(&self) -> Option<f64> {
        #[cfg(feature = "wgpu")]
        {
            Some(self.surface.last_damage_coverage())
        }
        #[cfg(not(feature = "wgpu"))]
        None
    }

    pub(crate) fn frame_budget(&self) -> Option<FrameBudgetSample> {
        #[cfg(feature = "wgpu")]
        {
            Some(self.surface.frame_budget())
        }
        #[cfg(not(feature = "wgpu"))]
        None
    }

    pub(crate) fn resize(&mut self, width: u32, height: u32) {
        #[cfg(feature = "wgpu")]
        self.surface.resize(width, height);
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn set_presentation_clock(
        &mut self,
        tick: Option<(u64, Option<Instant>)>,
        interval: std::time::Duration,
    ) {
        #[cfg(feature = "wgpu")]
        self.surface.set_presentation_clock(
            tick.map(|(serial, observed_at)| plico_wgpu::PresentationTick {
                serial,
                observed_at,
            }),
            interval,
        );
    }

    pub(crate) fn presentation_deferred(&self) -> bool {
        #[cfg(feature = "wgpu")]
        {
            self.surface.presentation_deferred()
        }
        #[cfg(not(feature = "wgpu"))]
        false
    }

    pub(crate) fn presentation_deadline(&self) -> Option<Instant> {
        #[cfg(feature = "wgpu")]
        {
            self.surface.presentation_deadline()
        }
        #[cfg(not(feature = "wgpu"))]
        None
    }

    pub(crate) fn surface_acquire_pending(&self) -> bool {
        #[cfg(feature = "wgpu")]
        {
            self.surface.surface_acquire_pending()
        }
        #[cfg(not(feature = "wgpu"))]
        false
    }

    pub(crate) fn present_skipped(&self) -> bool {
        #[cfg(feature = "wgpu")]
        {
            self.surface.last_present_skipped()
        }
        #[cfg(not(feature = "wgpu"))]
        false
    }

    pub(crate) fn invalidate_presentation(&mut self) {
        #[cfg(feature = "wgpu")]
        self.surface.invalidate_presentation();
    }

    pub(crate) fn set_surface_wake(&mut self, wake: WakeCallback) {
        #[cfg(feature = "wgpu")]
        self.surface.set_surface_wake(wake);
    }

    pub(crate) fn take_surface_ready(&self) -> bool {
        #[cfg(feature = "wgpu")]
        {
            self.surface.take_surface_ready()
        }
        #[cfg(not(feature = "wgpu"))]
        false
    }

    pub(crate) fn prepare_hide(&mut self) {
        #[cfg(feature = "wgpu")]
        self.surface.prepare_hide();
    }

    pub(crate) fn maintain(&mut self, now: Instant) -> Option<Instant> {
        #[cfg(feature = "wgpu")]
        {
            self.surface.maintain(now);
            self.surface.maintenance_deadline()
        }
        #[cfg(not(feature = "wgpu"))]
        None
    }

    pub(crate) fn trim_gpu_cache(&mut self) {
        #[cfg(feature = "wgpu")]
        self.surface.trim_gpu_cache();
    }

    pub(crate) fn present(
        &mut self,
        scene: &mut Scene,
        color: Color,
        first_show: bool,
    ) -> crate::Result<bool> {
        #[cfg(feature = "wgpu")]
        {
            let result = if first_show {
                self.surface.present_initial(scene, color)
            } else {
                self.surface.present(scene, color)
            };
            result.map_err(renderer_error)
        }
        #[cfg(not(feature = "wgpu"))]
        Err(wgpu_required())
    }

    #[cfg(feature = "recording")]
    pub(crate) fn copy_retained_frame(
        &self,
        reuse: Option<plico_wgpu::RetainedFrameReadback>,
    ) -> Option<plico_wgpu::RetainedFrameReadback> {
        self.surface.copy_retained_frame(reuse)
    }

    pub(crate) fn capture(
        &mut self,
        scene: &Scene,
        width: u32,
        height: u32,
        color: Color,
    ) -> crate::Result<image::RgbaImage> {
        #[cfg(feature = "wgpu")]
        {
            let mut pixels = self
                .surface
                .capture_rgba(scene, [width, height], color)
                .map_err(renderer_error)?;
            crate::renderer::unpremultiply(&mut pixels);
            image::RgbaImage::from_raw(width, height, pixels)
                .ok_or(crate::Error::InvalidImageBuffer { width, height })
        }
        #[cfg(not(feature = "wgpu"))]
        Err(wgpu_required())
    }
}

#[cfg(feature = "wgpu")]
fn renderer_error(error: plico_wgpu::Error) -> crate::Error {
    crate::Error::Renderer {
        message: error.to_string(),
    }
}

#[cfg(not(feature = "wgpu"))]
fn wgpu_required() -> crate::Error {
    crate::Error::Renderer {
        message: "wgpu requires plico-shell's wgpu feature".into(),
    }
}
