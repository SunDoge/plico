# Deterministic UI rendering

`plico-render` builds ordinary Plico views at injected frame times and renders
them without opening a window. The first implementation exports PNG sequences
and pipes premultiplied RGBA frames to an FFmpeg sidecar for H.264 MP4 or VP9
WebM. It keeps one wgpu renderer, glyph cache and offscreen target warm for the
whole composition.

```rust,ignore
let mut composition = Composition::new(1920, 1080, 180, |ui, frame| {
    text(ui, "Hello")
        .font_size(64)
        .opacity(frame.interpolate(&[0., 20.], &[0., 1.]) as f32);
})
.fps(30.)
.scale(2.);

plico_render::video(&mut composition, "hello.mp4", Options::default())?;
```

FFmpeg is not downloaded by the framework. `Options::ffmpeg` selects a binary;
otherwise `ffmpeg` is resolved by the operating system. `ffmpeg-sidecar` is
built without its download feature.

`MotionBlur` samples deterministic subframes across a shutter interval and
averages premultiplied pixels. `Frame::sequence`, `shift`, `looped`,
`interpolate`, `spring` and `spring_in` use the same injected clock as normal
Plico animation channels.

This version performs one GPU-to-CPU readback per sample before writing raw
RGBA to FFmpeg. Platform encoders and direct IOSurface/D3D shared-texture/
DMA-BUF sinks are future optional backends; public wgpu APIs do not currently
provide a portable external-memory path.
