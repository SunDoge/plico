use crate::{DEFAULT_MATERIAL, Error, Quad, Result};
use snafu::ResultExt;
use wgpu::util::DeviceExt;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Instance {
    rect: [f32; 4],
    color: [f32; 4],
    color2: [f32; 4],
    border: [f32; 4],
    config: [f32; 4],
    clip: [f32; 4],
    uv: [f32; 4],
    gradient: [f32; 4],
    clip_params: [f32; 4],
    mask_rect: [f32; 4],
    radii: [f32; 4],
}

/// Per-corner radii as the shader draws them: nonnegative and shrunk so that
/// adjacent corners on an edge never overlap (the CSS proportional rule).
/// Uniform single-radius quads pass through unchanged.
fn shrink_radii(size: [f32; 2], radii: [f32; 4]) -> [f32; 4] {
    let factor = |sum: f32, edge: f32| {
        if sum > edge && sum > 0. {
            edge / sum
        } else {
            1.
        }
    };
    let [width, height] = size;
    let top = factor(radii[0] + radii[1], width);
    let bottom = factor(radii[3] + radii[2], width);
    let right = factor(radii[1] + radii[2], height);
    let left = factor(radii[0] + radii[3], height);
    [
        radii[0] * top.min(left),
        radii[1] * top.min(right),
        radii[2] * bottom.min(right),
        radii[3] * bottom.min(left),
    ]
}
struct Target {
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    readback: Option<wgpu::Buffer>,
    size: [u32; 2],
    pitch: u32,
}
struct LayerTarget {
    target: Target,
    image: std::rc::Rc<crate::image::GpuImage>,
    blur: Option<(crate::blur::Workspace, std::rc::Rc<crate::image::GpuImage>)>,
}
impl LayerTarget {
    fn bytes(&self) -> u64 {
        u64::from(self.target.size[0])
            * u64::from(self.target.size[1])
            * if self.blur.is_some() { 12 } else { 4 }
    }
}
/// Cumulative layer allocation work and currently retained pool payload.
#[derive(Clone, Copy, Debug, Default)]
pub struct LayerPoolStats {
    pub allocations: u64,
    pub reuses: u64,
    /// Cumulative two-texture blur workspace creations.
    pub filter_allocations: u64,
    /// Cumulative blur workspace reuses.
    pub filter_reuses: u64,
    pub entries: usize,
    pub retained_bytes: u64,
}
/// Work encoded by the most recent root scene attempt, including child layers.
/// Counts exclude effect/blur pipelines, target readback and presentation blits.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FrameBatchStats {
    /// Instances prepared across scene color passes, before scissor replays.
    pub instances: u64,
    /// Instanced scene draw calls (each damage scissor replay counts).
    pub draw_calls: u64,
    /// Draw calls through the specialized plain-fill pipeline.
    pub plain_draw_calls: u64,
    /// Scene color render passes, including isolated layers.
    pub passes: u64,
    /// Explicit partial-damage replacement clears (not load-op clears).
    pub clears: u64,
}
// Preserve scope markers while culling expensive child layers before encoding.
// The later instance filter remains useful for glyph-level pruning.
fn draw_intersects(draw: &crate::Draw<'_>, regions: &[[u32; 4]]) -> bool {
    let intersects = |rect: [f64; 4], clip: Option<[f32; 4]>| {
        let [mut x0, mut y0, mut x1, mut y1] = rect;
        if let Some([x, y, w, h]) = clip {
            x0 = x0.max(x as f64 - 1.);
            y0 = y0.max(y as f64 - 1.);
            x1 = x1.min((x + w) as f64 + 1.);
            y1 = y1.min((y + h) as f64 + 1.);
        }
        x1 > x0
            && y1 > y0
            && regions.iter().any(|&[x, y, w, h]| {
                x0 < ((x + w) as f64) && x1 > x as f64 && y0 < ((y + h) as f64) && y1 > y as f64
            })
    };
    let quad = |q: &Quad| {
        let [x, y, w, h] = q.rect;
        intersects(
            [
                x as f64 - 1.,
                y as f64 - 1.,
                (x + w) as f64 + 1.,
                (y + h) as f64 + 1.,
            ],
            q.clip,
        )
    };
    match draw {
        crate::Draw::Quad(q) => quad(q),
        crate::Draw::Image(image) => quad(&image.quad),
        crate::Draw::Layer(layer) => quad(&layer.quad),
        crate::Draw::Effect(_, q) => quad(q),
        crate::Draw::SetMask(_) | crate::Draw::ResetMask => true,
        #[cfg(all(target_os = "linux", feature = "system-text"))]
        crate::Draw::Text(run) => run.layout.glyphs.iter().any(|glyph| {
            let [x, y, w, h] = glyph.ink_bounds;
            w > 0.
                && h > 0.
                && intersects(
                    [
                        run.origin[0] + glyph.x + x - 4.,
                        run.origin[1] + glyph.y + y - 4.,
                        run.origin[0] + glyph.x + x + w + 4.,
                        run.origin[1] + glyph.y + y + h + 4.,
                    ],
                    run.clip,
                )
        }),
    }
}
/// Owns an independent wgpu device, a persistent render target and instance buffer.
/// CPU readback is for experiments; it is not the future window submission path.
/// Prepared scenes retained for replay, keyed by (frame size, exact root-clip
/// bits). Each key holds the root scene plus rebuilt isolated-layer variants,
/// distinguished by command identity rather than the key itself.
pub(crate) type PreparedCache =
    std::collections::HashMap<([u32; 2], [u64; 4]), Vec<std::rc::Rc<crate::scene::PreparedScene>>>;

pub struct Renderer {
    device: wgpu::Device,
    queue: wgpu::Queue,
    info: wgpu::AdapterInfo,
    adapter: wgpu::Adapter,
    blitter: Option<crate::blit::Blitter>,
    layout: wgpu::PipelineLayout,
    frame: wgpu::Buffer,
    bind: wgpu::BindGroup,
    pipeline: Option<wgpu::RenderPipeline>,
    plus_pipeline: Option<wgpu::RenderPipeline>,
    /// Specialized pipeline for plain solid fills (mygo's ClassPlain). Static
    /// source: it never recompiles and is not part of pipeline_compilations.
    plain_pipeline: wgpu::RenderPipeline,
    /// Comparison switch disabling class specialization (mygo's NoClasses).
    force_general: bool,
    #[cfg(test)]
    profile_no_batch_cull: bool,
    material_module: Option<wgpu::ShaderModule>,
    clear_pipeline: wgpu::RenderPipeline,
    source: String,
    target: Option<Target>,
    instances: wgpu::Buffer,
    capacity: usize,
    /// Backdrop-reading effects collected during encode, in paint order.
    backdrop_effects: Vec<plico_renderer::ShaderEffect>,
    /// Reused instance staging vector: every frame keeps the capacity the
    /// previous frame grew instead of allocating from empty. Rust pattern:
    /// std::mem::take + clear + write back on success.
    instance_scratch: Vec<Instance>,
    compilations: usize,
    #[cfg(all(target_os = "linux", feature = "system-text"))]
    pub(crate) paragraphs: Option<std::rc::Rc<std::cell::RefCell<crate::paragraph::Cache>>>,
    #[cfg(all(target_os = "windows", feature = "system-text"))]
    pub(crate) windows_paragraphs:
        Option<std::rc::Rc<std::cell::RefCell<crate::windows_paragraph::Cache>>>,
    #[cfg(all(target_os = "macos", feature = "system-text"))]
    pub(crate) macos_paragraphs:
        Option<std::rc::Rc<std::cell::RefCell<crate::macos_paragraph::Cache>>>,
    #[cfg(all(target_arch = "wasm32", feature = "web"))]
    pub(crate) web_paragraphs: Option<std::rc::Rc<std::cell::RefCell<crate::web_text::Cache>>>,
    pub(crate) prepared_cache: PreparedCache,
    pub(crate) scene_preparations: u64,
    pub(crate) scene_cache_hits: u64,
    pub(crate) scene_damage: plico_renderer::damage::SceneDamageTracker,
    pub(crate) scene_background: Option<[f32; 4]>,
    images: crate::image::ImageCache,
    pub(crate) svgs: crate::svg::SvgCache,
    pub(crate) paths: crate::path::PathCache,
    #[cfg(all(target_os = "linux", feature = "system-text"))]
    pub(crate) glyphs: crate::glyphs::GlyphCache,
    pub(crate) masks: crate::mask::MaskCache,
    pub(crate) scene_prepare_depth: u8,
    layer_depth: u8,
    blur: Option<crate::blur::Blur>,
    frame_layer_bytes: u64,
    layer_pool: Vec<Option<LayerTarget>>,
    layer_used: Vec<bool>,
    layer_allocations: u64,
    layer_reuses: u64,
    filter_allocations: u64,
    filter_reuses: u64,
    effects: Option<plico_renderer::shader::ShaderRenderer>,
    frame_effect_draws: usize,
    frame_effect_bytes: u64,
    frame_images: Vec<std::rc::Rc<crate::image::GpuImage>>,
    frame_image_slots: std::collections::HashMap<usize, usize>,
    batch_stats: FrameBatchStats,
    atlas_texture: crate::texture::AtlasTexture,
    #[cfg(all(target_os = "linux", feature = "system-text"))]
    atlas: Option<plico_native::text::atlas::Atlas>,
    #[cfg(all(target_os = "linux", feature = "system-text"))]
    text_stats: crate::text::TextStats,
}
/// The specialized plain-fill fragment: the strict subset of the general
/// fs_main that covers solid colors under the analytic SDF coverage and the
/// hard or antialiased clip. mygo specializes the heaviest shader paths out
/// of the instance stream most of a window is made of; this is Plico's first
/// class. Drawn pixels must be identical to the general pipeline.
const PLAIN_FRAGMENT: &str = r#"
@fragment fn fs_main(input: Vertex) -> @location(0) vec4f {
    let world=input.position.xy+frame.origin.xy;
    let clip=input.clip;
    var clip_alpha=1.;
    if input.clip_params.x>0.5 {
        clip_alpha=coverage(world-clip.xy,clip.zw,vec4f(0.));
        if clip_alpha<=0. { discard; }
    } else if any(world<clip.xy) || any(world>=clip.xy+clip.zw) { discard; }
    let outer=coverage(input.local,input.size,input.radii);
    let alpha=input.color.a*outer*clip_alpha;
    return vec4f(input.color.rgb*alpha,alpha);
}
"#;

/// A plain fill: solid color (equal endpoint colors make the gradient mix a
/// no-op), no border, no material, not an image/glyph/shadow instance, and no
/// coverage mask. Everything the general fragment does beyond the SDF and
/// clip is absent, so the specialized pipeline draws the same pixels with a
/// much smaller shader.
/// The most pixels a run may cover for joinLast to merge it with its
/// neighbors of other classes (mygo's joinArea). On an M5 a draw call costs
/// about 1.1 µs of CPU and 0.1 µs of GPU, while a plain fill drawn through
/// the general shader costs the GPU at most 0.075 ns a pixel more: at this
/// area, the worst specialization loss equals the call it saves.
const JOIN_AREA: f32 = 128. * 128.;

/// Pixel area a run's quads cover as the vertex stage grows them (one AA
/// pixel each side). Stops early at JOIN_AREA; shadow quads already include
/// their four-sigma halo in the rect.
/// One ordered instance run to draw through a chosen pipeline.
struct RunFlush {
    start: usize,
    end: usize,
    plain: bool,
    plus: bool,
    texture: Option<usize>,
    mask: u32,
}

fn run_area(instances: &[Instance], start: usize, end: usize) -> f32 {
    let mut area = 0.;
    for instance in &instances[start..end] {
        let [_, _, w, h] = instance.rect;
        area += (w + 2.) * (h + 2.);
        if area > JOIN_AREA {
            return area;
        }
    }
    area
}

fn is_plain(instance: &Instance) -> bool {
    instance.config[2] == 0.
        && instance.config[1] == 0.
        && instance.clip_params[2] == 0.
        && instance.color == instance.color2
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
        let instance = wgpu::Instance::new(crate::instance_descriptor());
        Self::new_compatible(instance, None, false).await
    }
    /// Select an adapter compatible with the window surface before allocating resources.
    pub async fn new_for_surface(
        instance: &wgpu::Instance,
        surface: &wgpu::Surface<'_>,
    ) -> Result<Self> {
        Self::new_compatible(instance, Some(surface), false).await
    }
    async fn new_compatible(
        instance: &wgpu::Instance,
        surface: Option<&wgpu::Surface<'_>>,
        profiling: bool,
    ) -> Result<Self> {
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::LowPower,
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
                required_features: if profiling {
                    adapter.features() & wgpu::Features::TIMESTAMP_QUERY
                } else {
                    wgpu::Features::empty()
                },
                ..Default::default()
            })
            .await
            .context(crate::DeviceSnafu)?;
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
            source: wgpu::ShaderSource::Wgsl(include_str!("clear.wgsl").into()),
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
        let capacity = 64;
        let instances = Self::instance_buffer(&device, capacity);
        // Everything up to the general fragment entry point is shared vertex
        // and SDF code; the plain fragment below is a strict subset of it.
        let quad_source = include_str!("quad.wgsl");
        let plain_common = &quad_source[..quad_source
            .rfind("@fragment")
            .expect("the quad shader defines one fragment entry point")];
        let plain_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Plico plain quads"),
            source: wgpu::ShaderSource::Wgsl(format!("{plain_common}\n{PLAIN_FRAGMENT}").into()),
        });
        let plain_pipeline = Self::quad_pipeline(
            &device,
            &layout,
            &plain_shader,
            &wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING,
            "Plico plain quads",
        );
        let mut renderer = Self {
            device,
            queue,
            info,
            adapter,
            blitter: None,
            layout,
            frame,
            bind,
            pipeline: None,
            plus_pipeline: None,
            plain_pipeline,
            force_general: false,
            #[cfg(test)]
            profile_no_batch_cull: false,
            material_module: None,
            clear_pipeline,
            source: String::new(),
            target: None,
            instances,
            capacity,
            instance_scratch: Vec::new(),
            backdrop_effects: Vec::new(),
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
    /// Draw plain fills through the general shader instead of the specialized
    /// plain pipeline, so tests and measurements can compare the two (mygo's
    /// NoClasses). Pixels must be identical either way.
    pub fn force_general_pipelines(&mut self, force: bool) {
        self.force_general = force;
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
        let source = include_str!("quad.wgsl")
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
        blend: &wgpu::BlendState,
        label: &str,
    ) -> wgpu::RenderPipeline {
        let attributes = wgpu::vertex_attr_array![0=>Float32x4,1=>Float32x4,2=>Float32x4,3=>Float32x4,4=>Float32x4,5=>Float32x4,6=>Float32x4,7=>Float32x4,8=>Float32x4,9=>Float32x4,10=>Float32x4];
        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some(label),
            layout: Some(layout),
            vertex: wgpu::VertexState {
                module: shader,
                entry_point: Some("vs_main"),
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
    fn material_pipeline(
        &self,
        shader: &wgpu::ShaderModule,
        blend: wgpu::BlendState,
    ) -> wgpu::RenderPipeline {
        Self::quad_pipeline(
            &self.device,
            &self.layout,
            shader,
            &blend,
            "Plico SDF quads",
        )
    }
    fn ensure_plus_pipeline(&mut self) {
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
    fn instance_buffer(device: &wgpu::Device, capacity: usize) -> wgpu::Buffer {
        device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Plico quad instances"),
            contents: &vec![0; capacity * std::mem::size_of::<Instance>()],
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        })
    }
    fn target(&mut self, size: [u32; 2]) {
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
    fn encode_frame(
        &mut self,
        items: &[crate::Draw<'_>],
        size: [u32; 2],
        background: [f32; 4],
        time: f32,
    ) -> Result<wgpu::CommandEncoder> {
        self.scene_damage.invalidate();
        self.encode_frame_regions(items, size, background, time, None)
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
        items: &[crate::Draw<'_>],
        size: [u32; 2],
        background: [f32; 4],
        time: f32,
        regions: Option<&[[u32; 4]]>,
    ) -> Result<wgpu::CommandEncoder> {
        self.batch_stats = FrameBatchStats::default();
        self.validate_frame(items.len(), size, background, time)?;
        let unchanged = regions.is_some_and(|r| r.is_empty())
            && self.target.as_ref().is_some_and(|t| t.size == size);
        let counters = (self.layer_allocations, self.layer_reuses);
        if !unchanged {
            self.prepare_effect_frame(items)?;
        }
        let result = self.encode_frame_at(items, size, background, time, regions, [0.; 2]);
        // Encoded commands retain their own GPU references. The pass-local
        // interning table must not keep evicted textures alive until another frame.
        self.clear_frame_images();
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
                layer_pool_entries=stats.entries,layer_pool_bytes=stats.retained_bytes,filter_allocations=stats.filter_allocations,filter_reuses=stats.filter_reuses,effect_targets=effect_stats.0,effect_bytes=effect_stats.1,effect_pipelines=effect_stats.2,positioned_fonts=positioned.0,positioned_source_bytes=positioned.1,positioned_rasters=positioned.2,positioned_raster_bytes=positioned.3,image_sources=image_sources.0,image_texture_bytes=image_sources.1,image_uploads_total=image_sources.2,image_source_hits_total=image_sources.3,image_bindings=image_bindings.0,image_binding_creations_total=image_bindings.1,scene_instances=self.batch_stats.instances,scene_draw_calls=self.batch_stats.draw_calls,scene_passes=self.batch_stats.passes,damage_clears=self.batch_stats.clears,"wgpu frame resources");
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
    fn encode_frame_at(
        &mut self,
        items: &[crate::Draw<'_>],
        size: [u32; 2],
        background: [f32; 4],
        time: f32,
        regions: Option<&[[u32; 4]]>,
        origin: [f32; 2],
    ) -> Result<wgpu::CommandEncoder> {
        if self.layer_depth == 0 {
            self.frame_layer_bytes = 0;
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
                .copied()
                .filter(|draw| draw_intersects(draw, regions))
                .collect::<Vec<_>>();
            selected.as_slice()
        } else {
            items
        };
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
        self.target(size);
        let view = self
            .target
            .as_ref()
            .expect("target just created")
            .view
            .clone();
        let mut flush_draws: u64 = 0;
        let mut flush_plain_draws: u64 = 0;
        let mut encoder = self.device.create_command_encoder(&Default::default());
        self.batch_stats.instances += instances.len() as u64;
        self.batch_stats.passes += 1;
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Plico quad draw"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: if regions.is_some() {
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
            if regions.is_some() {
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
            let mut first = 0;
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
            while first < instances.len() {
                let key = |i: usize| {
                    (instances[i].config[2] == 4.).then_some(instances[i].config[3] as usize)
                };
                let texture = key(first);
                // Class runs: adjacent plain fills share the specialized
                // pipeline, everything else keeps the general shader. The
                // run boundary never reorders painter sequence.
                // Backdrop markers (color[3] == 2) are singleton runs: they
                // never join a class run or the joiner.
                let backdrop_at =
                    (instances[first].color[3] == 2.).then(|| instances[first].config[3] as usize);
                let plain =
                    backdrop_at.is_none() && !self.force_general && is_plain(&instances[first]);
                let mut end = first + 1;
                while backdrop_at.is_none()
                    && end < instances.len()
                    && instances[end].color[3] != 2.
                    && key(end) == texture
                    && instances[end].clip_params[2] == instances[first].clip_params[2]
                    && (instances[end].config[2] == 4. && instances[end].config[1] == 1.)
                        == (instances[first].config[2] == 4. && instances[first].config[1] == 1.)
                    && (!force_general && is_plain(&instances[end])) == plain
                {
                    end += 1;
                }
                let plus = instances[first].config[2] == 4. && instances[first].config[1] == 1.;
                let mask = instances[first].clip_params[2] as u32;
                let small = backdrop_at.is_none() && run_area(&instances, first, end) <= JOIN_AREA;
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
                        // Split the pass: everything before the effect is
                        // now in the target; copy that region out, render
                        // the effect with it bound, then continue the rest
                        // in a fresh pass (mygo's Backdrop step between
                        // batches).
                        drop(pass);
                        let effect = self.backdrop_effects[effect_index].clone();
                        let mut effects = self.effects.take();
                        let mut blitter = self.blitter.take();
                        pass = encode_backdrop_step(
                            &self.device,
                            &self.queue,
                            self.target.as_ref(),
                            &mut effects,
                            &mut blitter,
                            &mut encoder,
                            &view,
                            instances[run.start].rect,
                            &effect,
                            size,
                        );
                        self.effects = effects;
                        self.blitter = blitter;
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
        self.batch_stats.draw_calls += flush_draws;
        self.batch_stats.plain_draw_calls += flush_plain_draws;
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

    /// Render one backdrop-reading effect between the halves of the frame
    /// pass: the caller has just dropped the pass, so the target holds
    /// everything painted before the effect. Copy the effect's region out,
    /// render the effect against it, draw the result into the frame and
    /// return a fresh pass for the remaining instances (mygo's Backdrop
    /// step between batches).
    /// Render one backdrop-reading effect between the halves of the frame pass
    /// (mygo's Backdrop step between batches): copy the effect's region out of
    /// the live target, render the effect against it, blit the result back into
    /// the frame, and return a fresh pass for the remaining instances.
    /// All renderer state arrives as parameters so it stays disjoint from the
    /// caller's encoder/pass borrows.
    /// Offscreen readback; window submission uses render_to_view instead.
    pub fn render_items(
        &mut self,
        items: &[crate::Draw<'_>],
        size: [u32; 2],
        background: [f32; 4],
        time: f32,
    ) -> Result<Vec<u8>> {
        let encoder = self.encode_frame(items, size, background, time)?;
        self.read_frame(encoder, size)
    }
    #[tracing::instrument(
        level = "debug",
        target = "plico::perf",
        name = "wgpu.readback_including_gpu_wait",
        skip_all
    )]
    pub(crate) fn read_frame(
        &mut self,
        mut encoder: wgpu::CommandEncoder,
        size: [u32; 2],
    ) -> Result<Vec<u8>> {
        let [width, height] = size;
        let target = self.target.as_mut().expect("encode_frame created target");
        let readback = target.readback.get_or_insert_with(|| {
            self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("Plico readback"),
                size: u64::from(target.pitch) * u64::from(height),
                usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            })
        });
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &target.texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: readback,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(target.pitch),
                    rows_per_image: Some(height),
                },
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
        self.queue.submit([encoder.finish()]);
        let (tx, rx) = std::sync::mpsc::channel();
        readback
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |result| {
                let _ = tx.send(result);
            });
        self.device
            .poll(wgpu::PollType::wait_indefinitely())
            .context(crate::PollSnafu)?;
        rx.recv()
            .map_err(|e| Error::Readback {
                message: e.to_string(),
            })?
            .map_err(|e| Error::Readback {
                message: e.to_string(),
            })?;
        let data = readback
            .slice(..)
            .get_mapped_range()
            .map_err(|e| Error::Readback {
                message: e.to_string(),
            })?;
        let mut pixels = Vec::with_capacity((width * height * 4) as usize);
        for row in data.chunks_exact(target.pitch as usize) {
            pixels.extend_from_slice(&row[..(width * 4) as usize]);
        }
        drop(data);
        readback.unmap();
        Ok(pixels)
    }
}

impl Renderer {
    fn prepare_instances(
        &mut self,
        items: &[crate::Draw<'_>],
        size: [u32; 2],
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
            let result = self.encode_items(items, size);
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
                self.encode_items(items, size)
            } else {
                result
            };
            let instances = result?;
            self.upload_glyphs();
            Ok(instances)
        }
        #[cfg(not(all(target_os = "linux", feature = "system-text")))]
        self.encode_items(items, size)
    }
    #[cfg(all(target_os = "linux", feature = "system-text"))]
    fn warm_layer_text(
        &mut self,
        items: &[crate::Draw<'_>],
        size: [u32; 2],
        scratch: &mut Vec<Instance>,
    ) -> Result<()> {
        for item in items {
            match item {
                crate::Draw::Text(run) => {
                    scratch.clear();
                    self.encode_text(run, size, scratch)?;
                }
                crate::Draw::Layer(layer)
                    if layer.alpha > 0.
                        && !layer.quad.clip.is_some_and(|r| r[2] <= 0. || r[3] <= 0.) =>
                {
                    self.warm_layer_text(&layer.scene.draws(), size, scratch)?
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
    fn clear_frame_images(&mut self) {
        self.frame_image_slots.clear();
        self.frame_images.clear();
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
    fn encode_items(&mut self, items: &[crate::Draw<'_>], size: [u32; 2]) -> Result<Vec<Instance>> {
        let [width, height] = size;
        let mut instances = std::mem::take(&mut self.instance_scratch);
        instances.clear();
        self.backdrop_effects.clear();
        self.clear_frame_images();
        let mut active_mask: Option<(usize, [f32; 4])> = None;
        for item in items {
            let first = instances.len();
            match item {
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
                    let image = self.images.get(
                        &self.device,
                        &self.queue,
                        &self.atlas_texture.layout,
                        &mask.image,
                        crate::image::ImageSampling {
                            quality: peniko::ImageQuality::Low,
                            extend: [peniko::Extend::Pad; 2],
                            mipmaps: false,
                        },
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
                            q.border_width,
                            if q.shadow_sigma > 0. {
                                5.
                            } else if q.material {
                                1.
                            } else {
                                0.
                            },
                            q.parameter,
                        ],
                        clip: q.clip.unwrap_or([0., 0., width as f32, height as f32]),
                        uv: [0.; 4],
                        gradient: if q.shadow_sigma > 0. {
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
                            clip_params: [0.; 4],
                        });
                        continue;
                    }
                    {
                        q.validate()?;
                        let resource = self.render_effect(effect)?;
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
                    let resource = self.render_layer(layer)?;
                    let slot = self.intern_frame_image(resource);
                    let q = &layer.quad;
                    instances.push(Instance {
                        mask_rect: [0.; 4],
                        radii: shrink_radii([q.rect[2], q.rect[3]], q.radii),
                        rect: q.rect,
                        color: [0., 0., 0., layer.alpha],
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
            if instances.len() > 1_000_000 {
                return Err(Error::InvalidInput {
                    reason: "too many visible instances".into(),
                });
            }
        }
        Ok(instances)
    }
}
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
    fn encode_text(
        &mut self,
        run: &crate::text::TextRun<'_>,
        size: [u32; 2],
        instances: &mut Vec<Instance>,
    ) -> Result<()> {
        if !run.origin.iter().all(|v| v.is_finite() && v.abs() < 1e9)
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
            let x = glyph.x + run.origin[0];
            let y = glyph.y + run.origin[1];
            let quantized = (x * 4.).round() as i64;
            let (x, y, phase) = if run.continuous {
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
                || x + ix + iw + 3. < f64::from(clip[0])
                || y + iy + ih + 3. < f64::from(clip[1])
                || x + ix - 3. > f64::from(clip[0] + clip[2])
                || y + iy - 3. > f64::from(clip[1] + clip[3])
            {
                continue;
            }
            let entry = atlas.get(glyph, phase).context(crate::TextSnafu)?;
            let [ax, ay, w, h] = entry.rect;
            let rect = [
                x as f32 + entry.origin[0] as f32,
                y as f32 + entry.origin[1] as f32,
                w as f32,
                h as f32,
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
    fn upload_glyphs(&mut self) {
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

impl Renderer {
    pub fn surface_capabilities(&self, surface: &wgpu::Surface<'_>) -> wgpu::SurfaceCapabilities {
        surface.get_capabilities(&self.adapter)
    }
    pub fn configure_surface(
        &self,
        surface: &wgpu::Surface<'_>,
        config: &wgpu::SurfaceConfiguration,
    ) {
        surface.configure(&self.device, config);
    }
    /// GPU-only submit to a window/offscreen view, without readback or CPU waiting.
    /// Destination must have the requested size and format and support RENDER_ATTACHMENT.
    #[tracing::instrument(name="plico_wgpu.render_to_view",skip_all,fields(width=size[0],height=size[1]))]
    pub fn render_to_view(
        &mut self,
        items: &[crate::Draw<'_>],
        size: [u32; 2],
        background: [f32; 4],
        time: f32,
        destination: &wgpu::TextureView,
        format: wgpu::TextureFormat,
    ) -> Result<()> {
        if !matches!(
            format,
            wgpu::TextureFormat::Rgba8Unorm
                | wgpu::TextureFormat::Rgba8UnormSrgb
                | wgpu::TextureFormat::Bgra8Unorm
                | wgpu::TextureFormat::Bgra8UnormSrgb
        ) {
            return Err(Error::InvalidInput {
                reason: "unsupported present target format".into(),
            });
        }
        let encoder = self.encode_frame(items, size, background, time)?;
        self.submit_to_view(encoder, destination, format)
    }
    #[tracing::instrument(
        level = "debug",
        target = "plico::perf",
        name = "wgpu.submit",
        skip_all
    )]
    pub(crate) fn submit_to_view(
        &mut self,
        mut encoder: wgpu::CommandEncoder,
        destination: &wgpu::TextureView,
        format: wgpu::TextureFormat,
    ) -> Result<()> {
        if !matches!(
            format,
            wgpu::TextureFormat::Rgba8Unorm
                | wgpu::TextureFormat::Rgba8UnormSrgb
                | wgpu::TextureFormat::Bgra8Unorm
                | wgpu::TextureFormat::Bgra8UnormSrgb
        ) {
            return Err(Error::InvalidInput {
                reason: "unsupported present target format".into(),
            });
        }
        if self.blitter.as_ref().is_none_or(|b| b.format != format) {
            self.blitter = Some(crate::blit::Blitter::new(&self.device, format));
        }
        let view = &self.target.as_ref().unwrap().view;
        self.blitter
            .as_mut()
            .unwrap()
            .encode(&self.device, &mut encoder, view, destination);
        self.queue.submit([encoder.finish()]);
        Ok(())
    }
}

impl Renderer {
    /// Present a frame submitted on this renderer's queue.
    pub fn present(&self, frame: wgpu::SurfaceTexture) {
        self.queue.present(frame);
    }
}

impl Renderer {
    /// CPU raster cache entries and payload bytes, in path/SVG/clip-mask order.
    /// Excludes scene-owned source assets and externally retained prepared scenes.
    pub fn raster_cache_stats(&self) -> [(usize, usize); 3] {
        [self.paths.stats(), self.svgs.stats(), self.masks.stats()]
    }
    /// Release CPU path/SVG/clip rasters and the local prepared-scene snapshot.
    /// Retained final pixels remain valid; subsequent changes rebuild derived data.
    pub fn trim_raster_cache(&mut self) {
        self.prepared_cache.clear();
        self.paths = Default::default();
        self.svgs = Default::default();
        self.masks = Default::default();
    }
    /// Release immutable-image GPU textures and bindings, preserving cumulative
    /// counters and retained final pixels. Source images remain owned by the scene.
    pub fn trim_image_cache(&mut self) {
        self.clear_frame_images();
        self.images.clear();
    }
    /// Cached source textures, resident texture bytes, uploads and source hits.
    /// Sampling bindings are counted separately; native atlas textures are excluded.
    pub fn image_cache_stats(&self) -> (usize, usize, u64, u64) {
        self.images.stats()
    }
    /// Resident immutable-image sampling bindings and cumulative creations. Several
    /// bindings can share one source texture without another upload.
    pub fn image_binding_stats(&self) -> (usize, u64) {
        self.images.binding_stats()
    }
}

impl Renderer {
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
        stats
    }
    /// Release idle intermediate targets without invalidating retained final pixels.
    pub fn trim_layer_pool(&mut self) {
        self.layer_pool.clear();
        self.layer_used.clear();
        self.clear_frame_images();
    }
    fn trim_idle_layers(&mut self, keep: bool) {
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
    fn render_layer(
        &mut self,
        layer: &crate::scene::PreparedLayer,
    ) -> Result<std::rc::Rc<crate::image::GpuImage>> {
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
        let mut workspace = None;
        let cached_image = if let Some(entry) = saved {
            workspace = entry.blur;
            self.target = Some(entry.target);
            Some(entry.image)
        } else {
            None
        };
        let parent_images = std::mem::take(&mut self.frame_images);
        let parent_slots = std::mem::take(&mut self.frame_image_slots);
        let encoded = self.encode_frame_at(&layer.scene.draws(), size, [0.; 4], 0., None, [x, y]);
        let mut resource = cached_image;
        let result = encoded.map(|mut encoder| {
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
            let image = if let Some(blur) = layer.blur.filter(|b| b.sigma > 0.) {
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
                pipeline.encode(&self.queue, &mut encoder, work, blur.sigma, blur.edge);
                image.clone()
            } else {
                image
            };
            self.queue.submit([encoder.finish()]);
            image
        });
        if let (Some(target), Some(image)) = (self.target.take(), resource) {
            self.layer_pool[slot] = Some(LayerTarget {
                target,
                image,
                blur: workspace,
            });
        }
        self.target = parent_target;
        self.frame_images = parent_images;
        self.frame_image_slots = parent_slots;
        self.layer_depth -= 1;
        result
    }
}

fn visit_effects(items: &[crate::Draw<'_>], visit: &mut impl FnMut(&plico_renderer::ShaderEffect)) {
    for draw in items {
        match draw {
            crate::Draw::Effect(effect, _) => visit(effect),
            crate::Draw::Layer(layer) => visit_effects(&layer.scene.draws(), visit),
            _ => {}
        }
    }
}
impl Renderer {
    fn prepare_effect_frame(&mut self, items: &[crate::Draw<'_>]) -> Result<()> {
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
    fn render_effect(
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
            let mut effects = plico_renderer::shader::ShaderRenderer::new(&self.device);
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
        self.queue.submit([encoder.finish()]);
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

#[allow(clippy::too_many_arguments)]
fn encode_backdrop_step<'p>(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    target: Option<&Target>,
    effects: &mut Option<plico_renderer::shader::ShaderRenderer>,
    blitter: &mut Option<crate::blit::Blitter>,
    encoder: &'p mut wgpu::CommandEncoder,
    frame_view: &wgpu::TextureView,
    rect: [f32; 4],
    effect: &plico_renderer::ShaderEffect,
    size: [u32; 2],
) -> wgpu::RenderPass<'p> {
    let [x, y, w, h] = rect;
    let target = target.expect("backdrop steps only run with a live target");
    let region = wgpu::Extent3d {
        width: (w as u32)
            .min(size[0].saturating_sub(x.max(0.) as u32))
            .max(1),
        height: (h as u32)
            .min(size[1].saturating_sub(y.max(0.) as u32))
            .max(1),
        depth_or_array_layers: 1,
    };
    let backdrop = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Plico effect backdrop copy"),
        size: region,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    encoder.copy_texture_to_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &target.texture,
            mip_level: 0,
            origin: wgpu::Origin3d {
                x: x.max(0.) as u32,
                y: y.max(0.) as u32,
                z: 0,
            },
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyTextureInfo {
            texture: &backdrop,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        region,
    );
    let backdrop_view = backdrop.create_view(&wgpu::TextureViewDescriptor::default());
    let effects = effects.get_or_insert_with(|| {
        let mut effects = plico_renderer::shader::ShaderRenderer::new(device);
        effects.begin_frame();
        effects
    });
    let output = effects.render_view_with_backdrop(effect, &backdrop_view, device, queue, encoder);
    // Blit the effect output into the frame at the instance's rect: a
    // fullscreen premultiplied blit, scissored to the rect.
    let format = target.texture.format();
    if blitter.as_ref().is_none_or(|b| b.format != format) {
        *blitter = Some(crate::blit::Blitter::new(device, format));
    }
    let blitter = blitter.as_mut().expect("blitter just created");
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Plico backdrop effect composite"),
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
        pass.set_scissor_rect(
            x.max(0.) as u32,
            y.max(0.) as u32,
            region.width,
            region.height,
        );
        blitter.encode_scissored(device, &output, pass);
    }
    // Continue the frame in a fresh pass over the same target.
    encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some("Plico quad draw (continued)"),
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
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "requires a working wgpu adapter"]
    fn accepted_material_replacement_invalidates_layers_and_damage_but_rejection_does_not() {
        use plico_renderer::{PaintScene, Scene};
        let mut renderer = pollster::block_on(Renderer::new()).unwrap();
        let mut scene = Scene::new();
        scene.push_layer(
            peniko::Compose::Plus,
            0.5,
            kurbo::Affine::IDENTITY,
            &kurbo::Rect::new(0., 0., 16., 16.),
            None,
            None,
        );
        scene.fill(
            peniko::Fill::NonZero,
            kurbo::Affine::IDENTITY,
            peniko::Color::WHITE,
            None,
            &kurbo::Rect::new(0., 0., 16., 16.),
        );
        scene.pop_layer();
        let original = renderer
            .render_scene(&scene, [32, 32], [0.; 4], true)
            .unwrap();
        assert_eq!(
            renderer
                .render_scene(&scene, [32, 32], [0.; 4], true)
                .unwrap(),
            original
        );
        assert_eq!(renderer.scene_damage_coverage([32, 32]), 0.);
        let layers = renderer.layer_pool_stats();
        assert!(layers.entries > 0);
        let compilations = renderer.pipeline_compilations();
        assert!(pollster::block_on(renderer.set_material("invalid WGSL")).is_err());
        assert!(!pollster::block_on(renderer.set_material(DEFAULT_MATERIAL)).unwrap());
        assert_eq!(renderer.layer_pool_stats().entries, layers.entries);
        assert_eq!(renderer.pipeline_compilations(), compilations);
        assert_eq!(
            renderer
                .render_scene(&scene, [32, 32], [0.; 4], true)
                .unwrap(),
            original
        );
        assert_eq!(renderer.scene_damage_coverage([32, 32]), 0.);
        assert!(
            pollster::block_on(
                renderer.set_material("fn plico_material(m: Material)->vec4f {return m.color;}")
            )
            .unwrap()
        );
        assert_eq!(renderer.layer_pool_stats().entries, 0);
        assert!(renderer.plus_pipeline.is_none());
        assert_eq!(
            renderer
                .render_scene(&scene, [32, 32], [0.; 4], true)
                .unwrap(),
            original
        );
        assert_eq!(renderer.scene_damage_coverage([32, 32]), 1.);
        assert!(renderer.layer_pool_stats().allocations > layers.allocations);
        assert!(renderer.plus_pipeline.is_some());
    }

    #[test]
    #[ignore = "requires a working wgpu adapter"]
    fn cpu_mip_fallback_matches_gpu_for_borrowed_and_converted_uploads() {
        use peniko::{Blob, ImageAlphaType, ImageData, ImageFormat};
        use std::sync::Arc;
        let mut gpu = pollster::block_on(Renderer::new()).unwrap();
        let mut cpu = pollster::block_on(Renderer::new()).unwrap();
        cpu.images.force_cpu_mips();
        for format in [ImageFormat::Rgba8, ImageFormat::Bgra8] {
            for alpha in [ImageAlphaType::Alpha, ImageAlphaType::AlphaPremultiplied] {
                let source: Vec<u8> = (0..65 * 67)
                    .flat_map(|i| {
                        let mut pixel = [((i * 13) % 128) as u8, 20, 3, 128];
                        if format == ImageFormat::Bgra8 {
                            pixel.swap(0, 2);
                        }
                        pixel
                    })
                    .collect();
                let image = crate::image::ImageQuad {
                    quality: peniko::ImageQuality::High,
                    ..crate::image::ImageQuad::new(
                        Quad::new([0., 0., 7., 5.], [1.; 4]),
                        ImageData {
                            data: Blob::new(Arc::new(source)),
                            width: 65,
                            height: 67,
                            format,
                            alpha_type: alpha,
                        },
                    )
                };
                let expected = cpu
                    .render_items(&[crate::Draw::Image(&image)], [7, 5], [0.; 4], 0.)
                    .unwrap();
                let actual = gpu
                    .render_items(&[crate::Draw::Image(&image)], [7, 5], [0.; 4], 0.)
                    .unwrap();
                assert!(
                    actual.iter().zip(expected).all(|(a, b)| a.abs_diff(b) <= 3),
                    "{format:?} {alpha:?}"
                );
                assert!(
                    actual
                        .as_chunks::<4>()
                        .0
                        .iter()
                        .all(|pixel| pixel[3] == 128)
                );
            }
        }
    }

    #[test]
    #[ignore = "manual GPU staging comparison; run with --ignored --nocapture"]
    fn profile_explicit_staging_against_queue_write_texture() {
        use std::time::Instant;
        let renderer = pollster::block_on(Renderer::new()).unwrap();
        eprintln!("adapter: {:?}", renderer.adapter_info());
        for [width, height] in [[4096u32, 3072u32], [4095, 3071]] {
            let source: Vec<u8> = (0..width as usize * height as usize * 4)
                .map(|i| (i % 251) as u8)
                .collect();
            for mapped in [false, true] {
                let mut samples = Vec::new();
                for index in 0..7 {
                    let oracle = renderer.device.create_buffer(&wgpu::BufferDescriptor {
                        label: None,
                        size: 768,
                        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                        mapped_at_creation: false,
                    });
                    renderer
                        .device
                        .poll(wgpu::PollType::wait_indefinitely())
                        .unwrap();
                    let start = Instant::now();
                    let texture = renderer.device.create_texture(&wgpu::TextureDescriptor {
                        label: Some("upload staging comparison"),
                        size: wgpu::Extent3d {
                            width,
                            height,
                            depth_or_array_layers: 1,
                        },
                        mip_level_count: 1,
                        sample_count: 1,
                        dimension: wgpu::TextureDimension::D2,
                        format: wgpu::TextureFormat::Rgba8Unorm,
                        usage: wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::COPY_SRC,
                        view_formats: &[],
                    });
                    let mut encoder = renderer.device.create_command_encoder(&Default::default());
                    let destination = wgpu::TexelCopyTextureInfo {
                        texture: &texture,
                        mip_level: 0,
                        origin: wgpu::Origin3d::ZERO,
                        aspect: wgpu::TextureAspect::All,
                    };
                    if mapped {
                        let pitch = (width * 4).div_ceil(256) * 256;
                        let upload = renderer.device.create_buffer(&wgpu::BufferDescriptor {
                            label: Some("explicit staging comparison"),
                            size: u64::from(pitch) * u64::from(height),
                            usage: wgpu::BufferUsages::COPY_SRC,
                            mapped_at_creation: true,
                        });
                        {
                            let mut data = upload.slice(..).get_mapped_range_mut().unwrap();
                            for (row, input) in source.chunks_exact(width as usize * 4).enumerate()
                            {
                                let start = row * pitch as usize;
                                data.slice(start..start + input.len())
                                    .copy_from_slice(input);
                            }
                        }
                        upload.unmap();
                        encoder.copy_buffer_to_texture(
                            wgpu::TexelCopyBufferInfo {
                                buffer: &upload,
                                layout: wgpu::TexelCopyBufferLayout {
                                    offset: 0,
                                    bytes_per_row: Some(pitch),
                                    rows_per_image: None,
                                },
                            },
                            destination,
                            texture.size(),
                        );
                    } else {
                        renderer.queue.write_texture(
                            destination,
                            &source,
                            wgpu::TexelCopyBufferLayout {
                                offset: 0,
                                bytes_per_row: Some(width * 4),
                                rows_per_image: None,
                            },
                            texture.size(),
                        );
                    }
                    let points = [[0, 0], [width / 2, height / 2], [width - 1, height - 1]];
                    for (slot, [x, y]) in points.into_iter().enumerate() {
                        encoder.copy_texture_to_buffer(
                            wgpu::TexelCopyTextureInfo {
                                texture: &texture,
                                mip_level: 0,
                                origin: wgpu::Origin3d { x, y, z: 0 },
                                aspect: wgpu::TextureAspect::All,
                            },
                            wgpu::TexelCopyBufferInfo {
                                buffer: &oracle,
                                layout: wgpu::TexelCopyBufferLayout {
                                    offset: slot as u64 * 256,
                                    bytes_per_row: Some(256),
                                    rows_per_image: None,
                                },
                            },
                            wgpu::Extent3d {
                                width: 1,
                                height: 1,
                                depth_or_array_layers: 1,
                            },
                        );
                    }
                    renderer.queue.submit([encoder.finish()]);
                    let cpu = start.elapsed();
                    let start = Instant::now();
                    let (tx, rx) = std::sync::mpsc::channel();
                    oracle
                        .slice(..)
                        .map_async(wgpu::MapMode::Read, move |result| {
                            tx.send(result).unwrap();
                        });
                    renderer
                        .device
                        .poll(wgpu::PollType::wait_indefinitely())
                        .unwrap();
                    rx.recv().unwrap().unwrap();
                    let wait = start.elapsed();
                    let actual = oracle.slice(..).get_mapped_range().unwrap();
                    for (slot, [x, y]) in points.into_iter().enumerate() {
                        let offset = (y as usize * width as usize + x as usize) * 4;
                        assert_eq!(
                            &actual[slot * 256..slot * 256 + 4],
                            &source[offset..offset + 4]
                        );
                    }
                    if index == 0 {
                        eprintln!(
                            "{width}x{height} mapped={mapped} first: cpu={cpu:?}, wait={wait:?}"
                        );
                    } else {
                        samples.push((cpu, wait));
                    }
                }
                samples.sort_by_key(|sample| sample.0);
                let mut waits: Vec<_> = samples.iter().map(|sample| sample.1).collect();
                waits.sort();
                let mut totals: Vec<_> = samples.iter().map(|sample| sample.0 + sample.1).collect();
                totals.sort();
                eprintln!(
                    "{width}x{height} mapped={mapped} paired_total_upper_median={:?}",
                    totals[3]
                );
                eprintln!(
                    "{width}x{height} mapped={mapped} warm n=6: cpu_upper_median={:?}, wait_upper_median={:?}",
                    samples[3].0, waits[3]
                );
            }
        }
    }

    #[test]
    #[ignore = "manual GPU upload profile; run with --ignored --nocapture"]
    fn profile_image_upload_cpu_and_gpu_wait() {
        let _ = tracing_subscriber::fmt()
            .with_env_filter(
                tracing_subscriber::EnvFilter::try_from_default_env()
                    .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("off")),
            )
            .with_span_events(tracing_subscriber::fmt::format::FmtSpan::CLOSE)
            .with_ansi(false)
            .with_test_writer()
            .try_init();
        use peniko::{Blob, ImageAlphaType, ImageData, ImageFormat};
        use std::{sync::Arc, time::Instant};
        let mut renderer = pollster::block_on(Renderer::new()).unwrap();
        assert!(crate::mipmap::MipGenerator::supported(&renderer.device));
        eprintln!("adapter: {:?}", renderer.adapter_info());
        for size in [[4096, 3072], [4095, 3071]] {
            let pixels = vec![128u8; size[0] as usize * size[1] as usize * 4];
            for (mode, cpu, mipmaps) in [
                ("base-only", false, false),
                ("cpu-mips", true, true),
                ("gpu-mips", false, true),
            ] {
                renderer.trim_image_cache();
                // Reset the test override and pipeline outside the measured interval.
                renderer.images = Default::default();
                if cpu {
                    renderer.images.force_cpu_mips();
                }
                let mut samples = Vec::new();
                for index in 0..7 {
                    let source = ImageData {
                        data: Blob::new(Arc::new(pixels.clone())),
                        width: size[0],
                        height: size[1],
                        format: ImageFormat::Rgba8,
                        alpha_type: ImageAlphaType::AlphaPremultiplied,
                    };
                    renderer
                        .device
                        .poll(wgpu::PollType::wait_indefinitely())
                        .unwrap();
                    let start = Instant::now();
                    let image = renderer
                        .images
                        .get(
                            &renderer.device,
                            &renderer.queue,
                            &renderer.atlas_texture.layout,
                            &source,
                            crate::image::ImageSampling {
                                quality: peniko::ImageQuality::High,
                                extend: [peniko::Extend::Pad; 2],
                                mipmaps,
                            },
                        )
                        .unwrap();
                    // Flush write_texture staging on the CPU-only mip path too.
                    renderer.queue.submit([]);
                    let cpu_time = start.elapsed();
                    let wait = Instant::now();
                    renderer
                        .device
                        .poll(wgpu::PollType::wait_indefinitely())
                        .unwrap();
                    let gpu_wait = wait.elapsed();
                    std::hint::black_box(image);
                    if index == 0 {
                        eprintln!(
                            "{size:?} mode={mode} first: cpu={cpu_time:?}, subsequent_wait={gpu_wait:?}"
                        );
                    } else {
                        samples.push((cpu_time, gpu_wait));
                    }
                }
                samples.sort_by_key(|sample| sample.0);
                let mut waits: Vec<_> = samples.iter().map(|sample| sample.1).collect();
                waits.sort();
                eprintln!(
                    "{size:?} mode={mode} warm n=6: cpu_upper_median={:?}, wait_upper_median={:?}",
                    samples[3].0, waits[3]
                );
            }
        }
    }

    #[test]
    #[ignore = "requires a working wgpu adapter"]
    fn gpu_mip_levels_match_cpu_area_reference_and_preserve_premultiplication() {
        let renderer = pollster::block_on(Renderer::new()).unwrap();
        assert!(crate::mipmap::MipGenerator::supported(&renderer.device));
        let generator = crate::mipmap::MipGenerator::new(&renderer.device);
        for size in [[64u32, 64u32], [65, 67], [257, 193], [1, 129], [3, 1]] {
            let source: Vec<u8> = (0..size[0] * size[1])
                .flat_map(|i| {
                    let alpha = (i % 251) as u8;
                    [
                        ((i * 17) % (u32::from(alpha) + 1)) as u8,
                        alpha / 2,
                        0,
                        alpha,
                    ]
                })
                .collect();
            let levels = 32 - size[0].max(size[1]).leading_zeros();
            let texture = renderer.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("mip CPU oracle"),
                size: wgpu::Extent3d {
                    width: size[0],
                    height: size[1],
                    depth_or_array_layers: 1,
                },
                mip_level_count: levels,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::TEXTURE_BINDING
                    | wgpu::TextureUsages::STORAGE_BINDING
                    | wgpu::TextureUsages::COPY_DST
                    | wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            });
            renderer.queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                &source,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(size[0] * 4),
                    rows_per_image: None,
                },
                texture.size(),
            );
            generator.generate(&renderer.device, &renderer.queue, &texture);
            let mut previous_gpu = source.clone();
            let mut reference = source;
            let mut previous_size = size;
            for level in 1..levels {
                let target = [(size[0] >> level).max(1), (size[1] >> level).max(1)];
                let pitch = (target[0] * 4).div_ceil(256) * 256;
                let buffer = renderer.device.create_buffer(&wgpu::BufferDescriptor {
                    label: None,
                    size: u64::from(pitch) * u64::from(target[1]),
                    usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                    mapped_at_creation: false,
                });
                let mut encoder = renderer.device.create_command_encoder(&Default::default());
                encoder.copy_texture_to_buffer(
                    wgpu::TexelCopyTextureInfo {
                        texture: &texture,
                        mip_level: level,
                        origin: wgpu::Origin3d::ZERO,
                        aspect: wgpu::TextureAspect::All,
                    },
                    wgpu::TexelCopyBufferInfo {
                        buffer: &buffer,
                        layout: wgpu::TexelCopyBufferLayout {
                            offset: 0,
                            bytes_per_row: Some(pitch),
                            rows_per_image: None,
                        },
                    },
                    wgpu::Extent3d {
                        width: target[0],
                        height: target[1],
                        depth_or_array_layers: 1,
                    },
                );
                renderer.queue.submit([encoder.finish()]);
                let (tx, rx) = std::sync::mpsc::channel();
                buffer
                    .slice(..)
                    .map_async(wgpu::MapMode::Read, move |result| {
                        tx.send(result).unwrap();
                    });
                renderer
                    .device
                    .poll(wgpu::PollType::wait_indefinitely())
                    .unwrap();
                rx.recv().unwrap().unwrap();
                let mapped = buffer.slice(..).get_mapped_range().unwrap();
                let actual: Vec<u8> = mapped
                    .chunks_exact(pitch as usize)
                    .flat_map(|row| row[..target[0] as usize * 4].iter().copied())
                    .collect();
                let step = crate::image::area_reduce(&previous_gpu, previous_size, target);
                reference = crate::image::area_reduce(&reference, previous_size, target);
                assert!(
                    actual.iter().zip(&step).all(|(a, b)| a.abs_diff(*b) <= 1),
                    "step {size:?} level {level}"
                );
                assert!(
                    actual
                        .iter()
                        .zip(&reference)
                        .all(|(a, b)| a.abs_diff(*b) <= 3),
                    "chain {size:?} level {level}"
                );
                for pixel in actual.as_chunks::<4>().0 {
                    assert!(pixel[..3].iter().all(|c| *c <= pixel[3]));
                }
                previous_gpu = actual;
                previous_size = target;
            }
        }
    }

    #[test]
    #[ignore = "requires a working wgpu adapter"]
    fn readback_trim_preserves_target_and_recreates_only_for_capture() {
        let mut renderer = pollster::block_on(Renderer::new()).unwrap();
        assert_eq!(renderer.target_storage_bytes(), (0, 0));
        let quad = Quad::new([0., 0., 17., 19.], [1., 0., 0., 1.]);
        let expected = renderer.render(&[quad], [17, 19], [0.; 4], 0.).unwrap();
        assert_eq!(renderer.target_storage_bytes(), (17 * 19 * 4, 256 * 19));
        let view = renderer.target.as_ref().unwrap().view.clone();
        renderer.trim_readback_buffer();
        assert_eq!(renderer.target_storage_bytes(), (17 * 19 * 4, 0));
        let encoder = renderer
            .encode_frame(&[crate::Draw::Quad(&quad)], [17, 19], [0.; 4], 0.)
            .unwrap();
        renderer.queue.submit([encoder.finish()]);
        assert_eq!(renderer.target_storage_bytes(), (17 * 19 * 4, 0));
        assert_eq!(renderer.target.as_ref().unwrap().view, view);
        assert_eq!(
            renderer.render(&[quad], [17, 19], [0.; 4], 0.).unwrap(),
            expected
        );
        assert_eq!(renderer.target_storage_bytes(), (17 * 19 * 4, 256 * 19));
        renderer.trim_readback_buffer();
        renderer.trim_readback_buffer();
        assert_eq!(renderer.target_storage_bytes().1, 0);
    }

    #[test]
    #[ignore = "requires a working wgpu adapter"]
    fn encoded_frames_release_temporary_image_owners_on_success_and_failure() {
        use crate::{Draw, image::ImageQuad};
        use peniko::{Blob, ImageAlphaType, ImageData, ImageFormat};
        use std::sync::Arc;
        let mut renderer = pollster::block_on(Renderer::new()).unwrap();
        let image = ImageQuad::new(
            Quad::new([0., 0., 8., 8.], [1.; 4]),
            ImageData {
                data: Blob::new(Arc::new(vec![255u8, 0, 0, 255])),
                width: 1,
                height: 1,
                format: ImageFormat::Rgba8,
                alpha_type: ImageAlphaType::AlphaPremultiplied,
            },
        );
        let pixels = renderer
            .render_items(&[Draw::Image(&image)], [16, 16], [0.; 4], 0.)
            .unwrap();
        assert_eq!(&pixels[..4], &[255, 0, 0, 255]);
        assert!(renderer.frame_images.is_empty());
        assert!(renderer.frame_image_slots.is_empty());
        // A second upload fails after the first image has entered the pass table.
        let mut truncated = ImageQuad::new(image.quad, image.image.clone());
        truncated.image.width = 2;
        assert!(
            renderer
                .encode_frame(
                    &[Draw::Image(&image), Draw::Image(&truncated)],
                    [16, 16],
                    [0.; 4],
                    0.,
                )
                .is_err()
        );
        assert!(renderer.frame_images.is_empty());
        assert!(renderer.frame_image_slots.is_empty());
        assert_eq!(
            renderer
                .render_items(&[Draw::Image(&image)], [16, 16], [0.; 4], 0.)
                .unwrap(),
            pixels
        );
    }

    #[test]
    #[ignore = "requires a working wgpu adapter"]
    fn scene_present_skip_requires_valid_retained_target() {
        use plico_renderer::Scene;
        let mut renderer = pollster::block_on(Renderer::new()).unwrap();
        let texture = renderer.device.create_texture(&wgpu::TextureDescriptor {
            label: None,
            size: wgpu::Extent3d {
                width: 32,
                height: 32,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let view = texture.create_view(&Default::default());
        let scene = Scene::new();
        let draw = |renderer: &mut Renderer, background| {
            renderer
                .render_scene_to_view(
                    &scene,
                    [32, 32],
                    background,
                    true,
                    &view,
                    wgpu::TextureFormat::Rgba8Unorm,
                )
                .unwrap()
        };
        assert!(draw(&mut renderer, [0.; 4]));
        assert!(!draw(&mut renderer, [0.; 4]));
        renderer.invalidate_scene_damage();
        assert!(draw(&mut renderer, [0.; 4]));
        assert!(draw(&mut renderer, [1.; 4]));
        renderer.render_items(&[], [32, 32], [0.; 4], 0.).unwrap();
        assert!(draw(&mut renderer, [1.; 4]));
        assert!(!draw(&mut renderer, [1.; 4]));
    }

    #[test]
    #[ignore = "requires a working wgpu adapter"]
    fn gpu_present_preserves_encoded_pixels_on_unorm_and_srgb_targets() {
        let mut renderer = pollster::block_on(Renderer::new()).unwrap();
        let mut q = Quad::new([2.25, 3.5, 24., 23.], [0.15, 0.35, 0.65, 0.7]);
        q.radii = [5.; 4];
        q.colors[1] = [0.65, 0.2, 0.1, 0.3];
        let mut previous: Option<([u32; 2], wgpu::TextureView)> = None;
        for (index, (size, format)) in [
            ([32, 32], wgpu::TextureFormat::Rgba8Unorm),
            ([32, 32], wgpu::TextureFormat::Rgba8Unorm),
            ([64, 48], wgpu::TextureFormat::Rgba8Unorm),
            ([32, 32], wgpu::TextureFormat::Rgba8UnormSrgb),
            ([32, 32], wgpu::TextureFormat::Bgra8Unorm),
            ([32, 32], wgpu::TextureFormat::Bgra8UnormSrgb),
        ]
        .into_iter()
        .enumerate()
        {
            // Changed content catches a stale binding after target replacement.
            q.colors[0][0] = 0.1 + index as f32 * 0.1;
            let expected = renderer.render(&[q], size, [0.; 4], 0.).unwrap();
            let source = renderer.target.as_ref().unwrap().view.clone();
            if let Some((old_size, old_view)) = &previous {
                assert_eq!(*old_size == size, *old_view == source);
            }
            previous = Some((size, source));
            let texture = renderer.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("present oracle"),
                size: wgpu::Extent3d {
                    width: size[0],
                    height: size[1],
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            });
            renderer
                .render_to_view(
                    &[crate::Draw::Quad(&q)],
                    size,
                    [0.; 4],
                    0.,
                    &texture.create_view(&Default::default()),
                    format,
                )
                .unwrap();
            let buffer = renderer.device.create_buffer(&wgpu::BufferDescriptor {
                label: None,
                size: 256 * u64::from(size[1]),
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                mapped_at_creation: false,
            });
            let mut encoder = renderer.device.create_command_encoder(&Default::default());
            encoder.copy_texture_to_buffer(
                wgpu::TexelCopyTextureInfo {
                    texture: &texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                wgpu::TexelCopyBufferInfo {
                    buffer: &buffer,
                    layout: wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(256),
                        rows_per_image: Some(size[1]),
                    },
                },
                wgpu::Extent3d {
                    width: size[0],
                    height: size[1],
                    depth_or_array_layers: 1,
                },
            );
            renderer.queue.submit([encoder.finish()]);
            let (tx, rx) = std::sync::mpsc::channel();
            buffer.slice(..).map_async(wgpu::MapMode::Read, move |r| {
                tx.send(r).unwrap();
            });
            renderer
                .device
                .poll(wgpu::PollType::wait_indefinitely())
                .unwrap();
            rx.recv().unwrap().unwrap();
            let data = buffer.slice(..).get_mapped_range().unwrap();
            for (y, row) in data.as_chunks::<256>().0.iter().enumerate() {
                for (x, pixel) in row[..size[0] as usize * 4]
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .enumerate()
                {
                    let mut actual: [u8; 4] = *pixel;
                    if matches!(
                        format,
                        wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Bgra8UnormSrgb
                    ) {
                        actual.swap(0, 2);
                    }
                    for c in 0..4 {
                        assert!(
                            actual[c].abs_diff(expected[(y * size[0] as usize + x) * 4 + c]) <= 1,
                            "{format:?} pixel {x},{y} differs: {actual:?}"
                        );
                    }
                }
            }
            drop(data);
            buffer.unmap();
        }
    }
    #[test]
    #[ignore = "requires a working wgpu adapter"]
    fn tiled_gaussian_matches_direct_sampling_for_edges_and_partial_tiles() {
        use kurbo::{Affine, Rect};
        use peniko::{BlendMode, Color, Fill};
        use plico_renderer::{
            PaintScene, Scene,
            filter::{BlurEdge, GaussianBlur},
        };
        use std::sync::Arc;
        let mut tiled = pollster::block_on(Renderer::new()).unwrap();
        let mut reference = pollster::block_on(Renderer::new()).unwrap();
        tiled.blur = Some(crate::blur::Blur::tiled(&tiled.device));
        for sigma in [0.3, 3., 18., 64.] {
            for edge in [
                BlurEdge::Transparent,
                BlurEdge::Clamp,
                BlurEdge::Repeat,
                BlurEdge::Mirror,
            ] {
                let mut scene = Scene::new();
                scene.push_layer(
                    BlendMode::default(),
                    0.7,
                    Affine::IDENTITY,
                    &Rect::new(1.5, 2.25, 46.5, 30.75),
                    Some(Arc::new(GaussianBlur { sigma, edge }.into_filter())),
                    None,
                );
                scene.fill(
                    Fill::NonZero,
                    Affine::IDENTITY,
                    Color::from_rgba8(200, 30, 100, 173),
                    None,
                    &Rect::new(-200., -300., 17.5, 400.),
                );
                scene.pop_layer();
                let a = tiled
                    .render_scene(&scene, [49, 35], [0.; 4], false)
                    .unwrap();
                let b = reference
                    .render_scene(&scene, [49, 35], [0.; 4], false)
                    .unwrap();
                for (i, (a, b)) in a.iter().zip(&b).enumerate() {
                    assert!(
                        a.abs_diff(*b) <= 1,
                        "sigma={sigma} edge={edge:?} byte={i}: {a}/{b}"
                    );
                }
            }
        }
    }
    #[test]
    #[ignore = "manual GPU timing experiment; not a CI performance threshold"]
    fn profile_gaussian_workspace_cost() {
        use kurbo::{Affine, Rect};
        use peniko::{BlendMode, Color, Fill};
        use plico_renderer::{
            PaintScene, Scene,
            filter::{BlurEdge, GaussianBlur},
        };
        use std::{sync::Arc, time::Instant};
        fn percentiles(values: &mut [f64]) -> (f64, f64) {
            values.sort_by(f64::total_cmp);
            (
                values[values.len() / 2],
                values[(values.len() * 95 / 100).min(values.len() - 1)],
            )
        }
        let instance = wgpu::Instance::new(crate::instance_descriptor());
        let mut renderer =
            pollster::block_on(Renderer::new_compatible(&instance, None, true)).unwrap();
        if std::env::var_os("PLICO_TEST_TILED_BLUR").is_some() {
            renderer.blur = Some(crate::blur::Blur::tiled(&renderer.device));
            eprintln!("test-only tiled kernel selected");
        }
        eprintln!("adapter={:?}", renderer.adapter_info());
        for (width, height, sigma) in [(128, 96, 3.), (384, 256, 3.), (384, 256, 18.)] {
            let size = [width, height];
            let texture = renderer.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("Gaussian timing destination"),
                size: wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[],
            });
            let view = texture.create_view(&Default::default());
            // Build outside all timing intervals. Both policies replay identical
            // scene snapshots; neither performs CPU readback or swapchain present.
            let scenes: Vec<_> = (0..32)
                .map(|frame| {
                    let mut scene = Scene::new();
                    scene.push_layer(
                        BlendMode::default(),
                        0.8,
                        Affine::IDENTITY,
                        &Rect::new(8., 8., width as f64 - 8., height as f64 - 8.),
                        Some(Arc::new(
                            GaussianBlur {
                                sigma,
                                edge: BlurEdge::Transparent,
                            }
                            .into_filter(),
                        )),
                        None,
                    );
                    scene.fill(
                        Fill::NonZero,
                        Affine::IDENTITY,
                        Color::from_rgb8(50, 120, 230),
                        None,
                        &Rect::new(
                            0.,
                            0.,
                            width as f64 * 0.6 + frame as f64 * 0.25,
                            height as f64,
                        ),
                    );
                    scene.pop_layer();
                    scene
                })
                .collect();
            // Reversed second pair reduces attribution to first-run warming.
            for clear in [false, true, true, false] {
                renderer.trim_layer_pool();
                for scene in scenes.iter().take(8) {
                    if clear {
                        renderer.trim_layer_pool();
                    }
                    renderer
                        .render_scene_to_view(
                            scene,
                            size,
                            [1.; 4],
                            false,
                            &view,
                            wgpu::TextureFormat::Rgba8Unorm,
                        )
                        .unwrap();
                    renderer
                        .device
                        .poll(wgpu::PollType::wait_indefinitely())
                        .unwrap();
                }
                let before = renderer.layer_pool_stats();
                let mut submission = Vec::new();
                let mut completion = Vec::new();
                let mut gpu = Vec::new();
                for scene in &scenes {
                    let start = Instant::now();
                    if clear {
                        renderer.trim_layer_pool();
                    }
                    renderer
                        .render_scene_to_view(
                            scene,
                            size,
                            [1.; 4],
                            false,
                            &view,
                            wgpu::TextureFormat::Rgba8Unorm,
                        )
                        .unwrap();
                    submission.push(start.elapsed().as_secs_f64() * 1000.);
                    renderer
                        .device
                        .poll(wgpu::PollType::wait_indefinitely())
                        .unwrap();
                    completion.push(start.elapsed().as_secs_f64() * 1000.);
                    if let Some(time) = renderer.layer_pool.iter().flatten().find_map(|entry| {
                        entry
                            .blur
                            .as_ref()
                            .and_then(|(work, _)| work.timing.as_ref())
                    }) {
                        let (tx, rx) = std::sync::mpsc::channel();
                        time.readback
                            .slice(..)
                            .map_async(wgpu::MapMode::Read, move |r| {
                                let _ = tx.send(r);
                            });
                        renderer
                            .device
                            .poll(wgpu::PollType::wait_indefinitely())
                            .unwrap();
                        rx.recv().unwrap().unwrap();
                        let data = time.readback.slice(..).get_mapped_range().unwrap();
                        let ticks: Vec<_> = data
                            .as_chunks::<8>()
                            .0
                            .iter()
                            .map(|b| u64::from_ne_bytes(*b))
                            .collect();
                        gpu.push(
                            (ticks[1].wrapping_sub(ticks[0]) as f64
                                + ticks[3].wrapping_sub(ticks[2]) as f64)
                                * renderer.queue.get_timestamp_period() as f64
                                / 1e6,
                        );
                        drop(data);
                        time.readback.unmap();
                    }
                }
                let stats = renderer.layer_pool_stats();
                let (submit50, submit95) = percentiles(&mut submission);
                let (done50, done95) = percentiles(&mut completion);
                if !gpu.is_empty() {
                    let (p50, p95) = percentiles(&mut gpu);
                    eprintln!(
                        "blur_gpu_ms {width}x{height} sigma={sigma} clear={clear} p50={p50:.3} p95={p95:.3}"
                    );
                } else {
                    eprintln!("GPU timestamps unavailable on this adapter");
                }
                eprintln!(
                    "blur {width}x{height} sigma={sigma} clear={clear} submission_ms p50={submit50:.3} p95={submit95:.3} completion_ms p50={done50:.3} p95={done95:.3} workspace_allocations={} resident_bytes={}",
                    stats.filter_allocations - before.filter_allocations,
                    stats.retained_bytes
                );
            }
        }
    }
}

#[cfg(all(test, target_os = "linux", feature = "system-text"))]
mod atlas_tests {
    use super::*;
    #[test]
    #[ignore = "requires system fonts and a working wgpu adapter"]
    fn child_layer_cannot_recycle_parent_glyph_uvs() {
        use kurbo::{Affine, Rect};
        use plico_renderer::{
            PaintCommand, PaintScene, Scene, paragraph::ParagraphRequest, style::Paint,
        };
        use std::sync::Arc;
        let mut renderer = pollster::block_on(Renderer::new()).unwrap();
        let request = |text: &str, color| {
            Arc::new(
                ParagraphRequest::from_paint(
                    &Paint {
                        text: Some(text.into()),
                        font_size: 1000.,
                        text_color: color,
                        ..Default::default()
                    },
                    None,
                )
                .unwrap(),
            )
        };
        let first = request("W", peniko::Color::from_rgba8(200, 0, 0, 255));
        let second = request("Z", peniko::Color::from_rgba8(0, 0, 200, 255));
        let w = renderer.paragraph_layout(first.clone(), 1.).unwrap();
        let z = renderer.paragraph_layout(second.clone(), 1.).unwrap();
        let origin = |layout: &crate::text::GlyphLayout, x: f64| {
            let glyph = &layout.glyphs[0];
            Affine::translate((
                x - glyph.x - glyph.ink_bounds[0],
                8. - glyph.y - glyph.ink_bounds[1],
            ))
        };
        let mut scene = Scene::new();
        scene.commands.push(PaintCommand::Paragraph {
            request: first,
            transform: origin(&w, 8.),
        });
        scene.push_layer(
            peniko::BlendMode::default(),
            0.5,
            Affine::IDENTITY,
            &Rect::new(0., 0., 256., 64.),
            None,
            None,
        );
        scene.commands.push(PaintCommand::Paragraph {
            request: second,
            transform: origin(&z, 64.),
        });
        scene.pop_layer();
        let expected = renderer
            .render_scene(&scene, [256, 64], [1.; 4], false)
            .unwrap();
        let mut fill = crate::text::TextRequest::new("MNABCDEFGH");
        fill.size = 1000.;
        let fill = renderer.layout_text(&fill).unwrap();
        let atlas = renderer.atlas.as_mut().unwrap();
        atlas.clear();
        atlas.get(&w.glyphs[0], 0).unwrap();
        let mut full = false;
        'fill: for glyph in &fill.glyphs {
            for phase in 0..4 {
                match atlas.get(glyph, phase) {
                    Ok(_) => {}
                    Err(plico_native::text::Error::AtlasFull) => {
                        full = true;
                        break 'fill;
                    }
                    Err(error) => panic!("{error}"),
                }
            }
        }
        assert!(full);
        let actual = renderer
            .render_scene(&scene, [256, 64], [1.; 4], false)
            .unwrap();
        assert_eq!(
            actual, expected,
            "child layer invalidated already prepared parent UVs"
        );
        assert!(renderer.text_stats().recycled_frames > 0);
    }

    #[test]
    #[ignore = "requires system fonts and a working wgpu adapter"]
    fn full_atlas_recycles_before_draw_without_stale_uvs() {
        use crate::{
            Draw,
            text::{TextRequest, TextRun},
        };
        let mut renderer = pollster::block_on(Renderer::new()).unwrap();
        let mut first = TextRequest::new("W");
        first.size = 1000.;
        let layout = renderer.layout_text(&first).unwrap();
        let glyph = &layout.glyphs[0];
        let origin = [
            8. - glyph.x - glyph.ink_bounds[0],
            8. - glyph.y - glyph.ink_bounds[1],
        ];
        let draws = [Draw::Text(TextRun::new(&layout, origin, [0., 0., 0., 1.]))];
        let expected = renderer
            .render_items(&draws, [256, 64], [1.; 4], 0.)
            .unwrap();
        let mut request = TextRequest::new("MNABCDEFGH");
        request.size = 1000.;
        let large = renderer.layout_text(&request).unwrap();
        let atlas = renderer.atlas.as_mut().unwrap();
        // Simulate another document evicting the first frame's glyphs before
        // filling the fixed storage; the positioned layout itself stays alive.
        atlas.clear();
        let mut full = false;
        'fill: for glyph in &large.glyphs {
            for phase in 0..4 {
                match atlas.get(glyph, phase) {
                    Ok(_) => {}
                    Err(plico_native::text::Error::AtlasFull) => {
                        full = true;
                        break 'fill;
                    }
                    Err(e) => panic!("{e}"),
                }
            }
        }
        assert!(full, "fixture did not exhaust the atlas");
        let pixels = renderer
            .render_items(&draws, [256, 64], [1.; 4], 0.)
            .unwrap();
        assert_eq!(
            pixels, expected,
            "cache recycling used stale glyph rectangles"
        );
        let stats = renderer.text_stats();
        assert_eq!(stats.recycled_frames, 1);
        assert_eq!(stats.atlas_cpu_bytes, 16 * 1024 * 1024);
        assert_eq!(stats.atlas_gpu_bytes, 16 * 1024 * 1024);
    }
}

#[cfg(all(test, target_os = "linux", feature = "system-text"))]
#[path = "damage_profile.rs"]
mod damage_profile;
