//! GPU-only consumer for the shared Plico shell; window events and input stay in the host.
use crate::{Error, Renderer, Result};
use peniko::Color;
use plico_renderer::{Scene, TextContext};
use raw_window_handle::{
    DisplayHandle, HandleError, HasDisplayHandle, HasWindowHandle, WindowHandle,
};
use std::sync::Arc;
use winit::window::Window;
struct SharedWindow(Arc<dyn Window>);
impl HasWindowHandle for SharedWindow {
    fn window_handle(&self) -> std::result::Result<WindowHandle<'_>, HandleError> {
        self.0.window_handle()
    }
}
impl HasDisplayHandle for SharedWindow {
    fn display_handle(&self) -> std::result::Result<DisplayHandle<'_>, HandleError> {
        self.0.display_handle()
    }
}
/// Window renderer without its own event loop or application state.
pub struct WgpuWindow {
    renderer: Renderer,
    instance: wgpu::Instance,
    surface: wgpu::Surface<'static>,
    window: Arc<dyn Window>,
    config: wgpu::SurfaceConfiguration,
    damage_enabled: bool,
    present_skipped: bool,
    #[cfg(all(
        any(target_os = "linux", target_os = "windows"),
        feature = "system-text"
    ))]
    text_scale: Option<u64>,
}
impl WgpuWindow {
    pub fn new(window: Arc<dyn Window>, transparent: bool) -> Result<Self> {
        let instance = wgpu::Instance::new(crate::instance_descriptor());
        let surface = instance
            .create_surface(SharedWindow(window.clone()))
            .map_err(|e| Error::Surface {
                message: e.to_string(),
            })?;
        let renderer = pollster::block_on(Renderer::new_for_surface(&instance, &surface))?;
        let caps = renderer.surface_capabilities(&surface);
        let format = caps
            .formats
            .iter()
            .copied()
            .find(|f| {
                matches!(
                    f,
                    wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Rgba8Unorm
                )
            })
            .or_else(|| caps.formats.first().copied())
            .ok_or_else(|| Error::Surface {
                message: "no compatible surface formats".into(),
            })?;
        let alpha_mode = if transparent {
            [
                wgpu::CompositeAlphaMode::PreMultiplied,
                wgpu::CompositeAlphaMode::Inherit,
            ]
            .into_iter()
            .find(|mode| caps.alpha_modes.contains(mode))
            .ok_or_else(|| Error::Surface {
                message: "premultiplied transparent window surface unavailable".into(),
            })?
        } else {
            wgpu::CompositeAlphaMode::Auto
        };
        let size = window.surface_size();
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            color_space: wgpu::SurfaceColorSpace::Srgb,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode: wgpu::PresentMode::Fifo,
            desired_maximum_frame_latency: 2,
            alpha_mode,
            view_formats: vec![],
        };
        renderer.configure_surface(&surface, &config);
        tracing::info!(adapter=?renderer.adapter_info(),?format,"Plico wgpu window renderer initialized");
        Ok(Self {
            renderer,
            instance,
            surface,
            window,
            config,
            damage_enabled: matches!(
                std::env::var("PLICO_WGPU_DAMAGE").as_deref(),
                Ok("1") | Ok("true")
            ),
            present_skipped: false,
            #[cfg(all(
                any(target_os = "linux", target_os = "windows"),
                feature = "system-text"
            ))]
            text_scale: None,
        })
    }
    /// Reinstall the native measure provider only when device scale/context changes.
    pub fn configure_text(&mut self, text: &mut TextContext) -> Result<()> {
        #[cfg(all(
            any(target_os = "linux", target_os = "windows"),
            feature = "system-text"
        ))]
        {
            let scale = self.window.scale_factor();
            if self.text_scale != Some(scale.to_bits())
                || text.raster_backend_name()
                    != if cfg!(target_os = "windows") {
                        "directwrite-wgpu"
                    } else {
                        "pango-wgpu"
                    }
            {
                self.renderer.configure_text(text, scale)?;
                self.text_scale = Some(scale.to_bits());
            }
            Ok(())
        }
        #[cfg(not(all(
            any(target_os = "linux", target_os = "windows"),
            feature = "system-text"
        )))]
        {
            let _ = text;
            Err(Error::Unsupported {
                command: 0,
                reason: "system text provider is not implemented for this platform".into(),
            })
        }
    }
    pub fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        if self.config.width != width || self.config.height != height {
            self.config.width = width;
            self.config.height = height;
            self.renderer.configure_surface(&self.surface, &self.config);
        }
    }
    /// Compile supported paint, submit on the GPU, then present. An unsupported
    /// command returns before acquiring/submitting a partial application frame.
    pub fn present(&mut self, scene: &Scene, background: Color) -> Result<bool> {
        self.present_skipped = false;
        let size = self.window.surface_size();
        if size.width == 0 || size.height == 0 {
            return Ok(false);
        }
        self.resize(size.width, size.height);
        let plan = self.renderer.plan_scene_frame(
            scene,
            [size.width, size.height],
            background.components,
            self.damage_enabled,
        )?;
        if plan.unchanged() {
            self.present_skipped = true;
            return Ok(true);
        }
        let frame = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(frame)
            | wgpu::CurrentSurfaceTexture::Suboptimal(frame) => frame,
            wgpu::CurrentSurfaceTexture::Outdated => {
                self.renderer.invalidate_scene_damage();
                self.renderer.configure_surface(&self.surface, &self.config);
                return Ok(false);
            }
            wgpu::CurrentSurfaceTexture::Lost => {
                self.renderer.invalidate_scene_damage();
                self.surface = self
                    .instance
                    .create_surface(SharedWindow(self.window.clone()))
                    .map_err(|e| Error::Surface {
                        message: e.to_string(),
                    })?;
                self.renderer.configure_surface(&self.surface, &self.config);
                return Ok(false);
            }
            wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => {
                self.renderer.invalidate_scene_damage();
                return Ok(false);
            }
            other => {
                self.renderer.invalidate_scene_damage();
                return Err(Error::Surface {
                    message: format!("acquire failed: {other:?}"),
                });
            }
        };
        self.renderer.submit_scene_plan(
            plan,
            &frame.texture.create_view(&Default::default()),
            self.config.format,
        )?;
        self.window.pre_present_notify();
        self.renderer.present(frame);
        Ok(true)
    }
}

impl WgpuWindow {
    pub fn last_present_skipped(&self) -> bool {
        self.present_skipped
    }
    pub fn last_damage_coverage(&self) -> f64 {
        self.renderer
            .scene_damage_coverage([self.config.width, self.config.height])
    }
    /// Release derived raster, image, layer, effect and native text caches. Final pixels remain
    /// intact; text trimming forces the next frame to rebuild its atlas safely.
    pub fn trim_gpu_cache(&mut self) {
        self.renderer.trim_raster_cache();
        self.renderer.trim_image_cache();
        self.renderer.trim_layer_pool();
        self.renderer.trim_effect_cache();
        self.renderer.trim_positioned_glyph_cache();
        #[cfg(all(
            any(target_os = "linux", target_os = "windows"),
            feature = "system-text"
        ))]
        self.renderer.trim_text_cache();
    }
    pub fn invalidate_presentation(&mut self) {
        self.renderer.invalidate_scene_damage();
    }
    /// Enable partial redraw; changing the mode forces a fresh complete frame.
    pub fn set_damage_tracking(&mut self, enabled: bool) {
        if self.damage_enabled != enabled {
            self.invalidate_presentation();
        }
        self.damage_enabled = enabled;
    }
    /// Capture with this window's renderer and paragraph cache, never a different backend.
    pub fn capture_rgba(
        &mut self,
        scene: &Scene,
        size: [u32; 2],
        background: Color,
    ) -> Result<Vec<u8>> {
        let prepared = self.renderer.prepare_scene(scene, size)?;
        self.renderer
            .render_items(&prepared.draws(), size, background.components, 0.)
    }
}
