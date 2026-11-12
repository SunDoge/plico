//! Wgpu resources produced outside Plico's command encoder.

/// A texture allocated on the window renderer's device and updated by an
/// external producer such as libmpv.
pub struct ExternalTexture {
    texture: wgpu::Texture,
    size: [u32; 2],
}

impl ExternalTexture {
    /// Retain a sampleable 2D texture and its physical dimensions.
    pub fn new(texture: wgpu::Texture, size: [u32; 2]) -> Self {
        Self { texture, size }
    }

    /// Underlying texture for native interop.
    pub fn texture(&self) -> &wgpu::Texture {
        &self.texture
    }

    /// Physical pixel dimensions.
    pub const fn size(&self) -> [u32; 2] {
        self.size
    }
}
