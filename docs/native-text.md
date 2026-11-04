# Optional native text

`plico-native` is a renderer/window-independent home for optional desktop capabilities.
The first implemented capability is Linux text layout and rasterization via
Pango + Cairo/FreeType, following MyGO's Linux architecture. The ordinary Skia and Vello text paths are unchanged.

## Run

Linux needs `pkg-config`, Pango/Cairo development libraries and fonts. On Debian/Ubuntu:

```sh
sudo apt-get install libpango1.0-dev fonts-noto-cjk fonts-noto-color-emoji
cargo test -p plico-native --features linux-text
cargo run -p plico-native --features linux-text --example system_text
```

The example generates local comparison images at 1x, 1.5x and 2x under
`reports/system-text/`. Native and Parley samples use the same family request,
size, weight and baseline, and both pass through Plico's production GPU renderer.
Those generated images are not committed.

## Contract

```rust,ignore
use plico_native::text::{SystemText, TextRequest};

// Create and use on one host thread. Send the returned Arc<TextBitmap> to the UI.
let mut text = SystemText::new(16 * 1024 * 1024)?;
let mut request = TextRequest::new("Settings / 设置");
request.size = 14.;
request.scale = 1.5;
let bitmap = text.rasterize(&request)?;
```

Each result contains straight-alpha RGBA8, physical dimensions and origin, logical
layout size, first-line baseline, line count and missing-glyph count. Measurement
and pixels come from **the same Pango layout**. Pango supplies Fontconfig fallback,
HarfBuzz shaping, bidirectional layout and wrapping. The font-map context is private;
there is no global default map shared across workers.

Draw pixels 1:1 in physical space, at `logical_origin * scale + pixel_origin`.
The origin preserves negative italic/combining-mark overhangs. The comparison
example converts the pixels to `RasterImage` and sends them through `Scene::draw_image`.
Do not measure with Parley and substitute these pixels afterwards: line breaks,
advances, baselines and caret geometry can differ.

Gray AA and medium hinting are explicit, inspired by MyGO's settings-daemon-free
Linux defaults. This works on transparent surfaces without assuming an LCD subpixel
order or an opaque background. It does **not** yet follow desktop font settings.
No geometric emboldening is applied by Plico; Pango's normal font matching and
fallback policies apply. Font size is absolute logical pixels, not points.

A bounded LRU reuses entire rendered text resources. Keys include text, family,
size, weight, italic, width, scale and color. The caller chooses a cache byte budget;
entry/string allocator overhead and Pango/Cairo's own caches are additional.
Eviction releases cache ownership but does not invalidate caller-held bitmaps.
Call `clear()` to drop derived bitmaps; this is not a system-font reload API.

Inputs are capped at 64 KiB UTF-8, and raster output at 16384 pixels per axis and
64 MiB per bitmap. Rejected requests return Snafu errors. Limits are guardrails,
not a process-wide memory sandbox; native shaping and temporary surface copies
consume additional memory. Rasterize/cache on the host, not per-frame in show().

## Scope and evidence

Implemented: Linux plain-text paragraphs, wrap, weight/italic, system font listing,
DPI, metrics, owned bitmaps and cache. Tests cover invalid inputs, transparent color
conversion, empty/space metrics, wrapping, Unicode, eviction and caller-owned output
survival. A GPU test compares native pixels composited over white at 1x/1.5x/2x
against the CPU alpha reference, including non-opaque foreground color.

This is an opt-in platform capability and comparison, **not a replacement for the
ordinary text widget**. Editable caret/selection/IME integration, rich-text runs,
font assets and glyph-atlas caching are not connected here. Whole-paragraph bitmaps
cost more memory and cache invalidation work than reusable glyphs, and changing
color invalidates a bitmap. Rebuilding at target scale is required after DPI changes;
rotated/continuously scaled text remains better suited to the outline path.

macOS CoreText remains unimplemented; the experimental Windows DirectWrite service
and wgpu bridge are described below. No GTK window or main loop is introduced. `linux-text` is off
by default and target-gated; default builds, WASM and ESP-IDF do not link Pango/Cairo.

See [source and license notes](../crates/plico-native/SOURCES.md) for the local
MyGO and WGPUI references. Other useful future platform services include desktop
appearance/font settings, application activation, native menus and credential
storage; those are not implemented by this change.

## Independent GPU compositor experiment

[System text + wgpu](system-text-wgpu.md) adds a standalone glyph-atlas example,
SkParagraph comparisons, blend-space controls and finite-phase motion/cache checks.
It is separate from the whole-paragraph API above and is not a production backend.

`text::atlas` now exposes system-resolved glyph layouts and a fixed-budget raster
atlas reused by `plico-wgpu`. Layouts provide UTF-8 hit testing, selection/caret
geometry, native UTF-16 conversion and composition candidate anchors from their
own Pango layout. See the [wgpu window/text milestone](wgpu-backend-experiment.md).

## Windows DirectWrite foundation (experimental)

### Shared window acceptance fixture

```sh
cargo run --locked -p plico-desktop --no-default-features --features wgpu --example wgpu-text
```

This fixture uses ordinary Plico components and the shared desktop host. It has
Latin/CJK/Arabic/Hebrew samples, 120 scrollable rows, a native editing field,
font-size switching and a one-row update. It deliberately avoids shaders,
emoji, custom fonts and rich spans so their separate support gaps do not prevent
testing basic Windows text. It is not a replacement for testing those features.

On Windows, verify typing and IME composition, selection/caret placement, scroll
clipping, resizing and moving between monitors with different scaling settings.
Repeat with `PLICO_WGPU_DAMAGE=1` in the environment to compare damage replay.
Record OS, GPU, scale settings and screenshots when reporting a failure.
The example builds/runs on Linux. A full Windows GNU cross-check of the desktop
example also passes in this Linux environment with Zig 0.17's bundled Windows
C toolchain and LLVM ar 19. A temporary local compiler wrapper adapts cc-rs's
Rust target spelling to Zig's target spelling; no system package or repository
dependency was changed. This overcomes the earlier missing-MinGW check failure,
but a successful cross-check does not prove linking or Windows window behavior.
A subsequent full GNU build reached the final executable link after compiling
the Rust dependency graph. That link still failed because this Zig setup cannot
resolve Rust's GNU target MSVCRT/pthread libraries. LLVM dlltool was available
through another temporary tool-name adapter. No `.exe` or Windows execution is
claimed; use a matching MinGW runtime/toolchain or native Windows MSVC before
treating the fixture as built for distribution. Zig is not a project requirement
or a recommended substitute for that release-toolchain validation.

Local Linux/X11 acceptance used an actual wgpu window under Xvfb with the NVIDIA
Vulkan adapter: initial/scroll/resize screenshots were inspected, font-size
switching worked, and ASCII typing plus Ctrl+A selection was verified visually
with damage enabled. Generated screenshots remain ignored local reports. This
does not validate real IME, physical monitor transitions or Windows behavior.

The same window was also run at forced X11 initial scales 1.25, 1.5 and 2.0 with
damage enabled. Winit logs confirmed each actual scale. ASCII typing/selection
and wheel scrolling were exercised at scale-adjusted physical input coordinates;
initial and scrolled screenshots were inspected at native pixel resolution.
No stretched bitmap text, CJK baseline displacement or stale scroll-clip pixels
were observed in these samples. This is a visual acceptance result, not a
frame-pacing, power or cross-monitor transition measurement. Reproduce a scale
on Linux/X11 with:

```sh
WINIT_X11_SCALE_FACTOR=1.25 PLICO_WGPU_DAMAGE=1 cargo run --locked \
  -p plico-desktop --no-default-features --features wgpu --example wgpu-text
```

Use 1.5 or 2 for the other runs. On Xvfb, wait for the window to become visible
and explicitly focus it before synthesizing keyboard input; merely finding its
title can precede mapping and give a false input failure.

`plico-native/windows-text` exposes `plico_native::windows_text::{Context, Request,
Paragraph}` on Windows. It currently provides an isolated DirectWrite factory,
user-locale system-font layout, logical metrics/line baselines, UTF-8 caret and
hit-test APIs, and bidi-aware selection rectangles. Requests have one installed
family and an explicit paragraph base direction; CSS family lists,
multiline ellipsis and per-range line heights are not implemented in this service yet.
Raw grayscale glyph-run rasterization is available, with the limits below.
The experimental plico-wgpu Windows provider now connects ordinary paragraph
measurement/editing to the same native layout used for corrected run images.
The window installs this provider at the current scale; native geometry remains
in logical DIPs. Images use the shared premultiplied image compositor, clips,
opacity layers and damage pipeline. This bridge has only been cross-compiled.

Contexts/layouts are confined to their creating thread. A paragraph retains its
native layout and factory references; it does not borrow the request's text or
the context. Its UTF-8/UTF-16 index maps reject mid-scalar and surrogate-interior
offsets. Point hit testing uses DirectWrite's complete cluster length when moving
to a trailing edge, rather than adding a single UTF-16 unit. Caret widths are one
physical pixel at the requested scale; all other geometry stays in logical DIPs.
Text (1 MiB), size/width and native output buffer counts are bounded, and errors
are returned through Snafu.

Verified here: Windows GNU cross-check of the service/tests, Windows MSVC Clippy,
and the platform-independent Unicode index tests running on Linux. The native
API tests compile but have **not run on Windows**. Run them on Windows with:

```sh
cargo test --locked -p plico-native --no-default-features --features windows-text --lib
```

Before renderer adoption, verify actual Windows fonts,
DPI transitions, IME and window output. Cross-compilation proves none of those
runtime/visual properties.

The wgpu bridge supports unquoted comma-separated system family names, basic
weight/italic/wrapping, Start/Center/End/Justify alignment and UTF-8 editing
geometry. Alignment is set on the retained DirectWrite layout after intrinsic
measurement, so caret/selection geometry and raster positions agree. Native
tests check centered/trailing caret and selection shifts, unchanged intrinsic
widths and wrapped justification; they are cross-compiled only here.
Uniform line height accepts logical pixels or the shared font-size multiplier.
The service splits extra leading around the first natural line's baseline and
sets native uniform spacing, preserving glyph sizes. All native caret/selection
and raster geometry use that layout; tests cover two-line baseline spacing,
relative/absolute height equivalence and invalid inputs. Per-span line height
is not implemented. These Windows tests have only been cross-compiled here.
Uniform character spacing uses
[SetCharacterSpacing](https://learn.microsoft.com/en-us/windows/win32/api/dwrite_1/nf-dwrite_1-idwritetextlayout1-setcharacterspacing)
before intrinsic measurement: zero leading spacing, authored trailing spacing
in reading order, and zero minimum advance. Finite values within ±4096 DIPs are
accepted, including negative spacing. This is DirectWrite's native character
spacing behavior, not a promise of pixel-identical spacing with Pango or Skia.
Tests cover changed intrinsic widths, caret advances, wrapping and invalid input;
they have only been cross-compiled here.
`Context::layout_spans` applies UTF-8 font-size, weight and italic ranges before
native measurement. It validates scalar boundaries and bounds the span count
to 4096; later overlapping ranges win. Optional RGBA range colors use immutable
private COM drawing effects retained by DirectWrite. The synchronous glyph-run
callback copies their packed color into each raw mask; it holds no borrowed
application state or closure. `None` restores the paragraph foreground, including
for a later overlapping span. Native palette layers retain their RGB and use
the effective range foreground's alpha. Unknown drawing-effect interfaces fail
instead of silently losing color. The wgpu adapter accepts colored font spans
without per-range line height. Native line/caret/raster geometry use the styled layout, without
copying a separately shaped prefix. Tests cover non-ASCII range conversion,
overlap order, changed metrics and invalid ranges; they cross-compile only here.
Tests also cover colors after context drop, inheritance reset, opacity and
corrected image output. All Windows-specific tests are cross-compiled only here.
Single-line ellipsis uses native character trimming and a factory-created
ellipsis sign. The collector accepts only that retained inline object's COM
identity, guards recursive inline reentry, and synchronously rasterizes its
native glyph callback (whose source description may be absent). Ellipsis masks
use the empty source range at the original text end; ordinary text retains its
original UTF-8/UTF-16 mapping. The shared adapter accepts no-wrap ellipsis or
`max_lines == 1` ellipsis without hard newlines. Intrinsic requests remain
untrimmed. Tests cover LTR/RTL trimmed lines, unchanged intrinsic widths,
preserved offsets and the ellipsis's glyph output; they cross-compile only here.
Clip-only `max_lines` retains the complete native layout and filters raster
callbacks by the visible lines' original UTF-16 ranges. It never reshapes a
shortened prefix. A run crossing the line boundary is explicitly rejected rather
than partially drawing wrong glyphs. Visible height/baselines and line-fragment
width are reported separately from full-text intrinsic widths. Hit tests clamp
to the visible endpoint, hidden carets return no geometry through the shared
interface, and selection is restricted to visible text. Native newline lengths
preserve blank-line endpoints; clipped-end carets use the previous cluster's
trailing edge except for empty final lines. Tests compare visible mask bytes to
the full layout and cover blank lines and hidden selection; they cross-compile
only here. Full shaping still occurs on a cache miss, so this is not text
virtualization.
Per-range line height and multiline ellipsis
are explicitly rejected. DirectWrite outline color layers are now enumerated and
rasterized in native painter order. Each raw mask retains an optional straight
palette RGBA; foreground layers use the authored text color, while palette layers
retain their own RGB and multiply palette alpha by authored opacity. Layer runs
are copied before advancing the native enumerator, under the existing run/glyph/
byte limits. Their source range conservatively identifies the enclosing original
run, since native layer descriptions may be absent. Bitmap/SVG color formats and
newer paint graphs are not implemented by this outline-layer interface; this is
not general color-font parity.

A color-capable face alone does not reject a monochrome run:
the service uses
[TranslateColorGlyphRun](https://learn.microsoft.com/en-us/windows/win32/api/dwrite_2/nf-dwrite_2-idwritefactory2-translatecolorglyphrun)
and permits grayscale fallback only for its documented `DWRITE_E_NOCOLOR`
result. Other native errors propagate. The Segoe UI Emoji numeric/color-layer
fixture and palette/opacity tests are Windows-only and have only been
cross-compiled here; the installed-font fixture explicitly
skips on systems without that optional family.
The bridge infers the first paragraph's base direction using unicode-bidi's
Unicode P2/P3 implementation, including isolate handling and an LTR default for
neutral text. This scan does not allocate a dependency graph or duplicate native
shaping. Independent base directions for multiple hard paragraphs within one
layout are not implemented. Portable direction tests run on Linux; the native
RTL editing/raster positioning test is cross-compiled only. This is not Windows
gallery feature parity or a validated default renderer.

Run images use four horizontal phases with vertically snapped baselines;
integer translations reuse images. Layout and raster caches each have a
512-entry / 64 MiB accounted Rust-payload limit (raster accounting conservatively
includes its retained paragraph). These are separate budgets and exclude COM
allocations, externally retained entries and the shared GPU image cache. No
memory/performance claim follows from these limits alone. On Windows, run the
bridge tests with `cargo test -p plico-wgpu --lib windows_paragraph::tests`.

Each raster entry retains color-independent raw coverage and only its latest
corrected RGBA tint. Changing base text color reuses native coverage rather than
calling DirectWrite rasterization again; returning to an older tint recolorizes
the coverage instead of retaining every animation color. The raster byte budget
includes both R8 coverage and RGBA output. Live prepared scenes can still retain
previous images until released. `Renderer::windows_text_cache_stats` exposes
native rasterization and CPU colorization counts separately from layout/image
cache payloads; counters survive explicit cache trimming. These counters and
the recoloring identity tests are cross-compiled, not Windows runtime evidence.

Windows scene preparation checks native glyph-overhang bounds before requesting
coverage, so an entirely offscreen paragraph can retain layout without generating
R8/RGBA images or GPU uploads. The estimate includes `2 * scale + 2` physical
pixels of margin for layout-coordinate precision, grid fitting and snapping.
[DirectWrite documents](https://learn.microsoft.com/en-us/windows/win32/api/dwrite/ns-dwrite-dwrite_overhang_metrics)
that overhang is untransformed and can differ from final hinted pixel bounds.
The native test compares this estimate against actual run-mask rectangles for
italic, centered, tight-line-height text across scales; an opt-in Windows GPU
test checks zero raster calls offscreen and a raster call after moving onscreen.
Both tests have only been cross-compiled here. Visible paragraphs still rasterize
all of their runs; this is paragraph culling, not per-line virtualization.

DirectWrite metrics expose `min_content_width` and `max_content_width` separately
from the final constrained content width. The service measures hard lines with
wrapping disabled, queries DirectWrite's minimum width, and then applies the
requested available width to the same retained layout. No-wrap requests report
their maximum content width as their minimum. Windows tests check that changing
the available width preserves both intrinsic widths; these tests have only been
cross-compiled here, not executed on Windows.

`Context::resolve_family` accepts an ordered slice of already parsed family
names and checks the context's retained current font collection. It maps
`sans-serif`/`system-ui` to Segoe UI, `serif` to Times New Roman, and `monospace`
to Consolas, subject to installation. It returns `None` if no candidate exists,
leaving the host's final fallback policy explicit. This is primary-family
selection, not CSS parsing or a per-character fallback chain. Layout uses the
same system collection that resolution queried. Its Windows tests likewise
cross-compile here but have not executed on Windows.

### Windows private application fonts

`Context::register_font(bytes, alias)` and the shared `FontAsset` registration
hook now support private memory fonts on Windows 10 build 14393 or newer.
This uses
[DirectWrite's memory font loader](https://learn.microsoft.com/en-us/windows/win32/api/dwrite_3/nf-dwrite_3-idwriteinmemoryfontfileloader-createinmemoryfontfilereference)
with a null owner object, requesting a native copy at registration. No borrowed
Rust buffer, system installation or temporary font file is retained. Files are
analyzed before committing a new collection, and system fallback families remain
available. Aliases are context-local, case-folded lookup names for the file's
native family; an aliased file must contain exactly one family. This is name
selection in a combined collection, not font metadata rewriting or a guarantee
that an installed face with identical native metadata is distinguishable.

Each attempted file has its own registered loader guard. Failed candidates drop
their native objects before unregistering that loader, without replacing the
active collection. Successful collection snapshots share guards through `Rc`;
paragraphs keep their snapshot until their native layout drops, so destroying
or updating the context cannot unregister a live paragraph's font files.
Snapshots conservatively retain all font loaders in their collection, not just
the primary family. The service caps successful registrations at 64 files and
128 MiB of copied file payload per context; this excludes COM object/collection
overhead. Existing FontAsset source bytes remain separately shared, so native
copying has a memory cost. No total-RSS bound or zero-copy claim is made.

The wgpu provider deduplicates identical asset/family registrations and clears
derived layout/raster caches after registration. Explicit cache trimming retains
registered fonts. `windows_text_cache_stats` reports registered files and native
font-copy payload separately. Tests cover private aliases, context isolation,
old layouts surviving context replacement/drop, invalid-candidate rollback and
deduplication/trim. These Windows-only tests have been cross-compiled, not run
on Windows here. Older Windows may use basic system text but returns an explicit
unsupported error when memory font registration is requested.

### DirectWrite raw run coverage

`Paragraph::rasterize(scale, phase)` now invokes `IDWriteTextLayout::Draw` with a
private, non-agile COM renderer. It rasterizes the callback's exact native glyph
runs immediately through `IDWriteGlyphRunAnalysis`; borrowed native glyph arrays
do not escape the callback and no second shaping pass is introduced. The
returned ordered `GlyphMask`s own R8 coverage, physical bounds and source UTF-16
ranges. They contain no COM pointers and can outlive the paragraph.

The collector uses explicit grayscale antialiasing and native recommended raster/
grid-fit modes. Outline/default recommendations fall back to a renderable natural
symmetric mode. The single-channel `ALIASED_1x1` texture name describes the byte
format, not an instruction to disable antialiasing. Fractional phase is supplied
in physical pixels; glyph positioning stays in logical DIPs until native raster.

Output is limited to 4096 nonempty runs, 100,000 glyphs, 16,384 pixels per mask
axis and 64 MiB total coverage. Native failure returns an error, never a partial
mask list. Unsupported color formats, underline/strikethrough and non-ellipsis inline objects are explicitly
unsupported in this prototype. The renderer owns thread-local callback state and
does not advertise `IAgileObject`; the layout and factory remain UI-thread bound.

Coverage is **raw**, not final composited alpha. `Rasterized` includes the native
gamma and grayscale contrast values for future foreground-dependent correction;
these are not a simple `pow(alpha, gamma)` contract. CPU correction/colorization
is now available as described below. Color glyphs, caching/atlas upload and the
wgpu bridge remain unimplemented.

Windows raster API tests compile for GNU/MSVC and cover repeatability, phase and
scale, source ranges, empty content, invalid inputs and non-agile COM identity.
They have not run on Windows here; no Windows rendered-quality result follows
from these checks.

### Corrected grayscale colorization

The portable `directwrite_correction` module (behind `windows-text`, also testable
on Linux) ports Microsoft Terminal's MIT-licensed gamma table and grayscale
blend equations at commit `1283c0f5b99a2961673249fa77c6b986efb5086c`. It prepares
foreground-dependent contrast and intensity once per color, then corrects R8
coverage without allocation in the pixel loop. It does not expand outlines or
change font weight. Source attribution and license are retained in the crate.

`Rasterized::colorize(straight_srgb, thin_font)` turns ordered raw masks into
`ColorMask`s containing premultiplied sRGB RGBA8 and unchanged physical bounds/
source ranges. It retains raw coverage so tint/opacity changes do not require
Native layout or rasterization again. Geometry and total 64 MiB RGBA output are
validated before output allocation; allocation/native failures return no partial
result. Automatic canonical thin-family detection is not implemented: the flag
explicitly selects the upstream legacy-font contrast policy for the whole batch.
A future provider must resolve that policy per font run where necessary.

Portable Rust tests cover the upstream gamma ratios, known colored outputs,
monotonicity, endpoints, opacity, valid premultiplication and invalid inputs. The
optional standalone C++ oracle at
`crates/plico-native/tests/assets/directwrite-correction-reference.cpp` uses the
original pinned C++/HLSL functions with small portable vector wrappers. Its sample
outputs agree with the Rust port within 1/255. Cargo/CI does not compile C++ or
fetch upstream sources; the numeric assertions are checked in. Regenerate them
manually from the repository root with:

```sh
c++ -std=c++17 -O2 crates/plico-native/tests/assets/directwrite-correction-reference.cpp -o /tmp/dwrite-reference
/tmp/dwrite-reference
```

This verifies the coverage math against the published implementation, not native
Windows coverage quality, per-monitor settings or actual GPU/window parity. The
Windows-only colorization and COM tests cross-compile but have not run here.
