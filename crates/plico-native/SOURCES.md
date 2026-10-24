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
