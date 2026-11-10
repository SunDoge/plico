use super::*;

impl Renderer {
    pub fn surface_capabilities(&self, surface: &wgpu::Surface<'_>) -> wgpu::SurfaceCapabilities {
        surface.get_capabilities(&self.adapter)
    }
    pub fn configure_surface(
        &self,
        surface: &wgpu::Surface<'_>,
        config: &wgpu::SurfaceConfiguration,
    ) {
        surface.configure(&self.device, config);
    }
    /// GPU-only submit to a window/offscreen view, without readback or CPU waiting.
    /// Destination must have the requested size and format and support RENDER_ATTACHMENT.
    #[tracing::instrument(name="plico_wgpu.render_to_view",skip_all,fields(width=size[0],height=size[1]))]
    pub fn render_to_view(
        &mut self,
        items: &[crate::Draw<'_>],
        size: [u32; 2],
        background: [f32; 4],
        time: f32,
        destination: &wgpu::TextureView,
        format: wgpu::TextureFormat,
    ) -> Result<()> {
        if !matches!(
            format,
            wgpu::TextureFormat::Rgba8Unorm
                | wgpu::TextureFormat::Rgba8UnormSrgb
                | wgpu::TextureFormat::Bgra8Unorm
                | wgpu::TextureFormat::Bgra8UnormSrgb
        ) {
            return Err(Error::InvalidInput {
                reason: "unsupported present target format".into(),
            });
        }
        let encoder = self.encode_frame(items, size, background, time)?;
        self.submit_to_view(encoder, destination, format)
    }
    /// One startup frame, with no drawable held and no readback texture/buffer.
    /// Complete it before window exposure; regular frames never use this wait.
    #[cfg(feature = "window")]
    #[tracing::instrument(name = "wgpu.startup_prime", skip_all)]
    pub(crate) fn prime_scene_plan(
        &mut self,
        plan: crate::scene::SceneFramePlan,
        format: wgpu::TextureFormat,
    ) -> Result<wgpu::CommandEncoder> {
        let result = (|| {
            self.prepare_present(format)?;
            let mut encoder = self.encode_scene_plan(plan)?;
            let submit_started = std::time::Instant::now();
            let slot = self.end_gpu_budget(&mut encoder);
            let submission = self.submit_frame_commands(encoder);
            self.arm_gpu_budget(slot);
            self.frame_budget.submit_ms += submit_started.elapsed().as_secs_f64() * 1e3;
            let wait_started = std::time::Instant::now();
            self.device
                .poll(wgpu::PollType::Wait {
                    submission_index: Some(submission),
                    timeout: Some(std::time::Duration::from_secs(1)),
                })
                .map_err(|source| Error::Poll { source })?;
            self.frame_budget.wait_ms += wait_started.elapsed().as_secs_f64() * 1e3;
            self.poll_gpu_budget();
            Ok(self
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("initial window copy"),
                }))
        })();
        if result.is_err() {
            self.scene_damage.invalidate();
        }
        result
    }

    /// Drop unsubmitted draw commands after failed acquisition, while flushing
    /// pending write_buffer/write_texture staging. Otherwise repeated failures
    /// can retain upload staging until some future successful submission.
    #[cfg(feature = "window")]
    pub(crate) fn abandon_scene_frame(&mut self, encoder: wgpu::CommandEncoder) {
        self.cancel_gpu_budget();
        self.pending_commands.clear();
        drop(encoder);
        self.queue.submit([]);
        self.invalidate_scene_damage();
    }
    pub(crate) fn prepare_present(&mut self, format: wgpu::TextureFormat) -> Result<()> {
        if !matches!(
            format,
            wgpu::TextureFormat::Rgba8Unorm
                | wgpu::TextureFormat::Rgba8UnormSrgb
                | wgpu::TextureFormat::Bgra8Unorm
                | wgpu::TextureFormat::Bgra8UnormSrgb
        ) {
            return Err(Error::InvalidInput {
                reason: "unsupported present target format".into(),
            });
        }
        if self.blitter.as_ref().is_none_or(|b| b.format != format) {
            self.blitter = Some(crate::blit::Blitter::new(&self.device, format));
        }
        Ok(())
    }
    #[tracing::instrument(
        level = "debug",
        target = "plico::perf",
        name = "wgpu.submit",
        skip_all
    )]
    pub(crate) fn submit_to_view(
        &mut self,
        mut encoder: wgpu::CommandEncoder,
        destination: &wgpu::TextureView,
        format: wgpu::TextureFormat,
    ) -> Result<()> {
        self.prepare_present(format)?;
        let submit_started = Instant::now();
        let view = &self.target.as_ref().unwrap().view;
        self.blitter
            .as_mut()
            .unwrap()
            .encode(&self.device, &mut encoder, view, destination);
        let slot = self.end_gpu_budget(&mut encoder);
        self.submit_frame_commands(encoder);
        self.arm_gpu_budget(slot);
        self.frame_budget.submit_ms += submit_started.elapsed().as_secs_f64() * 1e3;
        Ok(())
    }
}

impl Renderer {
    /// Present a frame submitted on this renderer's queue.
    pub fn present(&self, frame: wgpu::SurfaceTexture) {
        self.queue.present(frame);
    }
}
