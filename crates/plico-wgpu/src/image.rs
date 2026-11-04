//! Immutable image resources, separate from the glyph atlas and bounded by bytes.
use crate::{Error, Quad, Result};
use peniko::{Extend, ImageAlphaType, ImageData, ImageFormat, ImageQuality};
use std::{collections::HashMap, num::NonZeroUsize, rc::Rc};

/// A physical-pixel textured quad. UVs are normalized source coordinates.
pub struct ImageQuad {
    pub quad: Quad,
    pub image: ImageData,
    pub uv: [f32; 4],
    /// Optional physical-world to normalized-source affine UV map.
    /// Geometry coverage is supplied separately by the quad/active clip mask.
    pub uv_transform: Option<[f32; 6]>,
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
            uv_transform: None,
            quality: ImageQuality::Medium,
            extend: [Extend::Pad; 2],
            alpha: 1.,
        }
    }
}
pub(crate) struct ImageSampling {
    pub quality: ImageQuality,
    pub extend: [Extend; 2],
    pub mipmaps: bool,
}
type Key = (u64, u32, u32, u8);
pub(crate) struct GpuImage {
    pub bind: wgpu::BindGroup,
    // Retain the actual source allocation, not just its identity.
    _source: Option<ImageData>,
}
struct TextureEntry {
    texture: wgpu::Texture,
    source: ImageData,
    bytes: usize,
    primary: Option<(u8, Rc<GpuImage>)>,
    additional: HashMap<u8, Rc<GpuImage>>,
}
pub(crate) struct ImageCache {
    entries: lru::LruCache<Key, TextureEntry>,
    bytes: usize,
    pub uploads: u64,
    pub hits: u64,
    bindings: usize,
    binding_creations: u64,
}
impl Default for ImageCache {
    fn default() -> Self {
        Self {
            entries: lru::LruCache::new(NonZeroUsize::new(256).unwrap()),
            bytes: 0,
            uploads: 0,
            hits: 0,
            bindings: 0,
            binding_creations: 0,
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
        sampling: ImageSampling,
    ) -> Result<Rc<GpuImage>> {
        let ImageSampling {
            quality,
            extend,
            mipmaps,
        } = sampling;
        // Pixel identity excludes sampler policy. Bindings share one source texture.
        let key = (
            image.data.id(),
            image.width,
            image.height,
            image.format as u8 | ((image.alpha_type as u8) << 4) | (u8::from(mipmaps) << 7),
        );
        if let Some(entry) = self.entries.get_mut(&key) {
            self.hits += 1;
            let (image, created) = entry.binding(device, layout, quality, extend);
            self.bindings += usize::from(created);
            self.binding_creations += u64::from(created);
            return Ok(image);
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
        // Small glyph/run images and coverage masks do not need a mip chain.
        // Keep the full allocation within the same per-source/cache budget.
        let mut sizes = vec![[image.width, image.height]];
        let mut texture_bytes = bytes;
        if mipmaps && image.width.min(image.height) >= 64 {
            let mut size = [image.width, image.height];
            while size != [1, 1] {
                size = [(size[0] / 2).max(1), (size[1] / 2).max(1)];
                texture_bytes += size[0] as usize * size[1] as usize * 4;
                sizes.push(size);
            }
            if texture_bytes > 128 * 1024 * 1024 {
                sizes.truncate(1);
                texture_bytes = bytes;
            }
        }
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Plico immutable image"),
            size: wgpu::Extent3d {
                width: image.width,
                height: image.height,
                depth_or_array_layers: 1,
            },
            mip_level_count: sizes.len() as u32,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let mut previous_size = sizes[0];
        for (level, &[width, height]) in sizes.iter().enumerate() {
            if level > 0 {
                pixels = area_reduce(&pixels, previous_size, [width, height]);
            }
            queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &texture,
                    mip_level: level as u32,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                &pixels,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(width * 4),
                    rows_per_image: Some(height),
                },
                wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
            );
            previous_size = [width, height];
        }
        let bytes = texture_bytes;
        while self.bytes + bytes > 128 * 1024 * 1024
            || self.entries.len() == self.entries.cap().get()
        {
            let Some((_, evicted)) = self.entries.pop_lru() else {
                break;
            };
            self.bytes -= evicted.bytes;
            self.bindings -= usize::from(evicted.primary.is_some()) + evicted.additional.len();
        }
        let mut entry = TextureEntry {
            texture,
            source: image.clone(),
            bytes,
            primary: None,
            additional: HashMap::new(),
        };
        let (binding, _) = entry.binding(device, layout, quality, extend);
        self.entries.put(key, entry);
        self.bindings += 1;
        self.binding_creations += 1;
        self.bytes += bytes;
        self.uploads += 1;
        Ok(binding)
    }
    pub fn clear(&mut self) {
        self.entries.clear();
        self.bytes = 0;
        self.bindings = 0;
    }
    pub fn binding_stats(&self) -> (usize, u64) {
        (self.bindings, self.binding_creations)
    }
    pub fn stats(&self) -> (usize, usize, u64, u64) {
        (self.entries.len(), self.bytes, self.uploads, self.hits)
    }
}

impl TextureEntry {
    fn binding(
        &mut self,
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        quality: ImageQuality,
        extend: [Extend; 2],
    ) -> (Rc<GpuImage>, bool) {
        let key = quality as u8 | ((extend[0] as u8) << 2) | ((extend[1] as u8) << 4);
        if let Some((mode, image)) = &self.primary
            && *mode == key
        {
            return (image.clone(), false);
        }
        if let Some(image) = self.additional.get(&key) {
            return (image.clone(), false);
        }
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
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            ..Default::default()
        });
        let view = self.texture.create_view(&Default::default());
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

        let image = Rc::new(GpuImage {
            bind,
            _source: Some(self.source.clone()),
        });
        if self.primary.is_none() {
            self.primary = Some((key, image.clone()));
        } else {
            self.additional.insert(key, image.clone());
        }
        (image, true)
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
        })
    }
}

/// Exact area weights for each reduction, including odd dimensions. Inputs are
/// premultiplied encoded RGBA; identical weights preserve RGB <= alpha.
fn area_reduce(source: &[u8], size: [u32; 2], target: [u32; 2]) -> Vec<u8> {
    let [sw, sh] = size;
    let [dw, dh] = target;
    if sw == dw * 2 && sh == dh * 2 {
        let stride = sw as usize * 4;
        let mut output = vec![0; dw as usize * dh as usize * 4];
        for (rows, destination) in source
            .chunks_exact(stride * 2)
            .zip(output.chunks_exact_mut(dw as usize * 4))
        {
            let (top, bottom) = rows.split_at(stride);
            for ((a, b), pixel) in top
                .as_chunks::<8>()
                .0
                .iter()
                .zip(bottom.as_chunks::<8>().0)
                .zip(destination.as_chunks_mut::<4>().0)
            {
                for channel in 0..4 {
                    pixel[channel] = ((u16::from(a[channel])
                        + u16::from(a[channel + 4])
                        + u16::from(b[channel])
                        + u16::from(b[channel + 4])
                        + 2)
                        / 4) as u8;
                }
            }
        }
        output
    } else {
        area_reduce_weighted(source, size, target)
    }
}

// A mip step covers at most three source pixels per axis, including 3 -> 1.
struct AreaWeights {
    taps: [(usize, u64); 3],
    len: usize,
}
fn axis_weights(source: u32, target: u32, stride: usize) -> Vec<AreaWeights> {
    (0..target)
        .map(|position| {
            let start = position * source;
            let end = (position + 1) * source;
            let first = start / target;
            let last = end.div_ceil(target);
            let mut weights = AreaWeights {
                taps: [(0, 0); 3],
                len: (last - first) as usize,
            };
            for (slot, index) in (first..last).enumerate() {
                let weight = ((index + 1) * target).min(end) - (index * target).max(start);
                weights.taps[slot] = (index as usize * stride, u64::from(weight));
            }
            weights
        })
        .collect()
}
fn area_reduce_weighted(source: &[u8], [sw, sh]: [u32; 2], [dw, dh]: [u32; 2]) -> Vec<u8> {
    let horizontal = axis_weights(sw, dw, 4);
    let vertical = axis_weights(sh, dh, sw as usize * 4);
    let denominator = u64::from(sw) * u64::from(sh);
    let mut output = vec![0; dw as usize * dh as usize * 4];
    for (row, destination) in vertical
        .iter()
        .zip(output.chunks_exact_mut(dw as usize * 4))
    {
        for (column, pixel) in horizontal.iter().zip(destination.as_chunks_mut::<4>().0) {
            let mut sums = [0u64; 4];
            for &(y, wy) in &row.taps[..row.len] {
                for &(x, wx) in &column.taps[..column.len] {
                    let weight = wx * wy;
                    let offset = x + y;
                    for channel in 0..4 {
                        sums[channel] += u64::from(source[offset + channel]) * weight;
                    }
                }
            }
            for channel in 0..4 {
                pixel[channel] = ((sums[channel] + denominator / 2) / denominator) as u8;
            }
        }
    }
    output
}

#[cfg(test)]
fn area_reduce_reference(source: &[u8], [sw, sh]: [u32; 2], [dw, dh]: [u32; 2]) -> Vec<u8> {
    let mut output = vec![0; dw as usize * dh as usize * 4];
    let denominator = u64::from(sw) * u64::from(sh);
    for y in 0..dh {
        for x in 0..dw {
            let mut sums = [0u64; 4];
            for sy in y * sh / dh..((y + 1) * sh).div_ceil(dh) {
                let wy = ((sy + 1) * dh).min((y + 1) * sh) - (sy * dh).max(y * sh);
                for sx in x * sw / dw..((x + 1) * sw).div_ceil(dw) {
                    let wx = ((sx + 1) * dw).min((x + 1) * sw) - (sx * dw).max(x * sw);
                    let weight = u64::from(wx) * u64::from(wy);
                    let offset = (sy as usize * sw as usize + sx as usize) * 4;
                    for channel in 0..4 {
                        sums[channel] += u64::from(source[offset + channel]) * weight;
                    }
                }
            }
            let offset = (y as usize * dw as usize + x as usize) * 4;
            for channel in 0..4 {
                output[offset + channel] = ((sums[channel] + denominator / 2) / denominator) as u8;
            }
        }
    }
    output
}

#[cfg(test)]
mod mip_tests {
    use super::{area_reduce, area_reduce_reference};
    #[test]
    fn odd_area_reduction_preserves_edges_and_premultiplied_alpha() {
        let source = [vec![0; 8], vec![255; 4]].concat();
        assert_eq!(area_reduce(&source, [3, 1], [1, 1]), [85; 4]);
        let pixels = [vec![100, 20, 0, 100], vec![0; 4]].concat();
        assert_eq!(area_reduce(&pixels, [2, 1], [1, 1]), [50, 10, 0, 50]);
        assert_eq!(area_reduce(&[128; 5 * 3 * 4], [5, 3], [2, 1]), [128; 8]);
    }
    #[test]
    fn even_reduction_matches_exact_area_weights_across_complete_chains() {
        for size in [[64, 64], [128, 96], [257, 193], [2, 128], [128, 2]] {
            let mut pixels: Vec<u8> = (0..size[0] * size[1] * 4)
                .map(|i| ((i * 73 + i / 7) % 256) as u8)
                .collect();
            let mut current = size;
            while current != [1, 1] {
                let next = [(current[0] / 2).max(1), (current[1] / 2).max(1)];
                let expected = area_reduce_reference(&pixels, current, next);
                pixels = area_reduce(&pixels, current, next);
                assert_eq!(pixels, expected, "{current:?} -> {next:?}");
                current = next;
            }
        }
    }

    #[test]
    fn precomputed_weights_match_reference_for_small_odd_and_degenerate_sizes() {
        for width in 1..34 {
            for height in 1..18 {
                let pixels: Vec<u8> = (0..width * height * 4)
                    .map(|i| ((i * 73 + i / 7) % 256) as u8)
                    .collect();
                let target = [(width / 2).max(1), (height / 2).max(1)];
                assert_eq!(
                    area_reduce(&pixels, [width, height], target),
                    area_reduce_reference(&pixels, [width, height], target),
                    "{width}x{height}"
                );
            }
        }
    }

    #[test]
    #[ignore = "manual CPU profile; run with --ignored --nocapture"]
    fn profile_mip_area_cost() {
        use std::{hint::black_box, time::Instant};
        for size in [[4096, 3072], [4095, 3071]] {
            let source: Vec<u8> = (0..size[0] * size[1] * 4)
                .map(|i| (i % 251) as u8)
                .collect();
            for (name, reduce) in [
                (
                    "weighted",
                    area_reduce_reference as fn(&[u8], [u32; 2], [u32; 2]) -> Vec<u8>,
                ),
                ("optimized", area_reduce),
            ] {
                let mut samples = Vec::new();
                for _ in 0..7 {
                    let start = Instant::now();
                    let mut current = [size[0] / 2, size[1] / 2];
                    let mut pixels = reduce(black_box(&source), size, current);
                    while current != [1, 1] {
                        let next = [(current[0] / 2).max(1), (current[1] / 2).max(1)];
                        pixels = reduce(black_box(&pixels), current, next);
                        current = next;
                    }
                    black_box(pixels);
                    samples.push(start.elapsed());
                }
                samples.sort();
                eprintln!(
                    "{name}: {size:?} complete mip chain median={:?}, range={:?}..{:?}",
                    samples[3], samples[0], samples[6]
                );
            }
        }
    }
}
