//! Vulkan swapchain presentation on the caller's live Winit window.
use super::{VulkanDevice, new_context};
use crate::{Error, SkiaSurface};
use ash::vk::{self, Handle};
use plico_renderer::Scene;
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
    swapchain: Option<Swapchain>,
    context: gpu::DirectContext,
    owner: VulkanDevice,
    window: Arc<dyn Window>,
    desired: [u32; 2],
    transparent: bool,
    recreate: bool,
}
impl SkiaWindow {
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
            swapchain: None,
            context,
            owner,
            window,
            desired: [size.width, size.height],
            transparent,
            recreate: true,
        };
        renderer.rebuild()?;
        Ok(renderer)
    }
    /// Actual Vulkan device used for both drawing and presentation.
    pub fn device_name(&self) -> &str {
        &self.owner.name
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
        unsafe { device.wait_for_fences(&[chain.fence], true, u64::MAX) }.context(
            crate::VulkanSnafu {
                operation: "wait frame fence",
            },
        )?;
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
        let target = &mut self.targets[index as usize];
        // SAFETY: this semaphore remains owned by Swapchain until its queue is idle.
        let acquired = unsafe { gpu::backend_semaphores::make_vk(chain.acquire.as_raw() as _) };
        if !target.surface.wait(&[acquired], false) {
            return Err(Error::Submit);
        }
        let draw_result = target.draw_scene(scene, background);
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
        if self.context.flush_surface_with_texture_state(
            &mut target.surface,
            &flush,
            Some(&present),
        ) != gpu::SemaphoresSubmitted::Yes
            || !self.context.submit(None)
        {
            return Err(Error::Submit);
        }
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
        let info = vk::PresentInfoKHR::default()
            .wait_semaphores(&wait)
            .swapchains(&chains)
            .image_indices(&indices);
        // SAFETY: image is acquired, transitioned to PRESENT_SRC_KHR, and the
        // presentation queue waits for Skia's render-finished signal.
        match unsafe { chain.loader.queue_present(self.owner.queue, &info) } {
            Ok(outdated) => self.recreate |= outdated || suboptimal,
            Err(vk::Result::ERROR_OUT_OF_DATE_KHR) => self.recreate = true,
            Err(source) => {
                return Err(Error::Vulkan {
                    operation: "present",
                    source,
                });
            }
        }
        if self.recreate {
            self.window.request_redraw();
        }
        draw_result?;
        Ok(true)
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
        self.swapchain.take();
        let (chain, targets) = Swapchain::create(
            &self.owner,
            &mut self.context,
            self.desired,
            self.transparent,
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
            let surface = gpu::surfaces::wrap_backend_render_target(
                context,
                &target,
                gpu::SurfaceOrigin::TopLeft,
                color,
                None,
                None,
            )
            .ok_or(Error::Gpu {
                message: "Skia could not wrap swapchain image",
            })?;
            targets.push(SkiaSurface::from_surface(surface));
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
