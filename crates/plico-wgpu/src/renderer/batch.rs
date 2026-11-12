use super::ring::ring_scissors;
use super::*;

fn rect(rect: [f32; 4], inset: f32) -> Option<[f32; 4]> {
    let [x, y, width, height] = rect;
    let out = [x + inset, y + inset, x + width - inset, y + height - inset];
    (rect.iter().all(|value| value.is_finite())
        && out.iter().all(|value| value.is_finite())
        && out[2] > out[0]
        && out[3] > out[1])
        .then_some(out)
}

fn intersect(a: [f32; 4], b: [f32; 4]) -> Option<[f32; 4]> {
    let out = [
        a[0].max(b[0]),
        a[1].max(b[1]),
        a[2].min(b[2]),
        a[3].min(b[3]),
    ];
    (out[2] > out[0] && out[3] > out[1]).then_some(out)
}

fn clipped(
    mut bounds: [f32; 4],
    clip: Option<[f32; 4]>,
    mask: Option<[f32; 4]>,
) -> Option<[f32; 4]> {
    if let Some(clip) = clip {
        bounds = intersect(bounds, rect(clip, -1.)?)?;
    }
    if let Some(mask) = mask {
        bounds = intersect(bounds, rect(mask, -1.)?)?;
    }
    Some(bounds)
}

fn quad_visible(quad: &Quad, mask: Option<[f32; 4]>) -> Option<[f32; 4]> {
    clipped(rect(quad.rect, -1.)?, quad.clip, mask)
}

/// The rectangle whose covered pixel samples are certainly opaque. One pixel
/// of slack excludes the quad's antialiased edge and any fractional clip edge.
fn opaque_core(quad: &Quad, masked: bool) -> Option<[f32; 4]> {
    if masked
        || quad.material
        || quad.shadow_hole.is_some()
        || quad.shadow_sigma != 0.
        || quad.shadow_smoothing != 0.
        || quad.border_width != 0.
        || quad.radii != [0.; 4]
        || quad.clip_radii != [0.; 4]
        || quad.colors[0][3] != 1.
        || quad.colors[1][3] != 1.
    {
        return None;
    }
    let mut core = rect(quad.rect, 1.)?;
    if let Some(clip) = quad.clip {
        core = intersect(core, rect(clip, 1.)?)?;
    }
    Some(core)
}

fn draw_clip_radii(draw: crate::Draw<'_>) -> [f32; 4] {
    let quad = |quad: &Quad| {
        quad.clip.map_or([0.; 4], |clip| {
            shrink_radii([clip[2], clip[3]], quad.clip_radii)
        })
    };
    match draw {
        crate::Draw::Quad(value) | crate::Draw::Effect(_, value) => quad(value),
        crate::Draw::Image(image) => quad(&image.quad),
        crate::Draw::ExternalImage(image) => quad(&image.quad),
        crate::Draw::Layer(layer) => quad(&layer.quad),
        crate::Draw::SetMask(_) | crate::Draw::ResetMask => [0.; 4],
        #[cfg(all(target_os = "linux", feature = "system-text"))]
        crate::Draw::Text(run) => run.clip.map_or([0.; 4], |clip| {
            shrink_radii([clip[2], clip[3]], run.clip_radii)
        }),
    }
}

fn draw_info(draw: crate::Draw<'_>, mask: Option<[f32; 4]>) -> OcclusionInfo {
    let quad = |quad: &Quad| quad_visible(quad, mask);
    match draw {
        crate::Draw::Quad(value) => OcclusionInfo {
            visible: quad(value),
            opaque: opaque_core(value, mask.is_some()),
            ..Default::default()
        },
        crate::Draw::Image(value) => OcclusionInfo {
            visible: quad(&value.quad),
            ..Default::default()
        },
        crate::Draw::ExternalImage(value) => OcclusionInfo {
            visible: quad(&value.quad),
            ..Default::default()
        },
        crate::Draw::Layer(value) => OcclusionInfo {
            visible: if value.scene.occlusion_cullable() {
                quad(&value.quad)
            } else {
                None
            },
            barrier: value.scene.reads_backdrop(),
            ..Default::default()
        },
        // Effects may carry renderer work beyond their output rectangle. MyGO
        // likewise never culls them; backdrop readers additionally stop covers
        // from reaching draws before the read.
        crate::Draw::Effect(effect, _) => OcclusionInfo {
            barrier: effect.reads_backdrop(),
            ..Default::default()
        },
        crate::Draw::SetMask(_) | crate::Draw::ResetMask => OcclusionInfo::default(),
        #[cfg(all(target_os = "linux", feature = "system-text"))]
        crate::Draw::Text(run) => {
            let mut visible: Option<[f32; 4]> = None;
            for glyph in &run.layout.glyphs {
                let [x, y, width, height] = glyph.ink_bounds;
                if width <= 0. || height <= 0. {
                    continue;
                }
                let glyph = [
                    (run.origin[0] + (glyph.x + x) * run.scale - 4.) as f32,
                    (run.origin[1] + (glyph.y + y) * run.scale - 4.) as f32,
                    (run.origin[0] + (glyph.x + x + width) * run.scale + 4.) as f32,
                    (run.origin[1] + (glyph.y + y + height) * run.scale + 4.) as f32,
                ];
                if !glyph.iter().all(|value| value.is_finite()) {
                    return OcclusionInfo::default();
                }
                visible = Some(match visible {
                    None => glyph,
                    Some(old) => [
                        old[0].min(glyph[0]),
                        old[1].min(glyph[1]),
                        old[2].max(glyph[2]),
                        old[3].max(glyph[3]),
                    ],
                });
            }
            OcclusionInfo {
                visible: visible.and_then(|bounds| clipped(bounds, run.clip, mask)),
                ..Default::default()
            }
        }
    }
}

fn contains(outer: [f32; 4], inner: [f32; 4]) -> bool {
    outer[0] <= inner[0] && outer[1] <= inner[1] && outer[2] >= inner[2] && outer[3] >= inner[3]
}

fn validate_image_draw(image: &crate::image::ImageQuad) -> Result<()> {
    image.quad.validate()?;
    if !image.alpha.is_finite()
        || !(0. ..=1.).contains(&image.alpha)
        || !image.uv.iter().all(|value| value.is_finite())
        || image
            .uv_transform
            .is_some_and(|matrix| matrix.iter().any(|value| !value.is_finite()))
    {
        return Err(Error::InvalidInput {
            reason: "invalid image alpha/UV".into(),
        });
    }
    Ok(())
}

fn validate_culled_draw(renderer: &Renderer, draw: crate::Draw<'_>, _size: [u32; 2]) -> Result<()> {
    match draw {
        crate::Draw::Quad(quad) | crate::Draw::Effect(_, quad) => quad.validate(),
        crate::Draw::Image(image) => {
            validate_image_draw(image)?;
            crate::image::ImageCache::validate_source(&renderer.device, &image.image).map(|_| ())
        }
        crate::Draw::ExternalImage(image) => image.quad.validate(),
        crate::Draw::Layer(layer) => layer.quad.validate(),
        crate::Draw::SetMask(_) | crate::Draw::ResetMask => Ok(()),
        #[cfg(all(target_os = "linux", feature = "system-text"))]
        crate::Draw::Text(run) => {
            if !run.scale.is_finite()
                || run.scale <= 0.
                || !run
                    .origin
                    .iter()
                    .all(|value| value.is_finite() && value.abs() < 1e9)
                || !run
                    .color
                    .iter()
                    .all(|value| value.is_finite() && (0. ..=1.).contains(value))
                || !run
                    .clip_radii
                    .iter()
                    .all(|radius| radius.is_finite() && *radius >= 0.)
                || (run.clip.is_none() && run.clip_radii != [0.; 4])
            {
                return Err(Error::InvalidInput {
                    reason: "invalid text origin, color, or clip radii".into(),
                });
            }
            let clip = run
                .clip
                .unwrap_or([0., 0., _size[0] as f32, _size[1] as f32]);
            let mut validation = Quad::new(clip, run.color);
            validation.clip = Some(clip);
            validation.validate()
        }
    }
}

fn mark_occluded(plan: &mut [OcclusionInfo]) -> u64 {
    // Keep only the eight largest later covers, bounding the reverse pass just
    // as MyGO does. Backdrop readers are painter-order barriers.
    let mut covers: [Option<([f32; 4], f32)>; 8] = [None; 8];
    let mut count = 0usize;
    let mut culled = 0u64;
    for info in plan.iter_mut().rev() {
        if info.barrier {
            covers.fill(None);
            count = 0;
            continue;
        }
        if info.visible.is_some_and(|visible| {
            covers[..count]
                .iter()
                .flatten()
                .any(|(cover, _)| contains(*cover, visible))
        }) {
            info.culled = true;
            culled += 1;
            continue;
        }
        let Some(cover) = info.opaque else { continue };
        let area = (cover[2] - cover[0]) * (cover[3] - cover[1]);
        if count < covers.len() {
            covers[count] = Some((cover, area));
            count += 1;
        } else {
            let smallest = covers
                .iter()
                .enumerate()
                .min_by(|(_, a), (_, b)| a.unwrap().1.total_cmp(&b.unwrap().1))
                .unwrap()
                .0;
            if area > covers[smallest].unwrap().1 {
                covers[smallest] = Some((cover, area));
            }
        }
    }
    culled
}

fn plan_occlusion(items: DrawSource<'_>, plan: &mut Vec<OcclusionInfo>) -> u64 {
    plan.clear();

    // Like MyGO, sprite/glyph-only scenes avoid the bounds pass entirely.
    let mut mask = None;
    let mut candidate = false;
    for draw in items.iter() {
        match draw {
            crate::Draw::SetMask(value) => mask = Some(value.rect),
            crate::Draw::ResetMask => mask = None,
            crate::Draw::Quad(value) => candidate |= opaque_core(value, mask.is_some()).is_some(),
            _ => {}
        }
    }
    if !candidate {
        return 0;
    }

    plan.resize(items.len(), OcclusionInfo::default());
    mask = None;
    for (index, draw) in items.iter().enumerate() {
        plan[index] = draw_info(draw, mask);
        match draw {
            crate::Draw::SetMask(value) => mask = Some(value.rect),
            crate::Draw::ResetMask => mask = None,
            _ => {}
        }
    }
    mark_occluded(plan)
}

impl Renderer {
    pub(super) fn prepare_instances(
        &mut self,
        items: DrawSource<'_>,
        size: [u32; 2],
        origin: [f32; 2],
        encoder: &mut wgpu::CommandEncoder,
    ) -> Result<PreparedInstances> {
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
            let result = self.encode_items(items, size, origin, encoder);
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
                self.encode_items(items, size, origin, encoder)
            } else {
                result
            };
            let instances = result?;
            self.upload_glyphs();
            Ok(instances)
        }
        #[cfg(not(all(target_os = "linux", feature = "system-text")))]
        self.encode_items(items, size, origin, encoder)
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
            self.instance_scratch.capacity() * std::mem::size_of::<Instance>()
                + self.scissor_scratch.capacity() * std::mem::size_of::<Option<[u32; 4]>>(),
        )
    }
    /// Return the instance stream to startup capacity and release CPU frame scratch.
    /// Retained pixels and pipelines stay valid. Encoded/submitted commands own
    /// their old GPU buffers independently until that work is released.
    pub fn trim_geometry_buffers(&mut self) {
        self.clear_frame_references();
        self.instance_scratch = Vec::new();
        self.scissor_scratch = Vec::new();
        self.occlusion_scratch = Vec::new();
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
        origin: [f32; 2],
        encoder: &mut wgpu::CommandEncoder,
    ) -> Result<PreparedInstances> {
        let [width, height] = size;
        let mut instances = std::mem::take(&mut self.instance_scratch);
        instances.clear();
        let mut scissors = std::mem::take(&mut self.scissor_scratch);
        scissors.clear();
        self.clear_frame_references();
        let depth = self.layer_depth as usize;
        self.occlusion_scratch.resize_with(depth + 1, Vec::new);
        let culled = if self.force_no_occlusion_culling {
            self.occlusion_scratch[depth].clear();
            0
        } else {
            plan_occlusion(items, &mut self.occlusion_scratch[depth])
        };
        self.batch_stats.occlusion_culled_draws += culled;
        let mut active_mask: Option<(usize, [f32; 4])> = None;
        for (index, item) in items.iter().enumerate() {
            let analytic_clip = draw_clip_radii(item);
            if analytic_clip != [0.; 4] && active_mask.is_some() {
                return Err(Error::InvalidInput {
                    reason: "analytic rounded clip cannot share a coverage mask".into(),
                });
            }
            if self.occlusion_scratch[depth]
                .get(index)
                .is_some_and(|info| info.culled)
            {
                validate_culled_draw(self, item, size)?;
                continue;
            }
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
                    validate_image_draw(image)?;
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
                crate::Draw::ExternalImage(image) => {
                    let q = &image.quad;
                    q.validate()?;
                    if q.rect[2] <= 0. || q.rect[3] <= 0. {
                        continue;
                    }
                    let texture = image
                        .surface
                        .downcast_ref::<crate::ExternalTexture>()
                        .ok_or_else(|| Error::Unsupported {
                            command: 0,
                            reason: "external image is not a wgpu texture".into(),
                        })?;
                    let view = texture
                        .texture()
                        .create_view(&wgpu::TextureViewDescriptor::default());
                    let resource = crate::image::GpuImage::from_view(
                        &self.device,
                        &self.atlas_texture.layout,
                        &view,
                    );
                    let slot = self.intern_frame_image(resource);
                    instances.push(Instance {
                        mask_rect: [0.; 4],
                        radii: shrink_radii([q.rect[2], q.rect[3]], q.radii),
                        rect: q.rect,
                        color: [0., 0., 0., image.alpha],
                        color2: [1., 0., 0., 0.],
                        border: [0.; 4],
                        config: [q.radii[0], 0., 4., slot as f32],
                        clip: q.clip.unwrap_or([0., 0., width as f32, height as f32]),
                        uv: [0., 0., 1., 1.],
                        gradient: [0.; 4],
                        clip_params: [if q.clip_antialias { 1. } else { 0. }, 0., 0., 0.],
                    });
                }
                #[cfg(all(target_os = "linux", feature = "system-text"))]
                crate::Draw::Text(run) => self.encode_text(run, size, &mut instances)?,
            }
            if analytic_clip != [0.; 4] {
                for instance in &mut instances[first..] {
                    // clip_params.x=2 selects rounded analytical coverage;
                    // mask_rect is otherwise unused without a sampled mask.
                    instance.clip_params[0] = 2.;
                    instance.mask_rect = analytic_clip;
                    self.batch_stats.analytic_clip_draws += 1;
                }
            }
            if let Some((slot, rect)) = active_mask {
                for instance in &mut instances[first..] {
                    instance.clip_params[2] = slot as f32 + 1.;
                    instance.mask_rect = rect;
                }
            }
            scissors.resize(instances.len(), None);
            if !self.force_no_ring_splitting
                && let crate::Draw::Quad(quad) = item
                && instances.len() == first + 1
                && let Some(strips) = ring_scissors(quad, size, origin)
            {
                let strips = strips.as_slice();
                let instance = instances[first];
                scissors[first] = Some(strips[0]);
                for strip in &strips[1..] {
                    instances.push(instance);
                    scissors.push(Some(*strip));
                }
                self.batch_stats.ring_split_quads += 1;
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
        Ok(PreparedInstances {
            instances,
            scissors,
        })
    }
}

#[cfg(test)]
mod occlusion_tests {
    use super::*;

    #[test]
    fn later_opaque_rectangles_cull_only_fully_hidden_draws() {
        let hidden = Quad::new([20., 20., 10., 10.], [1., 0., 0., 0.5]);
        let crossing = Quad::new([95., 20., 10., 10.], [0., 1., 0., 0.5]);
        let cover = Quad::new([0., 0., 100., 100.], [0., 0., 1., 1.]);
        let draws = [
            crate::Draw::Quad(&hidden),
            crate::Draw::Quad(&crossing),
            crate::Draw::Quad(&cover),
        ];
        let mut plan = Vec::new();
        assert_eq!(plan_occlusion(DrawSource::Borrowed(&draws), &mut plan), 1);
        assert!(plan[0].culled);
        assert!(!plan[1].culled);
        assert!(!plan[2].culled);
    }

    #[test]
    fn backdrop_barriers_stop_later_covers_from_reaching_prior_draws() {
        let mut plan = vec![
            OcclusionInfo {
                visible: Some([10., 10., 20., 20.]),
                ..Default::default()
            },
            OcclusionInfo {
                barrier: true,
                ..Default::default()
            },
            OcclusionInfo {
                visible: Some([0., 0., 100., 100.]),
                opaque: Some([1., 1., 99., 99.]),
                ..Default::default()
            },
        ];
        assert_eq!(mark_occluded(&mut plan), 0);
        assert!(!plan[0].culled);
    }

    #[test]
    fn rounded_translucent_and_masked_fills_are_not_occluders() {
        let mut rounded = Quad::new([0., 0., 100., 100.], [0.; 4]);
        rounded.radii = [8.; 4];
        assert!(opaque_core(&rounded, false).is_none());
        let translucent = Quad::new([0., 0., 100., 100.], [0., 0., 0., 0.9]);
        assert!(opaque_core(&translucent, false).is_none());
        let opaque = Quad::new([0., 0., 100., 100.], [0., 0., 0., 1.]);
        assert!(opaque_core(&opaque, true).is_none());
    }
}
