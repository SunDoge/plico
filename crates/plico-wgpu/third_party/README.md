# Continuous shadow attribution

`src/quad.wgsl` adapts the continuous-corner field and Illinois contour inverse
from MyGO `internal/gpu/metal/shader.metal`, revision
`ee3d4bd9bd741f197de231b9fd9b002e9db55577`, Copyright (c) 2026 MyGo contributors,
MIT. See `mygo-LICENSE`.

Plico's Gaussian quadrature, partial smoothing, replay and clipping are framework
implementations.

`src/surface_acquire.rs` follows the single-slot background drawable acquisition
and dedicated-thread USER_INTERACTIVE QoS of MyGO
`internal/gpu/metal/present.go` at the same MIT revision. Plico adapts that policy
to wgpu textures and its event-loop proxy, with Rust ownership and worker-failure
handling. See `mygo-LICENSE`.

`Renderer::trim_geometry_buffers` follows MyGO `internal/gpu/metal/metal.go`
`releaseTextures` / builder release at the same revision: idle cleanup releases
frame-derived geometry storage while keeping reusable pipelines. Plico retains
its startup-sized instance buffer and relies on wgpu command ownership for
already encoded/submitted buffers.

`renderer::skip_interior_clip` follows MyGO `internal/gpu/gpu.go`
`Builder.markFill` at the same MIT revision. A plain, unmasked fill at least
three physical pixels inside its rectangular clip skips redundant fragment clip
coverage. Plico retains its own instance layout and shader flag; edge-adjacent,
masked, material and gradient draws retain the existing coverage path.
