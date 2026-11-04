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
  No alpha-correction shader or gamma formula has been copied or guessed; native
  parameters are returned as metadata until compositing is implemented and checked.
