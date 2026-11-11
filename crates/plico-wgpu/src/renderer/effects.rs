use super::*;

pub(super) fn visit_images(items: DrawSource<'_>, visit: &mut impl FnMut(&peniko::ImageData)) {
    for draw in items.iter() {
        match draw {
            crate::Draw::Image(image) => visit(&image.image),
            crate::Draw::SetMask(mask) => visit(&mask.image),
            crate::Draw::Layer(layer) => visit_images(DrawSource::Prepared(&layer.scene), visit),
            _ => {}
        }
    }
}

fn visit_effects(items: DrawSource<'_>, visit: &mut impl FnMut(&plico_renderer::ShaderEffect)) {
    for draw in items.iter() {
        match draw {
            crate::Draw::Effect(effect, _) => visit(effect),
            crate::Draw::Layer(layer) => visit_effects(DrawSource::Prepared(&layer.scene), visit),
            _ => {}
        }
    }
}
impl Renderer {
    pub(super) fn prepare_effect_frame(&mut self, items: DrawSource<'_>) -> Result<()> {
        self.frame_effect_draws = 0;
        self.frame_effect_bytes = 0;
        if let Some(effects) = &mut self.effects {
            effects.begin_frame();
        }
        // Ordinary applications without WGSL surfaces do not traverse another
        // copy of the draw graph just to maintain an unused effect cache.
        if self
            .effects
            .as_ref()
            .is_none_or(|effects| effects.resource_stats().0 == 0)
        {
            return Ok(());
        }
        let mut bytes = 0_u64;
        let mut count = 0;
        let mut invalid = false;
        let limit = self.device.limits().max_texture_dimension_2d;
        visit_effects(items, &mut |effect| {
            let [w, h] = effect.physical_size();
            bytes = bytes.saturating_add(u64::from(w) * u64::from(h) * 4);
            count += 1;
            invalid |= w == 0 || h == 0 || u32::from(w) > limit || u32::from(h) > limit;
            if let Some(effects) = &mut self.effects {
                effects.retain_effect(effect);
            }
        });
        if invalid || count > 256 || bytes > 128 * 1024 * 1024 {
            return Err(Error::InvalidInput {
                reason: "WGSL surfaces exceed GPU dimensions, 256 draws or 128 MiB".into(),
            });
        }
        Ok(())
    }
    pub(super) fn render_effect(
        &mut self,
        effect: &plico_renderer::ShaderEffect,
    ) -> Result<std::rc::Rc<crate::image::GpuImage>> {
        let [w, h] = effect.physical_size();
        self.frame_effect_draws += 1;
        self.frame_effect_bytes = self
            .frame_effect_bytes
            .saturating_add(u64::from(w) * u64::from(h) * 4);
        let limit = self.device.limits().max_texture_dimension_2d;
        if w == 0
            || h == 0
            || u32::from(w) > limit
            || u32::from(h) > limit
            || self.frame_effect_draws > 256
            || self.frame_effect_bytes > 128 * 1024 * 1024
        {
            return Err(Error::InvalidInput {
                reason: "WGSL surfaces exceed GPU dimensions, 256 draws or 128 MiB".into(),
            });
        }
        let effects = self.effects.get_or_insert_with(|| {
            let mut effects = crate::shader::ShaderRenderer::new(&self.device);
            effects.begin_frame();
            effects
        });
        let scope = self.device.push_error_scope(wgpu::ErrorFilter::Validation);
        let mut encoder = self.device.create_command_encoder(&Default::default());
        let view = effects.render_view(effect, &self.device, &self.queue, &mut encoder);
        if let Some(error) = pollster::block_on(scope.pop()) {
            return Err(Error::Shader {
                message: error.to_string(),
            });
        }
        self.pending_commands.push(encoder.finish());
        Ok(crate::image::GpuImage::from_view(
            &self.device,
            &self.atlas_texture.layout,
            &view,
        ))
    }
    /// Native positioned-text cache: faces, unique retained source bytes,
    /// raster entries and raster/position payload bytes. Source bytes may be
    /// shared file mappings; this is not process RSS or newly allocated memory.
    pub fn positioned_glyph_cache_stats(&self) -> (usize, usize, usize, usize) {
        #[cfg(all(target_os = "linux", feature = "system-text"))]
        {
            self.glyphs.stats()
        }
        #[cfg(not(all(target_os = "linux", feature = "system-text")))]
        {
            (0, 0, 0, 0)
        }
    }
    /// Drop native positioned-font and bitmap caches; retained scenes can still
    /// own already prepared bitmap resources.
    pub fn trim_positioned_glyph_cache(&mut self) {
        #[cfg(all(target_os = "linux", feature = "system-text"))]
        {
            self.glyphs = Default::default();
        }
    }
    /// Live effect targets, pixel payload and cached pipelines.
    pub fn effect_cache_stats(&self) -> (usize, u64, usize) {
        self.effects
            .as_ref()
            .map_or((0, 0, 0), |effects| effects.resource_stats())
    }
    /// Discard derived effect resources while preserving final retained pixels.
    pub fn trim_effect_cache(&mut self) {
        self.effects = None;
    }
}
