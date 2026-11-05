//! Validated custom WGSL effects and their renderer-owned GPU resources.

use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};

#[cfg(not(target_arch = "wasm32"))]
use lru::LruCache;
use naga::valid::{Capabilities, ValidationFlags, Validator};
#[cfg(not(target_arch = "wasm32"))]
use rustc_hash::FxHashMap;
#[cfg(not(target_arch = "wasm32"))]
use std::num::NonZeroUsize;
#[cfg(not(target_arch = "wasm32"))]
use wgpu::{
    BindGroup, BindGroupLayout, Buffer, CommandEncoder, Device, Queue, RenderPipeline, Texture,
    TextureFormat, TextureView,
};

const MAX_SOURCE_BYTES: usize = 64 * 1024;
const PARAMETER_COUNT: usize = 256;
const UNIFORM_FLOAT_COUNT: usize = PARAMETER_COUNT + 4;
#[cfg(not(target_arch = "wasm32"))]
const PIPELINE_CACHE_SIZE: usize = 16;
#[cfg(not(target_arch = "wasm32"))]
const EFFECT_TEXTURE_FORMAT: TextureFormat = TextureFormat::Rgba8Unorm;

/// `.values()` carries finite f32 words, not arbitrary host-shared bytes. Validate
/// nested members too: accepting integer fields would reinterpret float bits.
fn validate_float_uniform(
    module: &naga::Module,
    root: naga::Handle<naga::Type>,
) -> Result<(), String> {
    let mut pending = vec![root];
    let mut visited = vec![false; module.types.len()];
    while let Some(ty) = pending.pop() {
        if std::mem::replace(&mut visited[ty.index()], true) {
            continue;
        }
        match &module.types[ty].inner {
            naga::TypeInner::Scalar(scalar)
            | naga::TypeInner::Vector { scalar, .. }
            | naga::TypeInner::Matrix { scalar, .. }
                if scalar.kind == naga::ScalarKind::Float && scalar.width == 4 => {}
            naga::TypeInner::Array { base, .. } => pending.push(*base),
            naga::TypeInner::Struct { members, .. } => {
                pending.extend(members.iter().map(|member| member.ty));
            }
            _ => {
                return Err(concat!(
                    "custom shader uniform data must contain only f32 scalars, vectors, ",
                    "matrices, arrays or structs; .values() uploads f32 words, ",
                    "so use explicit WGSL conversion for integer parameters"
                )
                .into());
            }
        }
    }
    Ok(())
}

const SHADER_PREFIX: &str = r#"
struct PlicoShaderUniforms {
    // x/y: physical output size, z: elapsed seconds, w: device scale.
    resolution_time_scale: vec4<f32>,
    values: array<vec4<f32>, 64>,
};

@group(0) @binding(0)
var<uniform> plico: PlicoShaderUniforms;

struct PlicoVertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> PlicoVertexOutput {
    var positions = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(3.0, -1.0),
        vec2<f32>(-1.0, 3.0),
    );
    let position = positions[vertex_index];
    var output: PlicoVertexOutput;
    output.position = vec4<f32>(position, 0.0, 1.0);
    output.uv = position * vec2<f32>(0.5, -0.5) + vec2<f32>(0.5, 0.5);
    return output;
}
"#;

const SHADER_SUFFIX: &str = r#"
@fragment
fn fs_main(input: PlicoVertexOutput) -> @location(0) vec4<f32> {
    return plico_effect(input.uv);
}
"#;

static NEXT_EFFECT_ID: AtomicU64 = AtomicU64::new(1);

/// Stable identity for one shader surface.
///
/// The renderer uses this identity to retain its output texture across frames
/// and drops the texture after the widget no longer contributes paint.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ShaderEffectId(u64);

impl ShaderEffectId {
    /// Allocate a process-unique shader surface identity.
    pub fn new() -> Self {
        Self(NEXT_EFFECT_ID.fetch_add(1, Ordering::Relaxed))
    }
}

impl Default for ShaderEffectId {
    fn default() -> Self {
        Self::new()
    }
}

/// Identity of a renderer-owned native shader output allocation.
/// Distinct versions of one effect in the same frame have distinct identities;
/// a retained target preserves its identity when resized. Backends may wrap
/// this value in their own external-texture ID, without exposing that type here.
#[cfg(not(target_arch = "wasm32"))]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ShaderTextureId(u64);

#[cfg(not(target_arch = "wasm32"))]
impl ShaderTextureId {
    /// Process-local identity; not a persistent asset or device handle.
    pub fn get(self) -> u64 {
        self.0
    }
}

/// A validated WGSL function compiled under the Plico shader prelude.
///
/// New source defines `fn plico_effect(uv: vec2<f32>) -> vec4<f32>` and reads
/// `plico.resolution_time_scale` and the 256 scalar values in `plico.values`.
/// Surface outputs
/// use premultiplied RGBA, matching the shared texture/SkSL compositing contract. Extra bind groups are deliberately rejected so renderer
/// resource ownership remains explicit.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
enum ShaderEffectInterface {
    PlicoEffect,
    CompleteModule,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct ParameterSlot {
    name: String,
    offset: usize,
    lanes: Option<usize>,
}

/// Validated WGSL source for a shader-layer effect.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct ShaderEffectSource {
    wgsl: Arc<str>,
    sksl: Option<Arc<str>>,
    interface: ShaderEffectInterface,
    parameters: Arc<[ParameterSlot]>,
}

impl ShaderEffectSource {
    /// Attach an optional SkSL implementation, compiled by the Skia backend.
    pub fn with_sksl(mut self, source: &str) -> Result<Self, String> {
        if source.trim().is_empty() || source.len() > MAX_SOURCE_BYTES {
            return Err("SkSL source must be nonempty and within the shader source limit".into());
        }
        self.sksl = Some(source.into());
        Ok(self)
    }
    /// Validate and retain a custom WGSL effect function.
    pub fn new(source: &str) -> Result<Self, String> {
        if source.trim().is_empty() {
            return Err("shader source must not be empty".into());
        }
        if source.len() > MAX_SOURCE_BYTES {
            return Err(format!(
                "shader source exceeds the {MAX_SOURCE_BYTES}-byte limit"
            ));
        }
        // The Plico shader prelude supplies the vertex stage, the `plico`
        // uniform and the fragment entry point; authored sources define
        // `plico_effect` against it.
        Self::validated(
            format!("{SHADER_PREFIX}\n{source}\n{SHADER_SUFFIX}"),
            ShaderEffectInterface::PlicoEffect,
        )
    }

    /// Validate a complete single-pass WGSL module.
    ///
    /// The module must expose `vs_main` and `fs_main`. Binding zero may contain
    /// an application-defined uniform struct backed by up to 256 f32 words.
    /// Scalars in nested arrays/structs, vectors and matrices must all be f32;
    /// explicit padding follows WGSL layout, not Rust struct layout.
    /// Plico overwrites floats 0, 1, and 2 with physical width, physical height,
    /// and native animation time. Textures and additional bindings remain
    /// unsupported so resource ownership stays inside the renderer. Vertex
    /// entry points must generate geometry without `@location` vertex inputs;
    /// the fullscreen pipeline does not supply vertex buffers. Fragment output
    /// must be a single `@location(0) vec4<f32>`, optionally wrapped in a struct.
    pub fn new_module(source: &str) -> Result<Self, String> {
        if source.trim().is_empty() {
            return Err("shader source must not be empty".into());
        }
        if source.len() > MAX_SOURCE_BYTES {
            return Err(format!(
                "shader source exceeds the {MAX_SOURCE_BYTES}-byte limit"
            ));
        }
        Self::validated(source.to_owned(), ShaderEffectInterface::CompleteModule)
    }

    fn validated(complete: String, interface: ShaderEffectInterface) -> Result<Self, String> {
        let module = naga::front::wgsl::parse_str(&complete)
            .map_err(|error| error.emit_to_string(&complete))?;
        Validator::new(ValidationFlags::all(), Capabilities::default())
            .validate(&module)
            .map_err(|error| format!("invalid shader interface: {error}"))?;
        let mut parameters = Vec::new();
        for (_, variable) in module.global_variables.iter() {
            if let Some(binding) = &variable.binding
                && (binding.group != 0 || binding.binding != 0)
            {
                return Err(format!(
                    "custom shader binding @group({}) @binding({}) is unsupported; use the f32 uniform parameter buffer",
                    binding.group, binding.binding
                ));
            }
            if variable.binding.is_some() && variable.space != naga::AddressSpace::Uniform {
                return Err("custom shader @group(0) @binding(0) must be a uniform buffer".into());
            }
            if variable.binding.is_some() {
                validate_float_uniform(&module, variable.ty)?;
                if interface == ShaderEffectInterface::CompleteModule
                    && let naga::TypeInner::Struct { members, .. } =
                        &module.types[variable.ty].inner
                {
                    for member in members {
                        if let Some(name) = &member.name {
                            let lanes = match module.types[member.ty].inner {
                                naga::TypeInner::Scalar(_) => Some(1),
                                naga::TypeInner::Vector { size, .. } => Some(size as usize),
                                _ => None,
                            };
                            parameters.push(ParameterSlot {
                                name: name.clone(),
                                offset: member.offset as usize / 4,
                                lanes,
                            });
                        }
                    }
                }
                let size = module.types[variable.ty].inner.size(module.to_ctx()) as usize;
                let capacity = UNIFORM_FLOAT_COUNT * std::mem::size_of::<f32>();
                if size > capacity {
                    return Err(format!(
                        "custom shader uniform buffer uses {size} bytes; Plico provides {capacity}"
                    ));
                }
            }
        }
        if interface == ShaderEffectInterface::CompleteModule {
            for (entry, stage) in [
                ("vs_main", naga::ShaderStage::Vertex),
                ("fs_main", naga::ShaderStage::Fragment),
            ] {
                if !module
                    .entry_points
                    .iter()
                    .any(|point| point.name == entry && point.stage == stage)
                {
                    return Err(format!("complete shader module must define {entry}"));
                }
            }
        }
        // The fullscreen effect pipeline supplies vertex indices, not vertex buffers.
        // Inspect struct members too: WGSL permits entry-point inputs in either form.
        fn has_vertex_location(
            module: &naga::Module,
            ty: naga::Handle<naga::Type>,
            binding: Option<&naga::Binding>,
        ) -> bool {
            if matches!(binding, Some(naga::Binding::Location { .. })) {
                return true;
            }
            if let naga::TypeInner::Struct { members, .. } = &module.types[ty].inner {
                return members
                    .iter()
                    .any(|member| has_vertex_location(module, member.ty, member.binding.as_ref()));
            }
            false
        }
        fn is_rgba_output(
            module: &naga::Module,
            ty: naga::Handle<naga::Type>,
            binding: Option<&naga::Binding>,
        ) -> bool {
            match (&module.types[ty].inner, binding) {
                (
                    naga::TypeInner::Vector {
                        size: naga::VectorSize::Quad,
                        scalar:
                            naga::Scalar {
                                kind: naga::ScalarKind::Float,
                                width: 4,
                            },
                    },
                    Some(naga::Binding::Location {
                        location: 0,
                        blend_src: None,
                        ..
                    }),
                ) => true,
                (naga::TypeInner::Struct { members, .. }, None) if members.len() == 1 => {
                    let member = &members[0];
                    is_rgba_output(module, member.ty, member.binding.as_ref())
                }
                _ => false,
            }
        }
        for entry in &module.entry_points {
            if entry.stage == naga::ShaderStage::Vertex
                && entry.name == "vs_main"
                && entry.function.arguments.iter().any(|argument| {
                    has_vertex_location(&module, argument.ty, argument.binding.as_ref())
                })
            {
                return Err(
                    "custom shader vertex @location inputs are unsupported; use @builtin(vertex_index) without vertex buffers"
                        .into(),
                );
            }
            if entry.stage == naga::ShaderStage::Fragment
                && entry.name == "fs_main"
                && !entry.function.result.as_ref().is_some_and(|result| {
                    is_rgba_output(&module, result.ty, result.binding.as_ref())
                })
            {
                return Err(
                    "custom shader fragment output must be one @location(0) vec4<f32>; additional color, depth, and sample-mask outputs are unsupported"
                        .into(),
                );
            }
        }
        Ok(Self {
            wgsl: complete.into(),
            sksl: None,
            interface,
            parameters: parameters.into(),
        })
    }

    /// Pack named top-level f32/vector fields using validated WGSL byte offsets.
    /// Width/height/time are host-owned. Arrays, matrices and nested structs may
    /// still use raw values(), but are not named-parameter targets yet.
    pub fn encode_parameters(
        &self,
        parameters: &[(String, plico::ShaderValue)],
    ) -> Result<Vec<f32>, String> {
        if parameters.is_empty() {
            return Ok(Vec::new());
        }
        if self.interface != ShaderEffectInterface::CompleteModule || self.parameters.is_empty() {
            return Err("named shader parameters require a complete module with a uniform struct at @group(0) @binding(0)".into());
        }
        if parameters.len() > PARAMETER_COUNT {
            return Err("too many named shader parameters".into());
        }
        let mut values = Vec::new();
        let mut written = vec![false; self.parameters.len()];
        for (name, value) in parameters {
            if name.is_empty() || name.len() > 1024 {
                return Err("invalid named shader parameter name".into());
            }
            let index = self
                .parameters
                .iter()
                .position(|slot| slot.name == *name)
                .ok_or_else(|| format!("unknown shader uniform field {name:?}"))?;
            let slot = &self.parameters[index];
            if std::mem::replace(&mut written[index], true) {
                return Err(format!("duplicate shader parameter {name:?}"));
            }
            if slot.offset < 3 {
                return Err(format!(
                    "shader uniform field {name:?} overlaps host-owned width/height/time"
                ));
            }
            let lanes = slot.lanes.ok_or_else(|| format!("named shader field {name:?} must be f32 or a float vector; use values() for structured fields"))?;
            let data = value.components();
            if data.len() != lanes {
                return Err(format!(
                    "shader uniform field {name:?} expects {lanes} float components, received {}",
                    data.len()
                ));
            }
            if data.iter().any(|v| !v.is_finite()) {
                return Err(format!(
                    "shader uniform field {name:?} contains a nonfinite value"
                ));
            }
            let end = slot.offset + lanes;
            if end > PARAMETER_COUNT {
                return Err(format!(
                    "shader uniform field {name:?} exceeds the parameter buffer"
                ));
            }
            values.resize(values.len().max(end), 0.);
            values[slot.offset..end].copy_from_slice(data);
        }
        Ok(values)
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn wgsl(&self) -> &Arc<str> {
        &self.wgsl
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn interface(&self) -> ShaderEffectInterface {
        self.interface
    }
}

/// One resolved custom-shader draw retained in the shared paint stream.
#[derive(Clone, Debug, PartialEq)]
pub struct ShaderEffect {
    pub(crate) id: ShaderEffectId,
    pub(crate) source: ShaderEffectSource,
    pub(crate) time: f32,
    pub(crate) values: Arc<[f32]>,
    pub(crate) physical_size: [u16; 2],
    pub(crate) logical_size: [f32; 2],
    pub(crate) device_scale: f32,
}

impl ShaderEffect {
    /// Skia implementation, when supplied.
    pub fn sksl(&self) -> Option<&str> {
        self.source.sksl.as_deref()
    }
    /// Injected animation time in seconds.
    pub fn time(&self) -> f32 {
        self.time
    }
    /// Application scalar parameters, packed into float4 values.
    pub fn values(&self) -> &[f32] {
        &self.values
    }
    /// Device pixels per logical pixel.
    pub fn device_scale(&self) -> f32 {
        self.device_scale
    }
    /// Output texture dimensions in physical pixels.
    pub fn physical_size(&self) -> [u16; 2] {
        self.physical_size
    }

    /// Destination dimensions in logical pixels.
    pub fn logical_size(&self) -> [f32; 2] {
        self.logical_size
    }
}

#[cfg(not(target_arch = "wasm32"))]
struct EffectTarget {
    _texture: Texture,
    view: TextureView,
    uniforms: Buffer,
    bind_group: BindGroup,
    size: [u16; 2],
    used_in_frame: u64,
    texture_id: ShaderTextureId,
    effect: Option<ShaderEffect>,
}

#[cfg(not(target_arch = "wasm32"))]
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
            texture_id: ShaderTextureId(ShaderEffectId::new().0),
            effect: None,
        }
    }
}

#[doc(hidden)]
#[allow(missing_docs)]
#[cfg(not(target_arch = "wasm32"))]
pub struct ShaderRenderer {
    layout: BindGroupLayout,
    pipelines: LruCache<Arc<str>, Arc<RenderPipeline>>,
    targets: FxHashMap<ShaderEffectId, Vec<EffectTarget>>,
    frame: u64,
}

#[allow(missing_docs)]
#[cfg(not(target_arch = "wasm32"))]
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
        Self {
            layout,
            pipelines: LruCache::new(NonZeroUsize::new(PIPELINE_CACHE_SIZE).unwrap()),
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

    /// Render and return a backend-neutral identity with its native view.
    /// Consumers register the view in their own external-texture mechanism.
    pub fn render_texture(
        &mut self,
        effect: &ShaderEffect,
        device: &Device,
        queue: &Queue,
        encoder: &mut CommandEncoder,
    ) -> (ShaderTextureId, TextureView) {
        self.render_target(effect, device, queue, encoder)
    }

    /// Render to a native wgpu view without a Vello texture-binding adapter.
    pub fn render_view(
        &mut self,
        effect: &ShaderEffect,
        device: &Device,
        queue: &Queue,
        encoder: &mut CommandEncoder,
    ) -> TextureView {
        self.render_target(effect, device, queue, encoder).1
    }

    /// Keep an unchanged surface alive when a damage consumer skips its draw.
    pub fn retain_effect(&mut self, effect: &ShaderEffect) {
        if let Some(targets) = self.targets.get_mut(&effect.id) {
            for target in targets {
                if target.effect.as_ref() == Some(effect) {
                    target.used_in_frame = self.frame;
                }
            }
        }
    }

    fn render_target(
        &mut self,
        effect: &ShaderEffect,
        device: &Device,
        queue: &Queue,
        encoder: &mut CommandEncoder,
    ) -> (ShaderTextureId, TextureView) {
        // A retained scene snapshot may contain several versions of one surface
        // (for example before/after a theme reveal). Distinct versions in the
        // same submission must never share a texture or uniform buffer.
        let targets = self.targets.entry(effect.id).or_default();
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
                    effect.physical_size,
                ));
                targets.len() - 1
            });
        let target = &mut targets[index];
        if target.size != effect.physical_size {
            let texture_id = target.texture_id;
            *target = EffectTarget::new(device, &self.layout, effect.physical_size);
            target.texture_id = texture_id;
        }
        target.used_in_frame = self.frame;
        if target.effect.as_ref() == Some(effect) {
            return (target.texture_id, target.view.clone());
        }
        let pipeline = Self::pipeline(&mut self.pipelines, &self.layout, device, &effect.source);
        let uniforms = effect_uniforms(effect);
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
        (target.texture_id, target.view.clone())
    }

    fn pipeline(
        cache: &mut LruCache<Arc<str>, Arc<RenderPipeline>>,
        group_layout: &BindGroupLayout,
        device: &Device,
        source: &ShaderEffectSource,
    ) -> Arc<RenderPipeline> {
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
        let pipeline = Arc::new(
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
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
            }),
        );
        cache.put(source.wgsl().clone(), pipeline.clone());
        pipeline
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn effect_uniforms(effect: &ShaderEffect) -> [f32; UNIFORM_FLOAT_COUNT] {
    let mut uniforms = [0.0; UNIFORM_FLOAT_COUNT];
    match effect.source.interface() {
        ShaderEffectInterface::PlicoEffect => {
            let count = effect.values.len().min(PARAMETER_COUNT);
            uniforms[4..4 + count].copy_from_slice(&effect.values[..count]);
            uniforms[3] = effect.device_scale;
        }
        ShaderEffectInterface::CompleteModule => {
            let count = effect.values.len().min(PARAMETER_COUNT);
            uniforms[..count].copy_from_slice(&effect.values[..count]);
        }
    }
    uniforms[0] = f32::from(effect.physical_size[0]);
    uniforms[1] = f32::from(effect.physical_size[1]);
    uniforms[2] = effect.time;
    uniforms
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn effect_parameters_preserve_header_and_clear_unused_lanes() {
        let source = ShaderEffectSource::new(
            "fn plico_effect(uv: vec2f) -> vec4f { return vec4f(plico.values[63]); }",
        )
        .unwrap();
        let values: Vec<f32> = (0..PARAMETER_COUNT).map(|i| i as f32 + 0.25).collect();
        let mut effect = ShaderEffect {
            id: ShaderEffectId::new(),
            source,
            time: 3.5,
            values: values.clone().into(),
            physical_size: [640, 360],
            logical_size: [320.0, 180.0],
            device_scale: 2.0,
        };
        let uniforms = effect_uniforms(&effect);
        assert_eq!(&uniforms[..4], &[640., 360., 3.5, 2.]);
        assert_eq!(&uniforms[4..], values.as_slice());

        // Reusing a surface with fewer parameters must not expose old values.
        effect.values = Arc::from([0.75]);
        let uniforms = effect_uniforms(&effect);
        assert_eq!(uniforms[4], 0.75);
        assert!(uniforms[5..].iter().all(|value| *value == 0.));
        assert_eq!(&uniforms[..4], &[640., 360., 3.5, 2.]);
    }

    #[test]
    fn validates_the_public_effect_function_contract() {
        ShaderEffectSource::new(
            "fn plico_effect(uv: vec2<f32>) -> vec4<f32> { return vec4<f32>(uv, 0.0, 1.0); }",
        )
        .unwrap();
        assert!(ShaderEffectSource::new("fn nope() {}").is_err());
        assert!(ShaderEffectSource::new("").is_err());
    }

    #[test]
    fn rejects_resources_outside_the_uniform_contract() {
        let error = ShaderEffectSource::new(
            "@group(1) @binding(0) var image: texture_2d<f32>;\nfn plico_effect(uv: vec2<f32>) -> vec4<f32> { return textureLoad(image, vec2<i32>(uv), 0); }",
        )
        .unwrap_err();
        assert!(error.contains("@group(1) @binding(0)"), "{error}");
        let error = ShaderEffectSource::new_module(
            r#"
@group(0) @binding(0) var image: texture_2d<f32>;
@vertex fn vs_main() -> @builtin(position) vec4<f32> { return vec4(0.0); }
@fragment fn fs_main() -> @location(0) vec4<f32> { return textureLoad(image, vec2<i32>(0), 0); }
"#,
        )
        .unwrap_err();
        assert!(error.contains("must be a uniform buffer"), "{error}");
    }

    #[test]
    fn validates_complete_single_pass_modules() {
        let source = r#"
struct Uniforms { size: vec2<f32>, time: f32, speed: f32 };
@group(0) @binding(0) var<uniform> u: Uniforms;
@vertex fn vs_main(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    let points = array<vec2<f32>, 3>(vec2(-1.0, -1.0), vec2(3.0, -1.0), vec2(-1.0, 3.0));
    return vec4(points[i], 0.0, 1.0);
}
@fragment fn fs_main(@builtin(position) p: vec4<f32>) -> @location(0) vec4<f32> {
    return vec4(p.xy / u.size, u.time * u.speed, 1.0);
}
"#;
        assert!(ShaderEffectSource::new_module(source).is_ok());
        assert!(ShaderEffectSource::new_module("fn nope() {}").is_err());
    }

    #[test]
    fn rejects_vertex_buffers_before_pipeline_creation() {
        for (declaration, argument, position) in [
            ("", "@location(0) position: vec2<f32>", "position"),
            (
                "struct Input { @location(0) position: vec2<f32> };",
                "input: Input",
                "input.position",
            ),
        ] {
            let source = format!(
                "{declaration}
@vertex fn vs_main({argument}) -> @builtin(position) vec4<f32> {{
    return vec4({position}, 0.0, 1.0);
}}
@fragment fn fs_main() -> @location(0) vec4<f32> {{ return vec4(1.0); }}"
            );
            let error = ShaderEffectSource::new_module(&source).unwrap_err();
            assert!(error.contains("vertex @location inputs"), "{error}");
        }
    }

    #[test]
    fn validates_fragment_output_against_the_single_rgba_target() {
        let vertex = "@vertex fn vs_main() -> @builtin(position) vec4<f32> { return vec4(0.0); }";
        for fragment in [
            "@fragment fn fs_main() -> @location(1) vec4<f32> { return vec4(1.0); }",
            "@fragment fn fs_main() -> @location(0) vec4<u32> { return vec4(1u); }",
            "@fragment fn fs_main() -> @builtin(frag_depth) f32 { return 0.5; }",
            "struct Output { @location(0) color: vec4<f32>, @location(1) other: vec4<f32> }; @fragment fn fs_main() -> Output { return Output(vec4(1.0), vec4(0.0)); }",
        ] {
            let error =
                ShaderEffectSource::new_module(&format!("{vertex}\n{fragment}")).unwrap_err();
            assert!(error.contains("fragment output must be"), "{error}");
        }
        let fragment = "struct Output { @location(0) color: vec4<f32> }; @fragment fn fs_main() -> Output { return Output(vec4(1.0)); }";
        ShaderEffectSource::new_module(&format!("{vertex}\n{fragment}")).unwrap();
    }

    #[test]
    fn uniform_transport_rejects_integer_members_and_accepts_float_layouts() {
        let module = |payload: &str| {
            format!(
                r#"
struct Inner {{ value: vec4<i32> }};
struct FloatInner {{ value: vec4f }};
struct Uniforms {{ header: vec4f, payload: {payload} }};
@group(0) @binding(0) var<uniform> u: Uniforms;
@vertex fn vs_main() -> @builtin(position) vec4f {{ return vec4f(0.); }}
@fragment fn fs_main() -> @location(0) vec4f {{ return vec4f(1.); }}
"#
            )
        };
        for payload in [
            "i32",
            "u32",
            "vec2u",
            "vec4<i32>",
            "array<vec4u, 2>",
            "Inner",
        ] {
            let error = ShaderEffectSource::new_module(&module(payload)).unwrap_err();
            assert!(error.contains("only f32"), "{payload}: {error}");
        }
        for payload in [
            "f32",
            "vec2f",
            "vec3f",
            "mat3x3f",
            "array<vec4f, 3>",
            "FloatInner",
            "array<FloatInner, 2>",
        ] {
            ShaderEffectSource::new_module(&module(payload))
                .unwrap_or_else(|error| panic!("{payload}: {error}"));
        }
    }

    #[test]
    fn named_parameters_follow_wgsl_vector_padding_and_reject_invalid_writes() {
        let source = ShaderEffectSource::new_module(
            r#"
struct U { size: vec2f, time: f32, progress: f32, color: vec3f, alpha: f32, matrix: mat4x4f };
@group(0) @binding(0) var<uniform> u: U;
@vertex fn vs_main() -> @builtin(position) vec4f { return vec4f(0.); }
@fragment fn fs_main() -> @location(0) vec4f { return vec4f(1.); }
"#,
        )
        .unwrap();
        let values = source
            .encode_parameters(&[
                ("alpha".into(), 0.5.into()),
                ("color".into(), [0.1, 0.2, 0.3].into()),
                ("progress".into(), 0.75.into()),
            ])
            .unwrap();
        assert_eq!(values, vec![0., 0., 0., 0.75, 0.1, 0.2, 0.3, 0.5]);
        for (name, value, message) in [
            ("missing", 1f32.into(), "unknown"),
            ("size", [1., 2.].into(), "host-owned"),
            ("color", [1., 2.].into(), "expects 3"),
            ("matrix", [1., 2., 3., 4.].into(), "structured"),
            ("progress", f32::NAN.into(), "nonfinite"),
        ] {
            assert!(
                source
                    .encode_parameters(&[(name.into(), value)])
                    .unwrap_err()
                    .contains(message)
            );
        }
        assert!(
            source
                .encode_parameters(&[("alpha".into(), 0.5.into()), ("alpha".into(), 1f32.into())])
                .unwrap_err()
                .contains("duplicate")
        );
    }

    #[test]
    fn complete_modules_receive_geometry_time_and_authored_uniforms() {
        let source = ShaderEffectSource::new_module(
            r#"
struct Uniforms { size: vec2<f32>, time: f32, speed: f32 };
@group(0) @binding(0) var<uniform> u: Uniforms;
@vertex fn vs_main(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    let points = array<vec2<f32>, 3>(vec2(-1.0, -1.0), vec2(3.0, -1.0), vec2(-1.0, 3.0));
    return vec4(points[i], 0.0, 1.0);
}
@fragment fn fs_main() -> @location(0) vec4<f32> { return vec4(u.speed); }
"#,
        )
        .unwrap();
        let values: Arc<[f32]> = [0.0, 0.0, 0.0, 0.75].into();
        let effect = ShaderEffect {
            id: ShaderEffectId::new(),
            source,
            time: 2.5,
            values,
            physical_size: [640, 360],
            logical_size: [320.0, 180.0],
            device_scale: 2.0,
        };
        let uniforms = effect_uniforms(&effect);
        assert_eq!(&uniforms[..4], &[640.0, 360.0, 2.5, 0.75]);
    }
}
