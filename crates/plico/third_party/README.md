# MyGO source attribution

`src/motion.rs` follows MyGO's keyed scalar animation channels, analytic spring
parameters, retargeting and reduced-motion behavior. `src/virtual_list.rs`
translates MyGO's sparse 64-row height blocks, bounded business-key anchor
search, fixed/variable row anchor retention, height-cache shifting, follow-end state and programmatic-scroll
distinction; Plico connects them to its own retained declaration, keyed motion
channels and Taffy geometry-feedback paths.

- Repository: https://github.com/egoist/mygo
- Revision: `6bd276dda679449336f68277eaaa988e1fadc54b`
- Sources: `ui/widgets.go`, `ui/motion.go`, `ui/list.go`
- License: `mygo-LICENSE` in this directory.

## Material implementation

`assets/materials/liquid_glass.wgsl` adapts the quartic/Snell lens, analytic
rounded-rectangle normal and tone curve from MyGO, Copyright (c) 2026 MyGo
contributors, MIT:

- Repository: https://github.com/egoist/mygo
- Revision: `ee3d4bd9bd741f197de231b9fd9b002e9db55577`
- Sources: `plugins/glass/glass.glsl`, `plugins/glass/glass.go`
- License: `mygo-LICENSE` in this directory.

The scene-sampling/coverage scaffold originated from Plico's Apache-2.0 shader
resource (`Plico.liquid_glass.wgsl`, SHA-256
`3a947918c0d32782a1bb065fdfb3f76c3dfe3cc9894921cf3712787ca194fe75`).
Framework-specific adaptation, parameterization, reflection and fallback code
are Apache-2.0. No GPL SDK or application business logic is included.

The interactive material also follows the hover/press timing and material-only
expansion in `Glass.BuildMaterial` / `pressed.PaintMaterial` in `glass.go`.
Retained uniform transitions, painting overflow, hit testing and scheduling are
Plico framework implementations.

Elevation presets separate contact and ambient shadows, following the two-shadow
treatment of MyGO `examples/effort-slider/effort.go`. MyGO's glass material itself
casts no shadow. The preset values are Plico's design tokens, not an assertion of
pixel equality with native macOS materials.

Inset shadow semantics follow `internal/scene/scene.go`: clip to the caster and
paint the complement of a blurred hole. The shared command, Gaussian evaluation
and Skia replay are Plico implementations.

`src/preferences.rs` and `Theme::follow_preferences` adapt the display preference
contract and 45%/40% border/secondary-text contrast adjustments from MyGO
`ui/preferences.go` at the same revision, Copyright (c) 2026 MyGo contributors,
MIT. The host notification bridge and Plico integration are maintained here.

The optical lens preset in `assets/materials/liquid_glass.wgsl` adapts the magnification,
dispersive backdrop sampling and directional rim from `src/components/liquid-glass/engine.ts`
at [bilalgursen/liquid-glass ecc6812](https://github.com/bilalgursen/liquid-glass/tree/ecc6812b3344d494e1817280cc8529c4f4b302b4).
See [the MIT license](liquid-glass-LICENSE).

The native controls in `widgets/liquid.rs` follow the component families and
interaction patterns in [bilalgursen/liquid-glass at c2a6aad](https://github.com/bilalgursen/liquid-glass/tree/c2a6aade68fd655771cb98cce21100bbdbd258f3/src/components/liquid-glass).
The Rust controls and popup neck WGSL are native implementations.

The action-button droplet contour in `assets/materials/liquid_glass.wgsl`,
the paths in `assets/materials/action_icon.wgsl`, and the corresponding timing
in `src/widgets/liquid.rs` adapt `engine.ts`, `glass-layer.tsx`, and
`glass-action-button.tsx` from bilalgursen/liquid-glass revision
`48d2260c16c0354dcd786ff9c38d216ab0d47aec` (MIT, see `liquid-glass-LICENSE`).

The glass switch and checkbox adapt the dimensions, pointer thresholds,
spring parameters and check path from `controls/Switch.tsx` and
`controls/Checkbox.tsx` at the same revision.

The Select and Dropdown glass faces in `liquid_glass.wgsl` and portal
shadows in `materials.rs` adapt the Card tint, refraction curve, edge line
and shadow from `glass-popup.tsx` and `engine.ts` at the same revision.

The clear Play button face follows `glass-button-clear.tsx`,
`glass-button.tsx`, `glass-layer.tsx`, and `engine.ts` at the same
revision for its XL layout, refraction curve, rim lighting, and shadow.

The popup opening and replay curves, joined Select contour, and centered
menu and popover growth adapt `glass-popup.tsx` and `engine.ts` at that
revision. The gallery samples follow the matching `src/docs/demos`
examples.

The Tabs following spring and selection threshold, Badge marquee cadence,
and Slide to Confirm thumb geometry and spring parameters follow
`glass-lens.tsx`, `glass-badge.tsx`, and `glass-slide-to-confirm.tsx` at the
same revision.

The frosted Card face follows `glass-card.tsx` and the radial bend in
`engine.ts`. The RangeSlider's grab offset and pressed thumb geometry
follow `controls/RangeSlider.tsx` and `controls/glass/physics.ts`.

The Switch and Checkbox share the control-lens shading used by RangeSlider.
Their pressed bevels, refraction strengths and light/dark wells follow
`controls/Switch.tsx`, `controls/Checkbox.tsx`, and
`controls/glass/{physics,runtime,tokens}.ts` at the same revision.
