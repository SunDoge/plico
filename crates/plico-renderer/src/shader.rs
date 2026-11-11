//! Backend-neutral validated shader effects and their retained scene data.

use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};

use naga::valid::{Capabilities, ValidationFlags, Validator};

const MAX_SOURCE_BYTES: usize = 64 * 1024;
const PARAMETER_COUNT: usize = 256;
const UNIFORM_FLOAT_COUNT: usize = PARAMETER_COUNT + 4;

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

/// Additional prelude for effects that declared `read_backdrop`: the host
/// binds the region's already-painted content as a texture at group 1.
/// backdrop_at samples it at the surface's own top-left-origin uv, bilinearly;
/// outside the surface it returns transparent black, never repeating edges.
const SHADER_BACKDROP_PREFIX: &str = r#"
@group(1) @binding(0)
var plico_backdrop: texture_2d<f32>;
@group(1) @binding(1)
var plico_backdrop_sampler: sampler;
@group(1) @binding(2)
var<uniform> plico_backdrop_mapping: vec4f;

fn backdrop_at_lod(uv: vec2<f32>, lod: f32) -> vec4<f32> {
    if uv.x < 0.0 || uv.y < 0.0 || uv.x > 1.0 || uv.y > 1.0 {
        return vec4<f32>(0.0);
    }
    let mapped = uv * plico_backdrop_mapping.zw + plico_backdrop_mapping.xy;
    if any(mapped < vec2f(0.0)) || any(mapped > vec2f(1.0)) { return vec4f(0.0); }
    return textureSampleLevel(plico_backdrop,plico_backdrop_sampler,mapped,lod);
}

fn backdrop_at(uv: vec2<f32>) -> vec4<f32> {
    return backdrop_at_lod(uv, 0.0);
}
fn plico_backdrop_at_lod(uv: vec2<f32>, lod: f32) -> vec4<f32> {
    return backdrop_at_lod(uv,lod);
}
// Gaussian residual over a box-prefiltered pyramid. Sigma is in physical
// pixels; interpolation between mip levels keeps progressive masks smooth.
fn plico_backdrop_blur(uv: vec2<f32>, sigma: f32) -> vec4<f32> {
    let resolution = max(plico.resolution_time_scale.xy,vec2f(1.));
    let pixel = 0.5/resolution;
    if sigma <= 0.01 { return backdrop_at(clamp(uv,pixel,vec2f(1.)-pixel)); }
    let lod = clamp(log2(max(sigma,1.)),0.,f32(textureNumLevels(plico_backdrop)-1u));
    let lower = floor(lod);
    let width = exp2(lower);
    let variance = (mix(width*width,4.*width*width,fract(lod))-1.)/12.;
    let residual = sqrt(max(sigma*sigma-variance,0.));
    let spread = residual/resolution;
    var sum = vec4f(0.);
    var weights = 0.;
    for(var y=-2; y<=2; y++) {
        for(var x=-2; x<=2; x++) {
            let offset=vec2f(f32(x),f32(y));
            let weight=exp(-dot(offset,offset)*0.5);
            sum+=backdrop_at_lod(clamp(uv+offset*spread,pixel,vec2f(1.)-pixel),lod)*weight;
            weights+=weight;
        }
    }
    return sum/weights;
}

/// The surface's own sample of its backdrop at uv, premultiplied.
fn plico_backdrop_at(uv: vec2<f32>) -> vec4<f32> {
    return backdrop_at(uv);
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
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ShaderTextureId(u64);

impl ShaderTextureId {
    /// Allocate an identity for a backend-owned effect texture.
    #[doc(hidden)]
    pub fn new() -> Self {
        Self(NEXT_EFFECT_ID.fetch_add(1, Ordering::Relaxed))
    }

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
    /// The effect reads its backdrop through `plico.backdrop_at` (mygo's
    /// scene.Effect.Backdrop). Hosts bind the region's rendered content.
    reads_backdrop: bool,
    // Validated finite sigma bits keep the retained source key Eq + Hash.
    prefilter_backdrop: Option<u32>,
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
    /// Request transient area-prefiltered backdrop levels from compatible hosts.
    pub fn with_backdrop_prefilter(mut self, max_sigma: Option<f32>) -> Result<Self, String> {
        if max_sigma.is_some_and(|sigma| {
            !self.reads_backdrop || !sigma.is_finite() || !(0. ..=128.).contains(&sigma)
        }) {
            return Err(
                "backdrop prefilter requires a backdrop-reading effect and finite sigma in 0..=128"
                    .into(),
            );
        }
        self.prefilter_backdrop = max_sigma.map(|sigma| sigma.max(0.).to_bits());
        Ok(self)
    }
    /// Validate and retain a custom WGSL effect function.
    pub fn new(source: &str) -> Result<Self, String> {
        Self::with_backdrop(source, false)
    }

    /// Validate a `plico_effect` function that may call `plico.backdrop_at`
    /// to sample what is painted behind the surface (mygo's scene.Effect
    /// Backdrop). Hosts may track bounded input dependencies; unknown hosts
    /// retain conservative full damage.
    pub fn with_backdrop(source: &str, reads_backdrop: bool) -> Result<Self, String> {
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
        // `plico_effect` against it. Backdrop readers get the backdrop
        // texture and its sampling helpers in addition.
        let prefix = if reads_backdrop {
            format!("{SHADER_PREFIX}\n{SHADER_BACKDROP_PREFIX}")
        } else {
            SHADER_PREFIX.to_owned()
        };
        Self::validated(
            format!("{prefix}\n{source}\n{SHADER_SUFFIX}"),
            ShaderEffectInterface::PlicoEffect,
            reads_backdrop,
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
        Self::validated(
            source.to_owned(),
            ShaderEffectInterface::CompleteModule,
            false,
        )
    }

    fn validated(
        complete: String,
        interface: ShaderEffectInterface,
        reads_backdrop: bool,
    ) -> Result<Self, String> {
        let module = naga::front::wgsl::parse_str(&complete)
            .map_err(|error| error.emit_to_string(&complete))?;
        Validator::new(ValidationFlags::all(), Capabilities::default())
            .validate(&module)
            .map_err(|error| format!("invalid shader interface: {error}"))?;
        let mut parameters = Vec::new();
        for (_, variable) in module.global_variables.iter() {
            if reads_backdrop
                && variable.space == naga::AddressSpace::Uniform
                && variable.name.as_deref() == Some("plico_backdrop_mapping")
                && variable
                    .binding
                    .as_ref()
                    .is_some_and(|b| b.group == 1 && b.binding == 2)
                && matches!(&module.types[variable.ty].inner, naga::TypeInner::Vector {size: naga::VectorSize::Quad,scalar} if scalar.kind == naga::ScalarKind::Float && scalar.width == 4)
            {
                continue;
            }
            // Only the texture/sampler owned by the generated backdrop prelude
            // are allowed outside the uniform group. Arbitrary user bindings
            // remain rejected, including for complete shader modules.
            if reads_backdrop
                && variable.space == naga::AddressSpace::Handle
                && variable.binding.as_ref().is_some_and(|binding| {
                    binding.group == 1
                        && matches!(
                            (
                                binding.binding,
                                variable.name.as_deref(),
                                &module.types[variable.ty].inner,
                            ),
                            (
                                0,
                                Some("plico_backdrop"),
                                naga::TypeInner::Image {
                                    dim: naga::ImageDimension::D2,
                                    arrayed: false,
                                    class: naga::ImageClass::Sampled {
                                        kind: naga::ScalarKind::Float,
                                        multi: false,
                                    },
                                },
                            ) | (
                                1,
                                Some("plico_backdrop_sampler"),
                                naga::TypeInner::Sampler { comparison: false },
                            )
                        )
                })
            {
                continue;
            }
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
            reads_backdrop,
            prefilter_backdrop: None,
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

    #[doc(hidden)]
    pub fn wgsl(&self) -> &Arc<str> {
        &self.wgsl
    }

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
    /// Stable surface identity used by renderer backends for retained output.
    #[doc(hidden)]
    pub fn id(&self) -> ShaderEffectId {
        self.id
    }

    /// Validated source consumed by renderer backends.
    #[doc(hidden)]
    pub fn source(&self) -> &ShaderEffectSource {
        &self.source
    }

    /// Backend uniform payload with host-owned dimensions and time injected.
    #[doc(hidden)]
    pub fn uniform_words(&self) -> [f32; UNIFORM_FLOAT_COUNT] {
        let mut uniforms = [0.0; UNIFORM_FLOAT_COUNT];
        match self.source.interface() {
            ShaderEffectInterface::PlicoEffect => {
                let count = self.values.len().min(PARAMETER_COUNT);
                uniforms[4..4 + count].copy_from_slice(&self.values[..count]);
                uniforms[3] = self.device_scale;
            }
            ShaderEffectInterface::CompleteModule => {
                let count = self.values.len().min(PARAMETER_COUNT);
                uniforms[..count].copy_from_slice(&self.values[..count]);
            }
        }
        uniforms[0] = f32::from(self.physical_size[0]);
        uniforms[1] = f32::from(self.physical_size[1]);
        uniforms[2] = self.time;
        uniforms
    }

    /// Skia implementation, when supplied.
    pub fn sksl(&self) -> Option<&str> {
        self.source.sksl.as_deref()
    }
    /// Whether the effect reads its backdrop (mygo's scene.Effect.Backdrop).
    pub fn reads_backdrop(&self) -> bool {
        self.source.reads_backdrop
    }
    /// Maximum logical-pixel sigma requested for area-prefiltered backdrop levels.
    pub fn prefilter_backdrop(&self) -> Option<f32> {
        self.source.prefilter_backdrop.map(f32::from_bits)
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

/// Built-in Liquid Glass effect body, shared with the managed SDK.
/// Requires backdrop input; see docs/liquid-glass.md for uniform slots.
pub const LIQUID_GLASS_WGSL: &str = include_str!("../assets/liquid_glass.wgsl");

/// Progressive backdrop blur body shared with the managed SDK.
pub const BACKDROP_BLUR_WGSL: &str = plico::materials::backdrop_blur_shader().source();

#[cfg(test)]
mod tests {
    #[test]
    fn built_in_backdrop_sources_validate_without_a_gpu() {
        super::ShaderEffectSource::with_backdrop(super::LIQUID_GLASS_WGSL, true).unwrap();
        super::ShaderEffectSource::with_backdrop(super::BACKDROP_BLUR_WGSL, true).unwrap();
    }

    use super::*;

    #[test]
    fn backdrop_prelude_accepts_the_public_sampling_helper() {
        let body = "fn plico_effect(uv: vec2<f32>) -> vec4<f32> { return plico_backdrop_at(uv); }";
        ShaderEffectSource::with_backdrop(body, true).unwrap();
        assert!(ShaderEffectSource::new(body).is_err());
        let source = ShaderEffectSource::with_backdrop(body, true).unwrap();
        assert!(source.clone().with_backdrop_prefilter(Some(8.)).is_ok());
        for sigma in [-1., f32::NAN, f32::INFINITY, 129.] {
            assert!(source.clone().with_backdrop_prefilter(Some(sigma)).is_err());
        }
        assert!(
            ShaderEffectSource::new("fn plico_effect(_uv:vec2f)->vec4f { return vec4f(1.); }")
                .unwrap()
                .with_backdrop_prefilter(Some(8.))
                .is_err()
        );
    }

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
        let uniforms = effect.uniform_words();
        assert_eq!(&uniforms[..4], &[640., 360., 3.5, 2.]);
        assert_eq!(&uniforms[4..], values.as_slice());

        // Reusing a surface with fewer parameters must not expose old values.
        effect.values = Arc::from([0.75]);
        let uniforms = effect.uniform_words();
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
        let uniforms = effect.uniform_words();
        assert_eq!(&uniforms[..4], &[640.0, 360.0, 2.5, 0.75]);
    }
}
