# Native window size and resize edges

Plico's desktop window host exposes initial size, minimum/maximum logical
client-area sizes, and the edges that can begin a custom-border resize gesture.
These options are available through `plico_vello::WindowOptions` and the
renderer/runtime/shell re-exports. They are renderer-independent.

```rust
use plico_vello::{WindowOptions, WindowResizeEdges};

let options = WindowOptions::new()
    .initial_inner_size(480, 420)
    .min_inner_size(480, 240)
    .max_inner_size(480, 900)
    .decorations(false)
    .resize_edges(WindowResizeEdges::TOP | WindowResizeEdges::BOTTOM);
```

This requests a fixed width of 480 logical pixels and a height between 240 and
900. Lock height instead to permit only horizontal resizing; equal minimum and
maximum on both dimensions requests a fixed size. `resizable(false)` disables
user resizing regardless of selected custom edges.

`resize_edges` only controls Plico's undecorated border hit testing. All four
edges are enabled by default. A corner becomes a diagonal resize only when
both adjacent edges are enabled; with only one enabled, it becomes a resize
along that edge. Dimensions locked by size bounds also disable their custom
resize edges automatically.

For native decorations, use size constraints to limit dimensions. Edge flags do
not hide/disable the OS's native border handles, maximize buttons, tiling, or
window-manager shortcuts. Size constraints are requests to the platform; their
behavior during maximizing/fullscreen/tiling depends on the compositor. This
API does not force-correct OS resize events in a loop. Mobile and Web behavior
is not certified by this desktop implementation.

At window creation, zero dimensions are normalized to at least one; minimums
win over conflicting maximums, independently of builder order. Initial sizes
and restored sizes from `WindowSizePersistence` are clamped to those bounds.
Old serialized window options default to no maximum and all custom edges.
These are creation-time options; runtime constraint mutation is not implemented.

## Try it

```sh
cargo run --example window_resize
PLICO_CUSTOM_DECORATIONS=1 cargo run --example window_resize
```

The first uses native decorations; the second uses Plico's top/bottom border
hit testing. The example provides an explicit Close button for the undecorated
case. Unit tests cover bound normalization, old option deserialization, corner
projection, and saved-size clamping. These tests do not certify every window
manager's native behavior.
