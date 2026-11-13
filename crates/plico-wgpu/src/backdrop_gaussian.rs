//! MyGO-style fixed backdrop Gaussian: area reduction, then two separable axes.
use crate::blur::{Blur, Workspace};

struct Slot {
    size: [u32; 2],
    down: u32,
    used: bool,
    snapshot: wgpu::Texture,
    reduced: Option<wgpu::Texture>,
    down_uniform: Option<wgpu::Buffer>,
    down_bind: Option<wgpu::BindGroup>,
    blur: Option<Workspace>,
}

pub(crate) struct GaussianBackdrops {
    downsample: wgpu::ComputePipeline,
    blur: Blur,
    slots: Vec<Slot>,
    allocations: u64,
    reuses: u64,
}

impl GaussianBackdrops {
    pub fn supported(device: &wgpu::Device) -> bool {
        crate::mipmap::MipGenerator::supported(device)
    }

    pub fn new(device: &wgpu::Device) -> Self {
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Plico fixed backdrop area reduction"),
            source: wgpu::ShaderSource::Wgsl(include_str!("backdrop_gaussian.wgsl").into()),
        });
        Self {
            downsample: device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some("Plico fixed backdrop area reduction"),
                layout: None,
                module: &module,
                entry_point: Some("main"),
                compilation_options: Default::default(),
                cache: None,
            }),
            blur: Blur::new(device),
            slots: Vec::new(),
            allocations: 0,
            reuses: 0,
        }
    }

    pub fn begin_frame(&mut self) {
        for slot in &mut self.slots {
            slot.used = false;
        }
    }

    fn slot(&mut self, device: &wgpu::Device, size: [u32; 2], down: u32, blurred: bool) -> usize {
        if let Some(index) = self
            .slots
            .iter()
            .position(|slot| !slot.used && slot.size == size && slot.down == down)
        {
            let slot = &mut self.slots[index];
            slot.used = true;
            let allocated_blur = blurred && slot.blur.is_none();
            if allocated_blur {
                let source = slot.reduced.as_ref().unwrap_or(&slot.snapshot);
                slot.blur = Some(self.blur.workspace(
                    device,
                    source,
                    [size[0].div_ceil(down), size[1].div_ceil(down)],
                ));
                self.allocations += 2;
            }
            self.reuses += 1
                + u64::from(slot.reduced.is_some())
                + if blurred && !allocated_blur { 2 } else { 0 };
            return index;
        }

        let index = self
            .slots
            .iter()
            .position(|slot| !slot.used)
            .unwrap_or(self.slots.len());
        {
            let snapshot = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("Plico fixed backdrop snapshot"),
                size: wgpu::Extent3d {
                    width: size[0],
                    height: size[1],
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            });
            let reduced_size = [size[0].div_ceil(down), size[1].div_ceil(down)];
            let (reduced, down_uniform, down_bind) = if down > 1 {
                let reduced = device.create_texture(&wgpu::TextureDescriptor {
                    label: Some("Plico fixed backdrop reduction"),
                    size: wgpu::Extent3d {
                        width: reduced_size[0],
                        height: reduced_size[1],
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: wgpu::TextureFormat::Rgba8Unorm,
                    usage: wgpu::TextureUsages::TEXTURE_BINDING
                        | wgpu::TextureUsages::STORAGE_BINDING,
                    view_formats: &[],
                });
                let uniform = device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("Plico fixed backdrop reduction parameters"),
                    size: 16,
                    usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                });
                let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("Plico fixed backdrop reduction"),
                    layout: &self.downsample.get_bind_group_layout(0),
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::TextureView(
                                &snapshot.create_view(&Default::default()),
                            ),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::TextureView(
                                &reduced.create_view(&Default::default()),
                            ),
                        },
                        wgpu::BindGroupEntry {
                            binding: 2,
                            resource: uniform.as_entire_binding(),
                        },
                    ],
                });
                (Some(reduced), Some(uniform), Some(bind))
            } else {
                (None, None, None)
            };
            let source = reduced.as_ref().unwrap_or(&snapshot);
            let blur = blurred.then(|| self.blur.workspace(device, source, reduced_size));
            let slot = Slot {
                size,
                down,
                used: true,
                snapshot,
                reduced,
                down_uniform,
                down_bind,
                blur,
            };
            self.allocations += 1 + u64::from(down > 1) + if blurred { 2 } else { 0 };
            if index == self.slots.len() {
                self.slots.push(slot);
            } else {
                self.slots[index] = slot;
            }
        }
        index
    }

    #[allow(clippy::too_many_arguments)]
    pub fn encode(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        source: &wgpu::Texture,
        region: [u32; 4],
        plan: plico_renderer::BackdropGaussian,
    ) -> (wgpu::TextureView, wgpu::TextureView, u64, u64) {
        let [left, top, width, height] = region;
        let index = self.slot(device, [width, height], plan.down, plan.reduced_sigma > 0.);
        let slot = &self.slots[index];
        encoder.copy_texture_to_texture(
            wgpu::TexelCopyTextureInfo {
                texture: source,
                mip_level: 0,
                origin: wgpu::Origin3d {
                    x: left,
                    y: top,
                    z: 0,
                },
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyTextureInfo {
                texture: &slot.snapshot,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
        let reduced_size = [width.div_ceil(plan.down), height.div_ceil(plan.down)];
        let mut passes = 0_u64;
        if plan.down > 1 {
            queue.write_buffer(
                slot.down_uniform.as_ref().unwrap(),
                0,
                bytemuck::cast_slice(&[plan.down, 0, 0, 0]),
            );
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("Plico fixed backdrop area reduction"),
                ..Default::default()
            });
            pass.set_pipeline(&self.downsample);
            pass.set_bind_group(0, slot.down_bind.as_ref().unwrap(), &[]);
            pass.dispatch_workgroups(reduced_size[0].div_ceil(8), reduced_size[1].div_ceil(8), 1);
            passes += 1;
        }
        let original = slot.snapshot.create_view(&Default::default());
        let view = if let Some(workspace) = &slot.blur {
            self.blur.encode_with_radius(
                queue,
                encoder,
                workspace,
                plan.reduced_sigma,
                plico_renderer::filter::BlurEdge::Clamp,
                plan.radius,
            );
            passes += 2;
            workspace.output.create_view(&Default::default())
        } else {
            slot.snapshot.create_view(&Default::default())
        };
        let base = u64::from(width) * u64::from(height) * 4;
        let reduced = u64::from(reduced_size[0]) * u64::from(reduced_size[1]) * 4;
        let bytes = base
            + u64::from(plan.down > 1) * reduced
            + u64::from(plan.reduced_sigma > 0.) * reduced * 2;
        (view, original, passes, bytes)
    }

    pub fn clear(&mut self) {
        self.slots.clear();
    }

    pub fn stats(&self) -> (usize, u64, u64, u64) {
        let mut entries = 0;
        let mut bytes = 0_u64;
        for slot in &self.slots {
            entries += 1;
            bytes += u64::from(slot.size[0]) * u64::from(slot.size[1]) * 4;
            if let Some(reduced) = &slot.reduced {
                entries += 1;
                bytes += u64::from(reduced.width()) * u64::from(reduced.height()) * 4;
            }
            if let Some(blur) = &slot.blur {
                entries += 2;
                bytes += blur.bytes();
            }
        }
        (entries, bytes, self.allocations, self.reuses)
    }
}
