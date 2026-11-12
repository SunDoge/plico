# Third-party source attribution

`src/corner_shape.rs` adapts the continuous corner field from MyGO:

- Repository: https://github.com/egoist/mygo
- Local reference revision: `ee3d4bd9`
- Sources: `internal/raster/corner.go`, `internal/gpu/metal/shader.metal`
- Copyright (c) 2026 MyGo contributors
- License: MIT, retained in `mygo-LICENSE`

The translated implicit equation and its constants follow that source. Plico's
adaptive contour tracing, interpolation parameter and bounded translation-free
cache are its own integration. MyGO is not a build or runtime dependency.

Liquid Glass now has one framework-owned source at
`../plico/assets/materials/liquid_glass.wgsl`; its attribution and integration
notes live in `../plico/third_party/README.md`. `plico-renderer` consumes that
embedded asset through `plico::materials::liquid_glass_shader()` instead of
maintaining a second shader copy.
