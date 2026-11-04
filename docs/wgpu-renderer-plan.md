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
   coverage masks, while rect clips keep the fast path. General filter graphs/non-default blend layers remain. Single Gaussian filtered
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
