# Video playback experiment

`plico-video` is an unpublished, opt-in desktop experiment. Its default build
contains no player backend and does not link libmpv. Enable `mpv` explicitly;
provide a compatible libmpv shared library and linker development files on the
machine. Linux was tested; Windows/macOS linking and playback remain unverified.

## Run

```sh
cargo run -p plico-video --features mpv,skia --example player -- /path/to/video.mp4
# Vello reference path (not yet visually validated):
cargo run -p plico-video --features mpv --example player -- /path/to/video.mp4
```

The example takes one file path or URL. It offers play/pause and ±10-second
seeking. It renders at a fixed 1280×720 output resolution and aspect-fits the
result inside a resizable, rounded/clipped layout slot. Source aspect-ratio
letterboxing is handled by mpv. This is not a finished media-player application.

## Architecture

The host owns `Player`; UI state never serializes mpv handles or pixel storage.
A control worker owns commands, event draining and property snapshots. A separate
render worker owns the software render context. The libmpv update callback only
posts a coalesced notification into a bounded channel; it does not call mpv or
paint UI. Rendering sleeps until a notification, rather than polling at video
refresh rate. Service snapshots are sampled at up to 10 Hz (including while
paused); the demo refreshes its time label on snapshot changes.

The renderer publishes one latest immutable RGBA frame, replacing the previous
one. `VideoSurface` implements the framework renderer-widget contract and draws
that frame directly through the backend-neutral image brush API. There is no
PNG encoding, hex encoding or frame pixel data in the declaration tree. New
image identity invalidates the scene, including with damage tracking and
present skipping. The latest-frame mailbox is bounded; renderer/cache resources
can additionally retain older frames while those are in flight.

mpv's `rgb0` output has an unspecified fourth byte: the adapter explicitly sets
alpha to 255. Output storage is backed by `u32` for the required minimum
alignment. This reference path allocates an output buffer and an RGBA buffer
per frame, copies/converts on CPU, and then uploads through the renderer. It is
**not** zero-copy and hardware decoding is explicitly disabled.

Dropping `Player` requests shutdown and joins its workers; the render context
is freed before its associated mpv client. Shutdown latency with stalled network
sources has not been validated. Errors are available in `Snapshot::error`.

## Validation and limits

A locally generated 1280×720/30 fps MPEG-4 test pattern was decoded and displayed
in a Skia Vulkan window under Xvfb. A screenshot was inspected for image output,
aspect fitting and rounded clipping, with `PLICO_SKIA_DAMAGE=1` and
`PLICO_SKIA_PRESENT_SKIP=1`. This is a software-driver functional test, not a
hardware GPU performance measurement or a cross-platform certification.

```sh
ffmpeg -f lavfi -i testsrc2=size=1280x720:rate=30 -t 6 -c:v mpeg4 /tmp/plico-video.mp4
cargo run -p plico-video --features mpv --example probe -- /tmp/plico-video.mp4
cargo test -p plico-video --features mpv --lib
```

The probe asserts frame production, advancing playback position, stable paused
position, exact seek, resuming, and changed image commands. Unit tests check
invalid dimensions and stable versus replaced frame scene identity. The probe's
`mean_render_wall_us` includes libmpv's frame-timing wait; it must **not** be
reported as decode CPU time or GPU upload time. Native media/window tests are
manual; no libmpv installation or GUI test was added to CI.

Still to validate: real audio output/synchronization, subtitles, 1080p/4K CPU
and GPU costs, native platform packaging, network errors/cancellation, HDR,
direct hardware-decode texture sharing and integration with Plico's normal
scene compositor. Software output does not prove that mpv's GPU output can be
integrated with Plico's Vulkan renderer.
Any distributed libmpv binary and its dependencies need their own license and
source-distribution compliance; this experiment does not bundle native binaries.

## wgpu OpenGL GPU-render probe

The opt-in `mpv-wgpu-gl` path integrates libmpv's OpenGL render API with the
normal Plico scene compositor on Linux EGL. libmpv performs decode, color
conversion and scaling directly into a GL texture imported by wgpu. The
staging implementation remains available behind `PLICO_VIDEO_ZERO_COPY=0` as
an A/B reference and compatibility fallback.

```sh
ffmpeg -f lavfi -i testsrc2=size=1280x720:rate=30 -t 4 \
  -c:v libx264 -pix_fmt yuv420p /tmp/plico-video-h264.mp4
WGPU_BACKEND=gl cargo run -p plico-video --features mpv-wgpu-gl \
  --example gpu_probe -- /tmp/plico-video-h264.mp4
```

On 2026-11-12 the end-to-end probe rendered and composed non-black 640×360
frames on `Mesa Intel(R) Graphics (ARL)`. Hardware decode is requested with
`hwdec=auto`; actual selection depends on the available X11, Wayland or DRM
native display and must be read from libmpv diagnostics.

This does not make the default Vulkan/Metal renderer interoperable. libmpv's
stable public render API has OpenGL (and newer builds have D3D11), but no
caller-owned Vulkan or Metal target. Product integration therefore needs a
generic external-GPU-surface lifecycle in Plico, a GLES renderer option for the
first backend, and software fallback. A high-level wgpu-created texture was
readable by copy commands but sampled stale after foreign GL writes on this
Mesa/wgpu GL combination. Explicitly importing an initialized native texture
with its correct HAL resource state resolves that failure. Vulkan DMA-BUF,
Windows D3D sharing and macOS IOSurface remain separate follow-up backends.

### HAL-imported GL texture experiment

The default GPU path replaces the staging bridge with a native GL texture
imported through `wgpu-hal`. Set `PLICO_VIDEO_ZERO_COPY=0` to force the staging
reference path for A/B comparison. The texture is allocated and initialized
in the wgpu EGL context, wrapped with
`Device::create_texture_from_hal(..., TextureUses::RESOURCE)`, rendered into by
libmpv, and sampled directly by Plico. This path allocates no CPU frame buffer
and performs neither `glReadPixels` nor a per-frame `Queue::write_texture`.
Both sides issue commands through wgpu-hal's single locked GL context, so the
handoff uses `glFlush` rather than blocking on `glFinish`. A future
multi-context implementation would require an explicit fence.

```sh
WGPU_BACKEND=gl \
PLICO_VIDEO_PROBE_FRAMES=90 \
PLICO_VIDEO_PROBE_WIDTH=1280 PLICO_VIDEO_PROBE_HEIGHT=720 \
cargo run --release -p plico-video --features mpv-wgpu-gl \
  --example gpu_probe -- /tmp/plico-video-h264.mp4
```

On the same Intel ARL/Mesa machine, both paths produced 90 non-black 1280x720
frames. A single warm run measured 24% process CPU for the HAL-import path and
29% for staging; wall time was 3.62s versus 3.63s and RSS was approximately
307 MiB for both. These short samples establish functional sharing, not a
general performance claim.

This is zero-copy only across the **libmpv output texture -> Plico compositor**
boundary. The probe logged `vaapi-copy`, because libmpv could not create a VA
display from the headless EGL context, so decode -> libmpv GL rendering was not
zero-copy on this run. Windowed playback now passes the host's X11/Wayland
display to libmpv; `Snapshot::hwdec` exposes the resulting decoder mode for
verification. A DMA-BUF/Vulkan backend remains a separate future path.

## Vulkan DMA-BUF import probe

The opt-in `plico-wgpu/vulkan-dmabuf` feature proves that native Linux external
memory can enter the ordinary Plico compositor without replacing the renderer.
The probe allocates a real linear ARGB8888 buffer through GBM on a DRM render
node, writes a deterministic pattern, imports its DMA-BUF fd into the Vulkan
device, wraps it as a `wgpu::Texture`, and draws it through `ExternalSurface`.

```sh
cargo run --release -p plico-wgpu --features vulkan-dmabuf \
  --example dmabuf-probe
```

On the Intel ARL machine the 256x256 byte-exact probe passed. The adopted
device reported both DMA-BUF external memory and external semaphore-fd support.
A warm invocation took approximately 0.10s and 131 MiB RSS; this is a
correctness/capability probe, not a throughput benchmark. The feature is not in
Plico's default dependency or device path.

This first probe is deliberately single-plane. VAAPI normally exports NV12 or
P010 with multiple plane layouts, so decoder integration still needs a small
raw-ash import boundary plus plane-aware sampling and explicit producer/consumer
synchronization. It does not require replacing Plico's wgpu renderer.

## Persistent host player and separate audio

`Player::with_options(PlayerOptions { .. })` creates an idle host player. Keep
its `VideoSurface` mounted and call `load(video_url, Some(audio_url))` to replace
media without replacing the widget or its wake callback. `open` remains the
single-file convenience entry point. `PlayerOptions` also supports a referrer
and user agent, for services that require those request headers. Volume is a
normalized 0..=1 value; snapshots expose loading, EOF, volume and decoded audio
channel count. External audio is added after mpv reports the video file loaded.

The bilimini Plico frontend exercised actual Bilibili DASH video plus separate
audio with the Skia Vulkan renderer: recommendations and covers loaded, selecting
a video switched to a compact window, and an inspected screenshot showed actual
video and playback controls. A null audio device was used for the automated
manual run; decoding reported two channels, but audible A/V synchronization was
not certified. These results do not validate Vello playback, hardware decoding,
HDR or zero-copy video integration.
