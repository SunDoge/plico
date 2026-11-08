//! Transient backdrop area pyramid. Render passes also work without compute.
pub(crate) struct Prefilter {
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
}
impl Prefilter {
    pub fn new(device: &wgpu::Device) -> Self {
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Plico backdrop prefilter"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: false },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            }],
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("backdrop area prefilter"),
            source: wgpu::ShaderSource::Wgsl(include_str!("backdrop_prefilter.wgsl").into()),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Plico backdrop area prefilter"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs"),
                compilation_options: Default::default(),
                buffers: &[],
            },
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
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            multiview_mask: None,
            cache: None,
        });
        Self { pipeline, layout }
    }
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
        for level in 1..texture.mip_level_count() {
            let source = view(level - 1);
            let destination = view(level);
            let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("Plico backdrop prefilter step"),
                layout: &self.layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&source),
                }],
            });
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Plico backdrop prefilter step"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &destination,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &bind, &[]);
            pass.draw(0..3, 0..1);
        }
    }
}
pub(crate) fn levels(width: u32, height: u32, max_sigma: Option<f32>) -> u32 {
    max_sigma.map_or(1, |sigma| {
        let requested = sigma.max(1.).log2().ceil() as u32;
        requested.min(width.max(height).ilog2()) + 1
    })
}
pub(crate) fn bytes(width: u32, height: u32, levels: u32) -> u64 {
    (0..levels)
        .map(|level| u64::from((width >> level).max(1)) * u64::from((height >> level).max(1)) * 4)
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn level_count_follows_sigma_and_payload_is_bounded_for_odd_and_thin_regions() {
        assert_eq!(levels(2048, 1024, None), 1);
        assert_eq!(levels(2048, 1024, Some(0.)), 1);
        assert_eq!(levels(2048, 1024, Some(1.)), 1);
        assert_eq!(levels(2048, 1024, Some(8.)), 4);
        assert_eq!(levels(2048, 1024, Some(12.)), 5);
        assert_eq!(levels(1, 1, Some(128.)), 1);
        for width in [1, 3, 127, 2048] {
            for height in [1, 3, 95, 1024] {
                let count = levels(width, height, Some(128.));
                assert!(bytes(width, height, count) <= u64::from(width) * u64::from(height) * 8);
            }
        }
    }
}
