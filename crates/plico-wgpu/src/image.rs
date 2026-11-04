//! Immutable image resources, separate from the glyph atlas and bounded by bytes.
use crate::{Error, Quad, Result};
use peniko::{Extend, ImageAlphaType, ImageData, ImageFormat, ImageQuality};
use std::{num::NonZeroUsize, rc::Rc};

/// A physical-pixel textured quad. UVs are normalized source coordinates.
pub struct ImageQuad {
    pub quad: Quad,
    pub image: ImageData,
    pub uv: [f32; 4],
    pub quality: ImageQuality,
    /// Source extension along the horizontal and vertical axes.
    pub extend: [Extend; 2],
    pub alpha: f32,
}
impl ImageQuad {
    /// Construct a full-source image with bilinear sampling, pad extension and
    /// full opacity. Fields can be adjusted for explicit low-level replay.
    pub fn new(quad: Quad, image: ImageData) -> Self {
        Self {
            quad,
            image,
            uv: [0., 0., 1., 1.],
            quality: ImageQuality::Medium,
            extend: [Extend::Pad; 2],
            alpha: 1.,
        }
    }
}
type Key = (u64, u32, u32, u8, u8);
pub(crate) struct GpuImage {
    pub bind: wgpu::BindGroup,
    // Retain the actual source allocation, not just its identity.
    _source: Option<ImageData>,
    bytes: usize,
}
pub(crate) struct ImageCache {
    entries: lru::LruCache<Key, Rc<GpuImage>>,
    bytes: usize,
    pub uploads: u64,
    pub hits: u64,
}
impl Default for ImageCache {
    fn default() -> Self {
        Self {
            entries: lru::LruCache::new(NonZeroUsize::new(256).unwrap()),
            bytes: 0,
            uploads: 0,
            hits: 0,
        }
    }
}
impl ImageCache {
    pub fn get(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        layout: &wgpu::BindGroupLayout,
        image: &ImageData,
        quality: ImageQuality,
        extend: [Extend; 2],
    ) -> Result<Rc<GpuImage>> {
        // Sampler is part of identity; changing sampling never uploads every frame.
        let key = (
            image.data.id(),
            image.width,
            image.height,
            image.format as u8 | ((image.alpha_type as u8) << 4),
            quality as u8 | ((extend[0] as u8) << 2) | ((extend[1] as u8) << 4),
        );
        if let Some(entry) = self.entries.get(&key) {
            self.hits += 1;
            return Ok(entry.clone());
        }
        let bytes = image
            .format
            .size_in_bytes(image.width, image.height)
            .ok_or_else(|| Error::InvalidInput {
                reason: "image byte size overflow".into(),
            })?;
        let limit = device.limits().max_texture_dimension_2d;
        if image.width == 0
            || image.height == 0
            || image.width > limit
            || image.height > limit
            || bytes > 128 * 1024 * 1024
            || image.data.data().len() < bytes
        {
            return Err(Error::InvalidInput {
                reason: "image exceeds dimensions/128 MiB limit or has truncated pixels".into(),
            });
        }
        let mut pixels = image.data.data()[..bytes].to_vec();
        for p in pixels.as_chunks_mut::<4>().0 {
            match image.format {
                ImageFormat::Rgba8 => {}
                ImageFormat::Bgra8 => p.swap(0, 2),
                _ => {
                    return Err(Error::Unsupported {
                        command: 0,
                        reason: "image pixel format".into(),
                    });
                }
            }
            if image.alpha_type == ImageAlphaType::Alpha {
                for c in 0..3 {
                    p[c] = ((u16::from(p[c]) * u16::from(p[3]) + 127) / 255) as u8;
                }
            }
        }
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Plico immutable image"),
            size: wgpu::Extent3d {
                width: image.width,
                height: image.height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &pixels,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(image.width * 4),
                rows_per_image: Some(image.height),
            },
            wgpu::Extent3d {
                width: image.width,
                height: image.height,
                depth_or_array_layers: 1,
            },
        );
        let filter = if quality == ImageQuality::Low {
            wgpu::FilterMode::Nearest
        } else {
            wgpu::FilterMode::Linear
        };
        let address = |mode| match mode {
            Extend::Pad => wgpu::AddressMode::ClampToEdge,
            Extend::Repeat => wgpu::AddressMode::Repeat,
            Extend::Reflect => wgpu::AddressMode::MirrorRepeat,
        };
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: address(extend[0]),
            address_mode_v: address(extend[1]),
            mag_filter: filter,
            min_filter: filter,
            ..Default::default()
        });
        let view = texture.create_view(&Default::default());
        let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Plico image binding"),
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
        while self.bytes + bytes > 128 * 1024 * 1024
            || self.entries.len() == self.entries.cap().get()
        {
            let Some((_, evicted)) = self.entries.pop_lru() else {
                break;
            };
            self.bytes -= evicted.bytes;
        }
        let entry = Rc::new(GpuImage {
            bind,
            _source: Some(image.clone()),
            bytes,
        });
        self.entries.put(key, entry.clone());
        self.bytes += bytes;
        self.uploads += 1;
        Ok(entry)
    }
    pub fn stats(&self) -> (usize, usize, u64, u64) {
        (self.entries.len(), self.bytes, self.uploads, self.hits)
    }
}

impl GpuImage {
    pub(crate) fn from_layer(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        texture: &wgpu::Texture,
    ) -> Rc<Self> {
        Self::from_view(device, layout, &texture.create_view(&Default::default()))
    }
    pub(crate) fn from_view(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        view: &wgpu::TextureView,
    ) -> Rc<Self> {
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });
        let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Plico isolated layer"),
            layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        });
        Rc::new(Self {
            bind,
            _source: None,
            bytes: 0,
        })
    }
}
