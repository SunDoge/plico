# Sources

The composition, frame, interpolation, sequence, motion-blur and raw FFmpeg
pipe contracts are adapted from MyGO `plugins/render` at revision
`13a0b3e7b3e161454889ff8e1bbf16ab6ffbbe0d`, Copyright (c) 2026 MyGo
contributors, MIT. The complete notice is retained in
`licenses/mygo-MIT.txt`.

Plico uses its own immediate engine, injected clock, wgpu scene renderer and
`ffmpeg-sidecar`; no MyGO checkout is required to build or run this crate.
