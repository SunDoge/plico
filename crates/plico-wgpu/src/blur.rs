//! Separable premultiplied Gaussian image filtering, entirely on the GPU.
pub(crate) struct Blur {
    pipeline: wgpu::ComputePipeline,
}
pub(crate) struct Workspace {
    temporary: wgpu::Texture,
    pub output: wgpu::Texture,
    uniforms: [wgpu::Buffer; 2],
    binds: [wgpu::BindGroup; 2],
}
impl Blur {
    pub fn new(device: &wgpu::Device) -> Self {
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Plico Gaussian blur"),
            source: wgpu::ShaderSource::Wgsl(include_str!("blur.wgsl").into()),
        });
        Self {
            pipeline: device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some("Plico separable blur"),
                layout: None,
                module: &module,
                entry_point: Some("main"),
                compilation_options: Default::default(),
                cache: None,
            }),
        }
    }
    pub fn workspace(
        &self,
        device: &wgpu::Device,
        source: &wgpu::Texture,
        size: [u32; 2],
    ) -> Workspace {
        let allocate = || {
            device.create_texture(&wgpu::TextureDescriptor {
                label: Some("Plico pooled blur intermediate"),
                size: wgpu::Extent3d {
                    width: size[0],
                    height: size[1],
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::STORAGE_BINDING,
                view_formats: &[],
            })
        };
        let temporary = allocate();
        let output = allocate();
        let uniforms = std::array::from_fn(|_| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("Plico pooled blur parameters"),
                size: 16,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            })
        });
        let binds = std::array::from_fn(|axis| {
            let (input, destination) = if axis == 0 {
                (source, &temporary)
            } else {
                (&temporary, &output)
            };
            let input = input.create_view(&Default::default());
            let destination = destination.create_view(&Default::default());
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("Plico pooled blur axis"),
                layout: &self.pipeline.get_bind_group_layout(0),
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&input),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(&destination),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: uniforms[axis].as_entire_binding(),
                    },
                ],
            })
        });
        Workspace {
            temporary,
            output,
            uniforms,
            binds,
        }
    }
    pub fn encode(
        &self,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        workspace: &Workspace,
        sigma: f32,
        edge: plico_renderer::filter::BlurEdge,
    ) {
        let size = workspace.temporary.size();
        for axis in 0..2 {
            queue.write_buffer(
                &workspace.uniforms[axis],
                0,
                bytemuck::cast_slice(&[sigma.to_bits(), axis as u32, edge as u32, 0]),
            );
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("Plico Gaussian axis"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &workspace.binds[axis], &[]);
            pass.dispatch_workgroups(size.width.div_ceil(8), size.height.div_ceil(8), 1);
        }
    }
}
