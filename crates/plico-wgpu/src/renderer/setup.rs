use super::*;

/// `PLICO_GPU_TIMER=1` requests timestamp queries for ad-hoc frame profiling.
fn gpu_timer_requested() -> bool {
    std::env::var_os("PLICO_GPU_TIMER").is_some_and(|value| value != "0")
}

fn power_preference() -> wgpu::PowerPreference {
    match std::env::var("PLICO_POWER_PREFERENCE")
        .unwrap_or_default()
        .to_ascii_lowercase()
        .as_str()
    {
        "high" | "high-performance" | "highperformance" => wgpu::PowerPreference::HighPerformance,
        "none" => wgpu::PowerPreference::None,
        _ => wgpu::PowerPreference::LowPower,
    }
}

impl Renderer {
    pub async fn new() -> Result<Self> {
        // Share the backend instance across default renderers. Besides avoiding
        // repeated backend discovery, this serializes Vulkan-loader startup.
        // wasm holds no static: the instance is not Send+Sync there (JS
        // objects bound to the main thread), so a shared OnceLock cannot hold.
        #[cfg(not(target_arch = "wasm32"))]
        let instance = {
            static INSTANCE: std::sync::OnceLock<wgpu::Instance> = std::sync::OnceLock::new();
            INSTANCE.get_or_init(|| wgpu::Instance::new(crate::instance_descriptor()))
        };
        #[cfg(target_arch = "wasm32")]
        let instance = &wgpu::Instance::new(crate::instance_descriptor());
        Self::new_compatible(instance, None, false).await
    }
    /// Select an adapter compatible with the window surface before allocating resources.
    pub async fn new_for_surface(
        instance: &wgpu::Instance,
        surface: &wgpu::Surface<'_>,
    ) -> Result<Self> {
        Self::new_compatible(instance, Some(surface), false).await
    }
    pub(super) async fn new_compatible(
        instance: &wgpu::Instance,
        surface: Option<&wgpu::Surface<'_>>,
        profiling: bool,
    ) -> Result<Self> {
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: power_preference(),
                compatible_surface: surface,
                ..Default::default()
            })
            .await
            .context(crate::AdapterSnafu)?;
        let info = adapter.get_info();
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("Plico experimental quad renderer"),
                required_limits: if info.backend == wgpu::Backend::Gl {
                    wgpu::Limits::downlevel_webgl2_defaults().using_resolution(adapter.limits())
                } else {
                    wgpu::Limits::default()
                },
                required_features: if profiling || gpu_timer_requested() {
                    adapter.features() & wgpu::Features::TIMESTAMP_QUERY
                } else {
                    wgpu::Features::empty()
                },
                ..Default::default()
            })
            .await
            .context(crate::DeviceSnafu)?;
        Self::from_device(adapter, device, queue).await
    }
    /// Build GPU resources on a host-owned device and queue.
    ///
    /// All three handles must originate from the same adapter/device. The host
    /// retains ownership of window acquisition, submission ordering and presentation.
    pub async fn from_device(
        adapter: wgpu::Adapter,
        device: wgpu::Device,
        queue: wgpu::Queue,
    ) -> Result<Self> {
        let info = adapter.get_info();
        let group = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("quad frame layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: wgpu::BufferSize::new(48),
                },
                count: None,
            }],
        });
        let clear_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Plico damaged region replacement"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../clear.wgsl").into()),
        });
        let clear_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[Some(&group)],
            immediate_size: 0,
        });
        let clear_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Plico replace background"),
            layout: Some(&clear_layout),
            vertex: wgpu::VertexState {
                module: &clear_shader,
                entry_point: Some("vs"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &clear_shader,
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
        });
        let atlas_texture = crate::texture::AtlasTexture::new(&device);
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[
                Some(&group),
                Some(&atlas_texture.layout),
                Some(&atlas_texture.layout),
            ],
            immediate_size: 0,
        });
        let frame = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("quad frame"),
            size: 48,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &group,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: frame.as_entire_binding(),
            }],
        });
        let capacity = INITIAL_INSTANCE_CAPACITY;
        let instances = Self::instance_buffer(&device, capacity);
        // Everything up to the general fragment entry point is shared vertex
        // and SDF code; class fragments below are strict subsets of it.
        let quad_source = include_str!("../quad.wgsl");
        let class_common = &quad_source[..quad_source
            .rfind("@fragment")
            .expect("the quad shader defines one fragment entry point")];
        #[cfg(not(target_arch = "wasm32"))]
        let (plain_pipeline, fill_pipeline, shadow_pipeline, class_pipeline_rx) = {
            let (sender, receiver) = std::sync::mpsc::channel();
            let warm_device = device.clone();
            let warm_layout = layout.clone();
            let warm_common = class_common.to_owned();
            let worker = std::thread::Builder::new()
                .name("plico-class-pipelines".into())
                .spawn(move || {
                    for (class, source, vertex, label) in [
                        (
                            PipelineClass::Plain,
                            PLAIN_CLASS_SHADER,
                            "vs_plain",
                            "Plico plain quads",
                        ),
                        (
                            PipelineClass::Fill,
                            FILL_CLASS_SHADER,
                            "vs_fill",
                            "Plico fill quads",
                        ),
                        (
                            PipelineClass::Shadow,
                            SHADOW_CLASS_SHADER,
                            "vs_shadow",
                            "Plico shadow quads",
                        ),
                    ] {
                        let pipeline = Self::class_pipeline(
                            &warm_device,
                            &warm_layout,
                            &warm_common,
                            source,
                            vertex,
                            label,
                        );
                        if sender.send((class, pipeline)).is_err() {
                            break;
                        }
                    }
                });
            if worker.is_ok() {
                (None, None, None, receiver)
            } else {
                let (plain, fill, shadow) = Self::class_pipelines(&device, &layout, class_common);
                (Some(plain), Some(fill), Some(shadow), receiver)
            }
        };
        #[cfg(target_arch = "wasm32")]
        let (plain_pipeline, fill_pipeline, shadow_pipeline) = {
            let (plain, fill, shadow) = Self::class_pipelines(&device, &layout, class_common);
            (Some(plain), Some(fill), Some(shadow))
        };
        let gpu_timer = gpu_timer::GpuTimer::new(&device);
        let mut renderer = Self {
            device,
            queue,
            info,
            adapter,
            blitter: None,
            backdrop_compositor: None,
            backdrop_prefilter: None,
            backdrop_batch_prefilter: None,
            backdrop_pool: Default::default(),
            force_separate_backdrops: false,
            frame_layout: group,
            layout,
            frame,
            bind,
            pipeline: None,
            plus_pipeline: None,
            plain_pipeline,
            fill_pipeline,
            shadow_pipeline,
            #[cfg(not(target_arch = "wasm32"))]
            class_pipeline_rx,
            force_general: false,
            #[cfg(test)]
            profile_no_batch_cull: false,
            #[cfg(test)]
            profile_no_clip_skip: false,
            material_module: None,
            clear_pipeline,
            source: String::new(),
            target: None,
            instances,
            capacity,
            instance_scratch: Vec::new(),
            backdrop_effects: Vec::new(),
            deferred_layers: Vec::new(),
            inherited_backdrop: None,
            backdrop_contexts: Vec::new(),
            context_charged: Vec::new(),
            compilations: 0,
            #[cfg(all(target_os = "linux", feature = "system-text"))]
            paragraphs: None,
            #[cfg(all(target_os = "windows", feature = "system-text"))]
            windows_paragraphs: None,
            #[cfg(all(target_os = "macos", feature = "system-text"))]
            macos_paragraphs: None,
            #[cfg(all(target_arch = "wasm32", feature = "web"))]
            web_paragraphs: None,
            prepared_cache: std::collections::HashMap::new(),
            scene_preparations: 0,
            scene_cache_hits: 0,
            scene_damage: Default::default(),
            scene_background: None,
            images: Default::default(),
            svgs: Default::default(),
            paths: Default::default(),
            #[cfg(all(target_os = "linux", feature = "system-text"))]
            glyphs: Default::default(),
            masks: Default::default(),
            scene_prepare_depth: 0,
            layer_depth: 0,
            blur: None,
            frame_layer_bytes: 0,
            layer_pool: Vec::new(),
            layer_used: Vec::new(),
            layer_allocations: 0,
            layer_reuses: 0,
            filter_allocations: 0,
            filter_reuses: 0,
            effects: None,
            frame_effect_draws: 0,
            frame_effect_bytes: 0,
            frame_images: Vec::new(),
            frame_image_slots: Default::default(),
            batch_stats: Default::default(),
            frame_budget: FrameBudget::default(),
            gpu_timer,
            pending_gpu_slot: None,
            frame_prefix_commands: Vec::new(),
            split_submits: std::env::var_os("PLICO_SPLIT_SUBMITS")
                .is_some_and(|value| value != "0"),
            atlas_texture,
            #[cfg(all(target_os = "linux", feature = "system-text"))]
            atlas: None,
            #[cfg(all(target_os = "linux", feature = "system-text"))]
            text_stats: Default::default(),
        };
        renderer.set_material(DEFAULT_MATERIAL).await?;
        Ok(renderer)
    }
    pub fn adapter_info(&self) -> &wgpu::AdapterInfo {
        &self.info
    }
    /// Total prepared scenes retained across cache keys.
    pub(crate) fn prepared_len(&self) -> usize {
        self.prepared_cache.values().map(Vec::len).sum()
    }
    /// Successful pipeline compilations, for checking that frames do not recompile shaders.
    pub fn pipeline_compilations(&self) -> usize {
        self.compilations
    }
    /// Draw every class through the general shader instead of the specialized
    /// plain/fill/shadow pipelines, so tests and measurements can compare them
    /// (mygo's NoClasses). Pixels must be identical either way.
    pub fn force_general_pipelines(&mut self, force: bool) {
        self.force_general = force;
    }
    /// Publish class pipelines completed by the native background warmer.
    /// Frames encoded before a class arrives use the general pipeline.
    pub(super) fn poll_class_pipelines(&mut self) {
        #[cfg(not(target_arch = "wasm32"))]
        while let Ok((class, pipeline)) = self.class_pipeline_rx.try_recv() {
            match class {
                PipelineClass::Plain => self.plain_pipeline = Some(pipeline),
                PipelineClass::Fill => self.fill_pipeline = Some(pipeline),
                PipelineClass::Shadow => self.shadow_pipeline = Some(pipeline),
                PipelineClass::General => unreachable!("general pipeline is not warmed"),
            }
        }
    }
    /// Disable same-stage backdrop batching for pixel/performance comparisons.
    pub fn force_separate_backdrops(&mut self, force: bool) {
        self.force_separate_backdrops = force;
    }
    /// Runtime shader replacement. Invalid input preserves the last working pipeline.
    /// Loading the same source again is a no-op. No Rust compilation is involved.
    pub async fn set_material(&mut self, material: &str) -> Result<bool> {
        if material == self.source {
            return Ok(false);
        }
        if material.len() > 64 * 1024 {
            return Err(Error::Shader {
                message: "material exceeds 64 KiB".into(),
            });
        }
        let source = include_str!("../quad.wgsl")
            .replace("// PLICO_INTERFACE", crate::MATERIAL_INTERFACE)
            .replace("// PLICO_MATERIAL", material);
        let module = naga::front::wgsl::parse_str(&source).map_err(|e| Error::Shader {
            message: e.emit_to_string(&source),
        })?;
        naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::default(),
        )
        .validate(&module)
        .map_err(|e| Error::Shader {
            message: e.to_string(),
        })?;
        if module
            .global_variables
            .iter()
            .filter(|(_, g)| g.binding.is_some())
            .count()
            != 5
            || module.entry_points.len() != 2
        {
            return Err(Error::Shader {
                message: "materials cannot introduce bindings or entry points".into(),
            });
        }
        let scope = self.device.push_error_scope(wgpu::ErrorFilter::Validation);
        let shader = self
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("Plico quad material"),
                source: wgpu::ShaderSource::Wgsl(source.into()),
            });
        let pipeline =
            self.material_pipeline(&shader, wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING);
        if let Some(error) = scope.pop().await {
            return Err(Error::Shader {
                message: error.to_string(),
            });
        }
        self.pipeline = Some(pipeline);
        self.plus_pipeline = None;
        self.material_module = Some(shader);
        self.source = material.into();
        self.compilations += 1;
        {
            // The same declaration may produce different pixels under a new
            // material. Neither old layer pixels nor scene equality prove reuse.
            self.trim_layer_pool();
            self.invalidate_scene_damage();
        }
        Ok(true)
    }
    fn quad_pipeline(
        device: &wgpu::Device,
        layout: &wgpu::PipelineLayout,
        shader: &wgpu::ShaderModule,
        vertex_entry: &str,
        blend: &wgpu::BlendState,
        label: &str,
    ) -> wgpu::RenderPipeline {
        let attributes = wgpu::vertex_attr_array![0=>Float32x4,1=>Float32x4,2=>Float32x4,3=>Float32x4,4=>Float32x4,5=>Float32x4,6=>Float32x4,7=>Float32x4,8=>Float32x4,9=>Float32x4,10=>Float32x4];
        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some(label),
            layout: Some(layout),
            vertex: wgpu::VertexState {
                module: shader,
                entry_point: Some(vertex_entry),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<Instance>() as u64,
                    step_mode: wgpu::VertexStepMode::Instance,
                    attributes: &attributes,
                })],
            },
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: wgpu::TextureFormat::Rgba8Unorm,
                    blend: Some(*blend),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        })
    }
    fn class_pipeline(
        device: &wgpu::Device,
        layout: &wgpu::PipelineLayout,
        common: &str,
        class: &str,
        vertex_entry: &str,
        label: &str,
    ) -> wgpu::RenderPipeline {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some(label),
            source: wgpu::ShaderSource::Wgsl(format!("{common}\n{class}").into()),
        });
        Self::quad_pipeline(
            device,
            layout,
            &shader,
            vertex_entry,
            &wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING,
            label,
        )
    }
    fn class_pipelines(
        device: &wgpu::Device,
        layout: &wgpu::PipelineLayout,
        common: &str,
    ) -> (
        wgpu::RenderPipeline,
        wgpu::RenderPipeline,
        wgpu::RenderPipeline,
    ) {
        (
            Self::class_pipeline(
                device,
                layout,
                common,
                PLAIN_CLASS_SHADER,
                "vs_plain",
                "Plico plain quads",
            ),
            Self::class_pipeline(
                device,
                layout,
                common,
                FILL_CLASS_SHADER,
                "vs_fill",
                "Plico fill quads",
            ),
            Self::class_pipeline(
                device,
                layout,
                common,
                SHADOW_CLASS_SHADER,
                "vs_shadow",
                "Plico shadow quads",
            ),
        )
    }
    fn material_pipeline(
        &self,
        shader: &wgpu::ShaderModule,
        blend: wgpu::BlendState,
    ) -> wgpu::RenderPipeline {
        Self::quad_pipeline(
            &self.device,
            &self.layout,
            shader,
            "vs_main",
            &blend,
            "Plico SDF quads",
        )
    }
    pub(super) fn ensure_plus_pipeline(&mut self) {
        if self.plus_pipeline.is_none() {
            let component = wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::One,
                dst_factor: wgpu::BlendFactor::One,
                operation: wgpu::BlendOperation::Add,
            };
            self.plus_pipeline = Some(self.material_pipeline(
                self.material_module.as_ref().expect("material installed"),
                wgpu::BlendState {
                    color: component,
                    alpha: component,
                },
            ));
            self.compilations += 1;
        }
    }
    pub(super) fn instance_buffer(device: &wgpu::Device, capacity: usize) -> wgpu::Buffer {
        device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Plico quad instances"),
            contents: &vec![0; capacity * std::mem::size_of::<Instance>()],
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        })
    }
    pub(super) fn target(&mut self, size: [u32; 2]) {
        if self.target.as_ref().is_some_and(|t| t.size == size) {
            return;
        }
        let root = self.layer_depth == 0;
        if root && let Some(blitter) = &mut self.blitter {
            blitter.clear_source();
        }
        let [width, height] = size;
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Plico quad offscreen target"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::COPY_DST
                | wgpu::TextureUsages::COPY_SRC
                | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let pitch = (width * 4).div_ceil(256) * 256;
        let view = texture.create_view(&Default::default());
        self.target = Some(Target {
            texture,
            view,
            readback: None,
            size,
            pitch,
        });
    }
}
