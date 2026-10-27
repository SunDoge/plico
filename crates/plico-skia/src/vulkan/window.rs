//! Vulkan swapchain presentation on the caller's live Winit window.
use super::{VulkanDevice, new_context};
use crate::{Error, SkiaSurface};
use ash::vk::{self, Handle};
use plico_renderer::{DamageRegion, Scene};
use raw_window_handle::{HasDisplayHandle, HasWindowHandle};
use skia_safe::gpu;
use snafu::ResultExt;
use std::sync::Arc;
use vello_common::peniko::Color;
use winit::window::Window;

/// Direct Skia/Vulkan swapchain renderer. The window is retained until after
/// Vulkan surfaces are destroyed. No frame readback or texture upload occurs.
pub struct SkiaWindow {
    targets: Vec<SkiaSurface>,
    resources: std::rc::Rc<std::cell::RefCell<crate::raster::Resources>>,
    swapchain: Option<Swapchain>,
    context: gpu::DirectContext,
    owner: VulkanDevice,
    window: Arc<dyn Window>,
    desired: [u32; 2],
    transparent: bool,
    recreate: bool,
    damage_tracking: bool,
    damage_validation: bool,
    last_damage: DamageRegion,
    incremental_present: bool,
    display_history: plico_renderer::damage::SceneDamageTracker,
    display_background: Option<Color>,
    last_present_damage: DamageRegion,
}
impl SkiaWindow {
    /// Use the window's shared SkParagraph engine for ordinary text.
    pub fn configure_text(&self, text: &mut plico_renderer::TextContext) {
        text.set_paragraph_engine(Box::new(crate::paragraph::Engine(
            self.resources.borrow().paragraphs.clone(),
        )));
    }
    /// Create a present-capable Vulkan device and swapchain for a Winit window.
    pub fn new(
        window: Arc<dyn Window>,
        transparent: bool,
        device_name: Option<&str>,
    ) -> Result<Self, Error> {
        let display = window
            .display_handle()
            .context(crate::HandleSnafu)?
            .as_raw();
        let handle = window.window_handle().context(crate::HandleSnafu)?.as_raw();
        let extensions =
            ash_window::enumerate_required_extensions(display).context(crate::VulkanSnafu {
                operation: "query window extensions",
            })?;
        // The local Arc outlives owner on initialization failure; the returned
        // renderer retains it until after owner on success.
        let owner = VulkanDevice::create(device_name, extensions, |entry, instance| {
            // SAFETY: raw handles refer to the retained, live Winit window.
            let surface =
                unsafe { ash_window::create_surface(entry, instance, display, handle, None) }
                    .context(crate::VulkanSnafu {
                        operation: "create window surface",
                    })?;
            Ok(Some((
                ash::khr::surface::Instance::new(entry, instance),
                surface,
            )))
        })?;
        let context = new_context(&owner)?;
        let size = window.surface_size();
        let mut renderer = Self {
            targets: Vec::new(),
            resources: Default::default(),
            swapchain: None,
            context,
            owner,
            window,
            desired: [size.width, size.height],
            transparent,
            recreate: true,
            damage_tracking: false,
            damage_validation: false,
            last_damage: DamageRegion::Full,
            incremental_present: false,
            display_history: Default::default(),
            display_background: None,
            last_present_damage: DamageRegion::Full,
        };
        renderer.rebuild()?;
        Ok(renderer)
    }
    /// Actual Vulkan device used for both drawing and presentation.
    pub fn device_name(&self) -> &str {
        &self.owner.name
    }
    /// Current GPU cache accounting, without readback or synchronization.
    pub fn gpu_cache_stats(&self) -> super::GpuCacheStats {
        super::cache_stats(&self.context)
    }
    /// Select command-stream damage or unconditional full drawing (the default).
    /// History is owned per swapchain image; newly created targets draw fully.
    pub fn set_damage_tracking(&mut self, enabled: bool) {
        self.damage_tracking = enabled;
    }
    /// Expensive diagnostic: compare each tracked draw against full drawing on
    /// the same acquired GPU image before presentation. Disabled by default.
    pub fn set_damage_validation(&mut self, enabled: bool) {
        self.damage_validation = enabled;
    }
    /// Physical-pixel damage applied to the last acquired image. This can be
    /// larger than display-to-display damage when that image is several frames old.
    pub fn last_damage(&self) -> DamageRegion {
        self.last_damage
    }
    /// Whether the device and current surface support our incremental-present
    /// path. Rotated surfaces use ordinary full presentation until coordinate
    /// conversion is implemented; partial drawing remains independent.
    pub fn incremental_present_supported(&self) -> bool {
        self.owner.incremental_present
            && self
                .swapchain
                .as_ref()
                .is_some_and(|s| s.identity_transform)
    }
    /// Enable optional system presentation hints. Disabled by default: callers
    /// should A/B measure compositor benefits against the extra comparison.
    pub fn set_incremental_present(&mut self, enabled: bool) {
        if self.incremental_present != enabled {
            self.display_history.invalidate();
            self.display_background = None;
        }
        self.incremental_present = enabled;
    }
    /// Damage relative to the last presented frame, for the compositor hint.
    /// This differs from [`Self::last_damage`], which repairs an acquired buffer.
    pub fn last_present_damage(&self) -> DamageRegion {
        self.last_present_damage
    }
    /// Defer swapchain recreation until the next nonzero-sized frame.
    pub fn resize(&mut self, width: u32, height: u32) {
        self.desired = [width, height];
        self.recreate = true;
    }
    /// Draw to an acquired swapchain image and present it. False means the
    /// surface is minimized/outdated; the caller should await the next redraw.
    pub fn present(&mut self, scene: &Scene, background: Color) -> Result<bool, Error> {
        let result = self.present_frame(scene, background);
        if result.is_err() {
            self.recreate = true;
        }
        result
    }
    fn present_frame(&mut self, scene: &Scene, background: Color) -> Result<bool, Error> {
        let size = self.window.surface_size();
        if size.width == 0 || size.height == 0 {
            return Ok(false);
        }
        if self.recreate {
            self.rebuild()?;
        }
        let Some(chain) = self.swapchain.as_mut() else {
            return Ok(false);
        };
        let device = self.owner.device.as_ref().expect("live device");
        // SAFETY: this single-frame fence protects reuse of the acquire semaphore.
        // The presentation-complete semaphores are per-image, not per-frame:
        // reacquiring that image guarantees its previous presentation wait ended.
        #[cfg(feature = "profiling")]
        let wait_span = tracing::trace_span!(target: "plico::skia", "skia.wait_fence").entered();
        unsafe { device.wait_for_fences(&[chain.fence], true, u64::MAX) }.context(
            crate::VulkanSnafu {
                operation: "wait frame fence",
            },
        )?;
        #[cfg(feature = "profiling")]
        drop(wait_span);
        #[cfg(feature = "profiling")]
        let acquire_span = tracing::trace_span!(target: "plico::skia", "skia.acquire").entered();
        let (index, suboptimal) = match unsafe {
            chain.loader.acquire_next_image(
                chain.handle,
                u64::MAX,
                chain.acquire,
                vk::Fence::null(),
            )
        } {
            Ok(value) => value,
            Err(vk::Result::ERROR_OUT_OF_DATE_KHR) => {
                self.recreate = true;
                self.window.request_redraw();
                return Ok(false);
            }
            Err(source) => {
                return Err(Error::Vulkan {
                    operation: "acquire swapchain image",
                    source,
                });
            }
        };
        #[cfg(feature = "profiling")]
        drop(acquire_span);
        let target = &mut self.targets[index as usize];
        // SAFETY: this semaphore remains owned by Swapchain until its queue is idle.
        let acquired = unsafe { gpu::backend_semaphores::make_vk(chain.acquire.as_raw() as _) };
        if !target.surface.wait(&[acquired], false) {
            return Err(Error::Submit);
        }
        let draw_result = if self.damage_tracking {
            target.draw_scene_tracked(scene, background).map(|damage| {
                self.last_damage = damage;
            })
        } else {
            self.last_damage = DamageRegion::Full;
            target.draw_scene(scene, background)
        }
        .and_then(|()| {
            if self.damage_validation {
                target.validate_scene(scene, background)
            } else {
                Ok(())
            }
        });
        if draw_result.is_err() {
            // Release acquired images with a clean background, never partial UI.
            // The original failure is returned after submission/presentation.
            target.draw_scene(&Scene::new(), background)?;
        }
        // SAFETY: render-finished remains alive until the acquired image's next
        // presentation is complete; the mutable slice outlives this flush.
        let mut finished = [unsafe {
            gpu::backend_semaphores::make_vk(chain.finished[index as usize].as_raw() as _)
        }];
        let mut flush = gpu::FlushInfo::default();
        unsafe {
            flush.set_signal_semaphores(&mut finished);
        }
        let present = gpu::vk::mutable_texture_states::new_vulkan(
            gpu::vk::ImageLayout::PRESENT_SRC_KHR,
            self.owner.queue_family,
        );
        #[cfg(feature = "profiling")]
        let submit_span =
            tracing::trace_span!(target: "plico::skia", "skia.flush_submit").entered();
        if self.context.flush_surface_with_texture_state(
            &mut target.surface,
            &flush,
            Some(&present),
        ) != gpu::SemaphoresSubmitted::Yes
            || !self.context.submit(None)
        {
            return Err(Error::Submit);
        }
        #[cfg(feature = "profiling")]
        drop(submit_span);
        // Associate a fence with the already-submitted Skia work. No CPU pixel
        // readback or device-wide wait is required for normal presentation.
        unsafe {
            device
                .reset_fences(&[chain.fence])
                .context(crate::VulkanSnafu {
                    operation: "reset frame fence",
                })?;
            device
                .queue_submit(self.owner.queue, &[], chain.fence)
                .context(crate::VulkanSnafu {
                    operation: "fence Skia submission",
                })?;
        }
        let wait = [chain.finished[index as usize]];
        let chains = [chain.handle];
        let indices = [index];
        let mut info = vk::PresentInfoKHR::default()
            .wait_semaphores(&wait)
            .swapchains(&chains)
            .image_indices(&indices);
        // Presentation damage is relative to the last image *displayed*, not
        // the prior contents of this acquired image. Never reuse target damage
        // here: A -> B -> A can repair zero pixels yet still change the display.
        self.last_present_damage = if self.incremental_present
            && self.owner.incremental_present
            && chain.identity_transform
            && draw_result.is_ok()
            && self.last_damage != DamageRegion::Full
        {
            let resources = &self.resources;
            let damage = self.display_history.update(
                scene,
                target.surface.width() as u32,
                target.surface.height() as u32,
                |command| resources.borrow_mut().ink_bounds(command),
            );
            if self.display_background == Some(background) {
                damage
            } else {
                DamageRegion::Full
            }
        } else {
            self.display_history.invalidate();
            DamageRegion::Full
        };
        self.display_background = Some(background);
        let rect = present_rectangle(
            self.last_present_damage,
            target.surface.width() as u32,
            target.surface.height() as u32,
        );
        let rectangles = rect;
        let regions = [vk::PresentRegionKHR::default().rectangles(rectangles.as_slice())];
        let mut hints = vk::PresentRegionsKHR::default().regions(&regions);
        if rectangles.is_some() {
            info = info.push_next(&mut hints);
        }
        // Wayland uses this notification to schedule frame callbacks. It must
        // precede the buffer commit; other Winit platforms make it a no-op.
        self.window.pre_present_notify();
        // SAFETY: image is acquired, transitioned to PRESENT_SRC_KHR, and the
        // presentation queue waits for Skia's render-finished signal.
        #[cfg(feature = "profiling")]
        let _present_span =
            tracing::trace_span!(target: "plico::skia", "skia.queue_present").entered();
        let presented = match unsafe { chain.loader.queue_present(self.owner.queue, &info) } {
            Ok(outdated) => {
                self.recreate |= outdated || suboptimal;
                true
            }
            Err(vk::Result::ERROR_OUT_OF_DATE_KHR) => {
                self.recreate = true;
                false
            }
            Err(source) => {
                return Err(Error::Vulkan {
                    operation: "present",
                    source,
                });
            }
        };
        if self.recreate {
            self.window.request_redraw();
        }
        draw_result?;
        Ok(presented)
    }
    fn rebuild(&mut self) -> Result<(), Error> {
        if self.desired.contains(&0) {
            return Ok(());
        }
        // Drop Skia wrappers before the swapchain images they reference.
        // Idle is required only for resize/recreation/destruction, not each frame.
        self.context.flush_submit_and_sync_cpu();
        unsafe {
            self.owner
                .device
                .as_ref()
                .expect("live device")
                .device_wait_idle()
        }
        .context(crate::VulkanSnafu {
            operation: "wait before swapchain recreation",
        })?;
        self.targets.clear();
        self.display_history.invalidate();
        self.display_background = None;
        self.swapchain.take();
        let (chain, targets) = Swapchain::create(
            &self.owner,
            &mut self.context,
            self.desired,
            self.transparent,
            &self.resources,
        )?;
        self.targets = targets;
        self.swapchain = Some(chain);
        self.recreate = false;
        Ok(())
    }
}
impl Drop for SkiaWindow {
    fn drop(&mut self) {
        self.context.flush_submit_and_sync_cpu();
        if let Some(device) = &self.owner.device {
            unsafe {
                let _ = device.device_wait_idle();
            }
        }
        self.targets.clear();
        self.swapchain.take();
        // context, owner (including VkSurfaceKHR), then window drop in that order.
    }
}

struct Swapchain {
    identity_transform: bool,
    device: ash::Device,
    loader: ash::khr::swapchain::Device,
    handle: vk::SwapchainKHR,
    acquire: vk::Semaphore,
    finished: Vec<vk::Semaphore>,
    fence: vk::Fence,
}
impl Swapchain {
    fn create(
        owner: &VulkanDevice,
        context: &mut gpu::DirectContext,
        desired: [u32; 2],
        transparent: bool,
        resources: &std::rc::Rc<std::cell::RefCell<crate::raster::Resources>>,
    ) -> Result<(Self, Vec<SkiaSurface>), Error> {
        let instance = owner.instance.as_ref().expect("live instance");
        let device = owner.device.as_ref().expect("live device");
        let (surface, handle) = owner.presentation.as_ref().expect("window surface");
        // SAFETY: every handle here is owned by the live VulkanDevice.
        let caps =
            unsafe { surface.get_physical_device_surface_capabilities(owner.physical, *handle) }
                .context(crate::VulkanSnafu {
                    operation: "query surface capabilities",
                })?;
        let formats =
            unsafe { surface.get_physical_device_surface_formats(owner.physical, *handle) }
                .context(crate::VulkanSnafu {
                    operation: "query surface formats",
                })?;
        let format = formats
            .iter()
            .find(|f| {
                f.format == vk::Format::B8G8R8A8_UNORM
                    && f.color_space == vk::ColorSpaceKHR::SRGB_NONLINEAR
            })
            .or_else(|| {
                formats.iter().find(|f| {
                    f.format == vk::Format::R8G8B8A8_UNORM
                        && f.color_space == vk::ColorSpaceKHR::SRGB_NONLINEAR
                })
            })
            .ok_or(Error::Gpu {
                message: "surface has no supported unorm RGBA/BGRA format",
            })?;
        let mut extent = caps.current_extent;
        if extent.width == u32::MAX {
            extent.width =
                desired[0].clamp(caps.min_image_extent.width, caps.max_image_extent.width);
            extent.height =
                desired[1].clamp(caps.min_image_extent.height, caps.max_image_extent.height);
        }
        if extent.width == 0 || extent.height == 0 {
            return Err(Error::Surface);
        }
        let alpha = if transparent {
            [
                vk::CompositeAlphaFlagsKHR::PRE_MULTIPLIED,
                vk::CompositeAlphaFlagsKHR::POST_MULTIPLIED,
            ]
            .into_iter()
            .find(|v| caps.supported_composite_alpha.contains(*v))
            .ok_or(Error::Gpu {
                message: "surface does not support window transparency",
            })?
        } else {
            [
                vk::CompositeAlphaFlagsKHR::OPAQUE,
                vk::CompositeAlphaFlagsKHR::INHERIT,
                vk::CompositeAlphaFlagsKHR::PRE_MULTIPLIED,
            ]
            .into_iter()
            .find(|v| caps.supported_composite_alpha.contains(*v))
            .ok_or(Error::Surface)?
        };
        // Straight-alpha presentation requires a conversion pass; reject it
        // rather than presenting premultiplied pixels with the wrong contract.
        if alpha == vk::CompositeAlphaFlagsKHR::POST_MULTIPLIED {
            return Err(Error::Gpu {
                message: "straight-alpha swapchain is not implemented",
            });
        }
        let count = if caps.max_image_count == 0 {
            caps.min_image_count + 1
        } else {
            (caps.min_image_count + 1).min(caps.max_image_count)
        };
        let usage = vk::ImageUsageFlags::COLOR_ATTACHMENT
            | vk::ImageUsageFlags::TRANSFER_SRC
            | vk::ImageUsageFlags::TRANSFER_DST;
        if !caps.supported_usage_flags.contains(usage) {
            return Err(Error::Gpu {
                message: "surface lacks Skia-required transfer/color usages",
            });
        }
        let info = vk::SwapchainCreateInfoKHR::default()
            .surface(*handle)
            .min_image_count(count)
            .image_format(format.format)
            .image_color_space(format.color_space)
            .image_extent(extent)
            .image_array_layers(1)
            .image_usage(usage)
            .image_sharing_mode(vk::SharingMode::EXCLUSIVE)
            .pre_transform(caps.current_transform)
            .composite_alpha(alpha)
            .present_mode(vk::PresentModeKHR::FIFO)
            .clipped(true);
        let loader = ash::khr::swapchain::Device::new(instance, device);
        let handle =
            unsafe { loader.create_swapchain(&info, None) }.context(crate::VulkanSnafu {
                operation: "create swapchain",
            })?;
        let mut chain = Self {
            identity_transform: caps.current_transform == vk::SurfaceTransformFlagsKHR::IDENTITY,
            device: device.clone(),
            loader,
            handle,
            acquire: vk::Semaphore::null(),
            finished: Vec::new(),
            fence: vk::Fence::null(),
        };
        let images =
            unsafe { chain.loader.get_swapchain_images(handle) }.context(crate::VulkanSnafu {
                operation: "get swapchain images",
            })?;
        chain.acquire =
            unsafe { device.create_semaphore(&vk::SemaphoreCreateInfo::default(), None) }.context(
                crate::VulkanSnafu {
                    operation: "create acquire semaphore",
                },
            )?;
        chain.fence = unsafe {
            device.create_fence(
                &vk::FenceCreateInfo::default().flags(vk::FenceCreateFlags::SIGNALED),
                None,
            )
        }
        .context(crate::VulkanSnafu {
            operation: "create frame fence",
        })?;
        let mut targets = Vec::new();
        let (skformat, color) = if format.format == vk::Format::B8G8R8A8_UNORM {
            (
                gpu::vk::Format::B8G8R8A8_UNORM,
                skia_safe::ColorType::BGRA8888,
            )
        } else {
            (
                gpu::vk::Format::R8G8B8A8_UNORM,
                skia_safe::ColorType::RGBA8888,
            )
        };
        for image in images {
            let semaphore =
                unsafe { device.create_semaphore(&vk::SemaphoreCreateInfo::default(), None) }
                    .context(crate::VulkanSnafu {
                        operation: "create present semaphore",
                    })?;
            chain.finished.push(semaphore);
            // SAFETY: swapchain images outlive the private Skia wrappers. Skia
            // does not own their memory; the Vulkan allocation remains default.
            let mut image_info = unsafe {
                gpu::vk::ImageInfo::new(
                    image.as_raw() as _,
                    Default::default(),
                    gpu::vk::ImageTiling::OPTIMAL,
                    gpu::vk::ImageLayout::UNDEFINED,
                    skformat,
                    1,
                    owner.queue_family,
                    None,
                    None,
                    None,
                )
            };
            image_info.image_usage_flags = usage.as_raw();
            let target = gpu::backend_render_targets::make_vk(
                (extent.width as i32, extent.height as i32),
                &image_info,
            );
            let props = super::text_surface_props();
            let surface = gpu::surfaces::wrap_backend_render_target(
                context,
                &target,
                gpu::SurfaceOrigin::TopLeft,
                color,
                None,
                Some(&props),
            )
            .ok_or(Error::Gpu {
                message: "Skia could not wrap swapchain image",
            })?;
            targets.push(SkiaSurface::with_resources(surface, resources.clone()));
        }
        Ok((chain, targets))
    }
}
impl Drop for Swapchain {
    fn drop(&mut self) {
        // SAFETY: Skia wrappers are dropped first; all GPU and present operations
        // finish before semaphore/fence/image destruction, including error paths.
        unsafe {
            let _ = self.device.device_wait_idle();
            for semaphore in self.finished.drain(..) {
                self.device.destroy_semaphore(semaphore, None);
            }
            if self.acquire != vk::Semaphore::null() {
                self.device.destroy_semaphore(self.acquire, None);
            }
            if self.fence != vk::Fence::null() {
                self.device.destroy_fence(self.fence, None);
            }
            self.loader.destroy_swapchain(self.handle, None);
        }
    }
}

// Zero rectangles means *full* presentation in Vulkan, not empty damage.
// Full/empty/unknown regions omit the optional hint entirely.
fn present_rectangle(damage: DamageRegion, width: u32, height: u32) -> Option<vk::RectLayerKHR> {
    let DamageRegion::Rect(r) = damage.for_surface(width, height) else {
        return None;
    };
    Some(vk::RectLayerKHR {
        offset: vk::Offset2D {
            x: r.x0 as i32,
            y: r.y0 as i32,
        },
        extent: vk::Extent2D {
            width: r.width() as u32,
            height: r.height() as u32,
        },
        layer: 0,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use vello_common::kurbo::Rect;
    #[test]
    fn present_regions_round_outwards_and_clamp_to_surface() {
        let r =
            present_rectangle(DamageRegion::Rect(Rect::new(-1.2, 2.2, 8.1, 90.)), 10, 20).unwrap();
        assert_eq!((r.offset.x, r.offset.y), (0, 2));
        assert_eq!((r.extent.width, r.extent.height, r.layer), (9, 18, 0));
        assert!(present_rectangle(DamageRegion::Full, 10, 20).is_none());
        assert!(present_rectangle(DamageRegion::Empty, 10, 20).is_none());
        assert!(
            present_rectangle(DamageRegion::Rect(Rect::new(f64::NAN, 0., 2., 2.)), 10, 20)
                .is_none()
        );
        assert!(
            present_rectangle(DamageRegion::Rect(Rect::new(20., 0., 30., 1.)), 10, 20).is_none()
        );
    }
}
