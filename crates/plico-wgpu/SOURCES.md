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
