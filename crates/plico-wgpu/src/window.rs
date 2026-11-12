//! GPU-only consumer for the shared Plico shell; window events and input stay in the host.
use crate::{Error, Renderer, Result};
use peniko::Color;
use plico_renderer::{Scene, TextContext};
use raw_window_handle::{
    DisplayHandle, HandleError, HasDisplayHandle, HasWindowHandle, RawDisplayHandle, WindowHandle,
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
#[cfg(any(target_os = "macos", target_os = "linux", test))]
#[derive(Default)]
struct IdleResources {
    deadline: Option<std::time::Instant>,
    #[cfg(any(target_os = "macos", test))]
    trimmed: bool,
}
#[cfg(any(target_os = "macos", target_os = "linux", test))]
impl IdleResources {
    fn used(&mut self, now: std::time::Instant) {
        self.deadline = Some(now + std::time::Duration::from_secs(2));
        // Offscreen captures also use GPU resources. They rearm cleanup but
        // do not restore the native swapchain: fit_surface owns that transition.
    }
    fn due(&self, now: std::time::Instant) -> bool {
        self.deadline.is_some_and(|deadline| deadline <= now)
    }
    fn trimmed(&mut self) {
        self.deadline = None;
        #[cfg(any(target_os = "macos", test))]
        {
            self.trimmed = true;
        }
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
    surface: Arc<wgpu::Surface<'static>>,
    #[cfg(any(target_os = "macos", all(test, not(target_arch = "wasm32"))))]
    surface_acquire: Option<crate::surface_acquire::Acquire<wgpu::CurrentSurfaceTexture>>,
    #[cfg(any(target_os = "macos", all(test, not(target_arch = "wasm32"))))]
    surface_wake: Option<plico_renderer::WakeCallback>,
    window: Arc<dyn Window>,
    config: wgpu::SurfaceConfiguration,
    configured_size: [u32; 2],
    damage_enabled: bool,
    present_skipped: bool,
    acquire_pending: bool,
    // One unsubmitted encoder, never a second queue of scenes/drawables.
    pending_frame: Option<wgpu::CommandEncoder>,
    pending_frame_stats: [u64; 3],
    // Encoding is not submission. In particular, a new pending target has no
    // readable retained pixels until its commands have reached the queue.
    retained_size: Option<[u32; 2]>,
    initial_frame_ready: bool,
    #[cfg(target_os = "macos")]
    frame_pass: Option<crate::macos_frame_pass::NativeFramePass>,
    #[cfg(target_os = "macos")]
    transaction: Option<crate::macos_transaction::NativeTransaction>,
    #[cfg(any(target_os = "macos", target_os = "linux"))]
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
        let surface = Arc::new(surface);
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
        #[cfg(target_os = "macos")]
        let frame_pass = (renderer.adapter_info().backend == wgpu::Backend::Metal)
            .then(crate::macos_frame_pass::NativeFramePass::new)
            .flatten();
        #[cfg(target_os = "macos")]
        let transaction = frame_pass
            .as_ref()
            .and_then(|_| crate::macos_transaction::NativeTransaction::new(&surface));
        Ok(Self {
            renderer,
            instance,
            surface,
            #[cfg(any(target_os = "macos", all(test, not(target_arch = "wasm32"))))]
            surface_acquire: None,
            #[cfg(any(target_os = "macos", all(test, not(target_arch = "wasm32"))))]
            surface_wake: None,
            window,
            configured_size: [config.width, config.height],
            config,
            damage_enabled: damage_enabled(std::env::var("PLICO_WGPU_DAMAGE").ok().as_deref()),
            present_skipped: false,
            acquire_pending: false,
            pending_frame: None,
            pending_frame_stats: [0; 3],
            retained_size: None,
            initial_frame_ready: false,
            #[cfg(target_os = "macos")]
            frame_pass,
            #[cfg(target_os = "macos")]
            transaction,
            #[cfg(any(target_os = "macos", target_os = "linux"))]
            idle_resources: IdleResources::default(),
            #[cfg(all(
                any(target_os = "linux", target_os = "windows", target_os = "macos"),
                feature = "system-text"
            ))]
            text_scale: None,
        })
    }
    /// Enable bounded background acquisition on macOS. The callback must only
    /// wake the event loop; it must never synchronously wait for the UI thread.
    pub fn set_surface_wake(&mut self, wake: plico_renderer::WakeCallback) {
        #[cfg(any(target_os = "macos", all(test, not(target_arch = "wasm32"))))]
        {
            self.abandon_pending_frame();
            self.acquire_pending = false;
            // Startup-only A/B switch; no environment lookup in the frame loop.
            if matches!(
                std::env::var("PLICO_WGPU_ASYNC_SURFACE").ok().as_deref(),
                Some("0" | "false")
            ) {
                self.surface_acquire = None;
                self.surface_wake = None;
                return;
            }
            self.surface_wake = Some(wake);
            self.reset_surface_acquire();
        }
        #[cfg(not(any(target_os = "macos", all(test, not(target_arch = "wasm32")))))]
        let _ = wake;
    }
    #[cfg(any(target_os = "macos", all(test, not(target_arch = "wasm32"))))]
    fn reset_surface_acquire(&mut self) {
        self.abandon_pending_frame();
        self.surface_acquire = None;
        if let Some(wake) = self.surface_wake.clone() {
            let surface = self.surface.clone();
            match crate::surface_acquire::Acquire::new(move || surface.get_current_texture(), wake)
            {
                Ok(worker) => self.surface_acquire = Some(worker),
                Err(error) => {
                    tracing::warn!(%error, "surface worker unavailable; using synchronous acquisition")
                }
            }
        }
    }
    fn drain_surface_acquire(&self) {
        #[cfg(any(target_os = "macos", all(test, not(target_arch = "wasm32"))))]
        if let Some(worker) = &self.surface_acquire {
            worker.drain();
        }
    }
    /// Adapter selected for this window; useful for native-backend diagnostics.
    pub fn adapter_info(&self) -> &wgpu::AdapterInfo {
        self.renderer.adapter_info()
    }
    /// Context shared with framework-owned external GPU surfaces.
    pub fn gpu_context(&self) -> plico_renderer::source::GpuContext {
        let mut context = self.renderer.external_gpu_context();
        context.native_display = self
            .window
            .display_handle()
            .ok()
            .and_then(|handle| native_display(handle.as_raw()));
        plico_renderer::source::GpuContext::new("wgpu", context)
    }
    /// Whether a background acquisition worker is installed. Useful for A/B
    /// diagnostics to distinguish requested configuration from a sync fallback.
    pub fn surface_prefetch_enabled(&self) -> bool {
        #[cfg(any(target_os = "macos", all(test, not(target_arch = "wasm32"))))]
        {
            self.surface_acquire.is_some()
        }
        #[cfg(not(any(target_os = "macos", all(test, not(target_arch = "wasm32")))))]
        {
            false
        }
    }
    /// Consume a readiness notification, not the acquired texture. The host
    /// requests a redraw only if an earlier frame actually waited for it.
    pub fn take_surface_ready(&self) -> bool {
        #[cfg(any(target_os = "macos", all(test, not(target_arch = "wasm32"))))]
        if let Some(worker) = &self.surface_acquire {
            return worker.take_signal();
        }
        false
    }
    fn cancel_surface_waiter(&self) {
        #[cfg(any(target_os = "macos", all(test, not(target_arch = "wasm32"))))]
        if let Some(worker) = &self.surface_acquire {
            worker.cancel_waiter();
        }
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
    /// Record a desired extent without restoring idle drawables. Presentation
    /// reconciles it with the live native size and configures the surface once.
    pub fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        self.config.width = width;
        self.config.height = height;
    }
    fn surface_needs_fit(&self) -> bool {
        #[cfg(not(target_os = "macos"))]
        let restore = false;
        #[cfg(target_os = "macos")]
        let restore = self.idle_resources.trimmed;
        self.configured_size != [self.config.width, self.config.height] || restore
    }
    fn fit_surface(&mut self) {
        if self.surface_needs_fit() {
            self.drain_surface_acquire();
            self.renderer.configure_surface(&self.surface, &self.config);
            self.configured_size = [self.config.width, self.config.height];
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
        let result = self.present_frame_inner(scene, background, initial);
        if result.is_err() {
            self.abandon_pending_frame();
            self.cancel_surface_waiter();
            self.cancel_transaction_wait();
        }
        result
    }
    fn present_frame_inner(
        &mut self,
        scene: &Scene,
        background: Color,
        initial: bool,
    ) -> Result<bool> {
        self.present_skipped = false;
        self.acquire_pending = false;
        let size = self.window.surface_size();
        if size.width == 0 || size.height == 0 {
            self.abandon_pending_frame();
            self.cancel_surface_waiter();
            self.cancel_transaction_wait();
            return Ok(false);
        }
        self.resize(size.width, size.height);
        self.renderer.prepare_present(self.config.format)?;
        if initial {
            self.abandon_pending_frame();
            self.renderer.invalidate_scene_damage();
        }
        let prepared = self.renderer.prepare_scene_frame(
            scene,
            [size.width, size.height],
            background.components,
        )?;
        // Planning the pending encoder advanced history, but its target pixels
        // have not been submitted. Only an identical scene may reuse it.
        if self.pending_frame.is_some()
            && (self.surface_needs_fit()
                || !self.renderer.prepared_frame_unchanged(&prepared, true))
        {
            self.abandon_pending_frame();
        }
        if self.pending_frame.is_none()
            && !initial
            && !self.surface_needs_fit()
            && self
                .renderer
                .prepared_frame_unchanged(&prepared, self.damage_enabled)
        {
            // Keep damage diagnostics identical to the normal unchanged path,
            // without arming a drawable waiter for work requiring no texture.
            let plan = self
                .renderer
                .plan_prepared_frame(prepared, self.damage_enabled);
            debug_assert!(plan.unchanged());
            self.present_skipped = true;
            self.cancel_surface_waiter();
            self.cancel_transaction_wait();
            return Ok(true);
        }
        #[cfg(target_os = "macos")]
        if !initial
            && self.transaction.as_mut().is_some_and(|tx| {
                tx.defer(
                    self.window.as_ref(),
                    [size.width, size.height],
                    self.frame_pass.as_ref().and_then(|pass| pass.boundary()),
                )
            })
        {
            self.abandon_pending_frame();
            self.cancel_surface_waiter();
            return Ok(false);
        }
        #[cfg(target_os = "macos")]
        let sync_frame = self
            .transaction
            .as_ref()
            .is_some_and(|tx| tx.sync(initial || self.surface_needs_fit()));
        #[cfg(all(not(target_os = "macos"), test, not(target_arch = "wasm32")))]
        let sync_frame = false;
        #[cfg(target_os = "macos")]
        if self
            .transaction
            .as_ref()
            .is_some_and(|tx| sync_frame || tx.changes_mode(sync_frame))
        {
            // A prefetched drawable snapshots its old transaction mode. Drain
            // before changing that mode or configuring the layer, outside its lock.
            self.abandon_pending_frame();
            self.drain_surface_acquire();
            if let Some(tx) = self.transaction.as_mut() {
                tx.apply(&self.surface, sync_frame)?;
            }
        }
        #[cfg(any(target_os = "macos", all(test, not(target_arch = "wasm32"))))]
        if !sync_frame
            && !initial
            && !self.surface_needs_fit()
            && let Some(worker) = &self.surface_acquire
        {
            match worker.ready() {
                Ok(true) => {}
                Ok(false) => {
                    if self.pending_frame.is_none() {
                        let plan = self
                            .renderer
                            .plan_prepared_frame(prepared, self.damage_enabled);
                        // Failed encoding can allocate upload/cache resources too.
                        #[cfg(any(target_os = "macos", target_os = "linux"))]
                        self.idle_resources.used(std::time::Instant::now());
                        if self.retained_size != Some([size.width, size.height]) {
                            self.retained_size = None;
                        }
                        self.pending_frame = Some(self.renderer.encode_scene_plan(plan)?);
                        self.pending_frame_stats[0] += 1;
                    }
                    self.acquire_pending = true;
                    return Ok(false);
                }
                Err(error) => {
                    self.surface_acquire = None;
                    return Err(Error::Surface {
                        message: error.to_string(),
                    });
                }
            }
        }
        let plan = if self.pending_frame.is_some() {
            None
        } else {
            Some(
                self.renderer
                    .plan_prepared_frame(prepared, self.damage_enabled),
            )
        };
        if plan.as_ref().is_some_and(|plan| plan.unchanged()) {
            self.present_skipped = true;
            // This request is satisfied without consuming the prefetched
            // texture. A delayed proxy wake must not redraw it again.
            self.cancel_surface_waiter();
            self.cancel_transaction_wait();
            return Ok(true);
        }
        #[cfg(any(target_os = "macos", all(test, not(target_arch = "wasm32"))))]
        let prefetched = match (
            sync_frame || initial || self.surface_needs_fit(),
            self.surface_acquire.as_ref(),
        ) {
            (false, Some(worker)) => match worker.take() {
                Ok(Some(frame)) => Some(frame),
                Ok(None) => {
                    self.acquire_pending = true;
                    // A pending encoder still owns this history. Without one,
                    // planning alone has not established target pixels.
                    if self.pending_frame.is_none() {
                        self.renderer.invalidate_scene_damage();
                    }
                    return Ok(false);
                }
                Err(error) => {
                    self.renderer.invalidate_scene_damage();
                    // Disable the failed worker; the host's bounded recovery
                    // retry can use the original synchronous acquisition path.
                    self.surface_acquire = None;
                    return Err(Error::Surface {
                        message: error.to_string(),
                    });
                }
            },
            _ => None,
        };
        // As in MyGO Metal's fit(), wait for a real frame to restore/change
        // drawables. Hidden resize/DPI notifications only update desired size.
        if initial {
            self.drain_surface_acquire();
        }
        self.fit_surface();
        // Configuration/encoding can allocate resources even if acquisition or
        // submission later fails. Keep one-shot cleanup armed for that case.
        #[cfg(any(target_os = "macos", target_os = "linux"))]
        self.idle_resources.used(std::time::Instant::now());
        // Match MyGO's ready-to-show ordering without acquiring a drawable
        // for a hidden window (wgpu Metal deliberately rejects that). The
        // synchronous path encodes before acquisition; a prefetched texture
        // lets the host encode the latest scene without waiting for the display.
        if self.retained_size != Some([size.width, size.height]) {
            self.retained_size = None;
        }
        let encoder = if initial {
            let encoder = self
                .renderer
                .prime_scene_plan(plan.expect("initial frame has a plan"), self.config.format)?;
            self.retained_size = Some([size.width, size.height]);
            self.initial_frame_ready = true;
            self.window.set_visible(true);
            encoder
        } else if let Some(encoder) = self.pending_frame.take() {
            self.pending_frame_stats[1] += 1;
            encoder
        } else {
            self.renderer
                .encode_scene_plan(plan.expect("fresh frame has a plan"))?
        };
        #[cfg(any(target_os = "macos", all(test, not(target_arch = "wasm32"))))]
        let acquired = prefetched.unwrap_or_else(|| self.surface.get_current_texture());
        #[cfg(not(any(target_os = "macos", all(test, not(target_arch = "wasm32")))))]
        let acquired = self.surface.get_current_texture();
        let frame = match acquired {
            wgpu::CurrentSurfaceTexture::Success(frame)
            | wgpu::CurrentSurfaceTexture::Suboptimal(frame) => frame,
            wgpu::CurrentSurfaceTexture::Outdated => {
                self.renderer.abandon_scene_frame(encoder);
                self.renderer.configure_surface(&self.surface, &self.config);
                #[cfg(target_os = "macos")]
                if let Some(tx) = &mut self.transaction {
                    tx.reconfigured();
                }
                return Ok(false);
            }
            wgpu::CurrentSurfaceTexture::Lost => {
                self.renderer.abandon_scene_frame(encoder);
                self.drain_surface_acquire();
                #[cfg(any(target_os = "macos", all(test, not(target_arch = "wasm32"))))]
                {
                    self.surface_acquire = None;
                }
                self.surface = Arc::new(
                    self.instance
                        .create_surface(SharedWindow(self.window.clone()))
                        .map_err(|e| Error::Surface {
                            message: e.to_string(),
                        })?,
                );
                self.renderer.configure_surface(&self.surface, &self.config);
                #[cfg(target_os = "macos")]
                if let Some(tx) = &mut self.transaction {
                    tx.replaced();
                }
                #[cfg(any(target_os = "macos", all(test, not(target_arch = "wasm32"))))]
                self.reset_surface_acquire();
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
        self.retained_size = Some([size.width, size.height]);
        self.window.pre_present_notify();
        self.renderer.present(frame);
        #[cfg(target_os = "macos")]
        if let Some(tx) = &mut self.transaction {
            tx.presented(
                [size.width, size.height],
                sync_frame,
                self.frame_pass.as_ref().and_then(|pass| pass.boundary()),
            );
        }
        #[cfg(target_os = "macos")]
        if let Some(pass) = &mut self.frame_pass {
            pass.presented();
        }
        #[cfg(any(target_os = "macos", all(test, not(target_arch = "wasm32"))))]
        if !sync_frame && let Some(worker) = &self.surface_acquire {
            worker.start();
        }
        #[cfg(any(target_os = "macos", target_os = "linux"))]
        self.idle_resources.used(std::time::Instant::now());
        Ok(true)
    }
}

fn native_display(handle: RawDisplayHandle) -> Option<crate::NativeDisplay> {
    match handle {
        RawDisplayHandle::Xlib(handle) => handle
            .display
            .map(|display| crate::NativeDisplay::X11(display.as_ptr() as usize)),
        RawDisplayHandle::Wayland(handle) => Some(crate::NativeDisplay::Wayland(
            handle.display.as_ptr() as usize,
        )),
        _ => None,
    }
}

impl WgpuWindow {
    /// Supply the host's display-link epoch and sampled interval. None uses
    /// a deadline fallback, useful for hosts without CADisplayLink integration.
    pub fn set_presentation_clock(
        &mut self,
        tick: Option<crate::PresentationTick>,
        interval: std::time::Duration,
    ) {
        #[cfg(target_os = "macos")]
        if let Some(tx) = &mut self.transaction {
            tx.clock(tick, interval);
        }
        #[cfg(not(target_os = "macos"))]
        let _ = (tick, interval);
    }
    /// Waiting for a display boundary, distinct from drawable acquisition.
    pub fn presentation_deferred(&self) -> bool {
        #[cfg(target_os = "macos")]
        {
            self.transaction.as_ref().is_some_and(|tx| tx.deferred())
        }
        #[cfg(not(target_os = "macos"))]
        {
            false
        }
    }
    /// A one-shot fallback deadline; absent when the host supplies display ticks.
    pub fn presentation_deadline(&self) -> Option<std::time::Instant> {
        #[cfg(target_os = "macos")]
        {
            self.transaction.as_ref().and_then(|tx| tx.deadline())
        }
        #[cfg(not(target_os = "macos"))]
        {
            None
        }
    }
    /// Synchronized submits, immediate submits and new display deferrals.
    pub fn transaction_stats(&self) -> Option<[u64; 3]> {
        #[cfg(target_os = "macos")]
        {
            self.transaction.as_ref().map(|tx| tx.stats())
        }
        #[cfg(not(target_os = "macos"))]
        {
            None
        }
    }
    fn cancel_transaction_wait(&mut self) {
        #[cfg(target_os = "macos")]
        if let Some(tx) = &mut self.transaction {
            tx.cancel();
        }
    }
    /// Submitted present calls, completed active run-loop passes, additional
    /// presentations in a pass, and observer callbacks. None off Metal/macOS
    /// or if unavailable. Observer ordering follows MyGO's CA commit boundary.
    /// This is observation only: same-pass frames can contain legitimate input.
    pub fn presentation_pass_stats(&self) -> Option<[u64; 4]> {
        #[cfg(target_os = "macos")]
        {
            self.frame_pass.as_ref().and_then(|pass| pass.snapshot())
        }
        #[cfg(not(target_os = "macos"))]
        {
            None
        }
    }
    /// Encoders built while waiting, reused for presentation, and abandoned.
    /// All zero when background acquisition is disabled. Counts are per window.
    pub fn pending_frame_stats(&self) -> [u64; 3] {
        self.pending_frame_stats
    }
    fn abandon_pending_frame(&mut self) {
        if let Some(encoder) = self.pending_frame.take() {
            self.pending_frame_stats[2] += 1;
            self.renderer.abandon_scene_frame(encoder);
        }
    }
    /// A frame is waiting for background acquisition; its event-loop wake will
    /// retry it. This is distinct from a surface failure or an unchanged frame.
    pub fn surface_acquire_pending(&self) -> bool {
        self.acquire_pending
    }
    pub fn last_present_skipped(&self) -> bool {
        self.present_skipped
    }
    pub fn last_damage_coverage(&self) -> f64 {
        self.renderer
            .scene_damage_coverage([self.config.width, self.config.height])
    }
    /// CPU and GPU work for the most recently submitted frame.
    pub fn frame_budget(&self) -> crate::FrameBudget {
        self.renderer.frame_budget()
    }
    /// Next one-shot idle maintenance wake on macOS/Linux.
    pub fn maintenance_deadline(&self) -> Option<std::time::Instant> {
        if self.acquire_pending {
            return None;
        }
        #[cfg(any(target_os = "macos", target_os = "linux"))]
        {
            self.idle_resources.deadline
        }
        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        {
            None
        }
    }
    /// Release idle resources when due without rendering or requesting a frame.
    pub fn maintain(&mut self, now: std::time::Instant) {
        #[cfg(any(target_os = "macos", target_os = "linux"))]
        if !self.acquire_pending && self.idle_resources.due(now) {
            self.trim_gpu_cache();
        }
        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        let _ = now;
    }
    /// Drain acquisition while the native window is still visible, before the
    /// host hides/minimizes it. Retain target pixels for OS window animations.
    pub fn prepare_hide(&mut self) {
        self.cancel_transaction_wait();
        self.abandon_pending_frame();
        self.drain_surface_acquire();
        self.acquire_pending = false;
    }
    /// Release derived caches and the macOS/Linux retained offscreen target.
    /// macOS also shrinks spare drawables; Linux keeps its real surface extent.
    /// The compositor owns the presented last frame. Next present redraws fully.
    pub fn trim_gpu_cache(&mut self) {
        self.cancel_transaction_wait();
        self.abandon_pending_frame();
        self.drain_surface_acquire();
        self.acquire_pending = false;
        #[cfg(any(target_os = "macos", target_os = "linux"))]
        {
            self.retained_size = None;
        }
        #[cfg(target_os = "macos")]
        if !self.idle_resources.trimmed {
            // MyGO's idle Metal trim: changing drawableSize preserves the last
            // compositor-owned frame while returning spare full-size buffers.
            let mut small = self.config.clone();
            small.width = 1;
            small.height = 1;
            self.renderer.configure_surface(&self.surface, &small);
            self.configured_size = [1, 1];
            self.idle_resources.trimmed();
        }
        #[cfg(target_os = "linux")]
        self.idle_resources.trimmed();
        #[cfg(any(target_os = "macos", target_os = "linux"))]
        self.renderer.trim_frame_target();
        if let Err(error) = self.renderer.trim_caches() {
            tracing::debug!(%error, "GPU completion cleanup failed during cache trim");
        }
    }
    pub fn invalidate_presentation(&mut self) {
        self.abandon_pending_frame();
        self.renderer.invalidate_scene_damage();
    }
    /// Enable partial redraw; changing the mode forces a fresh complete frame.
    pub fn set_damage_tracking(&mut self, enabled: bool) {
        if self.damage_enabled != enabled {
            self.invalidate_presentation();
        }
        self.damage_enabled = enabled;
    }
    /// Read retained premultiplied RGBA8 pixels without repainting. Size is the target's own
    /// physical extent, which can differ from a newly resized native window.
    pub fn read_retained_rgba(&mut self) -> Result<Option<([u32; 2], Vec<u8>)>> {
        if self.retained_size.is_none() {
            return Ok(None);
        }
        let result = self.renderer.read_retained_rgba()?;
        #[cfg(any(target_os = "macos", target_os = "linux"))]
        if result.is_some() {
            self.idle_resources.used(std::time::Instant::now());
        }
        Ok(result)
    }
    /// Capture with this window's renderer and paragraph cache, never a different backend.
    pub fn capture_rgba(
        &mut self,
        scene: &Scene,
        size: [u32; 2],
        background: Color,
    ) -> Result<Vec<u8>> {
        // Capture may replace targets and overwrite shared upload buffers.
        self.abandon_pending_frame();
        let prepared = self.renderer.prepare_scene(scene, size)?;
        #[cfg(any(target_os = "macos", target_os = "linux"))]
        self.idle_resources.used(std::time::Instant::now());
        self.retained_size = None;
        let result = self
            .renderer
            .render_prepared(&prepared, size, background.components, 0.);
        if result.is_ok() {
            self.retained_size = Some(size);
        }
        result
    }
}

impl Drop for WgpuWindow {
    fn drop(&mut self) {
        self.abandon_pending_frame();
        // Join before renderer/surface/window field destruction. Wake callbacks
        // are event-loop proxies and cannot wait on this dropping host thread.
        #[cfg(any(target_os = "macos", all(test, not(target_arch = "wasm32"))))]
        {
            self.surface_acquire = None;
        }
    }
}

#[cfg(test)]
mod tests {
    #[cfg(target_os = "linux")]
    #[test]
    #[ignore = "requires an X11 display and working wgpu adapter (can run under Xvfb)"]
    fn native_surface_resize_waits_for_presentation_and_uses_live_extent() {
        use super::*;
        use std::sync::atomic::{AtomicBool, Ordering};
        use winit::{
            application::ApplicationHandler,
            event::WindowEvent,
            event_loop::{ActiveEventLoop, EventLoop},
            platform::x11::EventLoopBuilderExtX11,
            window::WindowId,
        };
        struct Probe(Arc<AtomicBool>);
        impl ApplicationHandler for Probe {
            fn can_create_surfaces(&mut self, event_loop: &dyn ActiveEventLoop) {
                let window: Arc<dyn Window> = Arc::from(
                    event_loop
                        .create_window(
                            winit::window::WindowAttributes::default()
                                .with_surface_size(winit::dpi::LogicalSize::new(320., 240.)),
                        )
                        .unwrap(),
                );
                let actual = window.surface_size();
                let mut gpu = WgpuWindow::new(window, false).unwrap();
                gpu.damage_enabled = true;
                // Put the real surface into the same 1x1 configuration as
                // macOS idle trim, without relying on a mocked configure call.
                let mut small = gpu.config.clone();
                small.width = 1;
                small.height = 1;
                gpu.renderer.configure_surface(&gpu.surface, &small);
                gpu.configured_size = [1, 1];
                gpu.idle_resources.trimmed();
                for (width, height) in [(640, 480), (960, 720), (0, 0)] {
                    gpu.resize(width, height);
                    let configured = gpu.surface.get_configuration().unwrap();
                    assert_eq!((configured.width, configured.height), (1, 1));
                }
                assert_eq!((gpu.config.width, gpu.config.height), (960, 720));
                let scene = Scene::new();
                let capture = gpu
                    .capture_rgba(&scene, [actual.width, actual.height], Color::BLACK)
                    .unwrap();
                assert_eq!(
                    capture.len(),
                    actual.width as usize * actual.height as usize * 4
                );
                let configured = gpu.surface.get_configuration().unwrap();
                assert_eq!(
                    (configured.width, configured.height),
                    (1, 1),
                    "offscreen capture must not restore window drawables"
                );
                assert!(
                    gpu.idle_resources.trimmed,
                    "capture may allocate a target, but did not restore window drawables"
                );
                assert!(gpu.renderer.target_storage_bytes().0 > 0);
                let deadline = gpu
                    .maintenance_deadline()
                    .expect("capture arms one-shot cleanup");
                gpu.maintain(deadline);
                assert!(gpu.maintenance_deadline().is_none());
                assert_eq!(gpu.renderer.target_storage_bytes(), (0, 0));
                assert!(gpu.present(&scene, Color::BLACK).unwrap());
                let configured = gpu.surface.get_configuration().unwrap();
                assert_eq!(
                    (configured.width, configured.height),
                    (actual.width, actual.height),
                    "presentation must override stale desired sizes with the live native extent"
                );
                gpu.resize(actual.width * 2, actual.height * 2);
                gpu.resize(actual.width, actual.height);
                assert!(gpu.present(&scene, Color::BLACK).unwrap());
                assert!(
                    gpu.last_present_skipped(),
                    "coalesced sizes do not invalidate unchanged content"
                );
                // A controlled in-flight real surface acquisition must not
                // block a changed frame or mark its encoder as presented.
                let pending_stats = gpu.pending_frame_stats();
                let (release, gate) = std::sync::mpsc::channel();
                let (began, begins) = std::sync::mpsc::channel();
                let (wake, wakes) = std::sync::mpsc::channel();
                let wake: plico_renderer::WakeCallback = Arc::new(move || {
                    let _ = wake.send(());
                });
                gpu.set_surface_wake(wake.clone());
                // The controlled resource protocol test also runs when the
                // process uses the synchronous A/B comparison switch.
                gpu.surface_wake = Some(wake.clone());
                let surface = gpu.surface.clone();
                let mut first = true;
                gpu.surface_acquire = Some(
                    crate::surface_acquire::Acquire::new(
                        move || {
                            if first {
                                first = false;
                                began.send(()).unwrap();
                                gate.recv().unwrap();
                            }
                            surface.get_current_texture()
                        },
                        wake,
                    )
                    .unwrap(),
                );
                gpu.surface_acquire.as_ref().unwrap().start();
                begins
                    .recv_timeout(std::time::Duration::from_secs(2))
                    .unwrap();
                let (returned, received) = std::sync::mpsc::channel();
                let rescue = std::thread::spawn(move || {
                    let nonblocking = received
                        .recv_timeout(std::time::Duration::from_secs(2))
                        .is_ok();
                    release.send(()).unwrap();
                    nonblocking
                });
                use plico_renderer::PaintScene;
                let mut changed = Scene::new();
                changed.fill(
                    peniko::Fill::NonZero,
                    kurbo::Affine::IDENTITY,
                    Color::from_rgb8(255, 0, 0),
                    None,
                    &kurbo::Rect::new(10., 10., 30., 30.),
                );
                assert!(gpu.present(&scene, Color::BLACK).unwrap());
                assert!(
                    gpu.last_present_skipped(),
                    "static content needs no drawable"
                );
                assert!(!gpu.surface_acquire_pending());
                assert_eq!(gpu.last_damage_coverage(), 0.);
                let pending = gpu.present(&changed, Color::BLACK).unwrap();
                assert!(
                    gpu.pending_frame.is_some(),
                    "encode while the fetch is blocked"
                );
                assert_eq!(gpu.pending_frame_stats()[0], pending_stats[0] + 1);
                assert!(!gpu.present(&changed, Color::BLACK).unwrap());
                assert_eq!(
                    gpu.pending_frame_stats()[0],
                    pending_stats[0] + 1,
                    "identical retries must retain the one encoder"
                );
                let _ = returned.send(());
                assert!(
                    rescue.join().unwrap(),
                    "UI waited for the acquisition worker"
                );
                assert!(!pending);
                assert!(gpu.surface_acquire_pending());
                assert!(
                    gpu.maintenance_deadline().is_none(),
                    "waiting for a frame is active work, not idle"
                );
                wakes
                    .recv_timeout(std::time::Duration::from_secs(2))
                    .unwrap();
                assert!(gpu.take_surface_ready());
                assert!(!gpu.take_surface_ready());
                assert!(gpu.present(&changed, Color::BLACK).unwrap());
                assert!(
                    !gpu.last_present_skipped(),
                    "waiting never establishes retained pixels"
                );
                assert!(gpu.pending_frame.is_none());
                assert_eq!(
                    gpu.pending_frame_stats()[1],
                    pending_stats[1] + 1,
                    "resume submits the encoder built before readiness"
                );
                let coverage = gpu.last_damage_coverage();
                assert!(
                    coverage > 0. && coverage < 0.1,
                    "small updates must stay partial after waiting, got {coverage}"
                );
                let (captured_size, pixels) = gpu.read_retained_rgba().unwrap().unwrap();
                assert_eq!(captured_size, [actual.width, actual.height]);
                let mut reference = pollster::block_on(Renderer::new()).unwrap();
                let expected = reference
                    .render_scene(
                        &changed,
                        [actual.width, actual.height],
                        Color::BLACK.components,
                        false,
                    )
                    .unwrap();
                assert_eq!(
                    pixels, expected,
                    "partial resumed frame must match full replay"
                );
                // Exercise invalidation with a real blocked fetch and GPU
                // command encoder, not just a mock pending-state flag.
                fn hold(
                    gpu: &mut WgpuWindow,
                ) -> (std::sync::mpsc::Sender<()>, std::sync::mpsc::Receiver<()>) {
                    gpu.surface_acquire = None;
                    // Isolate each lifecycle case with a fresh, drained swapchain.
                    // Retained scene pixels live in the separate renderer target.
                    gpu.renderer.configure_surface(&gpu.surface, &gpu.config);
                    gpu.invalidate_presentation();
                    let (release, gate) = std::sync::mpsc::channel();
                    let (began, begins) = std::sync::mpsc::channel();
                    let (wake, wakes) = std::sync::mpsc::channel();
                    let callback: plico_renderer::WakeCallback = Arc::new(move || {
                        let _ = wake.send(());
                    });
                    gpu.surface_wake = Some(callback.clone());
                    let surface = gpu.surface.clone();
                    let mut first = true;
                    gpu.surface_acquire = Some(
                        crate::surface_acquire::Acquire::new(
                            move || {
                                if first {
                                    first = false;
                                    began.send(()).unwrap();
                                    gate.recv().unwrap();
                                }
                                surface.get_current_texture()
                            },
                            callback,
                        )
                        .unwrap(),
                    );
                    gpu.surface_acquire.as_ref().unwrap().start();
                    begins
                        .recv_timeout(std::time::Duration::from_secs(2))
                        .unwrap();
                    (release, wakes)
                }
                let mut newer = Scene::new();
                newer.fill(
                    peniko::Fill::NonZero,
                    kurbo::Affine::IDENTITY,
                    Color::from_rgb8(0, 255, 0),
                    None,
                    &kurbo::Rect::new(50., 10., 80., 40.),
                );
                for operation in [
                    "replace",
                    "capture",
                    "invalidate",
                    "hide",
                    "trim",
                    "full",
                    "fit",
                ] {
                    if operation == "full" {
                        gpu.set_damage_tracking(false);
                    }
                    let (release, wakes) = hold(&mut gpu);
                    let old = gpu.read_retained_rgba().unwrap().unwrap().1;
                    let stats = gpu.pending_frame_stats();
                    // Alternate background so this cannot hit an unchanged skip.
                    assert!(!gpu.present(&changed, Color::WHITE).unwrap());
                    assert!(gpu.pending_frame.is_some());
                    assert!(!gpu.present(&changed, Color::WHITE).unwrap());
                    assert_eq!(
                        gpu.pending_frame_stats()[0],
                        stats[0] + 1,
                        "same pending contents reuse the encoder even with damage disabled"
                    );
                    assert_eq!(
                        gpu.read_retained_rgba().unwrap().unwrap().1,
                        old,
                        "encoding alone cannot alter the last presented pixels"
                    );
                    match operation {
                        "replace" => {
                            assert!(!gpu.present(&newer, Color::BLACK).unwrap());
                            assert_eq!(gpu.pending_frame_stats()[0], stats[0] + 2);
                            assert!(gpu.pending_frame.is_some());
                        }
                        "capture" => {
                            gpu.capture_rgba(&newer, [80, 80], Color::BLACK).unwrap();
                        }
                        "invalidate" => gpu.invalidate_presentation(),
                        "full" => gpu.set_damage_tracking(true),
                        _ => {}
                    }
                    release.send(()).unwrap();
                    wakes
                        .recv_timeout(std::time::Duration::from_secs(2))
                        .unwrap();
                    match operation {
                        "hide" => gpu.prepare_hide(),
                        "trim" => gpu.trim_gpu_cache(),
                        "fit" => gpu.configured_size = [1, 1],
                        _ => {}
                    }
                    if operation != "fit" {
                        assert_eq!(
                            gpu.pending_frame_stats()[2],
                            stats[2] + 1,
                            "{operation} abandons the old encoder exactly once"
                        );
                    }
                    if operation != "replace" && operation != "fit" {
                        assert!(gpu.pending_frame.is_none());
                    }
                    let mut presented = false;
                    for _ in 0..3 {
                        if gpu.present(&newer, Color::BLACK).unwrap() {
                            presented = true;
                            break;
                        }
                        // Synchronous surface failures need a bounded retry;
                        // only a worker waiter promises a readiness wake.
                        if gpu.surface_acquire_pending() {
                            if operation == "trim" {
                                assert!(
                                    gpu.read_retained_rgba().unwrap().is_none(),
                                    "unsubmitted target is not retained pixels"
                                );
                            }
                            wakes
                                .recv_timeout(std::time::Duration::from_secs(2))
                                .unwrap_or_else(|error| {
                                    panic!("{operation}: fetch wake: {error:?}")
                                });
                        }
                    }
                    assert!(presented, "{operation}: bounded acquisition recovery");
                    assert_eq!(
                        gpu.pending_frame_stats()[2],
                        stats[2] + 1,
                        "{operation}: no old encoder survives recovery"
                    );
                    assert!(!gpu.last_present_skipped());
                    let pixels = gpu.read_retained_rgba().unwrap().unwrap().1;
                    let expected = reference
                        .render_scene(
                            &newer,
                            [actual.width, actual.height],
                            Color::BLACK.components,
                            false,
                        )
                        .unwrap();
                    assert_eq!(
                        pixels, expected,
                        "{operation}: no stale uploads, false skips or old target"
                    );
                }
                let retained = gpu.renderer.target_storage_bytes();
                gpu.prepare_hide();
                assert_eq!(
                    gpu.renderer.target_storage_bytes(),
                    retained,
                    "pre-hide acquisition drain preserves the native animation image"
                );
                gpu.renderer.configure_surface(&gpu.surface, &small);
                gpu.configured_size = [1, 1];
                gpu.trim_gpu_cache();
                // configure() would panic if the prefetched SurfaceTexture
                // were still live; this exercises drain with real GPU owners.
                gpu.renderer.configure_surface(&gpu.surface, &small);
                gpu.configured_size = [1, 1];
                assert!(gpu.present(&scene, Color::BLACK).unwrap());
                gpu.surface_acquire = None;
                let (wake, failure_wakes) = std::sync::mpsc::channel();
                let wake: plico_renderer::WakeCallback = Arc::new(move || {
                    let _ = wake.send(());
                });
                gpu.surface_wake = Some(wake.clone());
                gpu.surface_acquire = Some(
                    crate::surface_acquire::Acquire::new(
                        || panic!("controlled acquisition failure"),
                        wake,
                    )
                    .unwrap(),
                );
                assert!(!gpu.present(&scene, Color::WHITE).unwrap());
                failure_wakes
                    .recv_timeout(std::time::Duration::from_secs(2))
                    .unwrap();
                assert!(gpu.present(&scene, Color::WHITE).is_err());
                assert!(!gpu.surface_prefetch_enabled());
                assert!(
                    gpu.pending_frame.is_none(),
                    "failed fetch cannot retain its encoder"
                );
                assert!(
                    gpu.present(&scene, Color::WHITE).unwrap(),
                    "a failed worker must leave synchronous recovery available"
                );
                self.0.store(true, Ordering::SeqCst);
                event_loop.exit();
            }
            fn window_event(&mut self, _: &dyn ActiveEventLoop, _: WindowId, _: WindowEvent) {}
        }
        let checked = Arc::new(AtomicBool::new(false));
        let mut builder = EventLoop::builder();
        builder.with_x11().with_any_thread(true);
        builder
            .build()
            .unwrap()
            .run_app(Probe(checked.clone()))
            .unwrap();
        assert!(checked.load(Ordering::SeqCst));
    }

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
    fn idle_resources_arm_once_follow_gpu_work_and_stop_after_trim() {
        use std::time::{Duration, Instant};
        let now = Instant::now();
        let mut idle = super::IdleResources::default();
        assert_eq!(idle.deadline, None, "hidden startup needs no trim timer");
        idle.used(now);
        assert!(!idle.due(now + Duration::from_millis(1999)));
        assert!(idle.due(now + Duration::from_secs(2)));
        idle.used(now + Duration::from_secs(1));
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
        idle.used(now + Duration::from_secs(101));
        assert!(
            idle.trimmed,
            "offscreen work must preserve the surface trim state"
        );
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
