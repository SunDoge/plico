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
hardware decoding and GPU texture interoperability. Software output does not
prove that mpv's GPU output can be integrated with Plico's Vulkan renderer.
Any distributed libmpv binary and its dependencies need their own license and
source-distribution compliance; this experiment does not bundle native binaries.

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
