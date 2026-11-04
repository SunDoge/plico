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
  Linux; DirectWrite and CoreText are still required. Do not approximate glyph
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
   presentation. Linux prototype implemented and locally verified; physical
   cross-monitor DPI and actual IME sessions are not verified.
2. **Shared paint bridge:** a supported subset is implemented; consume Plico's retained output with explicit supported
   operations. Preserve analytical box geometry rather than guessing rounded
   rectangles from arbitrary flattened Bezier paths. Report unsupported operations.
3. **Resource coverage:** RGBA/BGRA images and cached resvg SVG rasterization are
   implemented. Image upload handles straight/premultiplied alpha before filtering;
   nearest/bilinear sampling, opacity and ordered texture batches are verified.
   Solid general paths have a tiny-skia fallback with even-odd fill, curves,
   rotation and dashed strokes. Nested nonrectangular clips now use cached
   coverage masks, while rect clips keep the fast path. Non-solid general paths,
   filter/blend layers and shared shader effects remain. Isolated source-over
   opacity groups now use cropped GPU-only intermediates with bounded depth/bytes. Bound derived caches and track resource generations.
4. **Host adoption:** integrate the shell/renderer selection, reuse existing window
   events, accessibility and input handling. Implement native Windows and macOS
   text providers. Single-line native input, selection and IME event geometry
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
layout language. Runtime shader edits already refresh headless captures without
Rust compilation. Further material capabilities need explicit texture/backdrop
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
