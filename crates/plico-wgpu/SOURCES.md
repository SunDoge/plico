# Source references

New Rust/WGSL code written for Plico. The following MyGO sources were read as
architectural and mathematical references (MIT, commit
`7e8688284e4b1fc752169ed9db5f51a5f213a2e6`):

- [instanced operation stream](https://github.com/egoist/mygo/blob/7e8688284e4b1fc752169ed9db5f51a5f213a2e6/internal/gpu/gpu.go)
- [OpenGL renderer shader](https://github.com/egoist/mygo/blob/7e8688284e4b1fc752169ed9db5f51a5f213a2e6/internal/gpu/gl/shader.glsl)
- [effect interface](https://github.com/egoist/mygo/blob/7e8688284e4b1fc752169ed9db5f51a5f213a2e6/internal/gpu/gl/effect.glsl)

References informed the instanced-quad layout, signed-distance rounded corners,
exact square-corner pixel-area coverage, sRGB-space blending, and separation of
geometry coverage from a programmable material. The original MIT notice is in
`licenses/mygo-MIT.txt`. No MyGO checkout or Go runtime is required. MyGO has a
much broader renderer (text, images, transforms, shadows and backdrop effects);
this prototype does not claim those capabilities.

Additional reference: gpui-fast (Apache-2.0), revision
`36956df0eb284faba6bd062f5258d5792d464001`:

- [text system and phase constants](https://github.com/longbridge/gpui-fast/blob/36956df0eb284faba6bd062f5258d5792d464001/crates/gpui/src/text_system.rs)
- [Linux Cosmic/Swash text](https://github.com/longbridge/gpui-fast/blob/36956df0eb284faba6bd062f5258d5792d464001/crates/gpui_wgpu/src/cosmic_text_system.rs)
- [native input handler and candidate geometry](https://github.com/longbridge/gpui-fast/blob/36956df0eb284faba6bd062f5258d5792d464001/crates/gpui/src/platform.rs)
- `gpui_linux/src/linux/{x11,wayland}/window.rs`, `gpui_windows/src/direct_write.rs`,
  `gpui_apple/src/{text_system.rs,fast/text_system.rs}`.

No GPUI source is copied. Its separation of font/layout/raster caches, finite glyph
phases, editing state, native input protocols and window-space candidate geometry
informs the new interfaces. Linux uses Cosmic Text/Swash; this is not evidence that
GPUI uses system font rasterization on Linux. Plico currently uses Pango/Cairo there.
Native candidate anchoring uses Pango's visual-line lookup rather than copying a
backward scan over UTF-16 units. UTF-16 is converted only at a platform boundary;
Plico's layout/editing geometry uses validated UTF-8 byte offsets.

Additional MyGO damage-policy reference (MIT, commit
`ee3d4bd9bd741f197de231b9fd9b002e9db55577`):

- [Linux default partial renderer](https://github.com/egoist/mygo/blob/ee3d4bd9bd741f197de231b9fd9b002e9db55577/ui/gpu_linux.go)
- [bounded damage rectangles and half-frame full-redraw cutoff](https://github.com/egoist/mygo/blob/ee3d4bd9bd741f197de231b9fd9b002e9db55577/internal/gpu/damage.go)

Plico adopts the default-on policy and the >=50% coverage full-replay cutoff using
its existing retained-target tracker. It does not copy MyGO's swapchain buffer-age
implementation or claim native incremental present support.

The same MyGO revision's [GL drawBuilt batch/area loop](https://github.com/egoist/mygo/blob/ee3d4bd9bd741f197de231b9fd9b002e9db55577/internal/gpu/gl/gl.go)
informs Plico's batch-major damage replay: clear disjoint regions first, keep
painter order, bind a batch once and omit disjoint batch/region pairs. Plico uses
conservative instance bounds including its own AA fringe, and retains the
unculled path for custom vertex materials. This is not opaque occlusion culling.
