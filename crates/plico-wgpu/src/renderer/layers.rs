use super::*;

impl Renderer {
    fn new_layer_buffers(&self) -> LayerBuffers {
        let frame = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Plico layer frame"),
            size: 48,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bind = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Plico layer frame bind"),
            layout: &self.frame_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: frame.as_entire_binding(),
            }],
        });
        LayerBuffers {
            frame,
            bind,
            instances: Self::instance_buffer(&self.device, INITIAL_INSTANCE_CAPACITY),
            capacity: INITIAL_INSTANCE_CAPACITY,
        }
    }

    /// Intermediate color-texture pixel payload requested by isolated layers
    /// in the most recently encoded frame (excluding driver overhead/in-flight frames).
    pub fn layer_frame_bytes(&self) -> u64 {
        self.frame_layer_bytes
    }
    pub fn layer_pool_stats(&self) -> LayerPoolStats {
        let mut stats = LayerPoolStats {
            allocations: self.layer_allocations,
            reuses: self.layer_reuses,
            filter_allocations: self.filter_allocations,
            filter_reuses: self.filter_reuses,
            ..Default::default()
        };
        for entry in self.layer_pool.iter().flatten() {
            stats.entries += 1;
            stats.retained_bytes += entry.bytes();
        }
        for context in self.backdrop_contexts.iter().flatten() {
            stats.backdrop_context_entries += 1;
            stats.backdrop_context_bytes +=
                u64::from(context.size[0]) * u64::from(context.size[1]) * 4;
        }
        let (entries, bytes, allocations, reuses) = self.backdrop_pool.stats();
        stats.backdrop_entries = entries;
        stats.backdrop_bytes = bytes;
        stats.backdrop_allocations = allocations;
        stats.backdrop_reuses = reuses;
        if let Some(compositor) = &self.backdrop_compositor {
            let (entries, allocations, reuses) = compositor.uniform_stats();
            stats.backdrop_uniform_entries = entries;
            stats.backdrop_uniform_allocations = allocations;
            stats.backdrop_uniform_reuses = reuses;
        }
        if let Some(effects) = &self.effects {
            let (entries, allocations, reuses) = effects.direct_uniform_stats();
            stats.backdrop_effect_uniform_entries = entries;
            stats.backdrop_effect_uniform_allocations = allocations;
            stats.backdrop_effect_uniform_reuses = reuses;
        }
        stats
    }
    /// Release idle intermediate targets without invalidating retained final pixels.
    pub fn trim_layer_pool(&mut self) {
        self.backdrop_contexts.clear();
        self.context_charged.clear();
        self.backdrop_pool.clear();
        if let Some(compositor) = &mut self.backdrop_compositor {
            compositor.clear_uniforms();
        }
        self.layer_pool.clear();
        self.layer_used.clear();
        self.clear_frame_images();
    }
    pub(super) fn trim_idle_layers(&mut self, keep: bool) {
        if !keep {
            self.layer_pool.clear();
            self.layer_used.clear();
            return;
        }
        let mut stats = self.layer_pool_stats();
        while stats.retained_bytes > 128 * 1024 * 1024 || stats.entries > 128 {
            let candidate = self
                .layer_pool
                .iter()
                .enumerate()
                .position(|(index, entry)| entry.is_some() && !self.layer_used[index])
                .or_else(|| self.layer_pool.iter().position(|entry| entry.is_some()));
            let Some(index) = candidate else { break };
            let entry = self.layer_pool[index].take().unwrap();
            stats.retained_bytes -= entry.bytes();
            stats.entries -= 1;
        }
        while self.layer_pool.last().is_some_and(|entry| entry.is_none()) {
            self.layer_pool.pop();
        }
    }
    pub(super) fn capture_backdrop_context(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        origin: [f32; 2],
        size: [u32; 2],
        input: kurbo::Rect,
    ) -> Result<crate::backdrop_context::Context> {
        let input = input.expand().intersect(kurbo::Rect::new(
            origin[0] as f64,
            origin[1] as f64,
            origin[0] as f64 + size[0] as f64,
            origin[1] as f64 + size[1] as f64,
        ));
        let source_offset = wgpu::Origin3d {
            x: (input.x0 - origin[0] as f64).max(0.) as u32,
            y: (input.y0 - origin[1] as f64).max(0.) as u32,
            z: 0,
        };
        let origin = [input.x0 as f32, input.y0 as f32];
        let size = [
            (input.width() as u32).max(1),
            (input.height() as u32).max(1),
        ];
        let depth = self.layer_depth as usize;
        self.backdrop_contexts
            .resize_with(self.backdrop_contexts.len().max(depth + 1), || None);
        self.context_charged
            .resize(self.context_charged.len().max(depth + 1), false);
        let replace = self.backdrop_contexts[depth]
            .as_ref()
            .is_none_or(|context| context.size != size);
        if replace || !self.context_charged[depth] {
            let bytes = u64::from(size[0]) * u64::from(size[1]) * 4;
            if self.frame_layer_bytes + bytes > 128 * 1024 * 1024 {
                return Err(Error::InvalidInput {
                    reason: "group backdrop context exceeds 128 MiB frame budget".into(),
                });
            }
            self.frame_layer_bytes += bytes;
            self.context_charged[depth] = true;
        }
        if replace {
            self.backdrop_contexts[depth] = Some(crate::backdrop_context::Context::new(
                &self.device,
                size,
                origin,
            ));
        }
        let mut output = self.backdrop_contexts[depth].as_ref().unwrap().clone();
        output.origin = origin;
        encoder.copy_texture_to_texture(
            wgpu::TexelCopyTextureInfo {
                origin: source_offset,
                ..self.target.as_ref().unwrap().texture.as_image_copy()
            },
            output.texture.as_image_copy(),
            wgpu::Extent3d {
                width: size[0],
                height: size[1],
                depth_or_array_layers: 1,
            },
        );

        Ok(output)
    }

    pub(super) fn render_layer(
        &mut self,
        layer: &crate::scene::PreparedLayer,
        inherited: Option<crate::backdrop_context::Context>,
        encoder: &mut wgpu::CommandEncoder,
    ) -> Result<LayerRaster> {
        let [x, y, width, height] = layer.quad.rect;
        let size = [width as u32, height as u32];
        let _span = tracing::trace_span!(
            "plico_wgpu.isolated_layer",
            width = size[0],
            height = size[1]
        )
        .entered();
        let bytes = u64::from(size[0])
            * u64::from(size[1])
            * if layer.blur.is_some_and(|b| b.sigma > 0.) {
                12
            } else {
                4
            };
        if self.layer_depth >= 16 {
            return Err(Error::InvalidInput {
                reason: format!(
                    "isolated layer depth {} exceeds limit 16 ({}x{} target)",
                    self.layer_depth + 1,
                    size[0],
                    size[1]
                ),
            });
        }
        if self.frame_layer_bytes + bytes > 128 * 1024 * 1024 {
            return Err(Error::InvalidInput {
                reason: format!(
                    "isolated layers exceed 128 MiB frame budget: {} bytes used, {} requested for {}x{} at depth {}",
                    self.frame_layer_bytes,
                    bytes,
                    size[0],
                    size[1],
                    self.layer_depth + 1
                ),
            });
        }
        self.frame_layer_bytes += bytes;
        self.layer_depth += 1;
        // A texture used earlier in this frame may still be sampled by its
        // parent. Only entries from prior frames are eligible for reuse.
        let slot = self
            .layer_pool
            .iter()
            .enumerate()
            .position(|(index, entry)| {
                !self.layer_used[index]
                    && entry
                        .as_ref()
                        .is_some_and(|entry| entry.target.size == size)
            })
            .or_else(|| {
                self.layer_pool
                    .iter()
                    .enumerate()
                    .position(|(index, entry)| !self.layer_used[index] && entry.is_none())
            })
            .unwrap_or_else(|| {
                self.layer_pool.push(None);
                self.layer_used.push(false);
                self.layer_pool.len() - 1
            });
        self.layer_used[slot] = true;
        let saved = self.layer_pool[slot].take();
        let parent_target = self.target.take();
        let (cached_image, mut workspace, child_buffers) = if let Some(entry) = saved {
            self.target = Some(entry.target);
            (Some(entry.image), entry.blur, entry.buffers)
        } else {
            (None, None, self.new_layer_buffers())
        };
        // Each child target owns its instance and frame buffers. This lets the
        // parent's already-recorded commands and the child commands coexist in
        // one queue submission without later write_buffer calls changing what
        // an earlier command buffer observes.
        let parent_buffers = LayerBuffers {
            frame: std::mem::replace(&mut self.frame, child_buffers.frame),
            bind: std::mem::replace(&mut self.bind, child_buffers.bind),
            instances: std::mem::replace(&mut self.instances, child_buffers.instances),
            capacity: std::mem::replace(&mut self.capacity, child_buffers.capacity),
        };
        let parent_images = std::mem::take(&mut self.frame_images);
        let parent_slots = std::mem::take(&mut self.frame_image_slots);
        // encode_items clears/rebuilds the marker table for each render target.
        // Child layers must not discard the parent's pending backdrop markers.
        let parent_backdrops = std::mem::take(&mut self.backdrop_effects);
        let parent_layers = std::mem::take(&mut self.deferred_layers);
        let parent_context = std::mem::replace(&mut self.inherited_backdrop, inherited);
        let encoded = self.encode_frame_into(
            DrawSource::Prepared(&layer.scene),
            size,
            [0.; 4],
            0.,
            None,
            [x, y],
            encoder,
        );
        let mut resource = cached_image;
        let result = encoded.map(|()| {
            let image = if let Some(image) = &resource {
                self.layer_reuses += 1;
                image.clone()
            } else {
                self.layer_allocations += 1;
                let image = crate::image::GpuImage::from_layer(
                    &self.device,
                    &self.atlas_texture.layout,
                    &self.target.as_ref().expect("encoded layer target").texture,
                );
                resource = Some(image.clone());
                image
            };
            let (image, texture) = if let Some(blur) = layer.blur.filter(|b| b.sigma > 0.) {
                let pipeline = self
                    .blur
                    .get_or_insert_with(|| crate::blur::Blur::new(&self.device));
                if workspace.is_none() {
                    let work = pipeline.workspace(
                        &self.device,
                        &self.target.as_ref().unwrap().texture,
                        size,
                    );
                    let image = crate::image::GpuImage::from_layer(
                        &self.device,
                        &self.atlas_texture.layout,
                        &work.output,
                    );
                    workspace = Some((work, image));
                    self.filter_allocations += 1;
                } else {
                    self.filter_reuses += 1;
                }
                let (work, image) = workspace.as_ref().unwrap();
                pipeline.encode(&self.queue, encoder, work, blur.sigma, blur.edge);
                (image.clone(), work.output.clone())
            } else {
                (image, self.target.as_ref().unwrap().texture.clone())
            };
            LayerRaster { image, texture }
        });
        let child_buffers = LayerBuffers {
            frame: std::mem::replace(&mut self.frame, parent_buffers.frame),
            bind: std::mem::replace(&mut self.bind, parent_buffers.bind),
            instances: std::mem::replace(&mut self.instances, parent_buffers.instances),
            capacity: std::mem::replace(&mut self.capacity, parent_buffers.capacity),
        };
        if let (Some(target), Some(image)) = (self.target.take(), resource) {
            self.layer_pool[slot] = Some(LayerTarget {
                target,
                image,
                blur: workspace,
                buffers: child_buffers,
            });
        }
        self.target = parent_target;
        self.frame_images = parent_images;
        self.frame_image_slots = parent_slots;
        self.backdrop_effects = parent_backdrops;
        self.deferred_layers = parent_layers;
        self.inherited_backdrop = parent_context;
        self.layer_depth -= 1;
        result
    }
}
