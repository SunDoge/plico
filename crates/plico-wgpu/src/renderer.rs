use crate::{DEFAULT_MATERIAL, Error, Quad, Result};
use snafu::ResultExt;
use wgpu::util::DeviceExt;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Instance {
    rect: [f32; 4],
    color: [f32; 4],
    color2: [f32; 4],
    border: [f32; 4],
    config: [f32; 4],
    clip: [f32; 4],
    uv: [f32; 4],
}
struct Target {
    texture: wgpu::Texture,
    readback: Option<wgpu::Buffer>,
    size: [u32; 2],
    pitch: u32,
}
/// Owns an independent wgpu device, a persistent render target and instance buffer.
/// CPU readback is for experiments; it is not the future window submission path.
pub struct Renderer {
    device: wgpu::Device,
    queue: wgpu::Queue,
    info: wgpu::AdapterInfo,
    adapter: wgpu::Adapter,
    blitter: Option<crate::blit::Blitter>,
    layout: wgpu::PipelineLayout,
    frame: wgpu::Buffer,
    bind: wgpu::BindGroup,
    pipeline: Option<wgpu::RenderPipeline>,
    source: String,
    target: Option<Target>,
    instances: wgpu::Buffer,
    capacity: usize,
    compilations: usize,
    atlas_texture: crate::texture::AtlasTexture,
    #[cfg(all(target_os = "linux", feature = "system-text"))]
    atlas: Option<plico_native::text::atlas::Atlas>,
    #[cfg(all(target_os = "linux", feature = "system-text"))]
    text_stats: crate::text::TextStats,
}
impl Renderer {
    pub async fn new() -> Result<Self> {
        // Share the backend instance across default renderers. Besides avoiding
        // repeated backend discovery, this serializes Vulkan-loader startup.
        static INSTANCE: std::sync::OnceLock<wgpu::Instance> = std::sync::OnceLock::new();
        let instance = INSTANCE.get_or_init(|| {
            wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env())
        });
        Self::new_compatible(instance, None).await
    }
    /// Select an adapter compatible with the window surface before allocating resources.
    pub async fn new_for_surface(
        instance: &wgpu::Instance,
        surface: &wgpu::Surface<'_>,
    ) -> Result<Self> {
        Self::new_compatible(instance, Some(surface)).await
    }
    async fn new_compatible(
        instance: &wgpu::Instance,
        surface: Option<&wgpu::Surface<'_>>,
    ) -> Result<Self> {
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::LowPower,
                compatible_surface: surface,
                ..Default::default()
            })
            .await
            .context(crate::AdapterSnafu)?;
        let info = adapter.get_info();
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("Plico experimental quad renderer"),
                ..Default::default()
            })
            .await
            .context(crate::DeviceSnafu)?;
        let group = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("quad frame layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: wgpu::BufferSize::new(16),
                },
                count: None,
            }],
        });
        let atlas_texture = crate::texture::AtlasTexture::new(&device);
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[Some(&group), Some(&atlas_texture.layout)],
            immediate_size: 0,
        });
        let frame = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("quad frame"),
            size: 16,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &group,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: frame.as_entire_binding(),
            }],
        });
        let capacity = 64;
        let instances = Self::instance_buffer(&device, capacity);
        let mut renderer = Self {
            device,
            queue,
            info,
            adapter,
            blitter: None,
            layout,
            frame,
            bind,
            pipeline: None,
            source: String::new(),
            target: None,
            instances,
            capacity,
            compilations: 0,
            atlas_texture,
            #[cfg(all(target_os = "linux", feature = "system-text"))]
            atlas: None,
            #[cfg(all(target_os = "linux", feature = "system-text"))]
            text_stats: Default::default(),
        };
        renderer.set_material(DEFAULT_MATERIAL).await?;
        Ok(renderer)
    }
    pub fn adapter_info(&self) -> &wgpu::AdapterInfo {
        &self.info
    }
    /// Successful pipeline compilations, for checking that frames do not recompile shaders.
    pub fn pipeline_compilations(&self) -> usize {
        self.compilations
    }
    /// Runtime shader replacement. Invalid input preserves the last working pipeline.
    /// Loading the same source again is a no-op. No Rust compilation is involved.
    pub async fn set_material(&mut self, material: &str) -> Result<bool> {
        if material == self.source {
            return Ok(false);
        }
        if material.len() > 64 * 1024 {
            return Err(Error::Shader {
                message: "material exceeds 64 KiB".into(),
            });
        }
        let source = include_str!("quad.wgsl")
            .replace("// PLICO_INTERFACE", crate::MATERIAL_INTERFACE)
            .replace("// PLICO_MATERIAL", material);
        let module = naga::front::wgsl::parse_str(&source).map_err(|e| Error::Shader {
            message: e.emit_to_string(&source),
        })?;
        naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::default(),
        )
        .validate(&module)
        .map_err(|e| Error::Shader {
            message: e.to_string(),
        })?;
        if module
            .global_variables
            .iter()
            .filter(|(_, g)| g.binding.is_some())
            .count()
            != 3
            || module.entry_points.len() != 2
        {
            return Err(Error::Shader {
                message: "materials cannot introduce bindings or entry points".into(),
            });
        }
        let scope = self.device.push_error_scope(wgpu::ErrorFilter::Validation);
        let shader = self
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("Plico quad material"),
                source: wgpu::ShaderSource::Wgsl(source.into()),
            });
        let attributes = wgpu::vertex_attr_array![0=>Float32x4,1=>Float32x4,2=>Float32x4,3=>Float32x4,4=>Float32x4,5=>Float32x4,6=>Float32x4];
        let pipeline = self
            .device
            .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("Plico SDF quads"),
                layout: Some(&self.layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_main"),
                    compilation_options: Default::default(),
                    buffers: &[Some(wgpu::VertexBufferLayout {
                        array_stride: std::mem::size_of::<Instance>() as u64,
                        step_mode: wgpu::VertexStepMode::Instance,
                        attributes: &attributes,
                    })],
                },
                primitive: Default::default(),
                depth_stencil: None,
                multisample: Default::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some("fs_main"),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: wgpu::TextureFormat::Rgba8Unorm,
                        blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                multiview_mask: None,
                cache: None,
            });
        if let Some(error) = scope.pop().await {
            return Err(Error::Shader {
                message: error.to_string(),
            });
        }
        self.pipeline = Some(pipeline);
        self.source = material.into();
        self.compilations += 1;
        Ok(true)
    }
    fn instance_buffer(device: &wgpu::Device, capacity: usize) -> wgpu::Buffer {
        device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Plico quad instances"),
            contents: &vec![0; capacity * std::mem::size_of::<Instance>()],
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        })
    }
    fn target(&mut self, size: [u32; 2]) {
        if self.target.as_ref().is_some_and(|t| t.size == size) {
            return;
        }
        let [width, height] = size;
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Plico quad offscreen target"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::COPY_SRC
                | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let pitch = (width * 4).div_ceil(256) * 256;
        self.target = Some(Target {
            texture,
            readback: None,
            size,
            pitch,
        });
    }
    /// Draw in painter order and return tightly packed premultiplied RGBA8.
    /// Time is externally injected, in seconds; the renderer never reads a clock.
    pub fn render(
        &mut self,
        quads: &[Quad],
        size: [u32; 2],
        background: [f32; 4],
        time: f32,
    ) -> Result<Vec<u8>> {
        let items: Vec<_> = quads.iter().map(crate::Draw::Quad).collect();
        self.render_items(&items, size, background, time)
    }
    #[tracing::instrument(name="plico_wgpu.encode_frame",skip_all,fields(width=size[0],height=size[1],draws=items.len()))]
    fn encode_frame(
        &mut self,
        items: &[crate::Draw<'_>],
        size: [u32; 2],
        background: [f32; 4],
        time: f32,
    ) -> Result<wgpu::CommandEncoder> {
        let limit = self.device.limits();
        let [width, height] = size;
        let pitch = u64::from(width).saturating_mul(4).div_ceil(256) * 256;
        if width == 0
            || height == 0
            || width > limit.max_texture_dimension_2d
            || height > limit.max_texture_dimension_2d
            || pitch.saturating_mul(u64::from(height)) > limit.max_buffer_size
            || items.len() > 1_000_000
            || !time.is_finite()
            || !background
                .iter()
                .all(|v| v.is_finite() && (0. ..=1.).contains(v))
        {
            return Err(Error::InvalidInput {
                reason: "frame exceeds GPU limits or contains invalid size/time/color".into(),
            });
        }
        let instances = self.prepare_instances(items, size)?;
        tracing::trace!(instances = instances.len(), "prepared ordered instances");
        if instances.len() > 1_000_000
            || (instances.len().max(1).next_power_of_two() * std::mem::size_of::<Instance>()) as u64
                > limit.max_buffer_size
        {
            return Err(Error::InvalidInput {
                reason: "instance stream exceeds GPU limits".into(),
            });
        }
        if instances.len() > self.capacity {
            self.capacity = instances.len().next_power_of_two();
            self.instances = Self::instance_buffer(&self.device, self.capacity);
        }
        if !instances.is_empty() {
            self.queue
                .write_buffer(&self.instances, 0, bytemuck::cast_slice(&instances));
        }
        self.queue.write_buffer(
            &self.frame,
            0,
            bytemuck::cast_slice(&[width as f32, height as f32, time, 1.]),
        );
        self.target(size);
        let target = self.target.as_ref().expect("target just created");
        let view = target.texture.create_view(&Default::default());
        let mut encoder = self.device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Plico quad draw"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: f64::from(background[0] * background[3]),
                            g: f64::from(background[1] * background[3]),
                            b: f64::from(background[2] * background[3]),
                            a: f64::from(background[3]),
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            });
            pass.set_pipeline(
                self.pipeline
                    .as_ref()
                    .expect("constructor installs pipeline"),
            );
            pass.set_bind_group(0, &self.bind, &[]);
            pass.set_bind_group(1, &self.atlas_texture.bind, &[]);
            pass.set_vertex_buffer(0, self.instances.slice(..));
            pass.draw(0..6, 0..instances.len() as u32);
        }
        Ok(encoder)
    }
    /// Offscreen readback; window submission uses render_to_view instead.
    pub fn render_items(
        &mut self,
        items: &[crate::Draw<'_>],
        size: [u32; 2],
        background: [f32; 4],
        time: f32,
    ) -> Result<Vec<u8>> {
        let mut encoder = self.encode_frame(items, size, background, time)?;
        let [width, height] = size;
        let target = self.target.as_mut().expect("encode_frame created target");
        let readback = target.readback.get_or_insert_with(|| {
            self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("Plico readback"),
                size: u64::from(target.pitch) * u64::from(height),
                usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            })
        });
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &target.texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: readback,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(target.pitch),
                    rows_per_image: Some(height),
                },
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
        self.queue.submit([encoder.finish()]);
        let (tx, rx) = std::sync::mpsc::channel();
        readback
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |result| {
                let _ = tx.send(result);
            });
        self.device
            .poll(wgpu::PollType::wait_indefinitely())
            .context(crate::PollSnafu)?;
        rx.recv()
            .map_err(|e| Error::Readback {
                message: e.to_string(),
            })?
            .map_err(|e| Error::Readback {
                message: e.to_string(),
            })?;
        let data = readback
            .slice(..)
            .get_mapped_range()
            .map_err(|e| Error::Readback {
                message: e.to_string(),
            })?;
        let mut pixels = Vec::with_capacity((width * height * 4) as usize);
        for row in data.chunks_exact(target.pitch as usize) {
            pixels.extend_from_slice(&row[..(width * 4) as usize]);
        }
        drop(data);
        readback.unmap();
        Ok(pixels)
    }
}

impl Renderer {
    fn prepare_instances(
        &mut self,
        items: &[crate::Draw<'_>],
        size: [u32; 2],
    ) -> Result<Vec<Instance>> {
        #[cfg(all(target_os = "linux", feature = "system-text"))]
        {
            let result = self.encode_items(items, size);
            let result = if matches!(
                result,
                Err(Error::Text {
                    source: plico_native::text::Error::AtlasFull
                })
            ) {
                self.atlas
                    .as_mut()
                    .expect("atlas full implies atlas exists")
                    .clear();
                self.text_stats.recycled_frames += 1;
                self.encode_items(items, size)
            } else {
                result
            };
            let instances = result?;
            self.upload_glyphs();
            Ok(instances)
        }
        #[cfg(not(all(target_os = "linux", feature = "system-text")))]
        self.encode_items(items, size)
    }
    fn encode_items(&mut self, items: &[crate::Draw<'_>], size: [u32; 2]) -> Result<Vec<Instance>> {
        let [width, height] = size;
        let mut instances = Vec::new();
        for item in items {
            match item {
                crate::Draw::Quad(q) => {
                    q.validate()?;
                    if q.rect[2] <= 0. || q.rect[3] <= 0. {
                        continue;
                    }
                    instances.push(Instance {
                        rect: q.rect,
                        color: q.colors[0],
                        color2: q.colors[1],
                        border: q.border_color,
                        config: [
                            q.radius,
                            q.border_width,
                            if q.material { 1. } else { 0. },
                            q.parameter,
                        ],
                        clip: q.clip.unwrap_or([0., 0., width as f32, height as f32]),
                        uv: [0.; 4],
                    });
                }
                #[cfg(all(target_os = "linux", feature = "system-text"))]
                crate::Draw::Text(run) => self.encode_text(run, size, &mut instances)?,
            }
            if instances.len() > 1_000_000 {
                return Err(Error::InvalidInput {
                    reason: "too many visible instances".into(),
                });
            }
        }
        Ok(instances)
    }
}
#[cfg(all(target_os = "linux", feature = "system-text"))]
impl Renderer {
    /// Shape at the requested DPI; retain this layout until text/style/DPI changes.
    /// System fallback omissions are observable through `missing_glyphs`.
    #[tracing::instrument(name="plico_wgpu.layout_text",skip_all,fields(bytes=request.text.len(),size=request.size,scale=request.scale))]
    pub fn layout_text(
        &mut self,
        request: &crate::text::TextRequest<'_>,
    ) -> Result<crate::text::GlyphLayout> {
        if self.atlas.is_none() {
            self.atlas = Some(plico_native::text::atlas::Atlas::new().context(crate::TextSnafu)?);
        }
        self.atlas
            .as_ref()
            .unwrap()
            .layout(request)
            .context(crate::TextSnafu)
    }
    pub fn text_stats(&self) -> crate::text::TextStats {
        let mut stats = self.text_stats;
        if let Some(atlas) = &self.atlas {
            stats.cached_glyphs = atlas.entry_count();
            stats.atlas_cpu_bytes = atlas.pixels.len();
        }
        stats.atlas_gpu_bytes = (self.atlas_texture.side * self.atlas_texture.side * 4) as usize;
        stats
    }
    fn encode_text(
        &mut self,
        run: &crate::text::TextRun<'_>,
        size: [u32; 2],
        instances: &mut Vec<Instance>,
    ) -> Result<()> {
        if !run.origin.iter().all(|v| v.is_finite() && v.abs() < 1e9)
            || !run
                .color
                .iter()
                .all(|v| v.is_finite() && (0. ..=1.).contains(v))
        {
            return Err(Error::InvalidInput {
                reason: "invalid text origin or color".into(),
            });
        }
        let clip = run.clip.unwrap_or([0., 0., size[0] as f32, size[1] as f32]);
        let mut validation = Quad::new(clip, run.color);
        validation.clip = Some(clip);
        validation.validate()?;
        if self.atlas.is_none() {
            self.atlas = Some(plico_native::text::atlas::Atlas::new().context(crate::TextSnafu)?);
        }
        let atlas = self.atlas.as_mut().unwrap();
        let side = plico_native::text::atlas::ATLAS_SIZE as f32;
        for glyph in &run.layout.glyphs {
            let x = glyph.x + run.origin[0];
            let y = glyph.y + run.origin[1];
            let quantized = (x * 4.).round() as i64;
            let (x, y, phase) = if run.continuous {
                (x, y, 0)
            } else {
                (
                    quantized.div_euclid(4) as f64,
                    y.round(),
                    quantized.rem_euclid(4) as u8,
                )
            };
            let [ix, iy, iw, ih] = glyph.ink_bounds;
            // Cull before rasterization/upload. Include hinting/phase/padding fringe.
            if iw <= 0.
                || ih <= 0.
                || x + ix + iw + 3. < f64::from(clip[0])
                || y + iy + ih + 3. < f64::from(clip[1])
                || x + ix - 3. > f64::from(clip[0] + clip[2])
                || y + iy - 3. > f64::from(clip[1] + clip[3])
            {
                continue;
            }
            let entry = atlas.get(glyph, phase).context(crate::TextSnafu)?;
            let [ax, ay, w, h] = entry.rect;
            let rect = [
                x as f32 + entry.origin[0] as f32,
                y as f32 + entry.origin[1] as f32,
                w as f32,
                h as f32,
            ];
            if rect[0] + rect[2] < clip[0]
                || rect[1] + rect[3] < clip[1]
                || rect[0] > clip[0] + clip[2]
                || rect[1] > clip[1] + clip[3]
            {
                continue;
            }
            instances.push(Instance {
                rect,
                color: run.color,
                color2: [0.; 4],
                border: [0.; 4],
                config: [0., 0., if entry.colored { 3. } else { 2. }, 0.],
                clip,
                uv: [
                    ax as f32 / side,
                    ay as f32 / side,
                    w as f32 / side,
                    h as f32 / side,
                ],
            });
        }
        Ok(())
    }
    fn upload_glyphs(&mut self) {
        let Some(atlas) = &mut self.atlas else {
            return;
        };
        let side = plico_native::text::atlas::ATLAS_SIZE;
        self.atlas_texture.ensure_size(&self.device, side);
        for [x, y, w, h] in atlas.take_dirty() {
            let start = ((y * side + x) * 4) as usize;
            let end = start + (((h - 1) * side + w) * 4) as usize;
            self.queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &self.atlas_texture.texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d { x, y, z: 0 },
                    aspect: wgpu::TextureAspect::All,
                },
                &atlas.pixels[start..end],
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(side * 4),
                    rows_per_image: None,
                },
                wgpu::Extent3d {
                    width: w,
                    height: h,
                    depth_or_array_layers: 1,
                },
            );
            self.text_stats.uploaded_bytes += u64::from(w * h * 4);
            self.text_stats.uploaded_regions += 1;
        }
    }
}

impl Renderer {
    pub fn surface_capabilities(&self, surface: &wgpu::Surface<'_>) -> wgpu::SurfaceCapabilities {
        surface.get_capabilities(&self.adapter)
    }
    pub fn configure_surface(
        &self,
        surface: &wgpu::Surface<'_>,
        config: &wgpu::SurfaceConfiguration,
    ) {
        surface.configure(&self.device, config);
    }
    /// GPU-only submit to a window/offscreen view, without readback or CPU waiting.
    /// Destination must have the requested size and format and support RENDER_ATTACHMENT.
    #[tracing::instrument(name="plico_wgpu.render_to_view",skip_all,fields(width=size[0],height=size[1]))]
    pub fn render_to_view(
        &mut self,
        items: &[crate::Draw<'_>],
        size: [u32; 2],
        background: [f32; 4],
        time: f32,
        destination: &wgpu::TextureView,
        format: wgpu::TextureFormat,
    ) -> Result<()> {
        if !matches!(
            format,
            wgpu::TextureFormat::Rgba8Unorm
                | wgpu::TextureFormat::Rgba8UnormSrgb
                | wgpu::TextureFormat::Bgra8Unorm
                | wgpu::TextureFormat::Bgra8UnormSrgb
        ) {
            return Err(Error::InvalidInput {
                reason: "unsupported present target format".into(),
            });
        }
        let mut encoder = self.encode_frame(items, size, background, time)?;
        if self.blitter.as_ref().is_none_or(|b| b.format != format) {
            self.blitter = Some(crate::blit::Blitter::new(&self.device, format));
        }
        let view = self
            .target
            .as_ref()
            .unwrap()
            .texture
            .create_view(&Default::default());
        self.blitter
            .as_ref()
            .unwrap()
            .encode(&self.device, &mut encoder, &view, destination);
        self.queue.submit([encoder.finish()]);
        Ok(())
    }
}

impl Renderer {
    /// Present a frame submitted on this renderer's queue.
    pub fn present(&self, frame: wgpu::SurfaceTexture) {
        self.queue.present(frame);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "requires a working wgpu adapter"]
    fn gpu_present_preserves_encoded_pixels_on_unorm_and_srgb_targets() {
        let mut renderer = pollster::block_on(Renderer::new()).unwrap();
        let mut q = Quad::new([2.25, 3.5, 24., 23.], [0.15, 0.35, 0.65, 0.7]);
        q.radius = 5.;
        q.colors[1] = [0.65, 0.2, 0.1, 0.3];
        let size = [32, 32];
        let expected = renderer.render(&[q], size, [0.; 4], 0.).unwrap();
        for format in [
            wgpu::TextureFormat::Rgba8Unorm,
            wgpu::TextureFormat::Rgba8UnormSrgb,
            wgpu::TextureFormat::Bgra8Unorm,
            wgpu::TextureFormat::Bgra8UnormSrgb,
        ] {
            let texture = renderer.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("present oracle"),
                size: wgpu::Extent3d {
                    width: 32,
                    height: 32,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            });
            renderer
                .render_to_view(
                    &[crate::Draw::Quad(&q)],
                    size,
                    [0.; 4],
                    0.,
                    &texture.create_view(&Default::default()),
                    format,
                )
                .unwrap();
            let buffer = renderer.device.create_buffer(&wgpu::BufferDescriptor {
                label: None,
                size: 256 * 32,
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                mapped_at_creation: false,
            });
            let mut encoder = renderer.device.create_command_encoder(&Default::default());
            encoder.copy_texture_to_buffer(
                wgpu::TexelCopyTextureInfo {
                    texture: &texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                wgpu::TexelCopyBufferInfo {
                    buffer: &buffer,
                    layout: wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(256),
                        rows_per_image: Some(32),
                    },
                },
                wgpu::Extent3d {
                    width: 32,
                    height: 32,
                    depth_or_array_layers: 1,
                },
            );
            renderer.queue.submit([encoder.finish()]);
            let (tx, rx) = std::sync::mpsc::channel();
            buffer.slice(..).map_async(wgpu::MapMode::Read, move |r| {
                tx.send(r).unwrap();
            });
            renderer
                .device
                .poll(wgpu::PollType::wait_indefinitely())
                .unwrap();
            rx.recv().unwrap().unwrap();
            let data = buffer.slice(..).get_mapped_range().unwrap();
            for (y, row) in data.as_chunks::<256>().0.iter().enumerate() {
                for (x, pixel) in row[..128].as_chunks::<4>().0.iter().enumerate() {
                    let mut actual: [u8; 4] = *pixel;
                    if matches!(
                        format,
                        wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Bgra8UnormSrgb
                    ) {
                        actual.swap(0, 2);
                    }
                    for c in 0..4 {
                        assert!(
                            actual[c].abs_diff(expected[(y * 32 + x) * 4 + c]) <= 1,
                            "{format:?} pixel {x},{y} differs: {actual:?}"
                        );
                    }
                }
            }
            drop(data);
            buffer.unmap();
        }
    }
}

#[cfg(all(test, target_os = "linux", feature = "system-text"))]
mod atlas_tests {
    use super::*;
    #[test]
    #[ignore = "requires system fonts and a working wgpu adapter"]
    fn full_atlas_recycles_before_draw_without_stale_uvs() {
        use crate::{
            Draw,
            text::{TextRequest, TextRun},
        };
        let mut renderer = pollster::block_on(Renderer::new()).unwrap();
        let mut first = TextRequest::new("W");
        first.size = 1000.;
        let layout = renderer.layout_text(&first).unwrap();
        let glyph = &layout.glyphs[0];
        let origin = [
            8. - glyph.x - glyph.ink_bounds[0],
            8. - glyph.y - glyph.ink_bounds[1],
        ];
        let draws = [Draw::Text(TextRun::new(&layout, origin, [0., 0., 0., 1.]))];
        let expected = renderer
            .render_items(&draws, [256, 64], [1.; 4], 0.)
            .unwrap();
        let mut request = TextRequest::new("MNABCDEFGH");
        request.size = 1000.;
        let large = renderer.layout_text(&request).unwrap();
        let atlas = renderer.atlas.as_mut().unwrap();
        // Simulate another document evicting the first frame's glyphs before
        // filling the fixed storage; the positioned layout itself stays alive.
        atlas.clear();
        let mut full = false;
        'fill: for glyph in &large.glyphs {
            for phase in 0..4 {
                match atlas.get(glyph, phase) {
                    Ok(_) => {}
                    Err(plico_native::text::Error::AtlasFull) => {
                        full = true;
                        break 'fill;
                    }
                    Err(e) => panic!("{e}"),
                }
            }
        }
        assert!(full, "fixture did not exhaust the atlas");
        let pixels = renderer
            .render_items(&draws, [256, 64], [1.; 4], 0.)
            .unwrap();
        assert_eq!(
            pixels, expected,
            "cache recycling used stale glyph rectangles"
        );
        let stats = renderer.text_stats();
        assert_eq!(stats.recycled_frames, 1);
        assert_eq!(stats.atlas_cpu_bytes, 16 * 1024 * 1024);
        assert_eq!(stats.atlas_gpu_bytes, 16 * 1024 * 1024);
    }
}
