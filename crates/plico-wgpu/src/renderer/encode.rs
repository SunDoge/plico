use super::backdrop::{encode_backdrop_step, plan_backdrops};
use super::effects::visit_images;
use super::*;

impl Renderer {
    pub(crate) fn validate_frame(
        &self,
        item_count: usize,
        size: [u32; 2],
        background: [f32; 4],
        time: f32,
    ) -> Result<()> {
        let limit = self.device.limits();
        let [width, height] = size;
        let pitch = u64::from(width).saturating_mul(4).div_ceil(256) * 256;
        if width == 0
            || height == 0
            || width > limit.max_texture_dimension_2d
            || height > limit.max_texture_dimension_2d
            || pitch.saturating_mul(u64::from(height)) > limit.max_buffer_size
            || item_count > 1_000_000
            || !time.is_finite()
            || !background
                .iter()
                .all(|v| v.is_finite() && (0. ..=1.).contains(v))
        {
            return Err(Error::InvalidInput {
                reason: "frame exceeds GPU limits or contains invalid size/time/color".into(),
            });
        }
        Ok(())
    }
    /// Draw in painter order and return tightly packed premultiplied RGBA8.
    /// Time is externally injected, in seconds; the renderer never reads a clock.
    pub fn render(
        &mut self,
        quads: &[Quad],
        size: [u32; 2],
        background: [f32; 4],
        time: f32,
    ) -> Result<Vec<u8>> {
        let items: Vec<_> = quads.iter().map(crate::Draw::Quad).collect();
        self.render_items(&items, size, background, time)
    }
    pub(super) fn encode_frame(
        &mut self,
        items: &[crate::Draw<'_>],
        size: [u32; 2],
        background: [f32; 4],
        time: f32,
    ) -> Result<wgpu::CommandEncoder> {
        self.scene_damage.invalidate();
        self.encode_frame_regions(DrawSource::Borrowed(items), size, background, time, None)
    }

    #[tracing::instrument(
        level = "debug",
        target = "plico::perf",
        name = "wgpu.encode",
        skip_all
    )]
    /// `regions` must be disjoint, as produced by SceneDamageTracker. None
    /// replays the full target; an empty set preserves all retained pixels.
    pub(crate) fn encode_frame_regions(
        &mut self,
        items: DrawSource<'_>,
        size: [u32; 2],
        background: [f32; 4],
        time: f32,
        regions: Option<&[[u32; 4]]>,
    ) -> Result<wgpu::CommandEncoder> {
        self.batch_stats = FrameBatchStats::default();
        self.frame_budget = FrameBudget::default();
        if self.layer_depth == 0 {
            self.pending_commands.clear();
            self.poll_gpu_budget();
            self.backdrop_pool.begin_frame();
            if let Some(compositor) = &mut self.backdrop_compositor {
                compositor.begin_frame();
            }
        }
        self.validate_frame(items.len(), size, background, time)?;
        let unchanged = regions.is_some_and(|r| r.is_empty())
            && self.target.as_ref().is_some_and(|t| t.size == size);
        let counters = (self.layer_allocations, self.layer_reuses);
        if !unchanged {
            self.images.begin_frame();
            visit_images(items, &mut |image| self.images.retain_source(image));
            self.prepare_effect_frame(items)?;
        }
        let result = self.encode_frame_at(items, size, background, time, regions, [0.; 2]);
        if result.is_ok()
            && let Some(mips) = self.images.pending_mip_commands(&self.device)
        {
            // Image reductions must execute before child layers or the final
            // frame samples them. They remain one command buffer in the same
            // queue submission rather than creating a hidden cold-frame submit.
            self.pending_commands.insert(0, mips);
        }
        if result.is_err() {
            self.pending_commands.clear();
        }
        // Encoded commands retain their own GPU references. Pass-local image
        // bindings, deferred layer scenes and effects must not keep evicted
        // textures/CPU raster payload alive until another frame or after trim.
        self.clear_frame_references();
        if !unchanged && result.is_ok() {
            self.images.finish_frame();
        }
        if !unchanged {
            if result.is_err() {
                self.effects = None;
            } else if let Some(effects) = &mut self.effects {
                effects.finish_frame();
            }
        }
        if self.layer_depth == 0 && (!unchanged || result.is_err()) {
            self.trim_idle_layers(
                items
                    .iter()
                    .any(|item| matches!(item, crate::Draw::Layer(_))),
            );
        }
        if !unchanged {
            let mut bytes = 0u64;
            for (depth, context) in self.backdrop_contexts.iter_mut().enumerate() {
                if let Some(entry) = context {
                    let payload = u64::from(entry.size[0]) * u64::from(entry.size[1]) * 4;
                    if result.is_err()
                        || !self.context_charged.get(depth).copied().unwrap_or(false)
                        || bytes + payload > 64 * 1024 * 1024
                    {
                        *context = None;
                    } else {
                        bytes += payload;
                    }
                }
            }
        }
        if tracing::enabled!(target: "plico::perf", tracing::Level::TRACE) {
            let stats = self.layer_pool_stats();
            let effect_stats = self.effect_cache_stats();
            let positioned = self.positioned_glyph_cache_stats();
            let image_sources = self.image_cache_stats();
            let image_bindings = self.image_binding_stats();
            let target = self.target_storage_bytes();
            let rasters = self.raster_cache_stats();
            tracing::trace!(target:"plico::perf",target_color_bytes=target.0,readback_buffer_bytes=target.1,path_raster_bytes=rasters[0].1,svg_raster_bytes=rasters[1].1,clip_raster_bytes=rasters[2].1,layer_allocations=self.layer_allocations-counters.0,
                layer_reuses=self.layer_reuses-counters.1,layer_frame_bytes=self.frame_layer_bytes,
                layer_pool_entries=stats.entries,layer_pool_bytes=stats.retained_bytes,filter_allocations=stats.filter_allocations,filter_reuses=stats.filter_reuses,backdrop_pool_entries=stats.backdrop_entries,backdrop_pool_bytes=stats.backdrop_bytes,backdrop_allocations_total=stats.backdrop_allocations,backdrop_reuses_total=stats.backdrop_reuses,backdrop_uniform_entries=stats.backdrop_uniform_entries,backdrop_uniform_allocations_total=stats.backdrop_uniform_allocations,backdrop_uniform_reuses_total=stats.backdrop_uniform_reuses,effect_targets=effect_stats.0,effect_bytes=effect_stats.1,effect_pipelines=effect_stats.2,positioned_fonts=positioned.0,positioned_source_bytes=positioned.1,positioned_rasters=positioned.2,positioned_raster_bytes=positioned.3,image_sources=image_sources.0,image_texture_bytes=image_sources.1,image_uploads_total=image_sources.2,image_source_hits_total=image_sources.3,image_bindings=image_bindings.0,image_binding_creations_total=image_bindings.1,scene_instances=self.batch_stats.instances,scene_draw_calls=self.batch_stats.draw_calls,scene_passes=self.batch_stats.passes,damage_clears=self.batch_stats.clears,"wgpu frame resources");
        }
        #[cfg(all(target_os = "linux", feature = "system-text"))]
        if tracing::enabled!(target: "plico::perf", tracing::Level::TRACE) {
            let stats = self.paragraph_cache_stats();
            let atlas = self.text_stats();
            tracing::trace!(target: "plico::perf", paragraphs=stats.layouts,
                paragraph_rust_bytes=stats.rust_payload_bytes, paragraph_hits_total=stats.hits,
                paragraph_misses_total=stats.misses, paragraph_evictions_total=stats.evictions,
                application_fonts=stats.fonts, application_font_bytes=stats.font_bytes,
                atlas_cpu_bytes=atlas.atlas_cpu_bytes, atlas_gpu_bytes=atlas.atlas_gpu_bytes,
                glyph_upload_bytes_total=atlas.uploaded_bytes, "wgpu native text resources");
        }
        #[cfg(all(target_os = "windows", feature = "system-text"))]
        if tracing::enabled!(target: "plico::perf", tracing::Level::TRACE) {
            let stats = self.windows_text_cache_stats();
            tracing::trace!(target: "plico::perf", paragraphs=stats.layouts,
                paragraph_rust_bytes=stats.layout_bytes, rasters=stats.rasters,
                raster_payload_bytes=stats.raster_bytes, rasterizations_total=stats.rasterizations,
                colorizations_total=stats.colorizations, application_fonts=stats.font_files,
                native_font_copy_bytes=stats.font_copy_bytes, "wgpu DirectWrite resources");
        }
        #[cfg(all(target_os = "macos", feature = "system-text"))]
        if tracing::enabled!(target: "plico::perf", tracing::Level::TRACE) {
            let stats = self.macos_text_cache_stats();
            tracing::trace!(target: "plico::perf", paragraphs=stats.layouts, paragraph_rust_bytes=stats.layout_bytes,
                rasters=stats.rasters, raster_payload_bytes=stats.raster_bytes, rasterizations_total=stats.rasterizations,
                intrinsic_entries=stats.intrinsic_entries, intrinsic_bytes=stats.intrinsic_bytes,
                intrinsic_measurements_total=stats.intrinsic_measurements, "wgpu CoreText resources");
        }
        result
    }
    #[tracing::instrument(name="plico_wgpu.encode_frame",skip_all,fields(width=size[0],height=size[1],draws=items.len()))]
    pub(super) fn encode_frame_at(
        &mut self,
        items: DrawSource<'_>,
        size: [u32; 2],
        background: [f32; 4],
        time: f32,
        regions: Option<&[[u32; 4]]>,
        origin: [f32; 2],
    ) -> Result<wgpu::CommandEncoder> {
        if self.layer_depth == 0 {
            self.frame_layer_bytes = 0;
            self.context_charged.fill(false);
            self.layer_used.clear();
            self.layer_used.resize(self.layer_pool.len(), false);
        }
        self.validate_frame(items.len(), size, background, time)?;
        let [width, height] = size;
        // If the persistent target was recreated, partial preservation is impossible.
        let regions = regions.filter(|_| self.target.as_ref().is_some_and(|t| t.size == size));
        let selected;
        let items = if let Some(regions) = regions.filter(|regions| !regions.is_empty()) {
            selected = items
                .iter()
                .filter(|draw| draw_intersects(draw, regions))
                .collect::<Vec<_>>();
            DrawSource::Borrowed(selected.as_slice())
        } else {
            items
        };
        let prepare_started = Instant::now();
        let mut instances = if regions.is_some_and(|r| r.is_empty()) {
            Vec::new()
        } else {
            self.prepare_instances(items, size)?
        };
        if let Some(regions) = regions {
            instances.retain(|instance| {
                let [x, y, w, h] = instance.rect;
                regions.iter().any(|&[rx, ry, rw, rh]| {
                    x - 1. < (rx + rw) as f32
                        && x + w + 1. > rx as f32
                        && y - 1. < (ry + rh) as f32
                        && y + h + 1. > ry as f32
                })
            });
        }
        self.frame_budget.prepare_ms += prepare_started.elapsed().as_secs_f64() * 1e3;
        let backdrop_plan = plan_backdrops(
            &instances,
            &self.backdrop_effects,
            origin,
            size,
            !self.force_separate_backdrops
                && crate::mipmap::MipGenerator::supported(&self.device)
                && self.device.limits().max_texture_array_layers >= 16
                && self.device.limits().max_compute_workgroups_per_dimension
                    >= self
                        .device
                        .limits()
                        .max_texture_dimension_2d
                        .div_ceil(16)
                        .max(16),
        );
        let mut backdrop_snapshots: Vec<Option<Vec<wgpu::TextureView>>> =
            (0..backdrop_plan.groups.len()).map(|_| None).collect();
        if instances
            .iter()
            .any(|instance| instance.config[2] == 4. && instance.config[1] == 1.)
        {
            self.ensure_plus_pipeline();
        }
        tracing::trace!(instances = instances.len(), "prepared ordered instances");
        if instances.len() > 1_000_000
            || (instances.len().max(1).next_power_of_two() * std::mem::size_of::<Instance>()) as u64
                > self.device.limits().max_buffer_size
        {
            return Err(Error::InvalidInput {
                reason: "instance stream exceeds GPU limits".into(),
            });
        }
        if instances.len() > self.capacity {
            self.capacity = instances.len().next_power_of_two();
            self.instances = Self::instance_buffer(&self.device, self.capacity);
        }
        let upload_started = Instant::now();
        if !instances.is_empty() {
            self.queue
                .write_buffer(&self.instances, 0, bytemuck::cast_slice(&instances));
        }
        self.queue.write_buffer(
            &self.frame,
            0,
            bytemuck::cast_slice(&[
                width as f32,
                height as f32,
                time,
                1.,
                background[0] * background[3],
                background[1] * background[3],
                background[2] * background[3],
                background[3],
                origin[0],
                origin[1],
                0.,
                0.,
            ]),
        );
        self.frame_budget.upload_ms += upload_started.elapsed().as_secs_f64() * 1e3;
        self.frame_budget.upload_bytes +=
            (instances.len() * std::mem::size_of::<Instance>()) as u64 + 48;
        self.target(size);
        let view = self
            .target
            .as_ref()
            .expect("target just created")
            .view
            .clone();
        let mut flush_draws: u64 = 0;
        let mut flush_plain_draws: u64 = 0;
        let encode_started = Instant::now();
        let mut encoder = self.device.create_command_encoder(&Default::default());
        self.begin_gpu_budget(&mut encoder);
        if let Some(base) = &self.inherited_backdrop {
            base.seed(
                &mut encoder,
                &self.target.as_ref().unwrap().texture,
                origin,
                size,
            );
        }
        self.batch_stats.instances += instances.len() as u64;
        self.batch_stats.passes += 1;
        let mut segment_start = 0;
        loop {
            let marker = (segment_start..instances.len()).find(|&i| instances[i].color[3] == 3.);
            let segment_end = marker.unwrap_or(instances.len());
            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("Plico quad draw"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &view,
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: if regions.is_some()
                                || segment_start > 0
                                || self.inherited_backdrop.is_some()
                            {
                                wgpu::LoadOp::Load
                            } else {
                                wgpu::LoadOp::Clear(wgpu::Color {
                                    r: f64::from(background[0] * background[3]),
                                    g: f64::from(background[1] * background[3]),
                                    b: f64::from(background[2] * background[3]),
                                    a: f64::from(background[3]),
                                })
                            },
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    ..Default::default()
                });
                let full = [[0, 0, width, height]];
                let areas = regions.unwrap_or(&full);
                let mut scissor = None;
                if regions.is_some() && segment_start == 0 {
                    // Disjoint damage regions can all be cleared before replay. Each
                    // batch then preserves painter order within every affected pixel.
                    pass.set_pipeline(&self.clear_pipeline);
                    pass.set_bind_group(0, &self.bind, &[]);
                    for &[x, y, w, h] in areas {
                        pass.set_scissor_rect(x, y, w, h);
                        scissor = Some([x, y, w, h]);
                        pass.draw(0..3, 0..1);
                        self.batch_stats.clears += 1;
                    }
                }
                pass.set_bind_group(0, &self.bind, &[]);
                pass.set_vertex_buffer(0, self.instances.slice(..));
                // Like MyGO drawBuilt: bind each ordered batch once, then draw only
                // the disjoint damage rectangles it can actually reach. No sorting.
                let mut first = segment_start;
                // mygo joinLast: scenes that interleave classes op by op - a
                // shadow then its fill per box, a label then its icon per row -
                // would otherwise draw a call per instance. Adjacent runs that
                // differ only in their class and each cover few pixels merge
                // into one run drawn through the general pipeline; plain
                // instances render identically there (tests/classes.rs), and at
                // mygo's measured 128x128-pixel trade-off the saved call costs
                // the CPU less than the specialized shader saves on those pixels.
                let mut scissor_run = scissor;
                let force_general = self.force_general;
                let mut pending: Option<(RunFlush, bool)> = None;
                while first < segment_end {
                    let key = |i: usize| {
                        (instances[i].config[2] == 4.).then_some(instances[i].config[3] as usize)
                    };
                    let texture = key(first);
                    // Class runs: adjacent plain fills share the specialized
                    // pipeline, everything else keeps the general shader. The
                    // run boundary never reorders painter sequence.
                    // Backdrop markers (color[3] == 2) are singleton runs: they
                    // never join a class run or the joiner.
                    let backdrop_at = (instances[first].color[3] == 2.)
                        .then(|| instances[first].config[3] as usize);
                    let plain =
                        backdrop_at.is_none() && !self.force_general && is_plain(&instances[first]);
                    let mut end = first + 1;
                    while backdrop_at.is_none()
                        && end < segment_end
                        && instances[end].color[3] != 2.
                        && key(end) == texture
                        && instances[end].clip_params[2] == instances[first].clip_params[2]
                        && (instances[end].config[2] == 4. && instances[end].config[1] == 1.)
                            == (instances[first].config[2] == 4.
                                && instances[first].config[1] == 1.)
                        && (!force_general && is_plain(&instances[end])) == plain
                    {
                        end += 1;
                    }
                    let plus = instances[first].config[2] == 4. && instances[first].config[1] == 1.;
                    let mask = instances[first].clip_params[2] as u32;
                    let small =
                        backdrop_at.is_none() && run_area(&instances, first, end) <= JOIN_AREA;
                    let run = RunFlush {
                        start: first,
                        end,
                        plain,
                        plus,
                        texture,
                        mask,
                    };
                    if let Some((previous, previous_small)) = &mut pending
                        && *previous_small
                        && small
                        && previous.texture == texture
                        && previous.mask == mask
                        && !previous.plus
                        && !plus
                        && backdrop_at.is_none()
                    {
                        // Only the class differs; extend the pending run. The
                        // merged run keeps the plain pipeline only if every
                        // merged part was plain.
                        previous.end = run.end;
                        previous.plain = previous.plain && run.plain;
                    } else {
                        if let Some((previous, _)) = pending.take() {
                            self.flush_run(
                                &mut pass,
                                previous,
                                &instances,
                                regions,
                                areas,
                                &mut scissor_run,
                                &mut flush_draws,
                                &mut flush_plain_draws,
                            );
                        }
                        if let Some(effect_index) = backdrop_at {
                            let marker = instances[run.start];
                            let [x, y, w, h] = marker.rect;
                            let [cx, cy, cw, ch] = marker.clip;
                            if x.max(cx).max(origin[0])
                                >= (x + w).min(cx + cw).min(origin[0] + width as f32)
                                || y.max(cy).max(origin[1])
                                    >= (y + h).min(cy + ch).min(origin[1] + height as f32)
                            {
                                first = end;
                                continue;
                            }
                            // Split the pass: everything before the effect is
                            // now in the target; copy that region out, render
                            // the effect with it bound, then continue the rest
                            // in a fresh pass (mygo's Backdrop step between
                            // batches).
                            drop(pass);

                            let shared =
                                backdrop_plan.members.get(run.start).copied().flatten().map(
                                    |(group, slot)| {
                                        if backdrop_snapshots[group].is_none() {
                                            let requests = &backdrop_plan.groups[group];
                                            let views = crate::backdrop_batch::capture(
                                                &self.device,
                                                &mut encoder,
                                                &self.target.as_ref().unwrap().texture,
                                                requests,
                                                &mut self.backdrop_batch_prefilter,
                                                &mut self.backdrop_pool,
                                            );
                                            let levels =
                                                requests.iter().map(|r| r.levels).max().unwrap();
                                            self.batch_stats.backdrop_copies +=
                                                requests.len() as u64;
                                            self.batch_stats.backdrop_textures += 1;
                                            self.batch_stats.backdrop_mip_passes +=
                                                u64::from(levels - 1);
                                            self.batch_stats.backdrop_bytes +=
                                                crate::backdrop_prefilter::bytes(
                                                    requests[0].region[2],
                                                    requests[0].region[3],
                                                    levels,
                                                ) * requests.len() as u64;
                                            backdrop_snapshots[group] = Some(views);
                                        }
                                        backdrop_snapshots[group].as_ref().unwrap()[slot].clone()
                                    },
                                );
                            let effect = self.backdrop_effects[effect_index].clone();
                            let mut effects = self.effects.take();
                            let mut compositor = self.backdrop_compositor.take();
                            pass = encode_backdrop_step(
                                &self.device,
                                &self.queue,
                                self.target.as_ref(),
                                &mut effects,
                                &mut compositor,
                                &mut self.backdrop_prefilter,
                                &mut self.backdrop_pool,
                                &mut self.batch_stats,
                                shared.as_ref(),
                                &self.atlas_texture.layout,
                                if run.mask > 0 {
                                    &self.frame_images[run.mask as usize - 1].bind
                                } else {
                                    &self.atlas_texture.bind
                                },
                                instances[run.start],
                                origin,
                                &mut encoder,
                                &view,
                                {
                                    let [x, y, w, h] = instances[run.start].rect;
                                    [x - origin[0], y - origin[1], w, h]
                                },
                                &effect,
                                size,
                            );
                            self.effects = effects;
                            self.backdrop_compositor = compositor;
                            self.batch_stats.passes += 1;
                            pass.set_bind_group(0, &self.bind, &[]);
                            pass.set_vertex_buffer(0, self.instances.slice(..));
                            scissor_run = None;
                            first = end;
                            continue;
                        }
                        pending = Some((run, small));
                    }
                    first = end;
                }
                if let Some((previous, _)) = pending.take() {
                    self.flush_run(
                        &mut pass,
                        previous,
                        &instances,
                        regions,
                        areas,
                        &mut scissor_run,
                        &mut flush_draws,
                        &mut flush_plain_draws,
                    );
                }
            }
            let Some(marker) = marker else {
                break;
            };
            let layer = self.deferred_layers[instances[marker].config[3] as usize].clone();
            let [x, y, w, h] = layer.quad.rect;
            let output = kurbo::Rect::new(x as f64, y as f64, (x + w) as f64, (y + h) as f64);
            let input = layer
                .scene
                .backdrop_bounds()
                .map_or(output, |rect| rect.union(output));
            let context = self.capture_backdrop_context(&mut encoder, origin, size, input)?;
            let previous = std::mem::replace(
                &mut encoder,
                self.device.create_command_encoder(&Default::default()),
            );
            self.pending_commands.push(previous.finish());

            let resource = self.render_layer(&layer, Some(context.clone()))?;
            let output_view = resource.texture.create_view(&Default::default());
            let base_view = context.texture.create_view(&Default::default());
            let mask = instances[marker].clip_params[2] as usize;
            let compositor = self.backdrop_compositor.get_or_insert_with(|| {
                crate::backdrop_composite::Compositor::new(&self.device, &self.atlas_texture.layout)
            });
            let i = instances[marker];
            let [x, y, w, h] = i.rect;
            let composite = [
                [x - origin[0], y - origin[1], w, h],
                i.clip,
                i.mask_rect,
                [origin[0], origin[1], f32::from(mask > 0), i.clip_params[0]],
            ];
            let mix = [
                layer.alpha,
                1.,
                origin[0] - context.origin[0],
                origin[1] - context.origin[1],
            ];
            let mut uniforms = None;
            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("atomic backdrop group"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &view,
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Load,
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    ..Default::default()
                });
                let full = [[0, 0, width, height]];
                for &[rx, ry, rw, rh] in regions.unwrap_or(&full) {
                    let x0 = (x - origin[0]).floor().max(rx as f32).max(0.) as u32;
                    let y0 = (y - origin[1]).floor().max(ry as f32).max(0.) as u32;
                    let x1 = (x + w - origin[0])
                        .ceil()
                        .min((rx + rw).min(width) as f32)
                        .max(0.) as u32;
                    let y1 = (y + h - origin[1])
                        .ceil()
                        .min((ry + rh).min(height) as f32)
                        .max(0.) as u32;
                    if x1 <= x0 || y1 <= y0 {
                        continue;
                    }
                    let uniforms = uniforms.get_or_insert_with(|| {
                        compositor.prepare_uniforms(
                            &self.device,
                            &self.queue,
                            [0.; 4],
                            composite,
                            mix,
                        )
                    });
                    pass.set_scissor_rect(x0, y0, x1 - x0, y1 - y0);
                    compositor.encode(
                        &self.device,
                        &output_view,
                        &base_view,
                        if mask > 0 {
                            &self.frame_images[mask - 1].bind
                        } else {
                            &self.atlas_texture.bind
                        },
                        uniforms,
                        &mut pass,
                    );
                    flush_draws += 1;
                }
            }
            self.batch_stats.passes += 1;
            segment_start = marker + 1;
        }
        self.batch_stats.draw_calls += flush_draws;
        self.batch_stats.plain_draw_calls += flush_plain_draws;
        self.frame_budget.instances += instances.len() as u64;
        self.frame_budget.draw_calls += flush_draws;
        self.frame_budget.encode_ms += encode_started.elapsed().as_secs_f64() * 1e3;
        // Keep this frame's grown capacity for the next frame's staging vector.
        // Error paths drop the buffer instead; the next frame simply reallocates.
        self.instance_scratch = instances;
        Ok(encoder)
    }
    #[allow(clippy::too_many_arguments)]
    fn flush_run(
        &self,
        pass: &mut wgpu::RenderPass<'_>,
        run: RunFlush,
        instances: &[Instance],
        regions: Option<&[[u32; 4]]>,
        areas: &[[u32; 4]],
        scissor_run: &mut Option<[u32; 4]>,
        draw_calls: &mut u64,
        plain_draw_calls: &mut u64,
    ) {
        // Resolve every self borrow before the pass ops so the caller's
        // encoder/pass lifetimes stay disjoint from self.
        #[cfg(test)]
        let cull_batches = !self.profile_no_batch_cull;
        #[cfg(not(test))]
        let cull_batches = true;
        let bounds = regions.filter(|_| cull_batches).and_then(|_| {
            instances[run.start..run.end].iter().try_fold(
                [
                    f32::INFINITY,
                    f32::INFINITY,
                    f32::NEG_INFINITY,
                    f32::NEG_INFINITY,
                ],
                |mut bounds, instance| {
                    if instance.config[2] == 1. {
                        return None;
                    }
                    let [x, y, w, h] = instance.rect;
                    bounds[0] = bounds[0].min(x - 1.);
                    bounds[1] = bounds[1].min(y - 1.);
                    bounds[2] = bounds[2].max(x + w + 1.);
                    bounds[3] = bounds[3].max(y + h + 1.);
                    Some(bounds)
                },
            )
        });
        let pipeline = if run.plus {
            self.plus_pipeline.as_ref().expect("plus pipeline prepared") as &wgpu::RenderPipeline
        } else if run.plain {
            &self.plain_pipeline
        } else {
            self.pipeline.as_ref().expect("material installed")
        };
        let atlas_bind = &self.atlas_texture.bind;
        let texture_bind = run
            .texture
            .map_or(atlas_bind, |slot| &self.frame_images[slot].bind);
        let mask_bind = if run.mask > 0 {
            &self.frame_images[run.mask as usize - 1].bind
        } else {
            atlas_bind
        };
        pass.set_pipeline(pipeline);
        pass.set_bind_group(1, texture_bind, &[]);
        pass.set_bind_group(2, mask_bind, &[]);
        for &[x, y, w, h] in areas {
            if bounds.is_some_and(|[left, top, right, bottom]| {
                right <= x as f32
                    || bottom <= y as f32
                    || left >= (x + w) as f32
                    || top >= (y + h) as f32
            }) {
                continue;
            }
            if *scissor_run != Some([x, y, w, h]) {
                pass.set_scissor_rect(x, y, w, h);
                *scissor_run = Some([x, y, w, h]);
            }
            pass.draw(0..6, run.start as u32..run.end as u32);
            *draw_calls += 1;
            if run.plain {
                *plain_draw_calls += 1;
            }
        }
    }
}
