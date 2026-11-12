//! Linux Vulkan DMA-BUF import through Plico's ordinary external-surface path.

#[cfg(target_os = "linux")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use plico_renderer::{ExternalSurface, Scene};
    use plico_wgpu::{ExternalTexture, Renderer};
    use std::sync::Arc;
    use wgpu_dmabuf::{
        format::{Fourcc, LINEAR},
        testkit::GbmAllocator,
    };

    const WIDTH: u32 = 256;
    const HEIGHT: u32 = 256;

    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
        backends: wgpu::Backends::VULKAN,
        ..wgpu::InstanceDescriptor::new_without_display_handle_from_env()
    });
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        force_fallback_adapter: false,
        compatible_surface: None,
        ..Default::default()
    }))?;
    let adapter_name = adapter.get_info().name;
    let (device, queue, caps) = wgpu_dmabuf::device::open_device(&adapter, &Default::default())?;
    if !caps.dmabuf {
        return Err("Vulkan adapter does not expose DMA-BUF external memory".into());
    }

    let allocator = GbmAllocator::open_default().ok_or("no usable DRM render node")?;
    let buffer = allocator.allocate_linear(WIDTH, HEIGHT, Fourcc::Argb8888, |x, y| {
        let red = (x.wrapping_mul(3).wrapping_add(11) % 256) as u8;
        let green = (y.wrapping_mul(5).wrapping_add(29) % 256) as u8;
        let blue = ((x ^ y) % 256) as u8;
        [blue, green, red, 255]
    })?;
    if buffer.dmabuf.modifier != LINEAR {
        return Err(format!(
            "GBM returned modifier {:#x}, expected LINEAR",
            buffer.dmabuf.modifier
        )
        .into());
    }
    let texture = wgpu_dmabuf::import(
        &device,
        &buffer.dmabuf,
        wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_SRC,
    )?;

    let mut renderer = pollster::block_on(Renderer::from_device(adapter, device, queue))?;
    let resource = Arc::new(ExternalTexture::new(texture, [WIDTH, HEIGHT]));
    let mut scene = Scene::new();
    scene.draw_external_surface(
        ExternalSurface::new(1, 1, [WIDTH, HEIGHT], "wgpu", resource),
        kurbo::Affine::IDENTITY,
        1.,
    );
    let pixels = renderer.render_scene(&scene, [WIDTH, HEIGHT], [0., 0., 0., 1.], false)?;

    for [x, y] in [[0, 0], [17, 31], [128, 64], [255, 255]] {
        let offset = ((y * WIDTH + x) * 4) as usize;
        let actual = &pixels[offset..offset + 4];
        let expected = [
            (x.wrapping_mul(3).wrapping_add(11) % 256) as u8,
            (y.wrapping_mul(5).wrapping_add(29) % 256) as u8,
            ((x ^ y) % 256) as u8,
            255,
        ];
        if actual != expected {
            return Err(format!(
                "DMA-BUF sample mismatch at ({x},{y}): {actual:?} != {expected:?}"
            )
            .into());
        }
    }

    println!(
        "PASS: GBM {WIDTH}x{HEIGHT} ARGB8888 DMA-BUF imported into wgpu and composed by Plico on {adapter_name}; caps={caps:?}"
    );
    Ok(())
}

#[cfg(not(target_os = "linux"))]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    Err("dmabuf-probe is Linux-only".into())
}
