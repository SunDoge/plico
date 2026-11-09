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

- GPUI Fast, Apache-2.0, commit `36956df0eb284faba6bd062f5258d5792d464001`:
  `crates/gpui_apple/src/fast/text_system.rs` (`SizedFonts`) and
  `crates/gpui_apple/src/text_system.rs` were consulted for CoreText ownership,
  exact-size font reuse, fallback runs and CoreGraphics raster configuration.
  The bounded thread-local font cache in `src/macos_text/fonts.rs` is newly
  written, not copied source. Its key also distinguishes explicit/native style
  selection, and text/tracking remain attributed-string properties. This change
  does not adopt GPUI's glyph atlas or foreground-dependent font smoothing.
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

- MyGO, MIT, commit `ee3d4bd9bd741f197de231b9fd9b002e9db55577`:
  `internal/text/coretext_darwin.go` registration and nearest-face selection.
  The CoreText application font collection uses private descriptors and family
  aliases rather than process-global registration. Ownership, per-collection
  caches and validation are Rust implementations. See `licenses/mygo-MIT.txt`.

The same MyGO CoreText revision's `systemFont` also informs AppKit UI/monospaced
font selection by native weight, local autorelease-pool scope and owned CTFont
bridging. The Rust implementation uses objc2-app-kit's public toll-free bridge;
no manually loaded Objective-C ABI is copied.

The bounded CoreText face cache retains hot exact-size faces across animation
samples using per-entry recency rather than flushing all faces at capacity.
This is newly written Rust; MyGO `internal/text/coretext_darwin.go` at
`ee3d4bd9bd741f197de231b9fd9b002e9db55577` informed the primary/style CTFont
retention principle. Plico retains its own 256-face bound and private collection
invalidation behavior; the existing MyGO MIT notice is retained.

The thread-local immutable sRGB resource in `src/macos_text/drawing.rs` follows
MyGO `internal/text/coretext_darwin.go`'s retained `coreText.srgb` at
`ee3d4bd9bd741f197de231b9fd9b002e9db55577` (MIT). Horizontal and vertical
bitmap contexts borrow the same named color space on their thread while each
context retains independent ownership. The helper is newly written Rust; the
existing MIT notice applies.
