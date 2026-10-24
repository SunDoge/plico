# Optional native text

`plico-native` is a renderer/window-independent home for optional desktop capabilities.
The first implemented capability is Linux text layout and rasterization via
Pango + Cairo/FreeType, following MyGO's Linux architecture. The normal Plico
Parley/Vello text path is unchanged.

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
