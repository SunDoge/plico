//! Separable premultiplied Gaussian image filtering, entirely on the GPU.
/// The compute pipeline, or the render-pass pipeline on backends without
/// compute support (WebGL2): the same convolution drawn by a fullscreen
/// triangle into a render attachment.
enum Pass {
    Compute(wgpu::ComputePipeline),
    Render(wgpu::RenderPipeline),
}
pub(crate) struct Blur {
    pass: Pass,
    #[cfg(test)]
    tiled: bool,
}
pub(crate) struct Workspace {
    temporary: wgpu::Texture,
    pub output: wgpu::Texture,
    uniforms: [wgpu::Buffer; 2],
    binds: [wgpu::BindGroup; 2],
    /// Destination views per axis; render passes need them as attachments,
    /// compute passes already address them through the bind group.
    destinations: [wgpu::TextureView; 2],
    pub timing: Option<Timing>,
}
pub(crate) struct Timing {
    queries: wgpu::QuerySet,
    resolve: wgpu::Buffer,
    pub readback: wgpu::Buffer,
}
impl Blur {
    pub fn new(device: &wgpu::Device) -> Self {
        Self::with_source(device, include_str!("blur.wgsl"), false)
    }
    #[cfg(test)]
    pub fn tiled(device: &wgpu::Device) -> Self {
        Self::with_source(
            device,
            include_str!("../tests/assets/blur-tiled.wgsl"),
            true,
        )
    }
    fn with_source(device: &wgpu::Device, source: &str, _tiled: bool) -> Self {
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Plico Gaussian blur"),
            source: wgpu::ShaderSource::Wgsl(source.into()),
        });
        let pass = if crate::mipmap::MipGenerator::supported(device) {
            Pass::Compute(
                device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                    label: Some("Plico separable blur"),
                    layout: None,
                    module: &module,
                    entry_point: Some("main"),
                    compilation_options: Default::default(),
                    cache: None,
                }),
            )
        } else {
            let render = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("Plico Gaussian blur pass"),
                source: wgpu::ShaderSource::Wgsl(include_str!("blur_pass.wgsl").into()),
            });
            let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Plico blur pass layout"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: false },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: wgpu::BufferSize::new(16),
                        },
                        count: None,
                    },
                ],
            });
            let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Plico blur pass pipeline"),
                bind_group_layouts: &[Some(&layout)],
                immediate_size: 0,
            });
            Pass::Render(
                device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                    label: Some("Plico Gaussian blur pass"),
                    layout: Some(&layout),
                    vertex: wgpu::VertexState {
                        module: &render,
                        entry_point: Some("vs"),
                        compilation_options: Default::default(),
                        buffers: &[],
                    },
                    primitive: Default::default(),
                    depth_stencil: None,
                    multisample: Default::default(),
                    fragment: Some(wgpu::FragmentState {
                        module: &render,
                        entry_point: Some("fs"),
                        compilation_options: Default::default(),
                        targets: &[Some(wgpu::ColorTargetState {
                            format: wgpu::TextureFormat::Rgba8Unorm,
                            blend: None,
                            write_mask: wgpu::ColorWrites::ALL,
                        })],
                    }),
                    multiview_mask: None,
                    cache: None,
                }),
            )
        };
        Self {
            #[cfg(test)]
            tiled: _tiled,
            pass,
        }
    }
    pub fn workspace(
        &self,
        device: &wgpu::Device,
        source: &wgpu::Texture,
        size: [u32; 2],
    ) -> Workspace {
        let storage = matches!(self.pass, Pass::Compute(_));
        let usage = wgpu::TextureUsages::TEXTURE_BINDING
            | if storage {
                wgpu::TextureUsages::STORAGE_BINDING
            } else {
                wgpu::TextureUsages::RENDER_ATTACHMENT
            };
        let allocate = || {
            device.create_texture(&wgpu::TextureDescriptor {
                label: Some("Plico pooled blur intermediate"),
                size: wgpu::Extent3d {
                    width: size[0],
                    height: size[1],
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage,
                view_formats: &[],
            })
        };
        let temporary = allocate();
        let output = allocate();
        let uniforms = std::array::from_fn(|_| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("Plico pooled blur parameters"),
                size: 16,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            })
        });
        let mut destinations = [
            temporary.create_view(&Default::default()),
            output.create_view(&Default::default()),
        ];
        let binds = std::array::from_fn(|axis| {
            let (input, destination) = if axis == 0 {
                (source, &temporary)
            } else {
                (&temporary, &output)
            };
            let input = input.create_view(&Default::default());
            let destination = destination.create_view(&Default::default());
            destinations[axis] = destination.clone();
            match &self.pass {
                Pass::Compute(pipeline) => device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("Plico pooled blur axis"),
                    layout: &pipeline.get_bind_group_layout(0),
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::TextureView(&input),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::TextureView(&destination),
                        },
                        wgpu::BindGroupEntry {
                            binding: 2,
                            resource: uniforms[axis].as_entire_binding(),
                        },
                    ],
                }),
                Pass::Render(pipeline) => device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("Plico pooled blur axis"),
                    layout: &pipeline.get_bind_group_layout(0),
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::TextureView(&input),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: uniforms[axis].as_entire_binding(),
                        },
                    ],
                }),
            }
        });
        let timing = device
            .features()
            .contains(wgpu::Features::TIMESTAMP_QUERY)
            .then(|| Timing {
                queries: device.create_query_set(&wgpu::QuerySetDescriptor {
                    label: Some("Plico blur timestamps"),
                    ty: wgpu::QueryType::Timestamp,
                    count: 4,
                }),
                resolve: device.create_buffer(&wgpu::BufferDescriptor {
                    label: None,
                    size: 32,
                    usage: wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC,
                    mapped_at_creation: false,
                }),
                readback: device.create_buffer(&wgpu::BufferDescriptor {
                    label: None,
                    size: 32,
                    usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                }),
            });
        Workspace {
            temporary,
            output,
            uniforms,
            binds,
            destinations,
            timing,
        }
    }
    pub fn encode(
        &self,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        workspace: &Workspace,
        sigma: f32,
        edge: plico_renderer::filter::BlurEdge,
    ) {
        let size = workspace.temporary.size();
        for axis in 0..2 {
            queue.write_buffer(
                &workspace.uniforms[axis],
                0,
                bytemuck::cast_slice(&[sigma.to_bits(), axis as u32, edge as u32, 0]),
            );
            match &self.pass {
                Pass::Compute(pipeline) => {
                    let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                        label: Some("Plico Gaussian axis"),
                        timestamp_writes: workspace.timing.as_ref().map(|t| {
                            wgpu::ComputePassTimestampWrites {
                                query_set: &t.queries,
                                beginning_of_pass_write_index: Some(axis as u32 * 2),
                                end_of_pass_write_index: Some(axis as u32 * 2 + 1),
                            }
                        }),
                    });
                    pass.set_pipeline(pipeline);
                    pass.set_bind_group(0, &workspace.binds[axis], &[]);
                    #[cfg(test)]
                    if self.tiled {
                        if axis == 0 {
                            pass.dispatch_workgroups(size.width.div_ceil(32), size.height, 1);
                        } else {
                            pass.dispatch_workgroups(size.width, size.height.div_ceil(32), 1);
                        }
                        continue;
                    }
                    pass.dispatch_workgroups(size.width.div_ceil(8), size.height.div_ceil(8), 1);
                }
                Pass::Render(pipeline) => {
                    let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: Some("Plico Gaussian axis"),
                        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                            view: &workspace.destinations[axis],
                            resolve_target: None,
                            depth_slice: None,
                            ops: wgpu::Operations {
                                load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                                store: wgpu::StoreOp::Store,
                            },
                        })],
                        ..Default::default()
                    });
                    pass.set_pipeline(pipeline);
                    pass.set_bind_group(0, &workspace.binds[axis], &[]);
                    pass.draw(0..3, 0..1);
                }
            }
        }
        if let Some(timing) = &workspace.timing {
            encoder.resolve_query_set(&timing.queries, 0..4, &timing.resolve, 0);
            encoder.copy_buffer_to_buffer(&timing.resolve, 0, &timing.readback, 0, 32);
        }
    }
}
