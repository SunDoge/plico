//! GPU-only consumer for the shared Plico shell; window events and input stay in the host.
use crate::{Error, Renderer, Result};
use peniko::Color;
use plico_renderer::{Scene, TextContext};
use raw_window_handle::{
    DisplayHandle, HandleError, HasDisplayHandle, HasWindowHandle, WindowHandle,
};
use std::sync::Arc;
use winit::window::Window;
// Partial redraw is the normal policy; retain an explicit full-redraw escape hatch.
fn damage_enabled(value: Option<&str>) -> bool {
    !matches!(value, Some("0" | "false"))
}
// wgpu 30's Metal backend labels non-opaque CAMetalLayer as PostMultiplied,
// although Core Animation consumes premultiplied pixels. Keep our existing
// premultiplied output. Restrict this compatibility case to Metal:
// https://github.com/gfx-rs/wgpu/issues/9896 (upstream fix: #9922).
fn surface_alpha_mode(
    transparent: bool,
    backend: wgpu::Backend,
    supported: &[wgpu::CompositeAlphaMode],
) -> Option<wgpu::CompositeAlphaMode> {
    use wgpu::CompositeAlphaMode as Alpha;
    if !transparent {
        return Some(Alpha::Auto);
    }
    [Alpha::PreMultiplied, Alpha::Inherit]
        .into_iter()
        .find(|mode| supported.contains(mode))
        .or_else(|| {
            (backend == wgpu::Backend::Metal && supported.contains(&Alpha::PostMultiplied))
                .then_some(Alpha::PostMultiplied)
        })
}

// Host maintenance uses a one-shot wake, never an idle polling loop.
#[cfg(any(target_os = "macos", test))]
#[derive(Default)]
struct IdleResources {
    deadline: Option<std::time::Instant>,
    trimmed: bool,
}
#[cfg(any(target_os = "macos", test))]
impl IdleResources {
    fn presented(&mut self, now: std::time::Instant) {
        self.deadline = Some(now + std::time::Duration::from_secs(2));
        self.trimmed = false;
    }
    fn due(&self, now: std::time::Instant) -> bool {
        self.deadline.is_some_and(|deadline| deadline <= now)
    }
    fn trimmed(&mut self) {
        self.deadline = None;
        self.trimmed = true;
    }
}

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
    initial_frame_ready: bool,
    #[cfg(target_os = "macos")]
    idle_resources: IdleResources,
    #[cfg(all(
        any(target_os = "linux", target_os = "windows", target_os = "macos"),
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
        let alpha_mode = surface_alpha_mode(
            transparent,
            renderer.adapter_info().backend,
            &caps.alpha_modes,
        )
        .ok_or_else(|| Error::Surface {
            message: format!(
                "premultiplied transparent window surface unavailable: backend {:?}, modes {:?}",
                renderer.adapter_info().backend,
                caps.alpha_modes,
            ),
        })?;
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
            damage_enabled: damage_enabled(std::env::var("PLICO_WGPU_DAMAGE").ok().as_deref()),
            present_skipped: false,
            initial_frame_ready: false,
            #[cfg(target_os = "macos")]
            idle_resources: IdleResources::default(),
            #[cfg(all(
                any(target_os = "linux", target_os = "windows", target_os = "macos"),
                feature = "system-text"
            ))]
            text_scale: None,
        })
    }
    /// Reinstall the native measure provider only when device scale/context changes.
    pub fn configure_text(&mut self, text: &mut TextContext) -> Result<()> {
        #[cfg(all(
            any(target_os = "linux", target_os = "windows", target_os = "macos"),
            feature = "system-text"
        ))]
        {
            let scale = self.window.scale_factor();
            if self.text_scale != Some(scale.to_bits())
                || text.raster_backend_name()
                    != if cfg!(target_os = "windows") {
                        "directwrite-wgpu"
                    } else if cfg!(target_os = "macos") {
                        "coretext-wgpu"
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
            any(target_os = "linux", target_os = "windows", target_os = "macos"),
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
        #[cfg(not(target_os = "macos"))]
        let restore = false;
        #[cfg(target_os = "macos")]
        let restore = self.idle_resources.trimmed;
        if self.config.width != width || self.config.height != height || restore {
            self.config.width = width;
            self.config.height = height;
            self.renderer.configure_surface(&self.surface, &self.config);
            #[cfg(target_os = "macos")]
            {
                self.idle_resources.trimmed = false;
            }
        }
    }
    /// Compile supported paint, submit on the GPU, then present. An unsupported
    /// command returns before acquiring/submitting a partial application frame.
    pub fn present(&mut self, scene: &Scene, background: Color) -> Result<bool> {
        self.present_frame(scene, background, false)
    }
    /// Complete the initial retained GPU frame before making a hidden window
    /// visible, then copy it to the surface. A single bounded GPU wait occurs
    /// here; no CPU pixel readback is used. Retries do not show/focus it twice.
    pub fn present_initial(&mut self, scene: &Scene, background: Color) -> Result<bool> {
        self.present_frame(scene, background, !self.initial_frame_ready)
    }
    fn present_frame(&mut self, scene: &Scene, background: Color, initial: bool) -> Result<bool> {
        self.present_skipped = false;
        let size = self.window.surface_size();
        if size.width == 0 || size.height == 0 {
            return Ok(false);
        }
        self.resize(size.width, size.height);
        self.renderer.prepare_present(self.config.format)?;
        if initial {
            self.renderer.invalidate_scene_damage();
        }
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
        // Match MyGO's ready-to-show ordering without acquiring a drawable
        // for a hidden window (wgpu Metal deliberately rejects that). Normal
        // frames also finish all scene encoding before holding the drawable.
        let encoder = if initial {
            let encoder = self.renderer.prime_scene_plan(plan, self.config.format)?;
            self.initial_frame_ready = true;
            self.window.set_visible(true);
            encoder
        } else {
            self.renderer.encode_scene_plan(plan)?
        };
        let frame = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(frame)
            | wgpu::CurrentSurfaceTexture::Suboptimal(frame) => frame,
            wgpu::CurrentSurfaceTexture::Outdated => {
                self.renderer.abandon_scene_frame(encoder);
                self.renderer.configure_surface(&self.surface, &self.config);
                return Ok(false);
            }
            wgpu::CurrentSurfaceTexture::Lost => {
                self.renderer.abandon_scene_frame(encoder);
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
                self.renderer.abandon_scene_frame(encoder);
                return Ok(false);
            }
            other => {
                self.renderer.abandon_scene_frame(encoder);
                return Err(Error::Surface {
                    message: format!("acquire failed: {other:?}"),
                });
            }
        };
        let submitted = self.renderer.submit_to_view(
            encoder,
            &frame.texture.create_view(&Default::default()),
            self.config.format,
        );
        if submitted.is_err() {
            self.renderer.invalidate_scene_damage();
        }
        submitted?;
        self.window.pre_present_notify();
        self.renderer.present(frame);
        #[cfg(target_os = "macos")]
        self.idle_resources.presented(std::time::Instant::now());
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
    /// Next one-shot idle maintenance wake on macOS; absent on other platforms.
    pub fn maintenance_deadline(&self) -> Option<std::time::Instant> {
        #[cfg(target_os = "macos")]
        {
            self.idle_resources.deadline
        }
        #[cfg(not(target_os = "macos"))]
        {
            None
        }
    }
    /// Release idle resources when due without rendering or requesting a frame.
    pub fn maintain(&mut self, now: std::time::Instant) {
        #[cfg(target_os = "macos")]
        if self.idle_resources.due(now) {
            self.trim_gpu_cache();
        }
        #[cfg(not(target_os = "macos"))]
        let _ = now;
    }
    /// Release derived caches. On macOS also return spare drawables and the
    /// retained target; Core Animation keeps the already presented last frame.
    /// The next present restores the real extent and performs a full redraw.
    pub fn trim_gpu_cache(&mut self) {
        #[cfg(target_os = "macos")]
        if !self.idle_resources.trimmed {
            // MyGO's idle Metal trim: changing drawableSize preserves the last
            // compositor-owned frame while returning spare full-size buffers.
            let mut small = self.config.clone();
            small.width = 1;
            small.height = 1;
            self.renderer.configure_surface(&self.surface, &small);
            self.idle_resources.trimmed();
        }
        #[cfg(target_os = "macos")]
        self.renderer.trim_frame_target();
        self.renderer.trim_readback_buffer();
        self.renderer.trim_raster_cache();
        self.renderer.trim_image_cache();
        self.renderer.trim_layer_pool();
        self.renderer.trim_effect_cache();
        self.renderer.trim_positioned_glyph_cache();
        #[cfg(all(
            any(target_os = "linux", target_os = "windows", target_os = "macos"),
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

#[cfg(test)]
mod tests {
    #[test]
    fn transparent_surface_accepts_metals_legacy_alpha_label_only_on_metal() {
        use wgpu::{Backend, CompositeAlphaMode as Alpha};
        let metal_caps = [Alpha::Opaque, Alpha::PostMultiplied];
        assert_eq!(
            super::surface_alpha_mode(true, Backend::Metal, &metal_caps),
            Some(Alpha::PostMultiplied)
        );
        for backend in [
            Backend::Vulkan,
            Backend::Dx12,
            Backend::Gl,
            Backend::BrowserWebGpu,
        ] {
            assert_eq!(super::surface_alpha_mode(true, backend, &metal_caps), None);
        }
        for backend in [Backend::Metal, Backend::Vulkan] {
            assert_eq!(
                super::surface_alpha_mode(true, backend, &[Alpha::Opaque]),
                None
            );
            assert_eq!(
                super::surface_alpha_mode(false, backend, &metal_caps),
                Some(Alpha::Auto)
            );
            assert_eq!(
                super::surface_alpha_mode(
                    true,
                    backend,
                    &[Alpha::PreMultiplied, Alpha::PostMultiplied]
                ),
                Some(Alpha::PreMultiplied)
            );
            assert_eq!(
                super::surface_alpha_mode(true, backend, &[Alpha::Inherit]),
                Some(Alpha::Inherit)
            );
        }
    }

    #[test]
    fn idle_resources_arm_once_follow_real_presents_and_stop_after_trim() {
        use std::time::{Duration, Instant};
        let now = Instant::now();
        let mut idle = super::IdleResources::default();
        assert_eq!(idle.deadline, None, "hidden startup needs no trim timer");
        idle.presented(now);
        assert!(!idle.due(now + Duration::from_millis(1999)));
        assert!(idle.due(now + Duration::from_secs(2)));
        idle.presented(now + Duration::from_secs(1));
        assert!(
            !idle.due(now + Duration::from_secs(2)),
            "drawing postpones idle trim"
        );
        assert!(idle.due(now + Duration::from_secs(3)));
        idle.trimmed();
        assert!(idle.trimmed);
        assert_eq!(idle.deadline, None);
        assert!(
            !idle.due(now + Duration::from_secs(100)),
            "no recurring idle wake"
        );
        idle.presented(now + Duration::from_secs(101));
        assert!(!idle.trimmed);
        assert_eq!(idle.deadline, Some(now + Duration::from_secs(103)));
    }

    #[test]
    fn damage_defaults_on_with_an_explicit_full_redraw_override() {
        for value in [None, Some("1"), Some("true"), Some("")] {
            assert!(super::damage_enabled(value));
        }
        for value in [Some("0"), Some("false")] {
            assert!(!super::damage_enabled(value));
        }
    }
}
