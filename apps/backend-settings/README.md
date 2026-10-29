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

## Explicit-region API comparison (opt-in)

The existing Preferences page also has a region implementation. Both variants
use the same page shell, controls, labels and style values; the default routed
application is unchanged.

```sh
# Region implementation; local profile state belongs to the page component.
cargo run -p backend-settings --example preferences_regions --features regions,skia
# Original Settings::preferences function in the same standalone shell.
cargo run -p backend-settings --example preferences_regions --features regions,skia -- --immediate
# CPU update benchmark: native event handling, Engine, text and Taffy; no GPU.
cargo run --release -p backend-settings --example preferences_compare --features regions -- --iterations 5000
cargo test -p backend-settings --features regions --test preferences_regions
```

The comparison covers editing/preview, notifications, an advanced branch,
save/reset, theme and Reduce motion. The region page uses RegionApp instead of a
custom View forwarding wrapper. Global theme/motion values live in the root;
editor, notifications, advanced and saved status live in the page component.
Only marked state is checkpointed; the editor buffer deliberately resets on
reload. Reduce motion is compared as a setting; this isolated page has no active
animations and does not verify the routed application's animation policy.

Tests replay identical semantic operations at 900px and 480px, compare control
states and leaf geometry (within one logical pixel). Notification changes execute
one watch; typing executes the editor watch and one property binding, while the
page root and event toolbar remain untouched. Preview and saved-status text use
`.reactive(...)` without extra layout containers. The native regions example also
binds font size, weight and color of its live style preview.
See [measured results and API tradeoffs](../../docs/regions-settings.md). This
small page currently favors immediate mode for most end-to-end CPU updates;
smaller declaration scope alone does not imply a faster layout pipeline.

### Scale and update density

```sh
# 1/8/32/128/512 fully mounted real forms; update 1, 10%, or all forms per frame.
cargo run --release -p backend-settings --example preferences_scale --features regions -- --iterations 100
# Same region tree, forcing a full declaration (diagnostic control).
cargo run --release -p backend-settings --example preferences_scale --features regions -- --updates 1 --full-control
# Keep the ordinary viewport; update the first visible form.
cargo run --release -p backend-settings --example preferences_scale --features regions -- --updates 1 --viewport-height 780
cargo test -p backend-settings --features regions --test preferences_scale
```

`--sizes`, `--updates`, `--scenarios`, `--iterations` and `--warmup` configure the
matrix. Percentages round up and duplicate K values are removed. Theme uses one
global broadcast, not K independent toggle clicks. Targets are resolved outside
timing; K native events are queued before one CPU frame. Semantic fingerprints
of the first measured and final frames must match between implementations;
those checks run outside timing. Tests additionally check exact changed subsets,
unchanged sibling forms, geometry and stable editor identities.

Without `--viewport-height`, height is 780px per form: all forms are mounted and
visible, with no framebuffer allocated. This is a retained-tree stress test,
not a recommended substitute for virtualization. The default CI does not run
the benchmark matrix. The measured scaling results and limits are recorded in
[the comparison document](../../docs/regions-settings.md).
