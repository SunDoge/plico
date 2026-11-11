use crate::{DEFAULT_MATERIAL, Error, Quad, Result, scene::DrawSource};
use snafu::ResultExt;
use std::time::Instant;
use wgpu::util::DeviceExt;

const INITIAL_INSTANCE_CAPACITY: usize = 64;

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
struct LayerRaster {
    image: std::rc::Rc<crate::image::GpuImage>,
    texture: wgpu::Texture,
}
struct LayerBuffers {
    frame: wgpu::Buffer,
    bind: wgpu::BindGroup,
    instances: wgpu::Buffer,
    capacity: usize,
}
struct LayerTarget {
    target: Target,
    image: std::rc::Rc<crate::image::GpuImage>,
    blur: Option<(crate::blur::Workspace, std::rc::Rc<crate::image::GpuImage>)>,
    buffers: LayerBuffers,
}
impl LayerTarget {
    fn bytes(&self) -> u64 {
        let textures = u64::from(self.target.size[0])
            * u64::from(self.target.size[1])
            * if self.blur.is_some() { 12 } else { 4 };
        let buffers = (self.buffers.capacity * std::mem::size_of::<Instance>()) as u64 + 48;
        textures + buffers
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
    pub backdrop_context_entries: usize,
    pub backdrop_context_bytes: u64,
    /// Snapshot textures retained for ordinary and grouped backdrop readers.
    pub backdrop_entries: usize,
    pub backdrop_bytes: u64,
    /// Cumulative snapshot texture creations and cross-frame reuses.
    pub backdrop_allocations: u64,
    pub backdrop_reuses: u64,
    /// Retained aligned mapping/composite uniform entries and cumulative use.
    pub backdrop_uniform_entries: usize,
    pub backdrop_uniform_allocations: u64,
    pub backdrop_uniform_reuses: u64,
    /// Small effect-parameter buffers used by fused direct backdrop draws.
    pub backdrop_effect_uniform_entries: usize,
    pub backdrop_effect_uniform_allocations: u64,
    pub backdrop_effect_uniform_reuses: u64,
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
    /// Prepared plain fills whose entire AA quad is inside a rectangular clip.
    /// Counted before damage filtering, rather than once per scissor replay.
    pub clip_coverage_skips: u64,
    /// Scene color render passes, including isolated layers.
    pub passes: u64,
    /// Explicit partial-damage replacement clears (not load-op clears).
    pub clears: u64,
    /// Backdrop snapshots encoded, including snapshots in child layers.
    pub backdrop_copies: u64,
    /// Area-prefilter passes encoded for opted-in backdrop shaders.
    pub backdrop_mip_passes: u64,
    /// Transient backdrop texture payload encoded this frame, including all mips.
    /// Not retained cache bytes or process RSS.
    pub backdrop_bytes: u64,
    /// Backdrop texture resources used this frame (arrays count once). The
    /// renderer may obtain them from its cross-frame pool.
    pub backdrop_textures: u64,
    /// Effects reading slices of a same-stage snapshot array.
    pub backdrop_shared_reads: u64,
}
/// CPU and GPU work spent on the most recently encoded frame, in milliseconds.
///
/// CPU stages are measured with `Instant` and are always available. `gpu_ms`
/// brackets the frame between timestamp queries, which requires
/// `wgpu::Features::TIMESTAMP_QUERY` and the `PLICO_GPU_TIMER` environment
/// variable (or a profiling renderer). Child-layer command buffers and the
/// closing presentation blit or readback copy execute in the same ordered queue
/// submission and are therefore included.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct FrameBudget {
    /// Building the instance stream: scene flattening, text/glyph preparation.
    pub prepare_ms: f64,
    /// CPU time in instance and frame-uniform `write_buffer` calls.
    pub upload_ms: f64,
    /// Recording render passes, batching decisions and blits.
    pub encode_ms: f64,
    /// CPU time for the frame's queue submission.
    pub submit_ms: f64,
    /// Queue submissions issued for the frame. Normal rendering should use
    /// one even when it contains several painter-ordered command buffers.
    pub submissions: u64,
    /// Blocking GPU wait for offscreen readback; zero for window presentation.
    pub wait_ms: f64,
    /// Instance-stream and frame-uniform bytes written this frame.
    pub upload_bytes: u64,
    /// Instances prepared across the root scene and child layers.
    pub instances: u64,
    /// Instanced scene draw calls, including damage-scissor replays.
    pub draw_calls: u64,
    /// GPU time of the root command encoder, when timestamps are available.
    pub gpu_ms: Option<f64>,
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
                        run.origin[0] + (glyph.x + x) * run.scale - 4.,
                        run.origin[1] + (glyph.y + y) * run.scale - 4.,
                        run.origin[0] + (glyph.x + x + w) * run.scale + 4.,
                        run.origin[1] + (glyph.y + y + h) * run.scale + 4.,
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
    backdrop_compositor: Option<crate::backdrop_composite::Compositor>,
    backdrop_prefilter: Option<crate::backdrop_prefilter::Prefilter>,
    backdrop_batch_prefilter: Option<crate::backdrop_batch::Prefilter>,
    backdrop_pool: crate::backdrop_pool::Pool,
    force_separate_backdrops: bool,
    frame_layout: wgpu::BindGroupLayout,
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
    #[cfg(test)]
    profile_no_clip_skip: bool,
    material_module: Option<wgpu::ShaderModule>,
    clear_pipeline: wgpu::RenderPipeline,
    source: String,
    target: Option<Target>,
    instances: wgpu::Buffer,
    capacity: usize,
    /// Backdrop-reading effects collected during encode, in paint order.
    backdrop_effects: Vec<plico_renderer::ShaderEffect>,
    deferred_layers: Vec<crate::scene::PreparedLayer>,
    inherited_backdrop: Option<crate::backdrop_context::Context>,
    backdrop_contexts: Vec<Option<crate::backdrop_context::Context>>,
    context_charged: Vec<bool>,
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
    effects: Option<crate::shader::ShaderRenderer>,
    frame_effect_draws: usize,
    frame_effect_bytes: u64,
    frame_images: Vec<std::rc::Rc<crate::image::GpuImage>>,
    frame_image_slots: std::collections::HashMap<usize, usize>,
    batch_stats: FrameBatchStats,
    frame_budget: FrameBudget,
    gpu_timer: Option<gpu_timer::GpuTimer>,
    pending_gpu_slot: Option<usize>,
    /// Painter-ordered command buffers from effects and isolated layer
    /// segments. The frame's final encoder is appended and all are submitted
    /// together, preserving dependencies without repeated queue submissions.
    pending_commands: Vec<wgpu::CommandBuffer>,
    /// Profiling-only A/B switch that submits each recorded segment
    /// separately while keeping the same independent layer buffers.
    split_submits: bool,
    atlas_texture: crate::texture::AtlasTexture,
    #[cfg(all(target_os = "linux", feature = "system-text"))]
    atlas: Option<plico_native::text::atlas::Atlas>,
    #[cfg(all(target_os = "linux", feature = "system-text"))]
    text_stats: crate::text::TextStats,
}

impl Renderer {
    pub(super) fn submit_frame_commands(
        &mut self,
        encoder: wgpu::CommandEncoder,
    ) -> wgpu::SubmissionIndex {
        self.pending_commands.push(encoder.finish());
        let commands = std::mem::take(&mut self.pending_commands);
        let submission = if self.split_submits {
            let mut commands = commands.into_iter();
            let first = commands.next().expect("a frame has a final command buffer");
            let mut submission = self.queue.submit([first]);
            self.frame_budget.submissions += 1;
            for command in commands {
                submission = self.queue.submit([command]);
                self.frame_budget.submissions += 1;
            }
            submission
        } else {
            self.frame_budget.submissions += 1;
            self.queue.submit(commands)
        };
        self.images.commit_pending_mips();
        submission
    }
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
    } else if input.clip_params.x >= 0. && (any(world<clip.xy) || any(world>=clip.xy+clip.zw)) { discard; }
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

/// MyGO Builder.markFill (MIT, ee3d4bd9): leave three physical pixels for
/// the vertex AA fringe and clip coverage. Masked and non-fill instances keep
/// the general path. Negative clip_params.x means coverage is identically one.
fn skip_interior_clip(instance: &mut Instance) -> bool {
    if !is_plain(instance) || !(0. ..=1.).contains(&instance.color[3]) {
        return false;
    }
    let [x, y, w, h] = instance.rect;
    let [cx, cy, cw, ch] = instance.clip;
    const ROOM: f32 = 3.;
    if cx <= x - ROOM && cy <= y - ROOM && cx + cw >= x + w + ROOM && cy + ch >= y + h + ROOM {
        instance.clip_params[0] = -1.;
        return true;
    }
    false
}

mod backdrop;
mod batch;
mod cache;
mod effects;
mod encode;
mod gpu_timer;
mod layers;
mod render;
mod setup;
mod surface;
mod text;

#[cfg(test)]
mod tests;

#[cfg(all(test, target_os = "linux", feature = "system-text"))]
mod atlas_tests;

#[cfg(all(test, target_os = "linux", feature = "system-text"))]
#[path = "damage_profile.rs"]
mod damage_profile;
