//! Replace final backdrop pixels while retaining ancestor rectangle/mask coverage.
//! SrcOver is unsuitable here: the effect already contains its sampled backdrop.

const MAPPING_BYTES: u64 = 16;
const COMPOSITE_BYTES: u64 = 80;

pub(crate) struct Compositor {
    layout: wgpu::BindGroupLayout,
    pipeline: wgpu::RenderPipeline,
    uniforms: Vec<wgpu::Buffer>,
    used: usize,
    allocations: u64,
    reuses: u64,
    composite_offset: u64,
}
impl Compositor {
    pub fn new(device: &wgpu::Device, mask_layout: &wgpu::BindGroupLayout) -> Self {
        let texture = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: false },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("backdrop composite"),
            entries: &[
                texture(0),
                texture(1),
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("clipped backdrop composite"),
            source: wgpu::ShaderSource::Wgsl(include_str!("backdrop_composite.wgsl").into()),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[Some(&layout), Some(mask_layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("clipped backdrop composite"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: wgpu::TextureFormat::Rgba8Unorm,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        let composite_offset = u64::from(device.limits().min_uniform_buffer_offset_alignment)
            .max(MAPPING_BYTES)
            .next_multiple_of(MAPPING_BYTES);
        Self {
            layout,
            pipeline,
            uniforms: Vec::new(),
            used: 0,
            allocations: 0,
            reuses: 0,
            composite_offset,
        }
    }

    pub fn begin_frame(&mut self) {
        self.used = 0;
    }

    pub fn prepare_uniforms(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        mapping: [f32; 4],
        data: [[f32; 4]; 4],
        mix: [f32; 4],
    ) -> wgpu::Buffer {
        let buffer = if let Some(buffer) = self.uniforms.get(self.used) {
            self.reuses += 1;
            buffer.clone()
        } else {
            self.allocations += 1;
            let buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("Plico backdrop draw uniforms"),
                size: self.composite_offset + COMPOSITE_BYTES,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            self.uniforms.push(buffer.clone());
            buffer
        };
        self.used += 1;
        queue.write_buffer(&buffer, 0, bytemuck::bytes_of(&mapping));
        queue.write_buffer(
            &buffer,
            self.composite_offset,
            bytemuck::cast_slice(&[data[0], data[1], data[2], data[3], mix]),
        );
        buffer
    }

    #[allow(clippy::too_many_arguments)]
    pub fn encode(
        &self,
        device: &wgpu::Device,
        output: &wgpu::TextureView,
        backdrop: &wgpu::TextureView,
        mask: &wgpu::BindGroup,
        uniforms: &wgpu::Buffer,
        pass: &mut wgpu::RenderPass<'_>,
    ) {
        let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(output),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(backdrop),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: uniforms,
                        offset: self.composite_offset,
                        size: std::num::NonZeroU64::new(COMPOSITE_BYTES),
                    }),
                },
            ],
        });
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &bind, &[]);
        pass.set_bind_group(1, mask, &[]);
        pass.draw(0..3, 0..1);
    }

    pub fn clear_uniforms(&mut self) {
        self.uniforms.clear();
        self.used = 0;
    }

    pub fn uniform_stats(&self) -> (usize, u64, u64) {
        (self.uniforms.len(), self.allocations, self.reuses)
    }
}
