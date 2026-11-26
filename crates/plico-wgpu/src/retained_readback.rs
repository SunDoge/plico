use crate::{Error, PollSnafu, Result};
use snafu::ResultExt;

/// A submitted copy of a retained window frame. Reading it may block on the
/// GPU, so wait on a worker. After reading, recycle it on the same window.
pub struct RetainedFrameReadback {
    device: wgpu::Device,
    buffer: wgpu::Buffer,
    submission: wgpu::SubmissionIndex,
    size: [u32; 2],
    pitch: u32,
}

impl RetainedFrameReadback {
    pub(crate) fn copy(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        texture: &wgpu::Texture,
        size: [u32; 2],
        pitch: u32,
        reuse: Option<Self>,
    ) -> Self {
        let buffer_size = u64::from(pitch) * u64::from(size[1]);
        let buffer = reuse
            .filter(|previous| previous.device == *device && previous.buffer.size() >= buffer_size)
            .map_or_else(
                || {
                    device.create_buffer(&wgpu::BufferDescriptor {
                        label: Some("Plico window recording readback"),
                        size: buffer_size,
                        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
                        mapped_at_creation: false,
                    })
                },
                |previous| previous.buffer,
            );
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Plico window recording copy"),
        });
        encoder.copy_texture_to_buffer(
            texture.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(pitch),
                    rows_per_image: Some(size[1]),
                },
            },
            wgpu::Extent3d {
                width: size[0],
                height: size[1],
                depth_or_array_layers: 1,
            },
        );
        let submission = queue.submit([encoder.finish()]);
        Self {
            device: device.clone(),
            buffer,
            submission,
            size,
            pitch,
        }
    }

    /// Physical dimensions of the copied frame.
    pub fn size(&self) -> [u32; 2] {
        self.size
    }

    /// Wait for the copy and retrieve tightly packed premultiplied RGBA bytes.
    pub fn read(&self) -> Result<Vec<u8>> {
        let [width, height] = self.size;
        let slice = self
            .buffer
            .slice(..u64::from(self.pitch) * u64::from(height));
        let (tx, rx) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |result| {
            let _ = tx.send(result);
        });
        self.device
            .poll(wgpu::PollType::Wait {
                submission_index: Some(self.submission.clone()),
                timeout: Some(std::time::Duration::from_secs(5)),
            })
            .context(PollSnafu)?;
        let error = |message: String| Error::Readback { message };
        rx.recv_timeout(std::time::Duration::from_secs(5))
            .map_err(|err| error(err.to_string()))?
            .map_err(|err| error(err.to_string()))?;
        let data = slice
            .get_mapped_range()
            .map_err(|err| error(err.to_string()))?;
        let row_bytes = width as usize * 4;
        let mut pixels = Vec::with_capacity(row_bytes * height as usize);
        for row in data.chunks_exact(self.pitch as usize) {
            pixels.extend_from_slice(&row[..row_bytes]);
        }
        drop(data);
        self.buffer.unmap();
        Ok(pixels)
    }
}
