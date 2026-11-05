# Sources and design references

The Rust implementation in this crate is newly written for Plico. No source checkout
is required at build or runtime. The following local code was read as reference:

- MyGO, MIT, commit `7e8688284e4b1fc752169ed9db5f51a5f213a2e6`:
  [`internal/text/pango_linux.go`](https://github.com/egoist/mygo/blob/7e8688284e4b1fc752169ed9db5f51a5f213a2e6/internal/text/pango_linux.go),
  [`internal/text/raster.go`](https://github.com/egoist/mygo/blob/7e8688284e4b1fc752169ed9db5f51a5f213a2e6/internal/text/raster.go).
  Borrowed design: private Pango font map, one layout for measurement and painting,
  native hinting, scale-aware reusable raster resources, and grayscale AA on
  transparent surfaces. We use safe gtk-rs bindings, not Go's manually loaded ABI.
  The original MIT notice is preserved in `licenses/mygo-MIT.txt`.
- WGPUI, Apache-2.0, commit `6112503b5a1d5205743ccec377111e315ecec3e3`:
  [`src/platform.rs`](https://github.com/Muktidaya/wgpui/blob/6112503b5a1d5205743ccec377111e315ecec3e3/src/platform.rs),
  [`src/platform/text_system.rs`](https://github.com/Muktidaya/wgpui/blob/6112503b5a1d5205743ccec377111e315ecec3e3/src/platform/text_system.rs).
  GPUI-shaped platform boundary and physical-pixel bitmap/placement contract informed
  the module boundary. No WGPUI Rust source is copied. This local repository uses
  Cosmic Text/Swash; it is not evidence that upstream GPUI Linux uses Pango.

Pango, Cairo and gtk-rs are linked dependencies under their respective licenses;
these attribution notes do not relicense them.
- cairo-rs 0.22.9, MIT, `src/font/font_face.rs` (`create_from_ft`):
  the positioned-font bridge follows its FreeType reference/user-data ownership
  pattern, generalized to a shared font-byte owner so font collections are not
  copied. The native wrapper is narrowly unsafe at Cairo's FFI face constructor;
  the cloned FreeType face and shared byte owner remain attached to Cairo user
  data. Copyright/license notices are retained in `licenses/cairo-rs-MIT.txt`.

- GPUI Windows `direct_write.rs` was consulted for DirectWrite factory/font API
  usage while implementing the optional Windows layout prototype. No GPUI Rust
  source is copied. COM signatures and ownership wrappers come from the linked
  `windows` crate; UTF-8 indexing, validation and the public service are local.
- GPUI Windows glyph analysis was also consulted for DirectWrite grayscale texture
  format selection and scale handling. The new collector/range/budget code is local.
  No GPUI alpha-correction shader source is copied. The correction port below
  instead follows the original Microsoft Terminal implementation.

- Microsoft Terminal, MIT, commit `1283c0f5b99a2961673249fa77c6b986efb5086c`:
  [dwrite.cpp](https://github.com/microsoft/terminal/blob/1283c0f5b99a2961673249fa77c6b986efb5086c/src/renderer/atlas/dwrite.cpp)
  and [dwrite.hlsl](https://github.com/microsoft/terminal/blob/1283c0f5b99a2961673249fa77c6b986efb5086c/src/renderer/atlas/dwrite.hlsl).
  Gamma coefficients and grayscale blend equations are ported into
  `src/directwrite_correction.rs`; original functions are retained in the optional
  portable C++ reference fixture under `tests/assets/`. Microsoft copyright and
  MIT notices remain in those files and `licenses/microsoft-terminal-MIT.txt`.
  The fixed legacy thin-family list/detection is not copied; callers explicitly
  choose the thin-family contrast policy.
