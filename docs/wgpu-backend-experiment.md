# Independent wgpu quad backend experiment

`plico-wgpu` is a small, offscreen renderer prototype. It renders axis-aligned
rectangles with circular rounded corners, vertical two-color gradients, uniform
inside borders, hard rectangular clips and painter-order alpha compositing.
These are supplied as typed `Quad` primitives. System text and a real-window demo
were added in the milestone below. It is **not yet an integrated Plico desktop renderer
or an implementation of the full `plico_renderer::Scene` contract**.

Base dependencies are wgpu, Naga, bytemuck, Snafu, tracing and pollster.
Linux system text additionally uses the optional `plico-native` Pango/Cairo provider. Neither Vello nor
Plico's existing renderer is on its normal rendering dependency path. Vello and
`plico-renderer` are development dependencies for the comparison example only.
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
values. The implementation retains a full offscreen color target and redraws it;
damage is not enabled yet.

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
