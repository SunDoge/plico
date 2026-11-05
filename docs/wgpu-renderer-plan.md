# wgpu renderer direction

The goal is to replace Plico's Vello rendering path with a GUI-focused renderer,
not to reimplement every geometry/font/image library. The current implementation
is an independent quad/text/window prototype; a supported subset can now be selected with `plico-desktop/wgpu`, while arbitrary
applications still encounter explicit unsupported-command errors.

## Reuse boundaries

- Keep the immediate Engine, component identity, event/input model, virtual clock,
  retained layout and Taffy. The renderer must not impose a new application API.
- Reuse Kurbo for paths, curves, transforms, bounds and stroke geometry. Geometry
  math is distinct from GPU coverage/rasterization; Kurbo alone is not a renderer.
- Retain compatible Peniko color/brush/gradient descriptions where useful. Explicitly
  choose interpolation/blending rules rather than inheriting accidental defaults.
- Reuse image decoding, SVG parsing and resource/service code. Investigate a mature
  cached CPU raster fallback for uncommon operations before writing a complete GPU
  vector rasterizer. SVG now uses a bounded resvg raster cache; uncommon solid
  Bezier fill/stroke commands use a bounded tiny-skia raster cache.
- Use native text layout and glyph rasterization: Pango/Cairo currently works on
  Linux; DirectWrite basic paragraphs now connect to shared geometry and corrected
  run-image painting, including bounded style/color ranges and outline color glyph layers, but are cross-compiled only and still lack full color-font
  parity. CoreText is still required. Do not approximate glyph
  coverage with the rectangle SDF shader. Existing editing/IME state remains outside
  the renderer; its geometry must come from the layout actually drawn.
- Reuse tests, virtual time, screenshot previews and damage concepts. Avoid taking
  dependency on Vello-specific strip/tile/scene internals as a renderer contract.

## Quality target

A narrower renderer can improve consistency for common GUI content: 12–16px text,
small rounded controls, thin/fractional borders, images and predictable compositing.
This is a goal, not a measured overall superiority claim. The quad comparison so
far shows close interiors with AA edge differences; it does not prove better text,
SVG, filtering or arbitrary-path rendering than Vello.

Validate native text at 100/125/150/200%, in light/dark themes and during motion.
Compare with Skia, native whole-layout rendering and Vello under the same fonts,
DPI and content. Separate motion sharpness from smoothness: Y snapping avoids
fractional filtering softness but creates pixel steps. Keep the policy explicit.
Never compensate for an unknown gamma/sampling problem by silently adding weight.

## Next integration gates

1. **Text foundation:** ordered text/quads, per-DPI glyphs, bounded atlas, no
   warmed-frame texture upload, native editing geometry and GPU-only window
   presentation. Linux prototype implemented and locally verified, including private application
   font assets/aliases, native rich-text ranges and scale-synchronized application
   zoom across ordinary/retained/host-surface paths; physical
   cross-monitor DPI and actual IME sessions are not verified.
2. **Shared paint bridge:** a supported subset is implemented; consume Plico's retained output with explicit supported
   operations. Preserve analytical box geometry rather than guessing rounded
   rectangles from arbitrary flattened Bezier paths. Report unsupported operations.
3. **Resource coverage:** RGBA/BGRA images and cached resvg SVG rasterization are
   implemented. Image upload handles straight/premultiplied alpha before filtering; sampling
   policies share source textures, with separate binding and upload counters;
   nearest/bilinear/Mitchell-bicubic sampling, independent pad/repeat/reflect axes,
   opacity and ordered texture batches are verified. Pass-local binding interning
   now merges adjacent repeated images without sorting; encoded batch counters are exposed. Affine images and image-filled
   paths now combine cached shape coverage with GPU world-to-source sampling.
   Solid, linear, radial and sweep gradient general paths have a tiny-skia fallback with even-odd fill, curves,
   rotation and dashed strokes. Nested nonrectangular clips now use cached
   coverage masks, while rect clips keep the fast path. General filter graphs and background-reading blend modes remain; isolated additive Plus/PlusLighter composition is implemented. Single Gaussian filtered
   layers now use two GPU passes, and the real gallery Glass page renders. Gaussian box shadows now use quad coverage with
   four-sigma damage bounds and a numerical convolution reference. Shared native WGSL effect surfaces now use the
   existing public shader widget with local damage and injected time; texture
   inputs/backdrop effects and typed uniforms still need explicit contracts. Isolated source-over
   opacity groups now use cropped GPU-only intermediates with bounded depth/bytes.
   Intermediate targets and Gaussian workspaces are pooled with combined byte accounting, and damage skips unaffected groups before
   texture work. Allocation counts are verified; frame-time performance is pending. Bound derived caches and track resource generations.
4. **Host adoption:** integrate the shell/renderer selection, reuse existing window
   events, accessibility and input handling. Implement native Windows and macOS
   text providers. A DirectWrite layout/editing and raw grayscale-run foundation now cross-compiles,
   with a Microsoft Terminal-compatible CPU coverage/colorization port;
   wgpu integration is implemented; Windows runtime/visual validation remains. Externally positioned solid glyph runs now use a native memory-font raster
   fallback, and the real editor fixture displays and accepts input. Normalized variable coordinates are supported by immutable native font instances;
   per-glyph affine matrices cover synthetic skew. Emboldening/stroke coverage and
   terminal throughput/scroll/IME stress validation remain. The terminal fixture now
   displays ANSI styles and CJK in the wgpu window, including resize. Single-line native input, selection and IME event geometry
   are wired; Xvfb keyboard editing is verified, actual input-method sessions remain.
5. **Damage:** retain a color target, calculate changed/removed old+new bounds,
   expand for AA/filter reach, and repaint all contributors intersecting dirty
   regions in painter order. Keep a full-redraw reference and pixel-equivalence
   tests. Prepared scenes now expose native ink bounds to the shared damage tracker;
   move/removal, unchanged snapshots, explicit invalidation and resize are tested.
   GPU scissored replacement clear/replay now preserves a retained color texture.
   A 12-frame text/alpha/clip/removal/resize sequence matches full redraw exactly,
   including background changes, unchanged frames and target replacement by raw
   rendering. The shared window enables this experiment with `PLICO_WGPU_DAMAGE=1`.
   Physical-display/Wayland verification and representative performance are pending.
6. **Replacement:** run representative gallery, editor, terminal, image/OCR and
   animation apps before changing defaults or removing the Vello implementation.

WGSL is a material/coverage implementation tool, not the application's state or
layout language. The existing public shader widget accepts embedded WGSL assets and explicit
parameter/time updates. Source file edits require rebuilding the application. Further material capabilities need explicit texture/backdrop
ownership and invalidation, not hidden access to the scene.

## Damage and allocation contracts

Damage is part of the renderer design, not an assumption that unchanged commands
need no drawing. The retained color target must survive partial frames. Moving or
removing a primitive damages both its previous and current bounds; transparent
objects require replay of every intersecting contributor in painter order. Dirty
rectangles must be disjoint before replay to avoid blending a pixel twice. Resize,
background changes, unknown effect reach and lost target contents require full
redraw. A failed presentation must not commit the new damage history. Pixel tests
must compare partial rendering against full rendering for removal, overlap, text,
clip changes and resize before enabling partial redraw by default.

The current shared bridge caches one prepared scene by retained immutable command
snapshot and viewport size. Holding the snapshot makes Arc identity comparison
safe; a raw allocation address alone would not be a valid cache key. Replacing the
text provider invalidates this cache. This avoids flattening and rebuilding draws
when the exact scene is reused, but does not yet skip GPU drawing or implement
partial damage by itself. The optional partial path consumes shared tracker
rectangles, clears them with replacement blending, filters instances and replays
all contributors in painter order. It currently prepares the full instance stream
before filtering; prepared draw references and instance preparation still allocate
some temporary vectors.

A temporary Rust struct is not necessarily a heap allocation: builders and
responses may remain on the stack, in registers, or be scalar-replaced. A Vec or
String header is also distinct from its backing allocation. Historical warmed
immediate measurements recorded zero allocations; that does not establish that
LLVM eliminated arbitrary heap allocations. Existing capacity reuse and text
formatting paths can explain this result. No original bumpalo comparison has been
located in this branch, so its result cannot be attributed to compiler elimination.
Measure allocation count/bytes and stage timings first; compare buffer reuse and
an arena only for genuinely frame-local owned data. Do not retain arena references
in scenes, glyph caches or asynchronous GPU work.

## Shared shader validation regression

After tightening the WGSL vertex-input and fragment-output contracts, the local
Linux regression passed 134 `plico-renderer` library tests, 82 `plico-wgpu` tests
(including the explicit GPU tests, excluding two manual profiling cases), and
four desktop/Vello shader tests. The desktop suite requires
`--no-default-features --features vello`; with the default Skia feature that test
file is cfg-disabled and reports zero tests. These results cover the shared
validation change, not Windows runtime quality or renderer replacement readiness.

## Minification evidence

`cargo run --locked -p plico-wgpu --example image-minification` renders an opaque
257×193 single-pixel checkerboard to four smaller sizes and compares readback to
an exact source-pixel area-average reference. This deliberately adversarial
high-frequency input tests alias suppression, not general photographic quality.
On Intel Graphics (ARL), Mesa 25.0.7 Vulkan, the 8×6 output had RMSE (0–255 scale)
127.497 / 41.574 / 37.427 for Low / Medium / High. At 17×13, High remained 37.308.
The source texture stayed shared (one upload, 198404 bytes) across quality modes.

Thus current level-zero bilinear and 16-tap Mitchell reconstruction do not solve
large-ratio minification. Mip/area prefiltering needs a separate implementation:
account for mip payload in the existing byte budget, preserve premultiplied alpha,
cover odd dimensions and affine derivatives, and avoid generating mips for text
coverage or every animated offscreen target. Preserve nearest-neighbor semantics
for Low and existing magnification behavior. This was the pre-mip baseline; the following section records the implemented path and its limits.

### Initial mip implementation

The immutable-image cache now uploads an area-prefiltered mip chain for sources
at least 64 pixels in both dimensions. Odd dimensions use exact integer area
weights per reduction. Straight RGBA/BGRA conversion and premultiplication happen
before prefiltering, preventing transparent RGB from bleeding into thumbnails.
Mip payload is included in the 128 MiB source/cache budget; a source whose full
chain exceeds that budget retains level zero only. Clip masks opt out explicitly;
small glyph/run images and animated layer/effect targets retain their existing
single-level path. Quality variants still share the same source allocation.

Medium uses trilinear mip sampling; High reconstructs adjacent mip levels with
Mitchell and blends them. Low samples level zero with nearest filtering. LOD uses
unfolded UV derivatives before clip/discard and repeat/reflect folding, including
affine source mappings. Magnification remains at level zero. This is isotropic,
not anisotropic filtering: extreme skew/one-axis compression can overblur. Mips
average encoded premultiplied channels, not linear-light color. First upload adds
CPU prefilter work and roughly one-third texture memory for ordinary 2D sources;
no upload-time or power improvement is claimed.

On the same Intel/Mesa diagnostic, 8×6 Medium/High RMSE fell from 41.574/37.427 to
0.497; 17×13 was 0.529. Low stayed unchanged. Texture payload rose from 198404 to
263936 bytes with one shared upload. These checkerboard results establish alias
suppression for this fixture, not universal superiority over another renderer.
The complete local wgpu regression passed 89 tests (two manual profiles excluded),
plus Clippy and Windows/WASM checks. New GPU tests cover odd-sized large
minification, hidden transparent-white colors, premultiplied output, nearest
semantics and shared-source memory accounting.

The follow-up GPU test
`mipmapped_affine_brushes_cross_extension_seams_and_match_partial_redraw`
adds eight scenes combining Medium/High, rotating/scaling brush transforms,
negative source coordinates, Repeat/Reflect/Pad and a circular clip. Checkerboard
interior channels stay within one byte of 128, including extension seams, and
damage output matches full replay pixel-for-pixel. The test asserts that a
non-full damage frame actually occurred. This fixture validates the existing
isotropic policy; it does not remove the anisotropic quality limitation above.

### GPU mip generation

On devices with the required compute/storage limits, only level zero is uploaded;
a lazily created WGSL compute pipeline generates subsequent levels in queue order
before sampling. Each pass binds disjoint single-level views of the same texture,
with no scene readback or GPU wait. The same full-chain memory budget and source
sharing apply. Masks and single-level targets are unchanged. Devices without these
limits retain the CPU area-prefilter fallback.

A GPU readback oracle covers even/odd images and one-pixel axes, checking each
level against CPU area reduction: within one byte for an individual step and
three bytes for the complete tested chains, with RGB <= alpha maintained. Floating
GPU quantization is not claimed bit-identical to integer CPU filtering. The
checkerboard diagnostic retained its post-mip error figures. The complete local
regression passed 93 tests, excluding three manual profiles; Clippy and
Windows/WASM cross-checks passed. WASM compilation does not prove browser runtime
compute or fallback behavior.

CPU source copying/premultiplication, first pipeline compilation and GPU execution
still have costs. The earlier 78 ms CPU odd-size figure now characterizes the
fallback/reference path, not compute-enabled mip generation. No end-to-end upload
speedup is claimed without measuring the new path. DEBUG `wgpu.generate_mips`
records mip encoding/submission CPU duration and dimensions/level count; it
excludes lazy pipeline construction and does not measure GPU completion.

## Real gallery image-window acceptance

After GPU mip/upload changes, the actual gallery window was built with:

```sh
cargo run --locked -p gallery --no-default-features --features wgpu
```

Local Linux/X11 Xvfb, Intel ARL/Mesa Vulkan, damage enabled: Workspace displayed
native text, icons and controls; Images displayed the 300×225, 100-frame WebP and
GIF sample through the shared hosted image surface. Two GIF playback screenshots
400 ms apart differed by 52567 pixels; after Pause, two screenshots 500 ms apart
were identical. The network-origin sample may use its existing disk cache; this
run does not prove a fresh download. Resize from 1120×820 to 760×780 and four
Ctrl-minus steps preserved image aspect and layout without an observed stale
frame. WM_DELETE_WINDOW closed with exit code 0. Generated screenshots/logs remain
local, not checked into the repository.

This exercises real host input, animated-image decode/cache/playback, hosted paint,
application zoom and window submission together. It does not prove pixel-perfect
image quality, physical-monitor DPI transitions, frame pacing or power use under
a real compositor. Xvfb still reports missing DRI3.

## Additive isolated layers

Shared `PushLayer` accepts Normal mixing with SrcOver, Plus and PlusLighter.
Children still render source-over into an isolated RGBA8 target; group alpha and
clip apply once, then the completed image uses either source-over or saturating
One + One blending for color and alpha. Transparent source pixels leave the
backdrop intact, so the existing cropped layer/damage bounds remain valid.
Other mixes/compositions remain explicit unsupported-command errors; this does
not implement Multiply, backdrop reads or arbitrary blend graphs.

The additive pipeline is created lazily from the validated current material.
Successful material replacement invalidates that variant; rejected material
updates keep the last working state. Adjacent batch grouping includes composition
identity, and following ordinary controls restore source-over. No destination
texture copy or CPU compositing is introduced. No performance advantage is claimed.

A GPU regression checks independent expected premultiplied colors, nested child
overlap/group opacity, following normal controls, saturation, material replacement,
and damage-enabled/full replay including removal. At this point the shared tracker
still promoted non-default blends to full damage; the follow-up below verifies
actual partial redraw. The complete local suite passed 95
tests (five manual profiles excluded), with Clippy and no-default/Windows/WASM
checks. Windows and browser runtime blend results remain unverified.

### Additive damage locality and material invalidation

The shared tracker and cached-fragment locality check now classify only Normal
SrcOver/Plus/PlusLighter as local unfiltered compositions. Filters, Multiply and
other unclassified combinations retain conservative full damage. A shared unit
test checks unchanged/moved additive commands and fragment classification; the
wgpu additive GPU test now asserts non-full coverage on an actual update and
still matches full replay pixels. Locality does not imply backend support:
Skia currently rejects additive layer commands at its painting interface.

Successful wgpu material replacement clears derived layer targets and invalidates
scene damage, while retaining the CPU prepared declaration. A rejected update or
identical source preserves both caches and pipelines. The GPU test checks empty
damage on unchanged/rejected updates, cleared layers on accepted replacement,
full next redraw, and recreation of the additive pipeline without changing
ordinary scene pixels. This establishes the resource-change invalidation contract;
it is not a claim that ordinary scene commands currently use custom materials.

Regression evidence: 135 shared-renderer library tests, 96 wgpu tests (five manual
profiles excluded), 23 existing Skia raster tests (one ignored), shared/wgpu Clippy,
and Windows/WASM checks passed. Windows runtime behavior remains unverified.

## Shared scenario capture integration

`plico-desktop` capture/preview now supports its wgpu-only feature configuration.
`capture(...).renderer(RenderBackend::Wgpu)`,
`PreviewScenario::render_with_backend`, and `render_source_with_backend` select
one renderer/text service for declaration, scripted input and final offscreen
pixels. Auto continues to prefer Skia when enabled. Unsupported explicit choices
error instead of silently selecting another backend. No second preview protocol
or dynamic reload path is introduced; see `docs/preview.md` for commands.

Four wgpu capture tests passed, covering repeatable state/click/clock at 1.5×,
premultiplied capture versus straight-alpha PNG, unavailable backend errors, and
a hosted image canvas at 1.25×. Seven existing Skia capture/preview tests passed;
wgpu-only, Skia-only and combined library Clippy plus Windows cross-compilation
passed. The actual gallery Controls JSON scenario produced and visually verified
count 5 from seeded count 4 plus one scripted click. Generated PNG is local only.
macOS native wgpu text capture remains unsupported; Windows runtime is unverified.

## CoreText foundation progress

The optional native `macos-text` feature now cross-compiles single-line CoreText
metrics and UTF-8/scalar editing offsets on Apple Silicon and Intel. It retains
the actual shaped CTLine, with bounded requests and UI-thread ownership. See
`docs/native-text.md` for the exact supported API and native-test commands.
Paragraph layout and wgpu integration are still missing; this is
progress toward the macOS gate, not evidence that the gate has passed.

The CoreText foundation also has a bounded CGContext bitmap primitive using the
retained line, with portable physical-bounds tests. Native raster tests only
cross-compile; orientation, emoji and text quality remain unverified. No macOS
renderer/capture capability is enabled by this step.

CoreText's bitmap primitive now also clips physical raster bounds before its
allocation limits, while retaining full line geometry. Portable tests cover
bounded output for enormous ink; native crop-equivalence tests only cross-compile.
This prepares viewport handling but does not establish actual CoreText glyph
traversal performance or complete the paragraph/renderer integration gate.

CoreText now also supports explicit native bold/italic face selection, preserving
named styles by default and reporting unavailable family traits instead of
synthesizing them. Resolved base-face style/PostScript identity is observable.
The native style/raster regressions only cross-compile; macOS runtime/window
acceptance remains open. Normalized native descriptor weights are now accepted
and their resolved values observable; the Plico CSS-weight mapping is pending.
The foundation does not guess a CSS/native equivalence or synthesize weight.

The CoreText line foundation also accepts bounded native tracking, shared by
measurement, editing offsets and rasterization. Zero tracking preserves default
native shaping. Tests compile for macOS but have not run there; this does not
turn on macOS wgpu text support by itself.

A bounded CoreText soft-wrapping primitive now retains native CTTypesetter lines,
original UTF-8 ranges and correct line-local editing offsets. Native regressions
cross-compile on both macOS targets. Hard-break paragraphs, vertical layout,
alignment/truncation and the shared paragraph/window bridge remain unfinished;
this is not yet macOS renderer acceptance.

CoreText multiline geometry now handles explicit/empty/trailing lines and native
soft wrapping, preserving original content/separator byte ranges, logical
baselines and paragraph hit testing. Portable hard-break/line-box tests execute
on Linux. Native paragraph tests cross-compile only; alignment, rich runs,
truncation and renderer-provider/window integration remain open.
