# System text with an independent wgpu compositor

This Linux-only experiment tests the MyGO-style split: system text shaping and
rasterization, a reusable glyph atlas, and our own GPU compositor. It does not
replace a Plico backend. The new compositor never calls Vello; SkParagraph is a
separate reference column, not the implementation of the experiment.

## Reproduce

Install Pango/Cairo development packages and Noto Sans, CJK and color-emoji fonts
(the same prerequisites as [native text](native-text.md)), then run from the
repository root:

```sh
cargo run --locked -p plico-native --features linux-text --example system_text_wgpu
cargo test --locked -p plico-native --features linux-text
```

The probe requests a Vulkan adapter and prints its name/type. Do not interpret a
software-adapter run as hardware GPU performance. Output goes to ignored
`reports/system-text-wgpu/`:

- `pango-wgpu-skparagraph-{scale}x.png`: left is a complete Pango/Cairo bitmap,
  middle is Pango-shaped glyphs rasterized individually by Cairo and composited
  with wgpu, right is Plico's real SkParagraph raster backend. Sizes are 12–48px,
  weights 400/500, scales 1/1.25/1.5/2. Same requested font families and first-line
  baselines; fallback and bidi defaults can differ between engines.
- `motion-sampled-vs-snapped.png`: eight positions, moving by 1/8 physical pixel
  per row. Left translates a phase-zero bitmap with linear texture filtering;
  right uses four horizontal raster phases and pixel-snapped vertical origins.
- `blend-encoded-vs-linear.png`: identical glyph masks and foreground colors,
  composited in encoded sRGB byte space on the left and linear light on the right.
  No coverage contrast curve, dilation or extra font weight is applied.

## What is implemented

Pango resolves fonts, shaping, ligatures, bidi runs and glyph positions. Cairo
rasterizes each glyph using grayscale AA, medium hinting and hinted metrics.
These are explicit probe settings, not settings read from the desktop environment.
The cache key is `(resolved Pango font object, glyph ID, horizontal phase)`.
There are four horizontal phases; raw floating-point translation never enters the
cache key. Negative translations use Euclidean quotient/remainder.

A fixed 2048×2048 RGBA atlas uses shelf packing and transparent padding. Cairo's
premultiplied coverage/color pixels are preserved. Monochrome glyphs can be tinted
without re-rasterizing; color glyphs retain their colors. Color detection currently
checks whether bitmap RGB channels differ; this is a probe heuristic, not a
complete color-font metadata API.

The shader expands six vertices per glyph instance. One instanced draw composites
a paragraph, including mixed color glyphs. The normal target is RGBA8 UNORM with
premultiplied alpha blending in encoded space. The control target is RGBA8 sRGB:
its fragment shader decodes unassociated foreground color before premultiplication,
and the render target blends in linear light and encodes the result.

The CPU atlas consumes 16 MiB and the GPU atlas another 16 MiB. Font/shaping caches,
entry objects, shaped glyph arrays, render targets and readback buffers are extra.
It is bounded by construction and returns an error if full; it does not yet evict
or grow. Atlas changes upload the whole texture in this probe, so cold uploads are
intentionally inefficient. Once warmed, moving text uploads no texture data.

## Observations on 2026-11-04

Environment: Linux, Intel Graphics ARL integrated GPU, Vulkan, Pango 1.56.3,
Cairo 1.18.4, Noto Sans / Noto Sans CJK SC. Cargo's default development profile
was used, not a release performance benchmark.

- The independent glyph-atlas output displays Latin, CJK, Arabic, ligatures and
  color emoji. Visual inspection found it close to the complete Pango/Cairo
  reference at 1x. This is not a pixel-equivalence guarantee across engines.
- After warming all four phases, 100,000 translations of a 12-glyph sample caused
  **zero additional cache entries and zero additional atlas uploads**, enforced by
  assertions. Across the scale/style samples plus that motion warmup there were
  519 entries, approximately 1,282 KiB of glyph rectangles in a fixed 16 MiB atlas.
  This verifies this translation path, not process-wide memory stability under
  arbitrarily changing documents or fonts.
- CPU placement, cache lookup and allocation of the instance Vec took roughly
  0.46 microseconds per translation in that small sample. Shaping, new glyph
  rasterization, GPU buffer allocation, submission, waiting and readback are
  **excluded**. This is not a comparison of full GUI frame costs.
- Merely changing blend space reduced red-channel darkness integrated over the
  white-background control image by about 19% in linear-light mode. The probe
  asserts that the two modes differ. This metric is not perceived weight or a
  quality score; it demonstrates that the same masks can look lighter without
  any shaping or outline change.
- Fractional bitmap translation visibly softens strokes; snapping preserves
  sharper edges but introduces pixel steps. System rasterization does not remove
  the clarity-versus-motion tradeoff.

## Decision and limits

This is sufficient evidence that a small wgpu compositor can use system-generated
text without depending on Vello's text coverage policy. It gives Plico control of
sampling, blend space and atlas lifetime. It does **not** establish that a custom
renderer is faster, prettier or cheaper to maintain than Skia/Vello.

The experiment is headless. It does not provide rounded rectangles, paths, image
filters, clip/layer handling, damage tracking, a window renderer, editable caret /
selection / IME integration, font reload, dynamic atlas eviction, or DirectWrite /
CoreText implementations. A full renderer would still need those contracts.
Keep Skia as the working application backend while evaluating this route.
