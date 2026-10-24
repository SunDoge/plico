# Raster images and animated WebP

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

## Animated WebP

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

This adds renderer resources and playback, not an automatic high-level `image(cx, ...)`
widget or host scheduling integration. Apps must connect frame deadlines to their host.
Tiled huge-image viewing, ICC/HDR color management and animated GIF/APNG are not implemented.
