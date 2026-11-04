//! Separable premultiplied Gaussian image filtering, entirely on the GPU.
use wgpu::util::DeviceExt;
pub(crate) struct Blur {
    pipeline: wgpu::ComputePipeline,
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
    pub fn encode(
        &self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        source: &wgpu::Texture,
        size: [u32; 2],
        sigma: f32,
        edge: plico_renderer::filter::BlurEdge,
    ) -> wgpu::Texture {
        let allocate = || {
            device.create_texture(&wgpu::TextureDescriptor {
                label: Some("Plico blur intermediate"),
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
        for (axis, input, destination) in [(0, source, &temporary), (1, &temporary, &output)] {
            let parameters = [sigma.to_bits(), axis, edge as u32, 0];
            let uniform = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: None,
                contents: bytemuck::cast_slice(&parameters),
                usage: wgpu::BufferUsages::UNIFORM,
            });
            let input = input.create_view(&Default::default());
            let destination = destination.create_view(&Default::default());
            let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: None,
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
                        resource: uniform.as_entire_binding(),
                    },
                ],
            });
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("Plico Gaussian axis"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &bind, &[]);
            pass.dispatch_workgroups(size[0].div_ceil(8), size[1].div_ceil(8), 1);
        }
        output
    }
}
