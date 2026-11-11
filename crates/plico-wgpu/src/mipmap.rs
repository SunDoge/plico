//! Single-source GPU mip generation, encoded before image sampling.
pub(crate) struct MipGenerator {
    pipeline: wgpu::ComputePipeline,
}
impl MipGenerator {
    pub fn supported(device: &wgpu::Device) -> bool {
        let limits = device.limits();
        limits.max_storage_textures_per_shader_stage > 0
            && limits.max_compute_invocations_per_workgroup >= 64
            && limits.max_compute_workgroup_size_x >= 8
            && limits.max_compute_workgroup_size_y >= 8
    }
    pub fn new(device: &wgpu::Device) -> Self {
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Plico image mip prefilter"),
            source: wgpu::ShaderSource::Wgsl(include_str!("mipmap.wgsl").into()),
        });
        Self {
            pipeline: device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some("Plico image mip prefilter"),
                layout: None,
                module: &module,
                entry_point: Some("main"),
                compilation_options: Default::default(),
                cache: None,
            }),
        }
    }
    #[tracing::instrument(
        level = "debug", target = "plico::perf", name = "wgpu.generate_mips",
        skip_all, fields(width = texture.width(), height = texture.height(), levels = texture.mip_level_count())
    )]
    pub fn encode(
        &self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        texture: &wgpu::Texture,
    ) {
        let view = |level| {
            texture.create_view(&wgpu::TextureViewDescriptor {
                base_mip_level: level,
                mip_level_count: Some(1),
                ..Default::default()
            })
        };
        let layout = self.pipeline.get_bind_group_layout(0);
        for level in 1..texture.mip_level_count() {
            let input = view(level - 1);
            let output = view(level);
            let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("Plico image mip step"),
                layout: &layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&input),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(&output),
                    },
                ],
            });
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("Plico image mip step"),
                ..Default::default()
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &bind, &[]);
            pass.dispatch_workgroups(
                (texture.width() >> level).max(1).div_ceil(8),
                (texture.height() >> level).max(1).div_ceil(8),
                1,
            );
        }
    }
}
