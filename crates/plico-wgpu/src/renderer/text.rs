#[cfg(all(target_os = "linux", feature = "system-text"))]
use super::*;

#[cfg(all(target_os = "linux", feature = "system-text"))]
impl Renderer {
    /// Shape at the requested DPI; retain this layout until text/style/DPI changes.
    /// System fallback omissions are observable through `missing_glyphs`.
    #[tracing::instrument(name="plico_wgpu.layout_text",skip_all,fields(bytes=request.text.len(),size=request.size,scale=request.scale))]
    pub fn layout_text(
        &mut self,
        request: &crate::text::TextRequest<'_>,
    ) -> Result<crate::text::GlyphLayout> {
        if self.atlas.is_none() {
            self.atlas = Some(plico_native::text::atlas::Atlas::new().context(crate::TextSnafu)?);
        }
        self.atlas
            .as_ref()
            .unwrap()
            .layout(request)
            .context(crate::TextSnafu)
    }
    pub(crate) fn trim_native_atlas(&mut self) {
        self.atlas = None;
        self.atlas_texture.ensure_size(&self.device, 1);
    }
    pub fn text_stats(&self) -> crate::text::TextStats {
        let mut stats = self.text_stats;
        if let Some(atlas) = &self.atlas {
            stats.cached_glyphs = atlas.entry_count();
            stats.atlas_cpu_bytes = atlas.pixels.len();
        }
        stats.atlas_gpu_bytes = (self.atlas_texture.side * self.atlas_texture.side * 4) as usize;
        stats
    }
    pub(super) fn encode_text(
        &mut self,
        run: &crate::text::TextRun<'_>,
        size: [u32; 2],
        instances: &mut Vec<Instance>,
    ) -> Result<()> {
        if !run.scale.is_finite()
            || run.scale <= 0.
            || !run.origin.iter().all(|v| v.is_finite() && v.abs() < 1e9)
            || !run
                .color
                .iter()
                .all(|v| v.is_finite() && (0. ..=1.).contains(v))
        {
            return Err(Error::InvalidInput {
                reason: "invalid text origin or color".into(),
            });
        }
        let clip = run.clip.unwrap_or([0., 0., size[0] as f32, size[1] as f32]);
        let mut validation = Quad::new(clip, run.color);
        validation.clip = Some(clip);
        validation.validate()?;
        if self.atlas.is_none() {
            self.atlas = Some(plico_native::text::atlas::Atlas::new().context(crate::TextSnafu)?);
        }
        let atlas = self.atlas.as_mut().unwrap();
        let side = plico_native::text::atlas::ATLAS_SIZE as f32;
        for glyph in &run.layout.glyphs {
            let x = glyph.x * run.scale + run.origin[0];
            let y = glyph.y * run.scale + run.origin[1];
            let quantized = (x * 4.).round() as i64;
            let (x, y, phase) = if run.continuous || run.scale != 1. {
                (x, y, 0)
            } else {
                (
                    quantized.div_euclid(4) as f64,
                    y.round(),
                    quantized.rem_euclid(4) as u8,
                )
            };
            let [ix, iy, iw, ih] = glyph.ink_bounds;
            // Cull before rasterization/upload. Include hinting/phase/padding fringe.
            if iw <= 0.
                || ih <= 0.
                || x + (ix + iw) * run.scale + 3. < f64::from(clip[0])
                || y + (iy + ih) * run.scale + 3. < f64::from(clip[1])
                || x + ix * run.scale - 3. > f64::from(clip[0] + clip[2])
                || y + iy * run.scale - 3. > f64::from(clip[1] + clip[3])
            {
                continue;
            }
            let entry = atlas.get(glyph, phase).context(crate::TextSnafu)?;
            let [ax, ay, w, h] = entry.rect;
            let rect = [
                (x + entry.origin[0] as f64 * run.scale) as f32,
                (y + entry.origin[1] as f64 * run.scale) as f32,
                w as f32 * run.scale as f32,
                h as f32 * run.scale as f32,
            ];
            if rect[0] + rect[2] < clip[0]
                || rect[1] + rect[3] < clip[1]
                || rect[0] > clip[0] + clip[2]
                || rect[1] > clip[1] + clip[3]
            {
                continue;
            }
            instances.push(Instance {
                mask_rect: [0.; 4],
                radii: [0.; 4],
                rect,
                color: glyph
                    .foreground
                    .map_or(run.color, |rgba| rgba.map(|v| f32::from(v) / 255.)),
                color2: [0.; 4],
                border: [0.; 4],
                config: [0., 0., if entry.colored { 3. } else { 2. }, 0.],
                clip,
                gradient: [0.; 4],
                clip_params: [if run.clip_antialias { 1. } else { 0. }, 0., 0., 0.],
                uv: [
                    ax as f32 / side,
                    ay as f32 / side,
                    w as f32 / side,
                    h as f32 / side,
                ],
            });
        }
        Ok(())
    }
    pub(super) fn upload_glyphs(&mut self) {
        let Some(atlas) = &mut self.atlas else {
            return;
        };
        let side = plico_native::text::atlas::ATLAS_SIZE;
        self.atlas_texture.ensure_size(&self.device, side);
        for [x, y, w, h] in atlas.take_dirty() {
            let start = ((y * side + x) * 4) as usize;
            let end = start + (((h - 1) * side + w) * 4) as usize;
            self.queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &self.atlas_texture.texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d { x, y, z: 0 },
                    aspect: wgpu::TextureAspect::All,
                },
                &atlas.pixels[start..end],
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(side * 4),
                    rows_per_image: None,
                },
                wgpu::Extent3d {
                    width: w,
                    height: h,
                    depth_or_array_layers: 1,
                },
            );
            self.text_stats.uploaded_bytes += u64::from(w * h * 4);
            self.text_stats.uploaded_regions += 1;
        }
    }
}
