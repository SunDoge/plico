//! Frame-local ownership and cross-frame reuse for backdrop snapshots.
//!
//! A texture already used by this frame cannot be overwritten: a later effect
//! may still sample it. The next frame may reuse it because queue submissions
//! execute in order, including the copy/prefilter work that overwrites it.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    Single,
    Array,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Key {
    width: u32,
    height: u32,
    layers: u32,
    levels: u32,
    kind: Kind,
}

impl Key {
    fn bytes(self) -> u64 {
        let mut width = self.width;
        let mut height = self.height;
        let mut pixels = 0_u64;
        for _ in 0..self.levels {
            pixels = pixels
                .saturating_add(u64::from(width) * u64::from(height) * u64::from(self.layers));
            width = (width / 2).max(1);
            height = (height / 2).max(1);
        }
        pixels.saturating_mul(4)
    }
}

struct Entry {
    texture: wgpu::Texture,
    key: Key,
    used: bool,
}

#[derive(Default)]
pub(crate) struct Pool {
    entries: Vec<Entry>,
    allocations: u64,
    reuses: u64,
}

impl Pool {
    pub fn begin_frame(&mut self) {
        for entry in &mut self.entries {
            entry.used = false;
        }
    }

    pub fn acquire(
        &mut self,
        device: &wgpu::Device,
        size: [u32; 2],
        layers: u32,
        levels: u32,
        kind: Kind,
    ) -> wgpu::Texture {
        let key = Key {
            width: size[0],
            height: size[1],
            layers,
            levels,
            kind,
        };
        if let Some(entry) = self
            .entries
            .iter_mut()
            .find(|entry| !entry.used && entry.key == key)
        {
            entry.used = true;
            self.reuses += 1;
            return entry.texture.clone();
        }

        let texture = Self::allocate(device, key);
        self.allocations += 1;
        if let Some(entry) = self.entries.iter_mut().find(|entry| !entry.used) {
            *entry = Entry {
                texture: texture.clone(),
                key,
                used: true,
            };
        } else {
            self.entries.push(Entry {
                texture: texture.clone(),
                key,
                used: true,
            });
        }
        texture
    }

    fn allocate(device: &wgpu::Device, key: Key) -> wgpu::Texture {
        let usage = wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::COPY_DST
            | match key.kind {
                Kind::Single => wgpu::TextureUsages::RENDER_ATTACHMENT,
                Kind::Array => wgpu::TextureUsages::STORAGE_BINDING,
            };
        device.create_texture(&wgpu::TextureDescriptor {
            label: Some(match key.kind {
                Kind::Single => "Plico pooled backdrop snapshot",
                Kind::Array => "Plico pooled backdrop snapshot array",
            }),
            size: wgpu::Extent3d {
                width: key.width,
                height: key.height,
                depth_or_array_layers: key.layers,
            },
            mip_level_count: key.levels,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage,
            view_formats: &[],
        })
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }

    pub fn stats(&self) -> (usize, u64, u64, u64) {
        (
            self.entries.len(),
            self.entries.iter().map(|entry| entry.key.bytes()).sum(),
            self.allocations,
            self.reuses,
        )
    }
}
