use super::*;

impl Renderer {
    pub(super) fn prepare_instances(
        &mut self,
        items: DrawSource<'_>,
        size: [u32; 2],
        encoder: &mut wgpu::CommandEncoder,
    ) -> Result<Vec<Instance>> {
        #[cfg(all(target_os = "linux", feature = "system-text"))]
        {
            if self.layer_depth == 0
                && items
                    .iter()
                    .any(|item| matches!(item, crate::Draw::Layer(_)))
            {
                // Reserve the complete graph's glyphs before submitting any
                // child. A child must never recycle UVs already prepared in its parent.
                let mut scratch = std::mem::take(&mut self.instance_scratch);
                scratch.clear();
                let warm = self.warm_layer_text(items, size, &mut scratch);
                if matches!(
                    warm,
                    Err(Error::Text {
                        source: plico_native::text::Error::AtlasFull
                    })
                ) {
                    self.atlas.as_mut().expect("full atlas").clear();
                    self.text_stats.recycled_frames += 1;
                    self.warm_layer_text(items, size, &mut scratch)?;
                } else {
                    warm?;
                }
                self.instance_scratch = scratch;
            }
            let result = self.encode_items(items, size, encoder);
            let result = if matches!(
                result,
                Err(Error::Text {
                    source: plico_native::text::Error::AtlasFull
                })
            ) {
                self.atlas
                    .as_mut()
                    .expect("atlas full implies atlas exists")
                    .clear();
                self.text_stats.recycled_frames += 1;
                self.encode_items(items, size, encoder)
            } else {
                result
            };
            let instances = result?;
            self.upload_glyphs();
            Ok(instances)
        }
        #[cfg(not(all(target_os = "linux", feature = "system-text")))]
        self.encode_items(items, size, encoder)
    }
    #[cfg(all(target_os = "linux", feature = "system-text"))]
    fn warm_layer_text(
        &mut self,
        items: DrawSource<'_>,
        size: [u32; 2],
        scratch: &mut Vec<Instance>,
    ) -> Result<()> {
        for item in items.iter() {
            match &item {
                crate::Draw::Text(run) => {
                    scratch.clear();
                    self.encode_text(run, size, scratch)?;
                }
                crate::Draw::Layer(layer)
                    if layer.alpha > 0.
                        && !layer.quad.clip.is_some_and(|r| r[2] <= 0. || r[3] <= 0.) =>
                {
                    self.warm_layer_text(DrawSource::Prepared(&layer.scene), size, scratch)?
                }
                _ => {}
            }
        }
        Ok(())
    }
    /// Root color-target and optional CPU-readback buffer payload bytes.
    /// Excludes layer pools, swapchain images, driver overhead and in-flight resources.
    pub fn target_storage_bytes(&self) -> (u64, u64) {
        self.target.as_ref().map_or((0, 0), |target| {
            (
                u64::from(target.size[0]) * u64::from(target.size[1]) * 4,
                target.readback.as_ref().map_or(0, wgpu::Buffer::size),
            )
        })
    }
    /// Release the retained offscreen target and its readback buffer.
    /// A compositor-owned presented frame is unaffected. Further rendering must
    /// rebuild the target and its complete contents, even for an unchanged scene.
    pub fn trim_frame_target(&mut self) {
        if let Some(blitter) = &mut self.blitter {
            blitter.clear_source();
        }
        self.target = None;
        self.invalidate_scene_damage();
    }
    /// Release derived frame caches and process already completed GPU work.
    /// Retained final pixels stay available. This does not wait for a GPU fence
    /// or schedule more polling; pending work keeps its own resource references.
    /// Like device polling/readback, this may run queued completion callbacks.
    pub fn trim_caches(&mut self) -> Result<()> {
        self.trim_readback_buffer();
        self.trim_geometry_buffers();
        self.trim_raster_cache();
        self.trim_image_cache();
        self.trim_layer_pool();
        self.trim_effect_cache();
        self.trim_positioned_glyph_cache();
        #[cfg(all(
            any(target_os = "linux", target_os = "windows", target_os = "macos"),
            feature = "system-text"
        ))]
        self.trim_text_cache();
        self.device
            .poll(wgpu::PollType::Poll)
            .map(|_| ())
            .context(crate::PollSnafu)
    }
    /// Current instance GPU buffer and CPU staging capacity, in bytes. Excludes
    /// buffers still owned by encoded/submitted GPU work; this is not process RSS.
    pub fn instance_storage_bytes(&self) -> (u64, usize) {
        (
            self.instances.size(),
            self.instance_scratch.capacity() * std::mem::size_of::<Instance>(),
        )
    }
    /// Return the instance stream to startup capacity and release CPU frame scratch.
    /// Retained pixels and pipelines stay valid. Encoded/submitted commands own
    /// their old GPU buffers independently until that work is released.
    pub fn trim_geometry_buffers(&mut self) {
        self.clear_frame_references();
        self.instance_scratch = Vec::new();
        self.frame_images = Vec::new();
        self.frame_image_slots = Default::default();
        self.backdrop_effects = Vec::new();
        self.deferred_layers = Vec::new();
        if self.capacity > INITIAL_INSTANCE_CAPACITY {
            self.capacity = INITIAL_INSTANCE_CAPACITY;
            self.instances = Self::instance_buffer(&self.device, self.capacity);
        }
    }
    /// Release the optional screenshot/readback buffer without changing final pixels.
    /// A later capture allocates it again; normal window submission does not need it.
    pub fn trim_readback_buffer(&mut self) {
        if let Some(target) = &mut self.target {
            target.readback = None;
        }
    }
    /// Statistics for the last encoding attempt; skipped window presentation
    /// without encoding leaves the previous statistics intact.
    pub fn batch_stats(&self) -> FrameBatchStats {
        self.batch_stats
    }
    pub(super) fn clear_frame_images(&mut self) {
        self.frame_image_slots.clear();
        self.frame_images.clear();
    }
    pub(super) fn clear_frame_references(&mut self) {
        self.clear_frame_images();
        self.backdrop_effects.clear();
        self.deferred_layers.clear();
    }
    fn intern_frame_image(&mut self, image: std::rc::Rc<crate::image::GpuImage>) -> usize {
        // Pointer identity is local to this pass, and every registered resource
        // is strongly owned by this pass's current/saved frame_images. The
        // map and owners are saved/restored together for nested layers.
        // No pointer is dereferenced or retained as a cross-frame resource ID.
        let identity = std::rc::Rc::as_ptr(&image) as usize;
        if let Some(slot) = self.frame_image_slots.get(&identity) {
            return *slot;
        }
        let slot = self.frame_images.len();
        self.frame_images.push(image);
        self.frame_image_slots.insert(identity, slot);
        slot
    }
    fn encode_items(
        &mut self,
        items: DrawSource<'_>,
        size: [u32; 2],
        encoder: &mut wgpu::CommandEncoder,
    ) -> Result<Vec<Instance>> {
        let [width, height] = size;
        let mut instances = std::mem::take(&mut self.instance_scratch);
        instances.clear();
        self.clear_frame_references();
        let mut active_mask: Option<(usize, [f32; 4])> = None;
        for item in items.iter() {
            let first = instances.len();
            match &item {
                crate::Draw::SetMask(mask) => {
                    let [x, y, w, h] = mask.rect;
                    if ![x, y, w, h]
                        .iter()
                        .all(|v| v.is_finite() && v.fract() == 0.)
                        || w != mask.image.width as f32
                        || h != mask.image.height as f32
                    {
                        return Err(Error::InvalidInput {
                            reason: "invalid mask rectangle".into(),
                        });
                    }
                    let image = self.images.get_mask(
                        &self.device,
                        &self.queue,
                        &self.atlas_texture.layout,
                        &mask.image,
                    )?;
                    let slot = self.intern_frame_image(image);
                    active_mask = Some((slot, mask.rect));
                }
                crate::Draw::ResetMask => active_mask = None,

                crate::Draw::Quad(q) => {
                    q.validate()?;
                    if q.rect[2] <= 0. || q.rect[3] <= 0. {
                        continue;
                    }
                    let radii = shrink_radii([q.rect[2], q.rect[3]], q.radii);
                    instances.push(Instance {
                        mask_rect: [0.; 4],
                        radii,
                        rect: q.rect,
                        color: q.colors[0],
                        color2: q.colors[1],
                        border: q.border_color,
                        config: [
                            q.radii[0],
                            if q.shadow_hole.is_some() {
                                q.shadow_sigma
                            } else {
                                q.border_width
                            },
                            if q.shadow_hole.is_some() {
                                6.
                            } else if q.shadow_sigma > 0. || q.shadow_smoothing > 0. {
                                5.
                            } else if q.material {
                                1.
                            } else {
                                0.
                            },
                            if q.shadow_sigma > 0.
                                || q.shadow_hole.is_some()
                                || q.shadow_smoothing > 0.
                            {
                                q.shadow_smoothing
                            } else {
                                q.parameter
                            },
                        ],
                        clip: q.clip.unwrap_or([0., 0., width as f32, height as f32]),
                        uv: [0.; 4],
                        gradient: if let Some(hole) = q.shadow_hole {
                            hole
                        } else if q.shadow_sigma > 0. || q.shadow_smoothing > 0. {
                            [
                                4. * q.shadow_sigma,
                                q.shadow_sigma,
                                q.rect[2] - 8. * q.shadow_sigma,
                                q.rect[3] - 8. * q.shadow_sigma,
                            ]
                        } else {
                            q.gradient
                        },
                        clip_params: [
                            if q.clip_antialias { 1. } else { 0. },
                            if q.gradient_premultiplied { 1. } else { 0. },
                            0.,
                            0.,
                        ],
                    });
                }
                crate::Draw::Effect(effect, q) => {
                    q.validate()?;
                    if effect.reads_backdrop() {
                        // Backdrop readers sample what the frame paints before
                        // them; that content is drawn by the pass this instance
                        // belongs to. Encode a placeholder here; encode_frame_at
                        // splits the pass at it, copies the target and renders
                        // the effect between the two halves (mygo's Backdrop
                        // step between batches).
                        self.backdrop_effects.push((*effect).clone());
                        instances.push(Instance {
                            mask_rect: [0.; 4],
                            radii: [0.; 4],
                            rect: q.rect,
                            color: [0., 0., 0., 2.], // kind: backdrop marker
                            color2: [0.; 4],
                            border: [0.; 4],
                            config: [6., 0., 0., (self.backdrop_effects.len() - 1) as f32],
                            clip: q.clip.unwrap_or([0., 0., width as f32, height as f32]),
                            uv: [0.; 4],
                            gradient: [0.; 4],
                            clip_params: [if q.clip_antialias { 1. } else { 0. }, 0., 0., 0.],
                        });
                    } else {
                        let resource = self.render_effect(effect, encoder)?;
                        let slot = self.intern_frame_image(resource);
                        instances.push(Instance {
                            mask_rect: [0.; 4],
                            radii: [0.; 4],
                            rect: q.rect,
                            color: [0., 0., 0., 1.],
                            color2: [0.; 4],
                            border: [0.; 4],
                            config: [0., 0., 4., slot as f32],
                            clip: q.clip.unwrap_or([0., 0., width as f32, height as f32]),
                            uv: [0., 0., 1., 1.],
                            gradient: [0.; 4],
                            clip_params: [if q.clip_antialias { 1. } else { 0. }, 0., 0., 0.],
                        });
                    }
                    #[cfg(target_arch = "wasm32")]
                    {
                        let _ = (effect, q);
                        return Err(Error::Unsupported {
                            command: 0,
                            reason: "WGSL surface compilation currently requires the native host"
                                .into(),
                        });
                    }
                }
                crate::Draw::Layer(layer) => {
                    if layer.alpha == 0.
                        || layer
                            .quad
                            .clip
                            .is_some_and(|clip| clip[2] <= 0. || clip[3] <= 0.)
                    {
                        continue;
                    }
                    let deferred = layer.scene.reads_backdrop();
                    let slot = if deferred {
                        let slot = self.deferred_layers.len();
                        self.deferred_layers.push((*layer).clone());
                        slot
                    } else {
                        let resource = self.render_layer(layer, None, encoder)?;
                        self.intern_frame_image(resource.image)
                    };
                    let q = &layer.quad;
                    instances.push(Instance {
                        mask_rect: [0.; 4],
                        radii: shrink_radii([q.rect[2], q.rect[3]], q.radii),
                        rect: q.rect,
                        color: [0., 0., 0., if deferred { 3. } else { layer.alpha }],
                        color2: [0.; 4],
                        border: [0.; 4],
                        config: [0., if layer.additive { 1. } else { 0. }, 4., slot as f32],
                        clip: q.clip.unwrap_or([0., 0., width as f32, height as f32]),
                        uv: [0., 0., 1., 1.],
                        gradient: [0.; 4],
                        clip_params: [if q.clip_antialias { 1. } else { 0. }, 0., 0., 0.],
                    });
                }
                crate::Draw::Image(image) => {
                    let q = &image.quad;
                    q.validate()?;
                    if !image.alpha.is_finite()
                        || !(0. ..=1.).contains(&image.alpha)
                        || !image.uv.iter().all(|v| v.is_finite())
                        || image
                            .uv_transform
                            .is_some_and(|m| m.iter().any(|v| !v.is_finite()))
                    {
                        return Err(Error::InvalidInput {
                            reason: "invalid image alpha/UV".into(),
                        });
                    }
                    if q.rect[2] <= 0. || q.rect[3] <= 0. {
                        continue;
                    }
                    let resource = self.images.get(
                        &self.device,
                        &self.queue,
                        &self.atlas_texture.layout,
                        &image.image,
                        crate::image::ImageSampling {
                            quality: image.quality,
                            extend: image.extend,
                            mipmaps: true,
                        },
                    )?;
                    let mip_levels = resource.mip_levels;
                    let slot = self.intern_frame_image(resource);
                    instances.push(Instance {
                        mask_rect: [0.; 4],
                        radii: shrink_radii([q.rect[2], q.rect[3]], q.radii),
                        rect: q.rect,
                        color: [0., 0., 0., image.alpha],
                        color2: [mip_levels as f32, 0., 0., 0.],
                        border: [0.; 4],
                        config: [q.radii[0], 0., 4., slot as f32],
                        clip: q.clip.unwrap_or([0., 0., width as f32, height as f32]),
                        uv: image
                            .uv_transform
                            .map_or(image.uv, |m| [m[0], m[1], m[2], m[3]]),
                        gradient: {
                            let mode = |v| match v {
                                peniko::Extend::Pad => 0.,
                                peniko::Extend::Repeat => 1.,
                                peniko::Extend::Reflect => 2.,
                            };
                            [
                                mode(image.extend[0]),
                                mode(image.extend[1]),
                                image.uv_transform.map_or(0., |m| m[4]),
                                image.uv_transform.map_or(0., |m| m[5]),
                            ]
                        },
                        clip_params: [
                            if q.clip_antialias { 1. } else { 0. },
                            if image.uv_transform.is_some() { 1. } else { 0. },
                            0.,
                            if image.quality == peniko::ImageQuality::High {
                                1.
                            } else if image.quality == peniko::ImageQuality::Low {
                                -1.
                            } else {
                                0.
                            },
                        ],
                    });
                }
                #[cfg(all(target_os = "linux", feature = "system-text"))]
                crate::Draw::Text(run) => self.encode_text(run, size, &mut instances)?,
            }
            if let Some((slot, rect)) = active_mask {
                for instance in &mut instances[first..] {
                    instance.clip_params[2] = slot as f32 + 1.;
                    instance.mask_rect = rect;
                }
            }
            #[cfg(test)]
            let skip_clip = !self.profile_no_clip_skip;
            #[cfg(not(test))]
            let skip_clip = true;
            if skip_clip {
                for instance in &mut instances[first..] {
                    self.batch_stats.clip_coverage_skips += u64::from(skip_interior_clip(instance));
                }
            }
            if instances.len() > 1_000_000 {
                return Err(Error::InvalidInput {
                    reason: "too many visible instances".into(),
                });
            }
        }
        Ok(instances)
    }
}
