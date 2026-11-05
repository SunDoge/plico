# WGSL effect surfaces

The experimental native `plico-desktop/wgpu` backend now consumes the shared
`ShaderEffect` paint command. Applications use the existing Plico widget API;
GPU objects stay in the renderer.

```rust
static PROGRESS: plico::ShaderAsset = plico::shader_asset!("assets/progress.wgsl");

// Inside an ordinary component:
shader(cx, &PROGRESS)
    .width(380.)
    .height(12.)
    .radius(6.)
    .values(&[progress])
    .animated(running);
```

```wgsl
// assets/progress.wgsl
fn plico_effect(uv: vec2f) -> vec4f {
    let progress = clamp(plico.values[0].x, 0., 1.);
    let time = plico.resolution_time_scale.z;
    let pulse = 0.92 + 0.08 * sin(time * 3.);
    let background = vec3f(0.90, 0.92, 0.96);
    let accent = vec3f(0.15, 0.33, 0.89) * pulse;
    return vec4f(mix(background, accent, step(uv.x, progress)), 1.);
}
```

Authored shader identifiers are not rewritten. The renderer supplies:

- `plico.resolution_time_scale.xy`: physical output width/height.
- `.z`: injected effect time in seconds; `.w`: device scale.
- `plico.values`: 64 float4 vectors, containing up to 256 scalar parameters.

Scalar parameter `i` maps to `plico.values[i / 4][i % 4]`; parameters are
packed consecutively, independently of Rust struct layout. Unspecified lanes
are zero on every update, including when an existing surface receives a shorter
parameter list. The geometry/time/scale header occupies a separate first float4
and cannot be overwritten by `.values(...)`. This function interface differs
from complete modules, whose authored buffer starts at offset zero and whose
first three floats are overwritten with width, height and time.

Complete-module uniform buffers must contain only `f32` scalars, vectors,
matrices, arrays and nested structs. Integer members (`u32`/`i32`) are rejected
before GPU pipeline creation: `.values(&[1.0])` uploads float bits, not the integer
one. Declare the parameter as `f32` and explicitly convert it in WGSL, for example
`let count = u32(u.count);`. WGSL uniform padding/strides still apply; this is a
flat-word transport, not automatic struct serialization. Local integers and
builtin vertex indices remain supported.

Do not upload an arbitrary Rust struct as these parameters: Rust layout and
WGSL uniform alignment are different contracts. With raw `.values(...)`,
explicitly flatten fields into scalar values and document their order beside the
WGSL declaration. Named scalar/vector parameters below resolve offsets from WGSL.

Effect surfaces output **premultiplied RGBA**, matching the shared texture/SkSL
compositing contract. For translucent color, return `vec4f(rgb * alpha, alpha)`.
This differs from the standalone quad-material function, whose wrapper accepts
straight RGBA. The new shared effect bridge does not silently convert either ABI.

`.animated(true)` explicitly requests clock-driven updates. Time does not require
another execution of application `show`. `.paused(true)` stops that effect;
virtual clocks and reduced motion use the existing runtime scheduling. A shader
reading time without opting into animation remains static. Syntax/validation
errors retain the previous valid source through the runtime diagnostics. Device
pipeline errors are caught and returned; a failed frame is not presented, and a
subsequent valid source can recover. Shader file watching is not added by this
bridge: `shader_asset!` embeds source at Rust compile time.

## Damage and resources

This interface permits only a uniform binding, not arbitrary textures, storage or
backdrop reads. Its output is constrained to a standalone texture rectangle, and
all changing inputs, including time, appear in `ShaderEffect`. The wgpu bridge
therefore supplies local ink bounds to damage tracking. An unchanged static effect
can skip rendering/presentation. Unknown bounds in other consumers still fall
back conservatively. Expanding effects or backdrop effects will need an explicit
resource/damage contract before extending this interface.

Native effect outputs and pipelines reuse the shared renderer resource helper,
through a wgpu-view interface rather than scene or texture-binding replay.
The shared `ShaderRenderer` returns its own process-local `ShaderTextureId`
and a native view through `render_texture`; the wgpu consumer uses `render_view`.
A GPU test renders two versions of one effect in the same snapshot through
both paths and checks matching output pixels, guarding against resource and
uniform aliasing at this boundary. The Vello adapter that once consumed these
has been removed with the backend.
The native bridge limits a frame to 256 effect draws and 128 MiB of declared output
pixel payload, in addition to device dimension limits. Pipelines use the shared
16-entry LRU. Removed output surfaces are released; cached pipelines may remain.
Multiple versions of the same effect ID within a snapshot use distinct output
textures/uniform buffers. `effect_cache_stats` and frame-resource tracing expose
live output count, pixel payload and cached pipeline count. These are not total
GPU memory or frame-time measurements.

Effects participate in ordinary clips and isolated opacity groups. The shared
X11 window demo has an independent WGSL progress file and Animate/Pause buttons:

```sh
PLICO_WGPU_DAMAGE=1 cargo run -p plico-desktop --no-default-features \
  --features wgpu --example wgpu-window
```

Skia requires an explicitly supplied SkSL variant via `ShaderAsset::with_sksl`;
WGSL is not translated automatically. This new native bridge does not implement
browser-side effect compilation; WASM type checking passes, but attempting such a
surface in that path returns an explicit unsupported error. Typed uniform
structures, texture inputs, resource file watching and backdrop/filter graphs are
future work, not current capabilities.

## Embedded complete modules

Use `shader_asset!(module, "assets/custom.wgsl")` for a complete module with
`vs_main` / `fs_main`, rather than an effect function. Both macro forms retain
the literal relative path in `ShaderAsset::source_path()` for authoring tools;
inline assets created with `ShaderAsset::new` or `module` have no path unless
`with_source_path` is used. This metadata does not enable runtime file watching
and prefixes runtime source-validation diagnostics with the resource path. GPU
pipeline diagnostics remain backend-specific. Module resources and uniform ABI
remain subject to the limits described above.

## Named module parameters

Complete WGSL modules can bind top-level `f32`, `vec2f`, `vec3f` and `vec4f` fields:

```rust
static EFFECT: plico::ShaderAsset = plico::shader_asset!(module, "effect.wgsl");
shader(cx, &EFFECT)
    .param("progress", 0.5)
    .param("color", [0.25, 0.5, 0.75])
    .param("alpha", 1.0);
```

The corresponding module can declare:

```wgsl
struct Uniforms {
    size: vec2f,  // supplied by the host
    time: f32,   // supplied by the injected clock
    progress: f32,
    color: vec3f,
    alpha: f32,
};
@group(0) @binding(0) var<uniform> u: Uniforms;
// Supply vs_main/fs_main as required by the complete-module interface.
```

Validated WGSL offsets determine packing, including `vec3f` followed by a scalar.
No Rust struct memory is uploaded, and WGSL is not reparsed for ordinary valid
parameter updates. Repeated builder names replace earlier values. Unknown fields,
wrong component counts, duplicate serialized entries and writes overlapping the
first three host-owned words are errors. Unspecified fields/padding are zero.

Choose named `.param(...)` or raw `.values(...)` per widget; mixing them is an
error. Effect-function assets, nested fields, arrays and matrices do not yet accept
named parameters. They retain the raw float-word API. Native resource/error limits
remain unchanged. Source validation and parameter packing commit together: an
invalid update retains the preceding valid program **and parameter words**, with
an authored-path diagnostic. Animation time/pause scheduling continues separately.
The same packed data is used by any supplied SkSL implementation; its parameter
interpretation must match the WGSL module offsets.
