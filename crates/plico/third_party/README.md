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
