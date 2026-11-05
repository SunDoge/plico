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

### 分数 DPI 下的整数滚动回归

Linux 系统文字的 GPU 测试
`native_text_scroll_preserves_pixels_and_atlas_at_fractional_dpi`
在 1、1.25、1.5、2 倍 scale 下布局 Latin / 中文 / 日本語，随后正向、反向
平移整数物理像素，包括进入负坐标与返回原位。重叠区域逐像素比较原图平移结果，
并检查 glyph 数量和累计上传字节保持不变；本机 GPU 验证通过。

```sh
cargo test --locked -p plico-wgpu --test render native_text_scroll -- --include-ignored
```

该测试证明整数物理平移不改变覆盖率或重新上传字形，不证明连续亚像素滚动的
视觉平滑度，也不覆盖 Windows、真实显示器切换或 IME。

共享 scene 的 `fractional_text_damage_matches_full_replay_at_phase_and_clip_boundaries`
另外覆盖四种 DPI、40 帧分数位移／删除／恢复，带分数裁剪边界及重叠透明矩形。
局部重绘逐像素匹配全量 GPU 重绘，并断言确实出现非全屏 damage。
端到端 `native_scroll_matches_full_replay_and_reuses_warm_glyphs_across_dpi`
已补入 1.5×，五段各 48 帧的虚拟时钟滚动序列通过，保留热字形上传与不重新声明页面的断言。
这些是像素正确性和缓存验证，不是滚动主观平滑度或功耗测量。

### CoreText 字体对象复用

macOS 原生路径现在按 family、精确字号、字重和 italic 复用 `CTFont`，参考
GPUI Fast 的 `SizedFonts`。文本内容与 tracking 不属于字体缓存 key，仍在每次
构造 attributed string 时设置。测量、换行与排版因此使用同一个原生字体对象，
允许 CoreText 内部缓存跨不同文本复用；没有量化字号或合成加粗。

缓存按线程持有，最多保留 256 个字体配置，达到上限后清空缓存引用。
已有的 line 自己持有字体，淘汰不会使正在显示的文本失效。这个上限只约束缓存
持有的字体，不代表整个进程存活的字体或 CoreText 内部内存上限。

新增 macOS 测试检查对象复用、字号/样式隔离、淘汰后存活 line，以及热缓存与
新字体的宽度、caret 和像素一致性。Linux 上只完成 Apple target 的交叉编译与
Clippy，像素断言仍需在 macOS 实际运行。

这项改动不等于 GPUI 的完整文字管线：目前仍使用整行 RGBA 位图缓存，尚未改成
按 fallback font + glyph ID 缓存的字形 atlas，也没有采用其颜色相关的字体平滑策略。
