//! Wgpu execution backend for renderer-neutral custom shader effects.

use std::{num::NonZeroUsize, sync::Arc};

use lru::LruCache;
use plico_renderer::{ShaderEffect, ShaderEffectId, ShaderEffectSource};
use rustc_hash::FxHashMap;
use wgpu::{
    BindGroup, BindGroupLayout, Buffer, CommandEncoder, Device, Queue, RenderPipeline, Texture,
    TextureFormat, TextureView,
};

const UNIFORM_FLOAT_COUNT: usize = 260;
const PIPELINE_CACHE_SIZE: usize = 16;
const EFFECT_TEXTURE_FORMAT: TextureFormat = TextureFormat::Rgba8Unorm;
struct EffectTarget {
    _texture: Texture,
    view: TextureView,
    uniforms: Buffer,
    bind_group: BindGroup,
    size: [u16; 2],
    used_in_frame: u64,
    rendered_in_frame: u64,
    effect: Option<ShaderEffect>,
}

impl EffectTarget {
    fn new(device: &Device, layout: &BindGroupLayout, size: [u16; 2]) -> Self {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Plico custom shader target"),
            size: wgpu::Extent3d {
                width: u32::from(size[0]),
                height: u32::from(size[1]),
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: EFFECT_TEXTURE_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let uniforms = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Plico custom shader uniforms"),
            size: (UNIFORM_FLOAT_COUNT * std::mem::size_of::<f32>()) as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Plico custom shader bind group"),
            layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniforms.as_entire_binding(),
            }],
        });
        Self {
            _texture: texture,
            view,
            uniforms,
            bind_group,
            size,
            used_in_frame: 0,
            rendered_in_frame: 0,
            effect: None,
        }
    }
}

#[doc(hidden)]
#[allow(missing_docs)]
pub struct ShaderRenderer {
    layout: BindGroupLayout,
    backdrop_layout: BindGroupLayout,
    backdrop_sampler: wgpu::Sampler,
    pipelines: LruCache<Arc<str>, RenderPipeline>,
    backdrop_pipelines: LruCache<Arc<str>, RenderPipeline>,
    targets: FxHashMap<ShaderEffectId, Vec<EffectTarget>>,
    frame: u64,
}

#[allow(missing_docs)]
impl ShaderRenderer {
    pub fn new(device: &Device) -> Self {
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Plico custom shader layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        // Backdrop readers: the sampled frame region as a texture+sampler,
        // matching the prelude's group-1 bindings.
        let backdrop_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Plico shader backdrop layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });
        let backdrop_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Plico shader backdrop sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            ..Default::default()
        });
        Self {
            layout,
            backdrop_layout,
            backdrop_sampler,
            pipelines: LruCache::new(NonZeroUsize::new(PIPELINE_CACHE_SIZE).unwrap()),
            backdrop_pipelines: LruCache::new(NonZeroUsize::new(PIPELINE_CACHE_SIZE).unwrap()),
            targets: FxHashMap::default(),
            frame: 0,
        }
    }

    /// Live output targets, output pixel payload and cached pipelines.
    pub fn resource_stats(&self) -> (usize, u64, usize) {
        let mut count = 0;
        let mut bytes = 0;
        for targets in self.targets.values() {
            for target in targets {
                count += 1;
                bytes += u64::from(target.size[0]) * u64::from(target.size[1]) * 4;
            }
        }
        (count, bytes, self.pipelines.len())
    }
    pub fn begin_frame(&mut self) {
        self.frame = self.frame.wrapping_add(1).max(1);
    }

    pub fn finish_frame(&mut self) {
        let frame = self.frame;
        self.targets.retain(|_, targets| {
            targets.retain(|target| target.used_in_frame == frame);
            !targets.is_empty()
        });
    }

    /// Render to a native wgpu view without a Vello texture-binding adapter.
    pub fn render_view(
        &mut self,
        effect: &ShaderEffect,
        device: &Device,
        queue: &Queue,
        encoder: &mut CommandEncoder,
    ) -> TextureView {
        self.render_target(effect, device, queue, encoder)
    }

    /// Sample a cropped backdrop through an effect-UV to texture-UV mapping.
    #[allow(clippy::too_many_arguments)]
    pub fn render_view_with_backdrop_mapping(
        &mut self,
        effect: &ShaderEffect,
        backdrop: &wgpu::TextureView,
        uniforms: &wgpu::Buffer,
        device: &Device,
        queue: &Queue,
        encoder: &mut CommandEncoder,
    ) -> TextureView {
        let (view, bind_group) = self.render_target_full(effect, device, queue, encoder);
        let pipeline = Self::backdrop_pipeline(
            &mut self.backdrop_pipelines,
            &self.layout,
            &self.backdrop_layout,
            device,
            effect.source(),
        );
        let backdrop_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Plico shader backdrop bind"),
            layout: &self.backdrop_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(backdrop),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.backdrop_sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: uniforms,
                        offset: 0,
                        size: std::num::NonZeroU64::new(16),
                    }),
                },
            ],
        });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Plico backdrop shader pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&pipeline);
            pass.set_bind_group(0, &bind_group, &[]);
            pass.set_bind_group(1, &backdrop_bind, &[]);
            pass.draw(0..3, 0..1);
        }
        view
    }

    /// Keep an unchanged surface alive when a damage consumer skips its draw.
    pub fn retain_effect(&mut self, effect: &ShaderEffect) {
        if let Some(targets) = self.targets.get_mut(&effect.id()) {
            // Reserve one matching output per live occurrence. Marking every
            // historical duplicate would keep the whole allocation history alive.
            if let Some(target) = targets.iter_mut().find(|target| {
                target.used_in_frame != self.frame && target.effect.as_ref() == Some(effect)
            }) {
                target.used_in_frame = self.frame;
            }
        }
    }

    fn render_target(
        &mut self,
        effect: &ShaderEffect,
        device: &Device,
        queue: &Queue,
        encoder: &mut CommandEncoder,
    ) -> TextureView {
        // A retained scene snapshot may contain several versions of one surface
        // (for example before/after a theme reveal). Distinct versions in the
        // same submission must never share a texture or uniform buffer.
        let targets = self.targets.entry(effect.id()).or_default();
        let index = targets
            .iter()
            .position(|target| {
                target.used_in_frame == self.frame && target.effect.as_ref() == Some(effect)
            })
            .or_else(|| {
                targets
                    .iter()
                    .position(|target| target.used_in_frame != self.frame)
            })
            .unwrap_or_else(|| {
                targets.push(EffectTarget::new(
                    device,
                    &self.layout,
                    effect.physical_size(),
                ));
                targets.len() - 1
            });
        let target = &mut targets[index];
        if target.size != effect.physical_size() {
            *target = EffectTarget::new(device, &self.layout, effect.physical_size());
        }
        target.used_in_frame = self.frame;
        target.rendered_in_frame = self.frame;
        if target.effect.as_ref() == Some(effect) && !effect.reads_backdrop() {
            return target.view.clone();
        }
        let pipeline = Self::pipeline(&mut self.pipelines, &self.layout, device, effect.source());
        let uniforms = effect.uniform_words();
        queue.write_buffer(&target.uniforms, 0, bytemuck::cast_slice(&uniforms));

        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Plico custom shader pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &target.view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&pipeline);
            pass.set_bind_group(0, &target.bind_group, &[]);
            pass.draw(0..3, 0..1);
        }

        target.effect = Some(effect.clone());
        target.view.clone()
    }

    /// Allocation-only half of render_target for backdrop readers: same
    /// target selection and uniform upload, without the non-backdrop pass
    /// (the caller draws its own). Backdrop content differs every frame, so
    /// equality never skips it.
    fn render_target_full(
        &mut self,
        effect: &ShaderEffect,
        device: &Device,
        queue: &Queue,
        _encoder: &mut CommandEncoder,
    ) -> (TextureView, BindGroup) {
        let targets = self.targets.entry(effect.id()).or_default();
        let index = targets
            .iter()
            // A preflight reservation is reusable by its actual draw. Only
            // another draw already encoded this frame makes an output busy.
            .position(|target| {
                target.rendered_in_frame != self.frame && target.effect.as_ref() == Some(effect)
            })
            .or_else(|| {
                targets
                    .iter()
                    .position(|target| target.used_in_frame != self.frame)
            })
            .unwrap_or_else(|| {
                targets.push(EffectTarget::new(
                    device,
                    &self.layout,
                    effect.physical_size(),
                ));
                targets.len() - 1
            });
        let target = &mut targets[index];
        if target.size != effect.physical_size() {
            *target = EffectTarget::new(device, &self.layout, effect.physical_size());
        }
        target.used_in_frame = self.frame;
        target.rendered_in_frame = self.frame;
        let uniforms = effect.uniform_words();
        queue.write_buffer(&target.uniforms, 0, bytemuck::cast_slice(&uniforms));
        let bind = target.bind_group.clone();
        target.effect = Some(effect.clone());
        (target.view.clone(), bind)
    }

    /// Pipeline for a backdrop-reading effect: group 1 binds the sampled
    /// frame region in addition to the uniform group.
    fn backdrop_pipeline(
        cache: &mut LruCache<Arc<str>, RenderPipeline>,
        group_layout: &BindGroupLayout,
        backdrop_layout: &BindGroupLayout,
        device: &Device,
        source: &ShaderEffectSource,
    ) -> RenderPipeline {
        if let Some(pipeline) = cache.get(source.wgsl()) {
            return pipeline.clone();
        }
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Plico backdrop shader"),
            source: wgpu::ShaderSource::Wgsl(source.wgsl().as_ref().into()),
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Plico backdrop shader pipeline layout"),
            bind_group_layouts: &[Some(group_layout), Some(backdrop_layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Plico backdrop shader pipeline"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: EFFECT_TEXTURE_FORMAT,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });
        cache.put(source.wgsl().to_owned(), pipeline.clone());
        pipeline
    }

    fn pipeline(
        cache: &mut LruCache<Arc<str>, RenderPipeline>,
        group_layout: &BindGroupLayout,
        device: &Device,
        source: &ShaderEffectSource,
    ) -> RenderPipeline {
        if let Some(pipeline) = cache.get(source.wgsl()) {
            return pipeline.clone();
        }
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Plico custom shader"),
            source: wgpu::ShaderSource::Wgsl(source.wgsl().as_ref().into()),
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Plico custom shader pipeline layout"),
            bind_group_layouts: &[Some(group_layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Plico custom shader pipeline"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: EFFECT_TEXTURE_FORMAT,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });
        cache.put(source.wgsl().clone(), pipeline.clone());
        pipeline
    }
}
