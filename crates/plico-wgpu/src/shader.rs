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
const EFFECT_UNIFORM_BYTES: u64 = (UNIFORM_FLOAT_COUNT * std::mem::size_of::<f32>()) as u64;
const MAPPING_BYTES: u64 = 16;
const PIPELINE_CACHE_SIZE: usize = 16;
const EFFECT_TEXTURE_FORMAT: TextureFormat = TextureFormat::Rgba8Unorm;
const DIRECT_COMPOSITE_SUFFIX: &str = include_str!("direct_backdrop.wgsl");
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

struct DirectUniform {
    buffer: Buffer,
    effect_bind: BindGroup,
    composite_bind: BindGroup,
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
    direct_backdrop_pipelines: LruCache<Arc<str>, RenderPipeline>,
    direct_composite_layout: BindGroupLayout,
    direct_uniforms: Vec<DirectUniform>,
    direct_used: usize,
    direct_uniform_allocations: u64,
    direct_uniform_reuses: u64,
    direct_mapping_offset: u64,
    direct_composite_offset: u64,
    direct_uniform_bytes: u64,
    direct_upload: Vec<u8>,
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
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
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
        let direct_composite_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Plico direct backdrop composite layout"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
            });
        let alignment = u64::from(device.limits().min_uniform_buffer_offset_alignment).max(16);
        let direct_mapping_offset = EFFECT_UNIFORM_BYTES.next_multiple_of(alignment);
        let direct_composite_offset =
            (direct_mapping_offset + MAPPING_BYTES).next_multiple_of(alignment);
        let direct_uniform_bytes =
            direct_composite_offset + crate::backdrop_composite::COMPOSITE_BYTES;
        Self {
            layout,
            backdrop_layout,
            backdrop_sampler,
            pipelines: LruCache::new(NonZeroUsize::new(PIPELINE_CACHE_SIZE).unwrap()),
            direct_backdrop_pipelines: LruCache::new(
                NonZeroUsize::new(PIPELINE_CACHE_SIZE).unwrap(),
            ),
            direct_composite_layout,
            direct_uniforms: Vec::new(),
            direct_used: 0,
            direct_uniform_allocations: 0,
            direct_uniform_reuses: 0,
            direct_mapping_offset,
            direct_composite_offset,
            direct_uniform_bytes,
            direct_upload: vec![0; direct_uniform_bytes as usize],
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
        (
            count,
            bytes,
            self.pipelines.len() + self.direct_backdrop_pipelines.len(),
        )
    }
    pub fn begin_frame(&mut self) {
        self.frame = self.frame.wrapping_add(1).max(1);
        self.direct_used = 0;
    }

    pub fn direct_uniform_stats(&self) -> (usize, u64, u64) {
        (
            self.direct_uniforms.len(),
            self.direct_uniform_allocations,
            self.direct_uniform_reuses,
        )
    }

    pub fn finish_frame(&mut self) {
        let frame = self.frame;
        self.targets.retain(|_, targets| {
            targets.retain(|target| target.used_in_frame == frame);
            !targets.is_empty()
        });
        if self.direct_used == 0 {
            self.direct_uniforms.clear();
        }
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

    /// Evaluate a backdrop-reading effect and replace the covered framebuffer
    /// pixels in one pass. The copied backdrop remains a separate sampled
    /// texture, so this does not read from the active color attachment.
    #[allow(clippy::too_many_arguments)]
    pub fn render_backdrop_direct(
        &mut self,
        effect: &ShaderEffect,
        backdrop: &wgpu::TextureView,
        original_backdrop: &wgpu::TextureView,
        mapping: [f32; 4],
        composite: [[f32; 4]; 4],
        mix: [f32; 4],
        mask_layout: &wgpu::BindGroupLayout,
        mask: &wgpu::BindGroup,
        device: &Device,
        queue: &Queue,
        encoder: &mut CommandEncoder,
        frame_view: &wgpu::TextureView,
        frame_size: [u32; 2],
        scissor: [u32; 4],
    ) {
        let (uniforms, effect_bind, composite_bind) =
            self.direct_uniform(effect, mapping, composite, mix, device, queue);
        let pipeline = Self::direct_backdrop_pipeline(
            &mut self.direct_backdrop_pipelines,
            &self.layout,
            &self.backdrop_layout,
            &self.direct_composite_layout,
            mask_layout,
            device,
            effect.source(),
        );
        let backdrop_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Plico direct backdrop bind"),
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
                        buffer: &uniforms,
                        offset: self.direct_mapping_offset,
                        size: std::num::NonZeroU64::new(MAPPING_BYTES),
                    }),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(original_backdrop),
                },
            ],
        });
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Plico direct backdrop effect"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: frame_view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            ..Default::default()
        });
        pass.set_scissor_rect(scissor[0], scissor[1], scissor[2], scissor[3]);
        pass.set_viewport(0., 0., frame_size[0] as f32, frame_size[1] as f32, 0., 1.);
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &effect_bind, &[]);
        pass.set_bind_group(1, &backdrop_bind, &[]);
        pass.set_bind_group(2, &composite_bind, &[]);
        pass.set_bind_group(3, mask, &[]);
        pass.draw(0..3, 0..1);
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

    fn direct_uniform(
        &mut self,
        effect: &ShaderEffect,
        mapping: [f32; 4],
        composite: [[f32; 4]; 4],
        mix: [f32; 4],
        device: &Device,
        queue: &Queue,
    ) -> (Buffer, BindGroup, BindGroup) {
        if self.direct_used == self.direct_uniforms.len() {
            self.direct_uniform_allocations += 1;
            let buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("Plico direct shader uniforms"),
                size: self.direct_uniform_bytes,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            let effect_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("Plico direct shader uniform bind"),
                layout: &self.layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: &buffer,
                        offset: 0,
                        size: std::num::NonZeroU64::new(EFFECT_UNIFORM_BYTES),
                    }),
                }],
            });
            let composite_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("Plico direct shader composite bind"),
                layout: &self.direct_composite_layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: &buffer,
                        offset: self.direct_composite_offset,
                        size: std::num::NonZeroU64::new(crate::backdrop_composite::COMPOSITE_BYTES),
                    }),
                }],
            });
            self.direct_uniforms.push(DirectUniform {
                buffer,
                effect_bind,
                composite_bind,
            });
        } else {
            self.direct_uniform_reuses += 1;
        }
        let target = &self.direct_uniforms[self.direct_used];
        let buffer = target.buffer.clone();
        let effect_bind = target.effect_bind.clone();
        let composite_bind = target.composite_bind.clone();
        self.direct_used += 1;
        let effect_words = effect.uniform_words();
        let effect_bytes = bytemuck::cast_slice(&effect_words);
        self.direct_upload[..effect_bytes.len()].copy_from_slice(effect_bytes);
        self.direct_upload[(4 + 21) * 4..(4 + 22) * 4].copy_from_slice(bytemuck::bytes_of(&mix[1]));
        let mapping_offset = self.direct_mapping_offset as usize;
        self.direct_upload[mapping_offset..mapping_offset + MAPPING_BYTES as usize]
            .copy_from_slice(bytemuck::bytes_of(&mapping));
        let composite_offset = self.direct_composite_offset as usize;
        self.direct_upload[composite_offset
            ..composite_offset + crate::backdrop_composite::COMPOSITE_BYTES as usize]
            .copy_from_slice(bytemuck::cast_slice(&[
                composite[0],
                composite[1],
                composite[2],
                composite[3],
                mix,
            ]));
        queue.write_buffer(&buffer, 0, &self.direct_upload);
        (buffer, effect_bind, composite_bind)
    }

    fn direct_backdrop_pipeline(
        cache: &mut LruCache<Arc<str>, RenderPipeline>,
        group_layout: &BindGroupLayout,
        backdrop_layout: &BindGroupLayout,
        composite_layout: &BindGroupLayout,
        mask_layout: &BindGroupLayout,
        device: &Device,
        source: &ShaderEffectSource,
    ) -> RenderPipeline {
        if let Some(pipeline) = cache.get(source.wgsl()) {
            return pipeline.clone();
        }
        let source_with_composite = format!("{}\n{}", source.wgsl(), DIRECT_COMPOSITE_SUFFIX);
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Plico direct backdrop shader"),
            source: wgpu::ShaderSource::Wgsl(source_with_composite.into()),
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Plico direct backdrop pipeline layout"),
            bind_group_layouts: &[
                Some(group_layout),
                Some(backdrop_layout),
                Some(composite_layout),
                Some(mask_layout),
            ],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Plico direct backdrop pipeline"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_direct_composite"),
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
