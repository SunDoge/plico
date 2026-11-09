//! A group's backdrop is its already-painted ancestors plus its own prior ink.
//! The complete group result is mixed with this snapshot exactly once, using
//! replacement compositing so translucent parent pixels are not counted twice.
#[derive(Clone)]
pub(crate) struct Context {
    pub texture: wgpu::Texture,
    pub origin: [f32; 2],
    pub size: [u32; 2],
}
impl Context {
    pub fn new(device: &wgpu::Device, size: [u32; 2], origin: [f32; 2]) -> Self {
        Self {
            texture: device.create_texture(&wgpu::TextureDescriptor {
                label: Some("group backdrop context"),
                size: wgpu::Extent3d {
                    width: size[0],
                    height: size[1],
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::COPY_DST
                    | wgpu::TextureUsages::COPY_SRC
                    | wgpu::TextureUsages::TEXTURE_BINDING
                    | wgpu::TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[],
            }),
            origin,
            size,
        }
    }
}

impl Context {
    pub fn seed(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::Texture,
        origin: [f32; 2],
        size: [u32; 2],
    ) {
        let view = target.create_view(&Default::default());
        {
            let _clear = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("initialize group context"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            });
        }
        let dx = (origin[0] - self.origin[0]) as i64;
        let dy = (origin[1] - self.origin[1]) as i64;
        let sx = dx.max(0) as u32;
        let sy = dy.max(0) as u32;
        let tx = (-dx).max(0) as u32;
        let ty = (-dy).max(0) as u32;
        let w = self.size[0]
            .saturating_sub(sx)
            .min(size[0].saturating_sub(tx));
        let h = self.size[1]
            .saturating_sub(sy)
            .min(size[1].saturating_sub(ty));
        if w > 0 && h > 0 {
            encoder.copy_texture_to_texture(
                wgpu::TexelCopyTextureInfo {
                    origin: wgpu::Origin3d { x: sx, y: sy, z: 0 },
                    ..self.texture.as_image_copy()
                },
                wgpu::TexelCopyTextureInfo {
                    origin: wgpu::Origin3d { x: tx, y: ty, z: 0 },
                    ..target.as_image_copy()
                },
                wgpu::Extent3d {
                    width: w,
                    height: h,
                    depth_or_array_layers: 1,
                },
            );
        }
    }
}
