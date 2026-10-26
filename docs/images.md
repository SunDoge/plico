# Raster images, animated WebP and GIF

## Implemented

`plico_renderer::image::RasterImage::decode` preserves full decoded resolution.
It no longer silently reduces images to 1024 pixels. PNG, JPEG, ICO and still WebP
are supported; EXIF orientation is applied before exposing pixels. `rgba()` exposes
straight-alpha RGBA8 for CPU consumers such as OCR. Keep this resource around rather
than decoding it on every declaration.

`DecodeLimits` bounds input bytes, dimensions and RGBA output. Defaults are 64 MiB
encoded, 16384 pixels per dimension and 256 MiB RGBA. Exceeding a budget returns an
error instead of reducing resolution. Decoder scratch memory, orientation copies,
and GPU copies are additional; these limits are not a process-memory sandbox.
`from_rgba` is a trusted, already-decoded path and does not apply these limits.

Use `image.thumbnail(width, height)` to create a separate aspect-preserving Lanczos3
thumbnail. This is explicit, never enlarges the image and leaves source pixels intact.
`with_quality(ImageQuality::Low)` selects nearest-neighbor sampling for pixel inspection;
`Medium` selects bilinear sampling. These change sampling, not the retained source.

## Animated WebP and GIF

`plico_renderer::animated_image::{AnimatedImage, AnimationLimits, ImagePlayer}` provides
bounded eager decoding, composed full-canvas frames and host-clock playback:

```rust,ignore
// Run decoding in a host worker, then send the resource back to the UI.
let image = Arc::new(AnimatedImage::decode_webp(&bytes, AnimationLimits::default())?);
let mut player = ImagePlayer::new(image);

// now is injected monotonic host time, not a clock read inside a component.
let sample = player.sample(now);
scene.draw_image(sample.image.brush().into(), transform);
// Schedule a host wake after sample.next_frame_after, if Some.
```

Partial frames, alpha blending, clear-to-transparent disposal, variable durations,
finite/infinite loops and final-frame hold are covered by generated fixtures. The
WebP background hint is intentionally not painted as an opaque UI background.
`pause`, `play`, `restart` and `seek_frame` use only injected time. Paused, completed
and still images request no future frame. Zero-duration frames use a 10 ms minimum.
Repeated samples reuse frame resources rather than decoding or uploading new identities.

`AnimatedImage::decode_gif` also decodes composed GIF frames. GIF loop extensions
are translated to total plays: no extension means one play, a finite repeat count
adds that many plays, and zero/infinite means looping.

Default animation budgets are 256 frames and 256 MiB retained RGBA, plus the per-image
limits. Oversized animations are rejected, not truncated. This is an eager cache,
not a streaming decoder for long videos. The static decode API rejects animated WebP
with an explicit error instead of silently showing only frame zero.

## Validation and current limits

```sh
cargo test -p plico-renderer --lib image
cargo test -p plico-shell --test image_quality
# Reproduce the known upstream far-edge sampler error:
cargo test -p plico-shell --test image_quality bilinear_pad_far_edge -- --ignored
```

The production GPU path is checked against source pixels at 1:1 and nearest-neighbor
4x, against bilinear interior samples, and for transparent-edge dark fringes and
composited animation frames. A generated PNG above the former 4-megapixel limit is
checked byte-for-byte after decoding. These tests measure pixel correctness, not GPU
performance or comprehensive codec conformance.

A known `vello_gpu 0.3` Pad-sampling error remains at the far image edge during bilinear
zoom: the shader clamps coordinates to `size - 1` before half-texel correction. The
ignored regression above preserves the exact failing assertion; this is not counted
as a passing quality check. Original pixels and nearest-neighbor inspection are unaffected.

## Retained surface and network gallery

`plico_renderer::image_surface::{ImageSurface, ImageSurfaceState}` implements the
framework Widget contract for a `SurfaceSource` layout slot. It paints decoded
frame handles with contain-fit scaling, without per-frame PNG encoding, repeated
network requests or decoding. Its clock is injectable; the native shell sleeps
until the next frame deadline and stops scheduling when paused, hidden or finished.
`image(cx, &ImageAsset)` remains the separate static embedded-image API.

The normal desktop gallery has an **Images** page with WebP/GIF tabs, Play/Pause,
Replay, Reload from network and error/retry states:

```sh
cargo run -p gallery
# Click Images in the sidebar.
```

The samples are Google's matching WebP/GIF animation comparison:

- https://www.gstatic.com/webp/animated/1.webp
- https://www.gstatic.com/webp/animated/1.gif

Downloads and decoding run in the resident `GalleryHost`, never in `show`.
One load runs at a time; pending selections coalesce and generation tickets reject
stale completions. HTTPS requests have a 25-second timeout and an 8 MiB encoded
limit; normal decoder budgets also apply. Successfully validated original files
are cached under ProjectDirs' gallery cache (`~/.cache/gallery/images/` on Linux).
Normal selection reads disk first; Reload from network bypasses it. A corrupt
cache is retried from the network. Loaded frames are retained in memory; samples
are not bundled or committed. The Images page is a native gallery demonstration;
the wasm/no-desktop build shows a capability notice instead.

Capture tools can load outside the UI loop and advance a virtual playback clock:

```sh
cargo run -p gallery --example capture -- --page images --image-format webp --image-ms 600 --out reports/images/webp.png
cargo run -p gallery --example capture -- --page images --image-format gif --image-ms 600 --out reports/images/gif.png
# Opt-in network-to-host integration test; never part of normal CI:
cargo test -p gallery --test images desktop_host_downloads_decodes_and_schedules_both_formats -- --ignored
```

The ordinary gallery image test uses generated bytes, no network. Renderer tests
cover GIF loops, frame timing/budgets, stale completion, pause/restart and hidden
surface scheduling with a virtual clock. The slot surface stays host-owned rather
than serialized in the application checkpoint; the default embedded-image API does
not automatically fetch URLs. This first gallery integration does not claim to
preserve image resources across module replacement.

Tiled huge-image viewing, ICC/HDR color management and animated APNG are not implemented.
