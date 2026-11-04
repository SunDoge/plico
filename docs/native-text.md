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

macOS CoreText and Windows DirectWrite are future implementations, not stubs claimed
as working backends. No GTK window or main loop is introduced. `linux-text` is off
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

`plico-native/windows-text` exposes `plico_native::windows_text::{Context, Request,
Paragraph}` on Windows. It currently provides an isolated DirectWrite factory,
user-locale system-font layout, logical metrics/line baselines, UTF-8 caret and
hit-test APIs, and bidi-aware selection rectangles. Requests have one installed
family and an explicit paragraph base direction; CSS family lists, rich spans,
application fonts and clipping/ellipsis are not implemented in this service yet.
Raw grayscale glyph-run rasterization is available, with the limits below. It is not connected to plico-wgpu, whose Windows native text
path still returns an unsupported-platform error.

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

Before renderer adoption, finish corrected glyph compositing and connect the
same layout to shared measurement/paint, then verify actual Windows fonts,
DPI transitions, IME and window output. Cross-compilation proves none of those
runtime/visual properties.

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
mask list. Color fonts, underline/strikethrough and inline objects are explicitly
unsupported in this prototype. The renderer owns thread-local callback state and
does not advertise `IAgileObject`; the layout and factory remain UI-thread bound.

Coverage is **raw**, not final composited alpha. `Rasterized` includes the native
gamma and grayscale contrast values for future foreground-dependent correction;
these are not a simple `pow(alpha, gamma)` contract. Gamma/contrast compositing,
color glyphs, caching/atlas upload and the wgpu bridge remain unimplemented.

Windows raster API tests compile for GNU/MSVC and cover repeatability, phase and
scale, source ranges, empty content, invalid inputs and non-agile COM identity.
They have not run on Windows here; no Windows rendered-quality result follows
from these checks.
