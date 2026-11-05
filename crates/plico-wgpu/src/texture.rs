/// One atlas binding shared by every instance. A 1px dummy costs little when no
/// text is drawn. Replacing the binding does not require recompiling pipelines.
pub(crate) struct AtlasTexture {
    pub layout: wgpu::BindGroupLayout,
    pub bind: wgpu::BindGroup,
    #[cfg(all(target_os = "linux", feature = "system-text"))]
    pub texture: wgpu::Texture,
    #[cfg(all(target_os = "linux", feature = "system-text"))]
    pub side: u32,
}
impl AtlasTexture {
    pub fn new(device: &wgpu::Device) -> Self {
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Plico glyph atlas layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let (_texture, bind) = Self::allocate(device, &layout, 1);
        Self {
            layout,
            #[cfg(all(target_os = "linux", feature = "system-text"))]
            texture: _texture,
            bind,
            #[cfg(all(target_os = "linux", feature = "system-text"))]
            side: 1,
        }
    }
    fn allocate(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        side: u32,
    ) -> (wgpu::Texture, wgpu::BindGroup) {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Plico glyph atlas"),
            size: wgpu::Extent3d {
                width: side,
                height: side,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let view = texture.create_view(&Default::default());
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("glyph bilinear sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        });
        (texture, bind)
    }
    #[cfg(all(target_os = "linux", feature = "system-text"))]
    pub fn ensure_size(&mut self, device: &wgpu::Device, side: u32) {
        if self.side != side {
            let (texture, bind) = Self::allocate(device, &self.layout, side);
            self.texture = texture;
            self.bind = bind;
            self.side = side;
        }
    }
}
