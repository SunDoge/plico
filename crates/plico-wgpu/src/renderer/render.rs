use super::*;

impl Renderer {
    /// Render one backdrop-reading effect between the halves of the frame
    /// pass: the caller has just dropped the pass, so the target holds
    /// everything painted before the effect. Copy the effect's region out,
    /// render the effect against it, draw the result into the frame and
    /// return a fresh pass for the remaining instances (mygo's Backdrop
    /// step between batches).
    /// Render one backdrop-reading effect between the halves of the frame pass
    /// (mygo's Backdrop step between batches): copy the effect's region out of
    /// the live target, render the effect against it, blit the result back into
    /// the frame, and return a fresh pass for the remaining instances.
    /// All renderer state arrives as parameters so it stays disjoint from the
    /// caller's encoder/pass borrows.
    /// Offscreen readback; window submission uses render_to_view instead.
    pub fn render_items(
        &mut self,
        items: &[crate::Draw<'_>],
        size: [u32; 2],
        background: [f32; 4],
        time: f32,
    ) -> Result<Vec<u8>> {
        let encoder = self.encode_frame(items, size, background, time)?;
        self.read_frame(encoder, size)
    }
    /// Full offscreen replay of an immutable prepared scene without collecting
    /// an intermediate vector of borrowed draw commands.
    pub fn render_prepared(
        &mut self,
        prepared: &crate::scene::PreparedScene,
        size: [u32; 2],
        background: [f32; 4],
        time: f32,
    ) -> Result<Vec<u8>> {
        self.scene_damage.invalidate();
        let encoder = self.encode_frame_regions(
            DrawSource::Prepared(prepared),
            size,
            background,
            time,
            None,
        )?;
        self.read_frame(encoder, size)
    }
    /// Read the retained premultiplied RGBA8 target without repainting or changing
    /// damage history. None means no target (including after trim). This explicit
    /// GPU readback waits for submitted work; it is not a WindowServer screenshot.
    pub fn read_retained_rgba(&mut self) -> Result<Option<([u32; 2], Vec<u8>)>> {
        let Some(size) = self.target.as_ref().map(|target| target.size) else {
            return Ok(None);
        };
        let encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Plico retained frame snapshot"),
            });
        let pixels = self.read_frame(encoder, size)?;
        Ok(Some((size, pixels)))
    }
    #[tracing::instrument(
        level = "debug",
        target = "plico::perf",
        name = "wgpu.readback_including_gpu_wait",
        skip_all
    )]
    pub(crate) fn read_frame(
        &mut self,
        mut encoder: wgpu::CommandEncoder,
        size: [u32; 2],
    ) -> Result<Vec<u8>> {
        let [width, height] = size;
        let target = self.target.as_mut().expect("encode_frame created target");
        let readback = target.readback.get_or_insert_with(|| {
            self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("Plico readback"),
                size: u64::from(target.pitch) * u64::from(height),
                usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            })
        });
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &target.texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: readback,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(target.pitch),
                    rows_per_image: Some(height),
                },
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
        self.queue.submit([encoder.finish()]);
        let (tx, rx) = std::sync::mpsc::channel();
        readback
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |result| {
                let _ = tx.send(result);
            });
        self.device
            .poll(wgpu::PollType::wait_indefinitely())
            .context(crate::PollSnafu)?;
        rx.recv()
            .map_err(|e| Error::Readback {
                message: e.to_string(),
            })?
            .map_err(|e| Error::Readback {
                message: e.to_string(),
            })?;
        let data = readback
            .slice(..)
            .get_mapped_range()
            .map_err(|e| Error::Readback {
                message: e.to_string(),
            })?;
        let mut pixels = Vec::with_capacity((width * height * 4) as usize);
        for row in data.chunks_exact(target.pitch as usize) {
            pixels.extend_from_slice(&row[..(width * 4) as usize]);
        }
        drop(data);
        readback.unmap();
        Ok(pixels)
    }
}
