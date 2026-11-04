# Independent wgpu quad backend experiment

`plico-wgpu` is a small, offscreen renderer prototype. It renders axis-aligned
rectangles with circular rounded corners, vertical two-color gradients, uniform
inside borders, hard rectangular clips and painter-order alpha compositing.
These are supplied as typed `Quad` primitives. System text and a real-window demo
were added in the milestone below. The shared-paint/desktop milestone below integrates a supported subset;
it is **not yet an implementation of the full `plico_renderer::Scene` contract**.

Base dependencies are wgpu, Naga, bytemuck, Snafu, tracing and pollster.
Linux system text additionally uses the optional `plico-native` Pango/Cairo provider
(Pango >= 1.50). The independent quad core does not invoke Vello. The default `paint-scene` bridge
now depends on `plico-renderer` and its legacy transitive dependencies; see below.
The Vello window consumer itself remains a comparison-only development dependency.
The [system text experiment](system-text-wgpu.md) supplied the atlas now reused by
the optional text provider.

## Run and inspect

From the repository root, on a machine with a working wgpu adapter:

```sh
cargo run --locked -p plico-wgpu --example compare_quads
```

Generated files live under ignored `reports/wgpu-quads/`:

- `wgpu-vello-diff-{scale}x.png`, scales 1/1.25/1.5/2. Left: own wgpu/SDF renderer;
  middle: Vello vector paths; right: absolute RGB difference amplified 8×.
- `material-time-snapshots.png`: the same quad list at injected times 0/0.5/1s.
  Only the progress material's shader output changes, not CPU geometry.

The 20-quad fixture covers square/rounded/capsule shapes, fractional positions,
0.5/1/2/4px borders, overlapping translucent fills, clipping, very thin shapes and
a WGSL progress bar. Both renderers consume the same quad descriptions and colors.
The example explicitly constructs Vello's `ImageRenderer`, not a default renderer
whose feature selection could silently choose Skia.

The reference translates rounded rectangles to Kurbo paths and borders to even-odd
rings. The procedural material's **same WGSL function** executes in Vello's existing
custom-effect surface, with equivalent inputs and external animation time. This
compares the new material/geometry integration; it is not an independent oracle
for the material's math. Vello rasterizes its borders over the filled shape; the
new shader combines inside/border coverage in one pass, so AA differences are
expected, especially at fractional edges. Neither image is designated ground truth.

## Let WGSL participate in development

The core renderer shader owns shape coverage, border coverage, clipping and
premultiplied alpha. A small separate file owns appearance:

```wgsl
fn plico_material(m: Material) -> vec4f {
    return mix(m.color, m.color2, clamp(m.uv.y, 0., 1.));
}
```

`Material` supplies UV, physical-pixel size, two straight sRGB colors, injected
seconds and one per-quad scalar parameter. It returns straight RGBA. `Quad.material`
selects it; other quads still use the built-in gradient. One installed material is
shared by its instances, with different instance parameters. The renderer expands
quads in a vertex shader and evaluates distance/coverage in the fragment shader.
There is no CPU tessellation or per-material offscreen texture in this path.

The example loads `examples/materials/progress.wgsl` **from disk at runtime**, not
via `include_str!`. To edit WGSL without rebuilding Rust:

```sh
cargo build --locked -p plico-wgpu --example compare_quads
PLICO_WGPU_WATCH=1 target/debug/examples/compare_quads
```

Save the material file: the running process validates it, creates a replacement GPU
pipeline and refreshes the images at fixed scales/times. An alternate material file
can be selected with `PLICO_WGPU_MATERIAL=/path/to/material.wgsl`. Watch mode polls
file contents every 200ms, but renders only on changes. Stop with Ctrl+C. This is
an image preview workflow, not a window or shader editor.

The public API is `Renderer::set_material(source).await`. Equal sources are a no-op;
invalid syntax/interface or a failed wgpu validation scope returns an error and
preserves the previous pipeline. Bindings and entry points remain renderer-owned.
This was exercised in one running watcher: invalid text left the output image hash
unchanged; valid replacement changed the hash and incremented the compilation
counter; restoring the original material restored the original output.

WGSL can express sheen, stripes, progress and other procedural appearance/animation.
Rust still owns layout, interaction, state and geometry. This is not a proposal to
move application logic into shaders. Texture/backdrop access is not implemented.

## Measured image differences

On 2026-11-04, Linux, Intel Graphics ARL integrated GPU, Mesa 25.0.7, Vulkan:

| Scale | Mean RGB absolute error, 0–255 | Pixels with any RGB error > 8 |
| --- | ---: | ---: |
| 1 | 0.164 | 0.56% |
| 1.25 | 0.274 | 0.93% |
| 1.5 | 0.112 | 0.41% |
| 2 | 0.069 | 0.25% |

These statistics include the unchanged background; they are **not** edge-only
accuracy, a perceptual score, or a performance comparison. Visual inspection at 1x
shows similar interiors, gradients and procedural output, with differences mostly
around rounded/antialiased borders. Images are generated locally and not committed.

## Verification and limits

```sh
cargo clippy --locked -p plico-wgpu --all-targets -- -D warnings
cargo test --locked -p plico-wgpu --test render -- --ignored --nocapture
```

The GPU test is intentionally ignored by default so routine CI does not need an
adapter. Explicit execution checks premultiplied alpha, fractional rectangle
coverage, painter order, clip bounds, rounded borders, gradient interpolation,
instance-buffer growth, target resize, time/parameter inputs, shader replacement,
failed-replacement rollback and invalid frame data. It does not skip itself when
no GPU is available; explicit invocation must succeed or return a test failure.

The target, readback buffer and growing instance buffer are retained between
same-size frames. Quads use one instanced draw in painter order. The renderer clears
the complete target every frame. Readback is synchronous and intended for tests;
no FPS/CPU/power advantage is claimed from these captures.

Missing at the initial quad-only milestone: full paint-command replay, window submission, text/images, general paths,
nonuniform radii/borders, transforms, nested rounded clips, layers, shadows, filters,
backdrop materials, damage and production GPU recovery. Skia remains the working
application backend. A next integration should translate a deliberately supported
subset of Plico paint input and report unsupported operations explicitly.

See [source references](../crates/plico-wgpu/SOURCES.md) for the MyGO code read.

## System text and window milestone

The prototype now supports ordered `Draw::Quad` / `Draw::Text` commands in the
same GPU pass on Linux. The default `system-text` feature uses the reusable
`plico-native::text::atlas` service. Native layouts retain resolved fonts, glyph
positions and Pango editing geometry. Glyphs are rasterized at the requested DPI;
color/tint changes do not force monochrome glyph rasterization. Four X phases and
one Y phase are the default. `TextRun.continuous` permits fractional bitmap motion
when smooth motion is preferred over snapped raster sharpness.

```sh
cargo run --locked -p plico-wgpu --example text_quality
cargo run --locked -p plico-wgpu --example text_window
```

`text_quality` writes light/dark settings-page images at 1/1.25/1.5/2× to
`reports/wgpu-text/`. `text_window` displays those primitives through a real winit
window. D switches theme, 1/2/3/4 requests 100/125/150/200% of the actual device
scale, the wheel scrolls, Space animates scrolling and C switches text sampling.
Actual monitor scale changes rebuild the native layouts. The keyboard scale
control is a test UI-scale override, not a simulation of OS display parameters.

Window rendering uses `new_for_surface` to choose a compatible adapter and
`render_to_view` for GPU-only drawing/compositing. It does not read pixels back or
wait on the CPU. Readback buffers are allocated lazily only for screenshot calls.
A pixel-exact fullscreen copy handles RGBA/BGRA and UNORM/sRGB surface formats;
GPU tests verify that format changes do not accidentally change encoded text/color
values. The raw draw API fully redraws its retained offscreen color target; the
shared Scene path now has optional damage replay (see below).

The atlas has fixed 16 MiB CPU and 16 MiB GPU storage. Only new glyph rectangles
are uploaded. Empty/off-clip glyphs are culled before rasterization. If storage
fills, preparation is retried after a frame-boundary recycle: all visible UVs are
regenerated, and the texture is cleared/uploaded before drawing. A visible working
set that cannot fit even in an empty atlas returns an error. There is no unbounded
per-translation cache key or atlas growth. Native font caches, caller-retained
layouts, frame targets and buffers are additional memory.

A grayscale-only intrinsic color glyph is distinguished from a tintable mask by a
second black-foreground raster probe on a cache miss. Colored glyphs retain their
premultiplied pixels and obey text opacity. This adds cold-raster work, never
per-frame work. It avoids treating grayscale emoji artwork as plain glyph coverage.

The 120-frame settings-page check, after warming, recorded 743 cached glyphs,
1,041,292 cumulative uploaded glyph bytes, and no additional entries or uploads
in those frames. These figures cover the four DPI variants in that run; they are
not a whole-process memory measurement or a claim about GPU performance.

In this session the real-window smoke run used Xvfb/X11 (no desktop session was
available), NVIDIA RTX 5080/Vulkan/BGRA8 UNORM. First presentation, theme change,
1.25× layout rebuilding and window resize were visually inspected. This does not
validate physical-monitor cross-screen DPI, Wayland input methods, or Windows.
A reproducible Vulkan-loader crash during parallel default Instance creation was
localized with GDB before text rendering. Default renderers now share an Instance;
parallel GPU tests pass. Window hosts should likewise share their backend Instance.

For future editor/IME integration, `GlyphLayout` provides hit tests, caret and
bidi selection rectangles, strict UTF-8/UTF-16 conversions and a candidate anchor
on the caret's composition line. These use the same retained Pango layout as the
glyph positions. They do not implement a text editor or install a system IME
session. Existing Plico input state/event handling should be reused when the
paint-scene bridge is connected. See the GPUI references in `SOURCES.md`.

Tracing spans distinguish frame encoding, text layout and view submission without
logging document text. To verify current behavior:

```sh
cargo test --locked -p plico-native --features linux-text --test text
cargo test --locked -p plico-wgpu -- --ignored
cargo clippy --locked -p plico-wgpu --all-targets -- -D warnings
cargo clippy --locked -p plico-wgpu --no-default-features --all-targets -- -D warnings
```

Linux builds with `system-text` require Pango/Cairo development libraries; minimal
quad-only builds may disable default features. Windows/CoreText text providers,
full Plico scene replay, actual editor/IME integration, images and damage remain
unfinished. The renderer cannot yet replace Vello for arbitrary Plico apps.

## Shared paint and desktop host milestone

The `paint-scene` feature (default) now consumes the real shared `Scene` stream.
Supported commands are analytical rectangle/rounded-rectangle fills and solid
strokes, sRGB two-stop linear gradients, nested rectangular clips, retained
fragments and ordinary native paragraphs. Positive uniform scale and translation
are supported. Fractional clip edges use pixel-area AA. Gradients preserve
Peniko's premultiplied versus straight interpolation choice, including alpha fades.

The recorder retains a `PaintShape`: exact source Kurbo primitives plus the
unchanged compatible Bezier path. Skia/Vello still receive that path; wgpu never
tries to identify a rectangle by heuristically decoding arbitrary curves.

Native paragraphs use one bounded cache for measurement, selection/hit testing
and drawing, with DPI in the key and color excluded from geometry. Alignment,
ellipsis, absolute/multiplier line height and character spacing use Pango APIs.
Rich spans, application font registration and non-ellipsized line clamps remain
unsupported. The measurement context does not allocate a second glyph atlas.

```sh
cargo run --locked -p plico-wgpu --example scene_counter
cargo run --locked -p plico-desktop --no-default-features --features wgpu --example wgpu-window
```

`scene_counter` generates real `plico::View` / `#[component]` / Taffy counter
snapshots through shared paint at 1/1.25/1.5/2×, with zero Parley layout cache
entries. The desktop example uses the normal shell, not a second event loop. In
an Xvfb/X11 run, clicking Increment changed 0 → 1, then Tab + Space activated Reset
and returned to 0. Existing host input, accessibility and IME routing are reused;
an actual editable/IME session is still not verified on this backend.

`plico-shell/wgpu`, `plico-desktop/wgpu` and `RenderBackend::Wgpu` expose the new
choice. Skia remains preferred for Auto when compiled. No scene-based screenshots
are silently sent through Vello. Window capture dispatch now uses the active window backend and its paragraph
cache. A wgpu window screenshot is not rerouted through a compiled Skia/Vello
backend. Generic headless rendering can also select wgpu when Skia is absent;
callers must configure native text before declaring the scene.

Unknown paths/SVG/images, nonuniform rounded corners, rounded clips, arbitrary
transforms, layers/opacity and effect commands still return an indexed error. No
partially supported frame is submitted. Native provider rejection is retained in
`TextContext::take_native_paragraph_error`; the shell rejects that frame before
presentation. Diagnostics remain visible on stderr without a tracing subscriber.
Changing native provider/DPI invalidates runtime text geometry, not only pixels.

Shared API changes: `PaintCommand` paths now use `PaintShape` (`BezPath.into()` is
available; mutation explicitly discards analytical metadata). `ParagraphEngine::layout`
returns `Result<_, ParagraphError>`. Existing Skia providers return their ordinary
paragraph through `Ok`. Layout callers no longer unwrap provider failures.

The `paint-scene` bridge depends on `plico-renderer`, whose legacy dependencies
still pull Vello-related crates into the build. The new rendering path does not
invoke Vello, but completely removing those transitive build dependencies remains
a separate cleanup task. `--no-default-features` retains the independent quad core.

Damage is part of the preserved-target design and should reuse the shared tracker:
changes/removals cover old and new bounds; redraw every intersecting contributor
in painter order; preserve a full-render pixel oracle. No partial redraw is enabled
yet in this milestone. It must not cache speculative UI closures or assume that
stable IDs alone identify which state changed.

The default new backend selects wgpu's primary APIs (Vulkan/Metal/DX12/WebGPU),
respecting `WGPU_BACKEND` for explicit overrides. This avoids initializing an
unused thread-owned EGL context during concurrent headless rendering; a reproduced
EGL `BadAccess` disappeared with primary-only discovery. Legacy GL is not a tested
fallback path. Existing Skia/Vello initialization policy is unchanged.


### Optional retained-target damage replay

The shared host accepts `PLICO_WGPU_DAMAGE=1` for the wgpu renderer. The default
remains full redraw while broader applications and platform behavior are checked.
`Renderer::render_scene` and `render_scene_to_view` share the same optional damage
path. It uses a persistent RGBA target, disjoint physical damage rectangles,
replacement (not source-over) background clear and ordered contributor replay.
The final window copy is still full-surface; this does not implement platform
incremental present. Identical scenes can skip drawing/presentation. The shared
HUD receives damage coverage and present-skipped status.

Local GPU verification compares every pixel over 12 changing frames with full
redraw: translucent overlap, fractional text movement, glyph changes, removal,
clip changes, background changes and resize. The test also replaces the target
through the raw rendering API and verifies invalidation/recovery after rejected
input. Tests explicitly assert partial coverage and zero unchanged coverage, so
passing by accidentally drawing every frame fully is excluded. The shared X11
window was exercised under Xvfb with mouse increment, keyboard reset and resize.
This does not establish physical-monitor, Wayland or cross-platform correctness.

Full instance preparation still precedes damage filtering. This first stage
reduces rasterized area and removes unaffected submitted instances, but CPU
savings must be measured separately before any performance claim.


### Images and SVG resources

The shared Scene bridge now accepts image brushes on analytical boxes. Image
textures are independent of the glyph atlas. RGBA8/BGRA8 and straight or
premultiplied alpha are normalized to premultiplied RGBA before filtering; the
shader applies brush opacity and shape/clip coverage. Nearest and bilinear are
implemented. Bicubic, repeated/reflected sampling, rotation/shear and elliptical
image corners return explicit errors. Positive independent X/Y scaling is allowed
for rectangular image geometry and axis-aligned brush transforms support cropping.
Adjacent instances sharing a texture are batched; texture sorting is forbidden
because it would change transparency/painter order.

The upload cache holds at most 256 entries and 128 MiB of texture pixel payload;
individual images above 128 MiB or GPU dimension limits are rejected. This is a
cache budget, not a peak process-memory guarantee: original immutable pixel blobs,
current-frame references, GPU allocations still in flight and allocator overhead
are additional. Current-frame references keep an image alive if LRU eviction
happens before its ordered draw is encoded. `image_cache_stats` exposes entries,
texture bytes, uploads and hits. Unchanged images do not reupload in the tested
working sets.

SVG commands consume the existing usvg tree directly through resvg 0.48.1, with
embedded raster-image support. The cache retains tree identity and effective
raster dimensions, is capped at 128 entries/64 MiB of raster pixels, and does not
rerasterize merely because an icon moves. The produced premultiplied image uses
the same GPU image path. SVG currently requires positive uniform scale plus
translation. Solid shared Bezier paths now have the additional fallback below.

GPU tests verify channels/alpha, overlapping images and shapes, premultiplied
bilinear filtering, failed-resource recovery, cache eviction with 300 images and
full-versus-damaged image frames. An SVG containing a gradient, mask and stroked
path matches direct resvg pixels at 1/1.25/2x. `scene_counter` now exercises ordinary
Plico Image and SVG declarations at four DPI scales, along with native text.
Generated captures remain under ignored `reports/wgpu-scene/`.


### General solid-path fallback

Analytical rectangles/roundrects keep the SDF path. Uncommon solid fills/strokes
are converted from Kurbo elements into tiny-skia coverage, cached as premultiplied
images and submitted in normal painter order. This covers quadratic/cubic curves,
even-odd/nonzero fills, affine rotation/shear, and even-length dashed strokes with
matching start/end caps. Brush gradients/images on general paths, mixed caps and
odd-length dash arrays are explicit unsupported errors. Zero-width strokes do
not become tiny-skia hairlines. Nonfinite/singular transforms and coordinates
outside the fallback precision limit are rejected.

The cache retains the normalized command and checks full equality after hashing;
a hash alone cannot select a raster. Raster bounds include conservative stroke
reach and AA padding. Integer physical translation is factored out of cache
identity; fractional phase, transform, fill rule and stroke parameters remain in
identity. The CPU cache is bounded to 256 entries/64 MiB, and each raster is limited
to 16384 pixels per side and 64 MiB. This budget is additional to the image/SVG
caches and current-frame/in-flight resources described above.

GPU tests compare a compound quadratic/cubic even-odd path pixel-for-pixel against
direct tiny-skia at 1/1.25/2x and different fractional positions. Integer motion
must not upload a new resource. A rotated curved dashed stroke matches the direct
CPU reference within one channel level, allowing float-transform rounding. The
path sequence also runs through optional damage replay; the existing text/image/
SVG tests continue to pass. This is a coverage fallback, not evidence that complex
CPU paths are faster than Vello.


### Nested nonrectangular clip masks

Rectangular clip scopes retain their analytical fast path. Other clip outlines
are rasterized through the solid path cache as white coverage, then intersected
with the current nonrectangular mask. The GPU samples the resulting alpha in
physical coordinates in addition to any rect clip. Mask state participates in
ordered batch boundaries; pop restores the previous mask and an empty intersection
cannot leak content. Text still uses native glyph layout/rasterization, not a
whole-paragraph bitmap fallback. The shared `avatar` component now exercises its
circular image clip in `scene_counter` captures at four DPI scales.

The mask-intersection cache has a 256-entry/64 MiB pixel budget, additional to the
path/SVG CPU caches. Mask GPU textures share the existing 128 MiB image-texture
cache and frame ownership rules. A nonrectangular clip adds a coverage sample per
fragment and can split a texture batch; no performance advantage is claimed.
Clip depth above 128 and invalid mask bounds return errors. Isolated opacity now
uses the GPU group path below; blend modes and filter layers remain unsupported.

GPU tests cover nested curved/triangular masks, fractional motion, independent
CPU mask intersection, disjoint masks, pop restoration and clipped native text.
Damaged replay and full replay must be pixel-identical. Against tiny-skia's
integer source-over oracle, coverage alpha is checked within one channel level
and RGB within two levels: the GPU performs floating-point compositing into an
8-bit UNORM target, whereas the CPU oracle rounds integer color products.


### Native single-line editing and window presentation recovery

The shared single-line input used to generate Parley GlyphRun commands regardless
of the selected paragraph provider. It now uses the optional
`ParagraphEngine::supports_editing` contract and `NativeParagraph::caret_rect`,
sharing native hit tests, selections, metrics and paragraph drawing. Providers
that do not opt in retain the existing Parley path without building a redundant
native layout. Clipping is a clip-only scope, not an unnecessary isolated layer.
The Pango caret width is one physical pixel at the selected device scale.
Password presentation is shaped only as bullets; raw credential text does not
enter paragraph requests or the glyph cache.

Tests exercise the retained host path at 1/1.25/2x: UTF-8 CJK/emoji/combining/RTL
paste, selection/copy, IME preedit versus committed content and commit, physical
caret width, full-versus-damage pixel equality, and zero Parley cache entries.
The ordinary/password/clipboard compatibility test in plico-desktop also passes.
The shared X11 demo now includes an ordinary text input. Real Xvfb keyboard input
and selection deletion were verified with optional damage enabled. This is not
verification of an IBus/Fcitx candidate popup or physical-monitor DPI changes.

That real window test exposed stale presentation when a surface image was acquired
and then dropped for an unchanged frame. The window now prepares a scene plan and
skips unchanged frames *before* acquisition. Every failed acquisition invalidates
advanced damage history, requiring a full subsequent frame. GPU target contents
still live in the independent retained texture. Repeated keyboard edits displayed
correctly after this change; the pure offscreen tests alone did not expose the
swapchain behavior.


### Isolated source-over opacity groups

`PushLayer` with the default source-over blend, no filter, and finite alpha in
0..=1 now produces a GPU-only intermediate. Children are composited into a
transparent texture, then group alpha and the enclosing/layer clip masks are
applied once. Applying alpha to every child independently would incorrectly darken
overlapping content. Nested groups preserve painter order, image/mask resources
and native glyph rendering. Nondefault blend modes and filter layers still return
explicit unsupported errors.

Targets are cropped to the outward-rounded layer clip bounds. A physical origin
in the frame uniform maps global geometry into the cropped viewport, while glyph
raster phases, mask coordinates and hit testing remain in screen space. This
avoids allocating a full-window texture for every fading control. The graph is
limited to 16 isolated levels and 128 MiB of requested intermediate color-pixel
payload per frame. `layer_frame_bytes` exposes that payload; it is not total GPU
memory and excludes driver overhead, the persistent target and in-flight frames.
A phase-preservation fixture requests 81*56*4 = 18,144 bytes for its clipped group.
Zero-opacity and empty groups skip GPU allocation. Intermediates now use the
bounded texture pool described below; animation throughput still needs measurement.

Before a graph containing layers submits children, it reserves the whole graph's
conservatively visible glyph working set. Otherwise a child could recycle the atlas after its
parent had already recorded glyph UVs. Exhaustion retries from an empty atlas;
a graph that still cannot fit returns an error. Ordinary non-layer frames keep
the previous text preparation path. This preflight costs extra CPU traversal and
is a correctness step, not a performance optimization.

GPU tests verify overlapping children get group alpha once, nested alpha multiplies,
masked groups retain native text/images, crop coordinates preserve text phase,
damaged/full replay match through alpha/content/resize changes, depth rejection
recovers, and a deliberately full glyph atlas cannot corrupt parent text when
preparing a child. The shared `scene_counter` card now uses opacity to exercise
this path through ordinary Plico declarations at four DPI scales. The shared X11
window card also uses group opacity; keyboard input and a mouse counter update
were verified under Xvfb with damage enabled.


### Recyclable layer targets and early damage selection

Layer textures and their sampling view/sampler bindings now persist in a reusable pool. A matching
size can reuse a target from an earlier frame, but never one already used in the
current frame: its parent may still need to sample it. Every reused texture is
cleared before drawing; this is resource reuse, not cached layer content. The
pool retains at most 128 targets and 128 MiB of pixel payload after each frame.
Idle sizes may remain for later frames until eviction or explicit trim. A scene
without any layers releases the pool, and the shared shell's GPU-cache trim hook
now releases these intermediates without invalidating the final color target.
Driver allocation overhead and resources from queued frames are outside the pool
payload metric.

`layer_pool_stats` exposes cumulative texture creations/reuses plus retained
entries/bytes. Tracing now records per-frame creations/reuses, pool payload and
per-layer encoding spans, including shared Scene frames. The 24-frame validation
with three layers and one size change creates 72 targets when explicitly trimming
before each reference frame, versus 4 creations and 68 reuses with the pool. Every
frame is pixel-identical to that reference. This proves fewer resource creations;
it is not a CPU/GPU frame-time or power measurement. Command encoders, attachment
views and some temporary draw/instance vectors are still constructed per frame;
this is not a zero-allocation claim.

Damage selection now also culls unneeded draws before text/texture preparation,
while preserving clip/mask scope markers. The existing glyph-instance filter
remains as a second stage. Updating only one of two separated opacity groups
renders/reuses one intermediate instead of both, and matches a fresh full render.
Unchanged snapshot/readback frames retain the warmed pool. Tests cover resize,
shrinking content, distinct live nested/sibling targets, explicit trim with valid
retained final pixels, and eventual layer removal. Layer-specific tests now live
in `tests/layers.rs`.


### Shared WGSL surfaces

The native bridge now consumes shared `ShaderEffect` using the existing renderer
GPU-resource helper's neutral `render_view` entry point. No Vello scene draw or
texture-binding adapter is used by this consumer. New authored functions can use
`plico_effect`/`plico`; legacy shader assets remain valid. See
[effect contracts and limits](wgsl-effects.md) for premultiplied output, scalar
parameters, clock scheduling, cache budgets and current platform boundaries.

GPU tests cover virtual time, pause, value changes, DPI, invalid-source retention,
shader removal, local versus full damage, distinct versions of one surface ID,
transparent output, damage-skipped target retention, explicit cache trimming and
recreation, nested mask/opacity composition and recovery from a module
whose color output is incompatible with the GPU target. An Xvfb window exercise
confirmed a file-authored progress surface changes with a Rust counter and that
Animate/Pause changes the time-driven output. This is functionality verification,
not a frame-time or power comparison with Skia/Vello.

WASM checking additionally exposed a WebGL replay exhaustiveness issue: retained
fragments are now explicitly treated as already flattened, and unsupported nested
WGSL commands are checked after flattening, before canvas mutation. The new native
WGSL renderer is gated so browser builds do not call a blocking native compile
path. Browser effect compilation remains unimplemented.

## Gaussian box shadows

Shared `BoxShadow` now uses the built-in quad pipeline. Rectangular coverage is
computed with separable Gaussian CDFs; circular rounded corners use analytic
horizontal integration and 32 vertical midpoint samples. The draw rectangle
includes a four-sigma margin on each side, which also feeds damage bounds.
Zero sigma uses ordinary rounded-rectangle coverage. Positive uniform scale and
translation are supported; arbitrary affine shadows remain explicit errors.

This path creates no intermediate blur texture or CPU shadow bitmap. It is an
approximation, not a claim of matching Skia's blur kernel pixel for pixel. Tests
compare sampled rounded corners against an 8192-sample f64 convolution oracle
(at sigma 0.3/1/4/16 and radius 2/16/32), with a two-alpha-level tolerance. They
also cover straight-edge/corner alpha, symmetry, zero sigma, fractional motion,
clipping and removal. Partial redraw must exactly match this renderer's full
redraw for every frame. Broad-shadow GPU cost and physical-display appearance
still need measurement; general filter/blend layers remain unsupported.

## Linear gradients on general paths

The bounded tiny-skia path fallback now evaluates linear sRGB brushes after
rasterizing coverage. Multiple/duplicate stops, Pad/Repeat/Reflect extension and
straight/premultiplied alpha interpolation are supported. Output is premultiplied
RGBA; transparent stops retain the selected interpolation semantics. The cache
key includes brush coordinates, stops, extension and alpha mode, with full command
equality as a collision guard. Integer translation reuses the normalized raster;
fractional motion and changed stops generate distinct cached results.

This remains a CPU raster fallback, not a new GPU path tessellator. Pixel-center
color sampling is multiplied by path coverage. Nonlinear gradients, non-sRGB
interpolation, image path brushes and nonidentity brush transforms remain explicit
errors. Analytical box gradients retain their existing narrower GPU fast path.
Unit tests cover alpha modes, duplicate stops, extension and cache reuse. GPU tests
cover transparent output plus fractional motion/removal against full redraw.

## Real gallery host check

The existing gallery can select the experimental renderer without pulling in its
default Skia feature:

```sh
cargo run --locked -p gallery --no-default-features --features wgpu
```

`gallery`'s binary now requires the backend-neutral `host` feature; the default
`desktop` feature still selects Skia. Its existing `--capture`/`--scenario` preview
path currently requires `desktop`, and a wgpu-only build returns an explicit
message for these options rather than silently capturing through another backend.

A local Xvfb X11 window with `PLICO_WGPU_DAMAGE=1` displayed Workspace, Motion
(including the existing animated WGSL asset), Controls, its Quick settings popover,
and scrolled Controls including avatars and Japanese ruby. This verifies shared
application composition and event dispatch, not physical-monitor scroll smoothness
or power consumption. Glass fails explicitly with `layer blend mode/filter`;
there is no silent rendering fallback. General filtered layers remain a required
replacement gate. Generated screenshots stay in ignored `reports/`.

## Gaussian filtered layers and Glass

Single-operation Gaussian filters now use two GPU compute passes over premultiplied
RGBA. Shared filter interpretation exposes a small backend-neutral Gaussian/edge
sampling descriptor; arbitrary graph operations still fail explicitly. Sigma is
scaled by the positive uniform layer transform and limited to 64 physical pixels.
The source intermediate extends four sigma beyond the final layer clip, so source
pixels outside that clip can contribute before final masking/alpha. Transparent,
clamp, repeat and mirror sampling are encoded explicitly. Zero sigma bypasses the
filter passes. This uses discrete four-sigma kernels and RGBA8 intermediates; it is
not a promise of pixel identity with Skia/Vello blur implementations.

Filtered layers conservatively trigger full damage. The existing depth/frame-byte
budget includes the source plus two filter intermediates. Filter pipeline creation
is cached; the two work textures and uniform buffers are currently allocated per
filtered draw. Target pooling and faster wide kernels remain performance work.
No CPU readback is introduced into window presentation.

GPU tests cover premultiplied color, group alpha, zero sigma, sigma rejection,
motion/removal against full rendering, and source contribution across the final
clip boundary. The existing gallery Glass page now displays in an Xvfb X11 wgpu
window, including moving background, blurred glass and crisp foreground controls.
This resolves the unsupported-filter failure recorded in the previous gallery
check. Physical-display quality, representative GPU timing and broad filter graphs
remain replacement gates. WASM type checking passed; browser execution was not
verified.

## Pooled Gaussian workspaces

Gaussian workspaces now live with their isolated-layer pool entry. Each retains
two work textures, two parameter buffers, two compute bind groups and the output
sampling binding. Matching-size entries from previous frames can reuse all of
these; entries already used by a sibling or ancestor in the current frame remain
ineligible. Parameter writes precede that layer's queue submission, so alternating
sigma values cannot overwrite parameters for an earlier unsubmitted layer.

The pool's resident-byte calculation includes all three RGBA textures (source,
horizontal temporary and vertical output), and its existing 128 MiB/128-entry
limits apply to this combined payload. Explicit GPU cache trimming and eventual
layer removal release workspaces too. Counters expose cumulative filter workspace
allocations/reuses in `LayerPoolStats` and the frame-resource trace.

A 12-frame, three-sibling test alternates blur sigma while moving child content.
It compares every pixel against a renderer whose layer pool is cleared each frame:
3 workspace creations and 33 reuses versus 36 creations, with identical output.
Trimming, recreation and complete layer removal also verify byte accounting. This
measures allocation avoidance, not CPU/GPU frame time. Wide-kernel optimization,
filter graphs and physical-display checks remain outstanding.

## Filter timing experiment

Run manually (ignored by default, no CI performance threshold):

```sh
cargo test --locked -p plico-wgpu --lib profile_gaussian_workspace_cost -- --ignored --nocapture
```

The test prebuilds identical animated scene snapshots, warms eight frames, then
measures 32 frames per policy in pooled/cleared/cleared/pooled order. It renders to
a GPU texture, with no CPU readback or swapchain presentation. Submission wall
time includes preparation, encoding, queue writes/submissions and any implicit
backpressure; completion wall time additionally waits for the device. Neither is
a GPU timestamp measurement or process CPU-utilization measurement. Scene build,
layout and application declaration are excluded. Logs identify the actual adapter
and include p50/p95, workspace allocations and resident pixel payload.

On the local Intel Graphics (ARL), Vulkan/Mesa 25.0.7 run, the 384x256/sigma=3
completion p50 was around 1.3–1.5 ms; sigma=18 was usually 3.3–3.6 ms, with a repeat
outlier at 4.8 ms. Submission p50 was generally 0.15–0.4 ms. Pooling avoided all
32 measured workspace creations but did not establish a large frame-time win.
First-case warming and tail variation were visible, so these values are local
observations rather than stable renderer performance guarantees.

A candidate replacing per-pixel exponential calculations with a cached normalized
uniform weight table was tested and removed: the same 384x256/sigma=3 case rose to
roughly 1.8–2.1 ms, while sigma=18 only improved modestly in that run. This is
insufficient evidence to replace the current kernel. The next wide-blur experiment
should compare different sampling algorithms with pixel-error bounds and GPU
timestamps, rather than assume that moving arithmetic to CPU improves GPU cost.

## Device timestamps for filtered passes

The manual filter profiler now requests `TIMESTAMP_QUERY` only when supported and
places begin/end timestamps directly inside each Gaussian compute pass. It sums
the horizontal and vertical durations using the queue's timestamp period. Query
resolution copies 32 bytes per measured workspace; query-buffer mapping happens
after the recorded completion wall interval. No image readback is added. An
adapter without the optional feature still reports wall timings and explicitly
reports unavailable device timing. Ordinary renderer/window construction does not
request timestamps or allocate query resources.

On the same local Intel ARL Vulkan adapter, GPU pass-time p50 was about 0.17 ms for
128x96/sigma=3, 0.31 ms for 384x256/sigma=3, and 2.62 ms for 384x256/sigma=18.
Both pooled and cleared policies produced essentially the same compute durations.
Unlike the previous completion metric, these values exclude child drawing,
composition, submission gaps and CPU waiting; they still describe this device and
kernel, not an application-wide speed claim. The next optimization target is the
wide Gaussian sampling workload, not texture allocation. Timing instrumentation
itself is optional and is not a default application cost.

## Rejected line-tiled Gaussian candidate

A test-only 32-lane line tile cooperatively loads a four-sigma halo into 8704 bytes
of workgroup memory. It then evaluates the same discrete kernel from shared memory
instead of repeatedly sampling the texture. Four edge modes, partial workgroups,
small sigma and the maximum supported sigma are compared with the direct kernel;
all tested output channels differ by at most one 8-bit level.

Reproduce the candidate timing with:

```sh
PLICO_TEST_TILED_BLUR=1 cargo test --locked -p plico-wgpu --lib profile_gaussian_workspace_cost -- --ignored --nocapture
```

The environment switch exists only in the ignored test. The candidate WGSL is
included only in unit-test builds; ordinary applications continue to use direct
sampling and do not gain an experimental switch or extra dispatch branch.

On the local Intel ARL Vulkan adapter the candidate regressed: 384x256/sigma=3
compute p50 was around 1.25–1.38 ms versus the direct kernel's 0.31 ms;
sigma=18 was around 7.2–10 ms versus 2.62 ms. This rejects this particular tiling
scheme on this device, not shared-memory algorithms in general. The measurement
does not identify whether occupancy, shared-memory accesses, barriers or another
hardware/compiler interaction caused the regression. Further work should test
filtering/sampling alternatives with the same pixel oracle and timestamp protocol.

## Filter source halo at viewport edges

A regression fixture exposed premature viewport clipping: a white rectangle wholly
outside a small viewport contributed no blur inside it, while rendering the same
scene on a translated larger canvas and cropping produced the expected tail.
Layer texture padding alone was insufficient because prepared child draws had
already discarded the source samples.

Preparation now distinguishes the output viewport from the source region required
by a filter. The four-sigma halo is carried through nested layer preparation;
final clipping/masking still happens at composition. Prepared-scene cache identity
includes this source root clip, so a snapshot prepared for one sampling region
cannot be reused with another region's clipping.

The formerly failing test now compares all pixels against large-canvas/crop
references for left/right/top/bottom source rectangles, both directly inside a
filter and inside an additional opacity group. This verifies viewport-edge source
contributions without weakening the final output clip. Filter frames still use
conservative full damage; this change does not claim incremental-filter redraw.

## Externally positioned glyph runs

Linux shared `GlyphRun` commands now have a native Cairo/FreeType bitmap fallback.
The original font bytes, collection index, glyph IDs and positions are preserved;
this path does not reshape text or substitute a family. Ordinary Pango paragraphs
continue to use the existing glyph atlas. Current positioned-run coverage includes
solid fills, positive uniform scaling/translation, hinting and brush alpha.
Nondefault variable coordinates, synthetic emboldening/skew, stroke and non-solid
brushes still fail explicitly. It is not yet full arbitrary glyph-run coverage.

`plico-native::text::positioned::Font` retains shared font storage without copying
it. A real editor run encountered a roughly 793 MiB CJK collection; copying or
limiting every collection to 64 MiB was unsuitable. The resource cache keeps up to
32 native faces and 1 GiB of unique retained source bytes, deduplicated across
collection indices. Sources can be shared mappings: this counter is neither new
allocation size nor RSS. Positioned raster/position payload has a separate
256-entry/64 MiB budget, and the existing GPU image cache applies after upload.
Retained prepared scenes can keep image resources alive independently of caches.
This fallback caches whole runs, not individual glyph atlas entries; fractional
scrolling may generate new rasters and remains performance work.

The native font constructor extends cairo-rs' FreeType user-data ownership pattern
to shared byte owners, with provenance/license notices in plico-native. Tests check
font ownership, baseline positioning, integer translation invariance, premultiplied
alpha and invalid glyph/font errors. Shared replay tests check a Parley-generated
English/CJK run, integer-motion upload reuse and explicit cache clearing. Native
font/raster byte counters are exposed in frame resource traces.

Run the actual editor through wgpu with:

```sh
cargo run --locked -p gallery --no-default-features --features wgpu --example code_editor
```

An Xvfb X11 window displayed its English/CJK sample and syntax highlighting, and
keyboard insertion updated the code and caret. This verifies this editor fixture;
it does not establish terminal compatibility, real IME sessions, variable-font
coverage or physical-display scrolling quality.

## Positioned variable-font instances

Positioned glyph replay now preserves normalized OpenType F2Dot14 coordinates.
The native bridge converts them to FreeType's 16.16 blend coordinates before
creating the Cairo font face. Faces are immutable after Cairo receives them;
changing a run's weight creates/reuses a distinct face instance instead of mutating
a face referenced by Cairo's scaled-font caches. Coordinates participate in both
font-instance and raster keys. Unique font source byte accounting is shared across
instances; the 32-face cache limit still includes every instance.

Requests are limited to 64 axes with values in [-1, 1]. The conversion follows
FreeType's [normalized blend-coordinate API](https://freetype.org/freetype2/docs/reference/ft2-multiple_masters.html#ft_set_var_blend_coordinates),
not design-unit weight values. Synthetic styles and stroked glyphs remain explicit
unsupported operations. This does not add automatic font fallback or reshape runs.

Native tests raster thin/heavy instances of an installed variable font, then reraster
the thin face after heavy rendering to detect mutation/cache contamination. GPU
tests use Parley-generated weight 100/900 runs, verify changed alpha coverage,
restore the original pixels, and check separate instances/rasters without duplicate
source-byte accounting. These establish the tested TrueType variable-font path;
other variable formats and all installed font combinations are not claimed verified.

## Per-glyph affine synthesis

Positioned runs now support an affine matrix on each glyph outline independently
of baseline placement. The bridge sets Cairo's font matrix, leaving supplied glyph
positions untouched. The run's uniform scale applies to outline size and glyph-local
translation; it does not shear the line or inter-glyph offsets. Matrix coefficients
participate in raster identity, and singular/nonfinite matrices fail explicitly.
Geometric emboldening and stroked/non-solid glyph brushes are still unsupported.

A native pixel oracle draws two glyphs on different baselines by translating to
each origin, applying its outline transform and drawing at zero. The run-font-matrix
implementation matches that reference within one alpha level, including skew and
translation. GPU replay tests change skew/translation, restore the original run,
and remove it; partial redraw matches full redraw and original cache results remain
unchanged. These verify this synthesis path without claiming all affine/font/hinting
combinations or changing the application's shaping and selection geometry.

## Terminal adoption and run-level affine text

The terminal app now offers an opt-in wgpu feature while keeping Skia as default:

```sh
cargo run --locked -p terminal --no-default-features --features wgpu
```

An actual PTY fixture initially failed on italic cells: terminal replay applies
skew to the whole glyph run, rather than storing it only in `glyph_transform`.
Positioned replay now handles a finite nonsingular affine run transform by
transforming the supplied glyph origins and composing the run's linear matrix
with each glyph outline matrix before native rasterization. It never stretches
or rotates an already rasterized text bitmap. A positive normalization factor
keeps the existing uniform-scale path and integer-motion cache reuse intact.

GPU tests compare skew, nonuniform scaling, rotation and reflection with manually
transformed positions/outline matrices. They also require partial redraw to match
full redraw through transform changes and removal. This extends positioned glyph
coverage; it does not relax the separate ordinary-paragraph/shape transform limits.

A local Xvfb X11 terminal window with damage enabled displayed PTY output containing
English/CJK, ANSI colors, bold and italic. Resizing the window and the process-exit
indicator also displayed without renderer errors. This is fixture validation, not
a terminal throughput, scroll smoothness, clipboard or real IME-session benchmark.
The underlying whole-run raster fallback and its bounded caches remain unchanged.

## High-quality image reconstruction

`ImageQuality::High` now selects a 16-tap Mitchell-Netravali bicubic shader
(B=C=1/3) instead of returning unsupported. Low/Medium retain nearest/bilinear
sampling. Source RGBA/BGRA is premultiplied before filtering, and cubic ringing
is clamped into valid premultiplied RGBA before opacity/coverage are applied.
Pad extension clamps individual texel accesses. Extreme finite UV offsets are
bounded beyond the reconstruction halo before integer conversion to avoid overflow.
Sampling quality remains part of immutable image cache identity; moving an image
does not reupload its pixels. Clip masks, rounded coverage and painter order share
the existing image path.

An independent f64 Mitchell reference checks all output pixels for magnification
and modest reduction, within one 8-bit channel level. Tests also verify the result
is distinct from bilinear, transparent RGB does not contaminate edges, partial
motion/removal matches full rendering, immutable upload reuse, and large positive/
negative UV pad extension. This is point reconstruction, not a mipmap/prefilter
implementation: large minification, visual quality on representative photographs
and GPU cost still need evaluation. Rotated/sheared or repeated image brushes
remain separate unsupported cases. No overall quality/performance superiority over
Skia or Vello is inferred from these numerical tests.

## Independent image extension axes

Shared rectangular `ImageBrush` replay and low-level `ImageQuad` now support
Pad/Repeat/Reflect independently on X and Y. Nearest/bilinear use explicit hardware
address modes; bicubic applies the corresponding extension to every source texel
in its reconstruction footprint. UV reduction handles negative coordinates and
cross-period sampling; large finite coordinates are reduced/bounded before integer
indexing. Source premultiplication and valid-RGBA output clamping are unchanged.

Sampling quality plus both extension axes are included in resource identity. The
current cache owns an image/sampler pair per combination, so changing modes can
create another bounded texture entry; this is not sampler-only deduplication.
`ImageQuad::new` supplies full-source UV, bilinear, pad and opacity-one defaults.
Layers/effect outputs, native positioned-text bitmaps, SVG and path fallbacks keep
pad sampling and their existing behavior.

A 27-combination CPU oracle checks three qualities and all X/Y extension pairs,
including negative UVs and seams, within one 8-bit level. A shared-scene brush test
checks independent repeated/mirrored axes against low-level image replay and
resource reuse. General-path image brushes and rotated/sheared image geometry
remain separate coverage gaps; this change does not claim those are implemented.

## Affine images and image-filled paths

Images now support finite nonsingular affine geometry/brush transforms, including
rotation, shear and reflection. The common axis-aligned rectangular path retains
its previous fast replay. Other fills rasterize only shape coverage through the
bounded tiny-skia/path-mask caches, intersect the active parent mask, and sample
the original immutable image with a GPU world-to-source affine UV map. The map is
`normalize_image_size * inverse(shape_transform * brush_transform)`; changing a
shape/transform does not resample or reupload the source image.

The affine UV coefficients reuse existing instance attributes; no increase to the
shared instance stride is needed. `ImageQuad` can optionally supply the same
physical-world UV map, with coverage handled by its quad/active mask. Its constructor
keeps the previous full-source defaults. All sampling qualities and independent
extension modes remain available. Arbitrary image-filled paths preserve the fill
rule, including even-odd holes. Coverage-mask scope is restored before subsequent
draws, and output bounds participate in ordinary damage tracking.

GPU tests check rotated/sheared/reflected image interior texels against inverse-
transform CPU sampling, a circular path with an independently transformed repeated/
reflected brush, even-odd holes, and fractional rotation/motion/removal against full
rendering. A cropped opacity group checks world UVs remain correct with a nonzero
frame origin and that later draws do not inherit the image's mask. Returning to
ordinary replay verifies source upload reuse. Coverage is still a CPU raster
fallback for these shapes; large rotating images and animation cost need profiling.
Image strokes, projective transforms and large-minification prefiltering remain
outside this implementation.

## Compact prepared painter-order storage

Prepared painter order now stores typed-table indices instead of the largest
inline draw payload. Quad, text, image, effect and isolated-layer values live in
separate contiguous vectors owned by the immutable prepared scene. Scope/mask
markers stay in painter order. Indices are private to that scene; returned draw
references borrow its payload vectors, with no per-node box or raw pointer.
This does not change stable application IDs or introduce cross-frame node handles.

`PreparedScene::storage_stats` and the `wgpu prepared scene storage` trace expose
order, typed-payload and ink-vector capacity bytes. These exclude command snapshots,
shared backing resources, nested scene allocations and GPU/driver memory; they are
not process RSS. The shared GPU instance format is unchanged.

Manual reproduction (ignored by default):

```sh
cargo test --locked -p plico-wgpu --lib prepared_scene_storage_and_materialization -- --ignored --nocapture
```

On this 64-bit build the order entry shrank from 216 to 16 bytes. For the same
10,000-item fixture (90% quads, 10% images), order+payload vector capacity fell from
3,538,944 to 2,449,408 bytes, about 31%. Draw-reference materialization p50 was about
14.3 microseconds both before and after; tail variation was visible, so no CPU or
whole-frame speedup is claimed. The profile excludes scene construction and GPU
work. Image-only scenes can use more capacity because they add a small order index
to unchanged image payloads, and additional vector headers have a per-scene cost.
The change targets mixed GUI scenes, not universal memory reduction. Existing GPU
pixel-oracle tests continue to cover painter order, masks, text, filters and damage.

## One source texture across sampling policies

The immutable image cache now owns source textures independently of sampling
bindings. Source identity includes allocation, dimensions, pixel format and alpha
encoding. Quality plus X/Y extension selects a binding within that source entry;
changing sampling does not reconvert or reupload its pixels. The first binding is
stored directly, so ordinary single-policy images and native glyph bitmaps do not
allocate an additional HashMap. Extra policies use a small lazy binding map.

The existing 256-source/128 MiB cached texture-payload limits now count unique
source textures. Eviction removes the source and all its cached bindings together;
current-frame references keep already recorded GPU resources alive. There are at
most 27 policies per source under the current enums. Binding/driver allocation
overhead and in-flight references are not part of the texture pixel budget.
`image_cache_stats` reports source textures/bytes/uploads/source hits;
`image_binding_stats` separately reports resident bindings and cumulative creations.
Both are recorded in the frame-resource trace. Layer/effect output views are not
immutable-source cache entries.

The 27-policy CPU pixel oracle still passes, now with one upload and one resident
source texture instead of 27 uploads/textures. A 300-source/two-policy painter-order
fixture verifies every pixel even after source eviction, expects 256 cached sources
and 512 bindings, and verifies reimport evicts all policies together with correct
counters. This establishes upload/payload avoidance, not a frame-time or process
memory multiplier for applications that use only one policy.

## Pass-local image binding interning and ordered batching

Source/binding caching alone did not make adjacent repeated images batch: each draw
previously appended the same binding to a different temporary slot, and the batch
key compared slot numbers. A pass-local registry now gives the same strongly owned
`GpuImage` binding one slot. Registry identity is an Rc allocation address used
only while its current/saved pass-owner vector keeps the resource alive; it is not
a raw dereference, application ID or cross-frame identity. Nested isolated layers
save/restore the map with its owner vector. Normal root encoding clears both and
reuses their capacities.

Only adjacent instances with matching image and mask bindings merge. There is no
sorting: red/blue/red remains three draws, while repeated same-binding instances
still blend independently in one instanced draw. Different mask rectangles can
share a batch because their rectangles are instance data; changing the mask binding
or disabling masking splits it. Effect views that create distinct bindings are not
claimed to share image slots merely because their effect IDs match.

`FrameBatchStats` exposes encoded scene instances, instanced draw calls, color
passes and explicit damage clears, including child layers. These are recorded in
resource traces. They exclude blur/effect passes, load-op clears, readback and
presentation blits; failed encoding may leave partial attempt counts. Window skips
that never encode leave previous stats unchanged.

GPU tests verify 100 adjacent image instances use one scene draw, preserve
transparent red/blue/red order and repeated-alpha blending, handle same-texture
masks with different rectangles and binding transitions, and cannot confuse child
slots with parent bindings across frames or pool trimming. This demonstrates draw
submission avoidance, not an application FPS/CPU improvement measurement. Unique-
binding workloads additionally pay registry lookup cost and need representative
profiling before broader performance claims.

### Native scrolling and DPI regression coverage

`native_scroll_matches_full_replay_and_reuses_warm_glyphs_across_dpi` drives a
real `Source` scroll container with wheel events and a virtual clock. It covers
ASCII, Chinese and Japanese text at scales 1, 1.25, 2 and then 1 again, with
48 frames per scale. The fixture asserts that scrolling actually occurs.
Every damaged frame is compared pixel-for-pixel with an independent full replay.
After the first scroll cycle, glyph upload bytes and atlas recycle counts remain
unchanged; scrolling does not rerun the page's `show`, and measurement does not
fall back to Parley. The local GPU test passes.

This is deterministic rendering/cache coverage, not evidence of physical-monitor
DPI transitions or perceived scrolling smoothness. Default native text still
uses vertically pixel-snapped glyph placement.
