# wgpu renderer direction

The goal is to replace Plico's Vello rendering path with a GUI-focused renderer,
not to reimplement every geometry/font/image library. The current implementation
is an independent quad/text/window prototype; arbitrary applications still cannot
select it through `plico-desktop`.

## Reuse boundaries

- Keep the immediate Engine, component identity, event/input model, virtual clock,
  retained layout and Taffy. The renderer must not impose a new application API.
- Reuse Kurbo for paths, curves, transforms, bounds and stroke geometry. Geometry
  math is distinct from GPU coverage/rasterization; Kurbo alone is not a renderer.
- Retain compatible Peniko color/brush/gradient descriptions where useful. Explicitly
  choose interpolation/blending rules rather than inheriting accidental defaults.
- Reuse image decoding, SVG parsing and resource/service code. Investigate a mature
  cached CPU raster fallback for uncommon path/SVG operations before writing a
  complete GPU vector rasterizer. This fallback is a design option, not implemented.
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
2. **Shared paint bridge:** consume Plico's retained output with explicit supported
   operations. Preserve analytical box geometry rather than guessing rounded
   rectangles from arbitrary flattened Bezier paths. Report unsupported operations.
3. **Resource coverage:** images, SVG/path fallback, clip scopes, opacity/layers and
   shader material resources. Bound derived caches and track resource generations.
4. **Host adoption:** integrate the shell/renderer selection, reuse existing window
   events, accessibility and input handling. Implement native Windows and macOS
   text providers, and validate real text-input sessions.
5. **Damage:** retain a color target, calculate changed/removed old+new bounds,
   expand for AA/filter reach, and repaint all contributors intersecting dirty
   regions in painter order. Keep a full-redraw reference and pixel-equivalence
   tests. Initial prototype intentionally redraws fully.
6. **Replacement:** run representative gallery, editor, terminal, image/OCR and
   animation apps before changing defaults or removing the Vello implementation.

WGSL is a material/coverage implementation tool, not the application's state or
layout language. Runtime shader edits already refresh headless captures without
Rust compilation. Further material capabilities need explicit texture/backdrop
ownership and invalidation, not hidden access to the scene.
