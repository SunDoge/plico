//! Owned Vulkan device and Ganesh render target. No window, swapchain or CPU upload.
use crate::{Error, SkiaSurface};
use ash::{
    Entry,
    vk::{self, Handle},
};
use plico_renderer::Scene;
use skia_safe::gpu;
use snafu::ResultExt;
use std::{ffi::CStr, marker::PhantomData, path::Path, rc::Rc};
use vello_common::peniko::Color;

/// Ganesh resource-cache accounting; excludes driver allocations and CPU assets.
#[derive(Clone, Copy, Debug)]
pub struct GpuCacheStats {
    /// Budgeted resources currently retained, including resources still in use.
    pub bytes: usize,
    /// Unlocked resources Skia can evict without discarding live content.
    pub purgeable_bytes: usize,
    /// Configured cache budget; live resources can exceed it.
    pub limit_bytes: usize,
}
pub(super) fn cache_stats(context: &gpu::DirectContext) -> GpuCacheStats {
    GpuCacheStats {
        bytes: context.resource_cache_usage().resource_bytes,
        purgeable_bytes: context.resource_cache_purgeable_bytes(),
        limit_bytes: context.resource_cache_limit(),
    }
}

/// A Vulkan-backed Skia surface with its own device and graphics queue.
///
/// Keep it on the creating thread. Fields are private so surfaces/contexts cannot
/// escape their Vulkan owner. Declaration order drops the surface, then Skia
/// context, then Vulkan device/instance/loader.
pub struct SkiaGpuSurface {
    surface: SkiaSurface,
    context: gpu::DirectContext,
    owner: VulkanDevice,
    _thread: PhantomData<Rc<()>>,
}
impl SkiaGpuSurface {
    /// Use this renderer's SkParagraph cache for host text layout.
    pub fn configure_text(&self, text: &mut plico_renderer::TextContext) {
        self.surface.configure_text(text);
    }
    /// Current GPU cache accounting, without readback or synchronization.
    pub fn gpu_cache_stats(&self) -> GpuCacheStats {
        cache_stats(&self.context)
    }
    /// Prefer a discrete GPU, then integrated/virtual GPUs, then software Vulkan.
    /// There is no CPU-raster fallback; inspect `is_software_device` when needed.
    pub fn new(width: u32, height: u32) -> Result<Self, Error> {
        Self::create(width, height, None)
    }
    /// Select a Vulkan device by case-insensitive substring of its name.
    pub fn with_device(width: u32, height: u32, device_name: &str) -> Result<Self, Error> {
        Self::create(width, height, Some(device_name))
    }
    fn create(width: u32, height: u32, name: Option<&str>) -> Result<Self, Error> {
        if width == 0 || height == 0 || width > i32::MAX as u32 || height > i32::MAX as u32 {
            return Err(Error::InvalidSize { width, height });
        }
        let owner = VulkanDevice::new(name)?;
        let mut context = new_context(&owner)?;
        let info = skia_safe::ImageInfo::new(
            (width as i32, height as i32),
            skia_safe::ColorType::RGBA8888,
            skia_safe::AlphaType::Premul,
            None,
        );
        let surface = gpu::surfaces::render_target(
            &mut context,
            gpu::Budgeted::Yes,
            &info,
            None,
            gpu::SurfaceOrigin::TopLeft,
            None,
            false,
            false,
        )
        .ok_or(Error::Surface)?;
        Ok(Self {
            surface: SkiaSurface::from_surface(surface),
            context,
            owner,
            _thread: PhantomData,
        })
    }
    /// Current render-target dimensions in physical pixels.
    pub fn dimensions(&self) -> (u32, u32) {
        (
            self.surface.surface.width() as u32,
            self.surface.surface.height() as u32,
        )
    }
    /// Replace the GPU render target while retaining the context and font cache.
    /// Allocation failure preserves the old target and its successful frame.
    pub fn resize(&mut self, width: u32, height: u32) -> Result<(), Error> {
        if width == 0 || height == 0 || width > i32::MAX as u32 || height > i32::MAX as u32 {
            return Err(Error::InvalidSize { width, height });
        }
        if self.dimensions() == (width, height) {
            return Ok(());
        }
        let info = skia_safe::ImageInfo::new(
            (width as i32, height as i32),
            skia_safe::ColorType::RGBA8888,
            skia_safe::AlphaType::Premul,
            None,
        );
        let surface = gpu::surfaces::render_target(
            &mut self.context,
            gpu::Budgeted::Yes,
            &info,
            None,
            gpu::SurfaceOrigin::TopLeft,
            None,
            false,
            false,
        )
        .ok_or(Error::Surface)?;
        self.surface.surface = surface;
        self.surface.invalidate();
        Ok(())
    }

    /// Human-readable name of the Vulkan device actually used.
    pub fn device_name(&self) -> &str {
        &self.owner.name
    }
    /// Whether Vulkan selected a CPU implementation such as lavapipe.
    pub fn is_software_device(&self) -> bool {
        self.owner.software
    }
    /// Record and submit to the GPU without reading pixels or waiting on the CPU.
    pub fn draw_scene(&mut self, scene: &Scene, background: Color) -> Result<(), Error> {
        self.surface.draw_scene(scene, background)?;
        self.context.flush_surface(&mut self.surface.surface);
        if !self.context.submit(None) {
            self.surface.invalidate();
            return Err(Error::Submit);
        }
        Ok(())
    }
    /// Submit damage-tracked drawing on this preserved GPU target.
    pub fn draw_scene_tracked(
        &mut self,
        scene: &Scene,
        background: Color,
    ) -> Result<plico_renderer::DamageRegion, Error> {
        let damage = self.surface.draw_scene_tracked(scene, background)?;
        self.context.flush_surface(&mut self.surface.surface);
        if !self.context.submit(None) {
            self.surface.invalidate();
            return Err(Error::Submit);
        }
        Ok(damage)
    }
    /// Synchronous screenshot readback, separate from normal draw/submit.
    pub fn read_pixels(&mut self) -> Result<Vec<u8>, Error> {
        self.surface.read_pixels()
    }
    /// Draw, submit, and synchronously read premultiplied RGBA pixels.
    pub fn render_scene(&mut self, scene: &Scene, background: Color) -> Result<Vec<u8>, Error> {
        self.draw_scene(scene, background)?;
        self.read_pixels()
    }
    /// Encode a screenshot. Unlike draw_scene this synchronizes with the GPU.
    pub fn write_png(&mut self, path: &Path) -> Result<(), Error> {
        // Read back explicitly: GPU image encoding without a DirectContext is
        // not supported by all Skia configurations.
        let pixels = self.read_pixels()?;
        let info = skia_safe::ImageInfo::new(
            (self.surface.surface.width(), self.surface.surface.height()),
            skia_safe::ColorType::RGBA8888,
            skia_safe::AlphaType::Premul,
            None,
        );
        let image = skia_safe::images::raster_from_data(
            &info,
            skia_safe::Data::new_copy(&pixels),
            info.min_row_bytes(),
        )
        .ok_or(Error::ReadPixels)?;
        let data = image
            .encode(None, skia_safe::EncodedImageFormat::PNG, 100)
            .ok_or(Error::Encode)?;
        std::fs::write(path, data.as_bytes()).context(crate::WriteSnafu)
    }
}
impl Drop for SkiaGpuSurface {
    fn drop(&mut self) {
        // Finish queued work while the surface, context and device are all live.
        self.context.flush_submit_and_sync_cpu();
    }
}

struct VulkanDevice {
    entry: Entry,
    instance: Option<ash::Instance>,
    device: Option<ash::Device>,
    physical: vk::PhysicalDevice,
    queue: vk::Queue,
    queue_family: u32,
    name: String,
    software: bool,
    incremental_present: bool,
    presentation: Option<(ash::khr::surface::Instance, vk::SurfaceKHR)>,
}
impl VulkanDevice {
    fn new(name: Option<&str>) -> Result<Self, Error> {
        Self::create(name, &[], |_, _| Ok(None))
    }
    fn create(
        name: Option<&str>,
        extensions: &[*const std::ffi::c_char],
        create_surface: impl FnOnce(
            &Entry,
            &ash::Instance,
        ) -> Result<
            Option<(ash::khr::surface::Instance, vk::SurfaceKHR)>,
            Error,
        >,
    ) -> Result<Self, Error> {
        // SAFETY: ash loads the system Vulkan loader; Entry remains owned until
        // every instance/device/Skia object has been destroyed.
        let entry = unsafe { Entry::load() }.context(crate::VulkanLoaderSnafu)?;
        let mut owner = Self {
            entry,
            instance: None,
            device: None,
            physical: vk::PhysicalDevice::null(),
            queue: vk::Queue::null(),
            queue_family: 0,
            name: String::new(),
            software: false,
            incremental_present: false,
            presentation: None,
        };
        let application = vk::ApplicationInfo::default().api_version(vk::API_VERSION_1_1);
        let info = vk::InstanceCreateInfo::default()
            .application_info(&application)
            .enabled_extension_names(extensions);
        // SAFETY: info's references live through this call; no custom allocator.
        owner.instance = Some(unsafe { owner.entry.create_instance(&info, None) }.context(
            crate::VulkanSnafu {
                operation: "create instance",
            },
        )?);
        let instance = owner.instance.as_ref().expect("initialized instance");
        owner.presentation = create_surface(&owner.entry, instance)?;
        // SAFETY: instance is live and exclusively owned here.
        let physicals =
            unsafe { instance.enumerate_physical_devices() }.context(crate::VulkanSnafu {
                operation: "enumerate devices",
            })?;
        let mut candidates = Vec::new();
        for physical in physicals {
            // SAFETY: enumerated physical device belongs to this live instance.
            let properties = unsafe { instance.get_physical_device_properties(physical) };
            if properties.api_version < vk::API_VERSION_1_1 {
                continue;
            }
            // SAFETY: Vulkan device_name is a NUL-terminated fixed-size array.
            let device_name = unsafe { CStr::from_ptr(properties.device_name.as_ptr()) }
                .to_string_lossy()
                .into_owned();
            if name
                .is_some_and(|filter| !device_name.to_lowercase().contains(&filter.to_lowercase()))
            {
                continue;
            }
            // SAFETY: physical device is owned by this instance.
            let families =
                unsafe { instance.get_physical_device_queue_family_properties(physical) };
            let mut index = None;
            for (i, family) in families.iter().enumerate() {
                if family.queue_count == 0 || !family.queue_flags.contains(vk::QueueFlags::GRAPHICS)
                {
                    continue;
                }
                if let Some((surface, handle)) = &owner.presentation {
                    // SAFETY: physical and surface belong to this instance.
                    if !unsafe {
                        surface.get_physical_device_surface_support(physical, i as u32, *handle)
                    }
                    .context(crate::VulkanSnafu {
                        operation: "query presentation queue",
                    })? {
                        continue;
                    }
                }
                index = Some(i);
                break;
            }
            let Some(index) = index else {
                continue;
            };
            let rank = match properties.device_type {
                vk::PhysicalDeviceType::DISCRETE_GPU => 0,
                vk::PhysicalDeviceType::INTEGRATED_GPU => 1,
                vk::PhysicalDeviceType::VIRTUAL_GPU => 2,
                vk::PhysicalDeviceType::CPU => 3,
                _ => 4,
            };
            candidates.push((rank, physical, index as u32, device_name));
        }
        candidates.sort_by_key(|c| c.0);
        let (rank, physical, queue_family, device_name) =
            candidates.into_iter().next().ok_or(Error::Gpu {
                message: "no matching Vulkan 1.1 graphics device",
            })?;
        let priorities = [1.];
        let queues = [vk::DeviceQueueCreateInfo::default()
            .queue_family_index(queue_family)
            .queue_priorities(&priorities)];
        // SAFETY: features are queried from the same physical device. Enable
        // supported core features so Skia's physical-feature discovery agrees.
        let features = unsafe { instance.get_physical_device_features(physical) };
        let mut device_extensions = if owner.presentation.is_some() {
            vec![ash::khr::swapchain::NAME.as_ptr()]
        } else {
            Vec::new()
        };
        if owner.presentation.is_some() {
            // SAFETY: physical belongs to this live instance; Vulkan returns
            // NUL-terminated extension names in these fixed-size arrays.
            let available = unsafe { instance.enumerate_device_extension_properties(physical) }
                .context(crate::VulkanSnafu {
                    operation: "query device extensions",
                })?;
            owner.incremental_present = available.iter().any(|extension| unsafe {
                CStr::from_ptr(extension.extension_name.as_ptr())
                    == ash::khr::incremental_present::NAME
            });
            if owner.incremental_present {
                device_extensions.push(ash::khr::incremental_present::NAME.as_ptr());
            }
        }
        let info = vk::DeviceCreateInfo::default()
            .queue_create_infos(&queues)
            .enabled_features(&features)
            .enabled_extension_names(&device_extensions);
        // SAFETY: all referenced create data lives through the call. Owner's
        // Drop also handles failures before logical-device creation succeeds.
        owner.device = Some(
            unsafe { instance.create_device(physical, &info, None) }.context(
                crate::VulkanSnafu {
                    operation: "create device",
                },
            )?,
        );
        owner.physical = physical;
        owner.queue_family = queue_family;
        owner.name = device_name;
        owner.software = rank == 3;
        // SAFETY: graphics queue zero was requested when creating this device.
        owner.queue = unsafe {
            owner
                .device
                .as_ref()
                .expect("initialized device")
                .get_device_queue(queue_family, 0)
        };
        Ok(owner)
    }
}
impl Drop for VulkanDevice {
    fn drop(&mut self) {
        // SAFETY: no dependent Skia objects remain when the owner is dropped.
        // On initialization errors none escaped. Each handle is destroyed once.
        unsafe {
            if let Some(device) = self.device.take() {
                let _ = device.device_wait_idle();
                device.destroy_device(None);
            }
            if let Some((loader, surface)) = self.presentation.take() {
                loader.destroy_surface(surface, None);
            }
            if let Some(instance) = self.instance.take() {
                instance.destroy_instance(None);
            }
        }
    }
}

fn new_context(owner: &VulkanDevice) -> Result<gpu::DirectContext, Error> {
    let instance = owner.instance.as_ref().expect("initialized instance");
    let device = owner.device.as_ref().expect("initialized device");
    let context = {
        let get_proc = |query| {
            // SAFETY: handles came from this live loader/instance/device;
            // Skia provides a valid NUL-terminated Vulkan function name.
            let function = unsafe {
                match query {
                    gpu::vk::GetProcOf::Instance(handle, name) => owner
                        .entry
                        .get_instance_proc_addr(vk::Instance::from_raw(handle as _), name),
                    gpu::vk::GetProcOf::Device(handle, name) => {
                        instance.get_device_proc_addr(vk::Device::from_raw(handle as _), name)
                    }
                }
            };
            function.map_or(std::ptr::null(), |function| function as *const _)
        };
        // SAFETY: owner outlives the Skia context and every render target.
        // Vulkan 1.1 is enabled; no instance or device extensions are assumed.
        let backend = unsafe {
            gpu::vk::BackendContext::new_builder(
                instance.handle().as_raw() as _,
                owner.physical.as_raw() as _,
                device.handle().as_raw() as _,
                (owner.queue.as_raw() as _, owner.queue_family as usize),
                &get_proc,
                Some(vk::API_VERSION_1_1.into()),
            )
            .build()
        };
        gpu::direct_contexts::make_vulkan(&backend, None).ok_or(Error::Gpu {
            message: "Skia Ganesh context creation failed",
        })?
    };

    Ok(context)
}

#[cfg(feature = "window")]
mod window;
#[cfg(feature = "window")]
pub use window::SkiaWindow;
