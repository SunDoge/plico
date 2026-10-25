# Shared backend settings demo

```sh
cargo run -p backend-settings --features fltk
cargo run -p backend-settings --features vello
cargo run -p backend-settings --features skia
```

One `src/main.rs` runs all backends. A single enabled backend starts directly;
with multiple features, select one explicitly (default order: Vello, Skia, FLTK):

```sh
cargo run -p backend-settings --features vello,skia -- --backend skia
```

All backends run the same Plico view and state:

- Overview: responsive metric Grid, project list and animated Details split pane.
- Activity: keyed, reversible scrolling rows with an aligned fixed header.
- Preferences: native editing, saved state, theme and Reduce motion.

Try Collapse, Details, Run preview/Stop, navigation and Back/Forward. Animations
use injected time and finite plico-motion tweens; the host sleeps after settling.
Preferences remain in application state when another route unmounts the editor.

```sh
xvfb-run -a python3 scripts/check-fltk.py
cargo test -p backend-settings --features vello --all-targets
```
See [the FLTK experiment](../../docs/fltk.md) for scope and validation.

Skia uses a native Vulkan swapchain and the same Settings view, router, Taffy
layout, Parley text, and animations as Vello. The skia feature builds directly
against plico-runtime and plico-shell (it does not require the vello feature);
the declaration runtime is shared by both window backends. Its SVG support covers the selected Fluent Color gradients and Lucide paths;
SVG patterns, images, masks, filters and clips still return explicit errors. This is an experimental backend, not full gallery parity.

```sh
cargo test -p backend-settings --features skia --all-targets
python3 scripts/check-skia-settings.py
python3 scripts/check-settings-parity.py
```

The window smoke test starts its own Xvfb display with llvmpipe, visits all three
routes, toggles Details/theme, resizes, and saves local captures under reports/.
The parity check runs the binary with `--backend vello` and `--backend skia` through the identical
route state machine on one display and asserts the same-state cross-backend
RMSE stays well below half the different-state baseline, detecting large visual drift between backends without pretending rasterizers are pixel-identical.
See [Skia support and limitations](../../docs/skia.md).


Navigation and project entries use selected Fluent System Icons Color SVGs;
actions retain Lucide line icons. Sources and licenses live in `assets/fluent/`.
Regenerate the pinned selection with `python3 scripts/update-fluent-icons.py`;
ordinary builds embed these files offline. Activity sorting is private persistent
local state, keeping its order when leaving and returning to the page.
