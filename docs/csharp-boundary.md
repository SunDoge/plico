# C# / native development boundary

The desktop frontend is C#. Rust is a reusable foundation, not a second widget
implementation that every desktop component must update. Existing Rust widget
APIs remain for Rust applications and embedded/native-widget backends; they are
not the implementation path for a new managed desktop widget.

| C# owns | Rust owns |
| --- | --- |
| Widget composition and style defaults | Generic nodes, layout and retained storage |
| Themes, component-local state and validation | Native text/IME/editor mechanisms |
| Pages, router and application workflows | Focus, hit testing, pointer capture and portal mechanics |
| Interaction policy and task snapshots | Resource handles, decoding and lifecycle |
| Animation targets and WGSL effect bodies | Animation clock execution, drawing, filters/compositing and damage |
| Unit/component tests and scene configurations | Native integration/geometry/rasterization tests and capture execution |

## Adding a feature

1. Compose existing primitives in C# first. A calendar, disclosure, menu, player
   control or OCR settings form must not introduce a native widget opcode.
2. Add a native property only for a genuinely reusable layout/input/rendering
   mechanism. Foreground Gaussian blur and shared continuous-corner geometry are
   examples; a specific lyrics component is not.
3. Resources cross as owned handles; declarations/events cross as bounded batches.
   Do not send images or per-property P/Invokes every frame. Rust retains the
   application build callback/context, not captured per-widget managed closures
   or references into C# component state.
4. Keep raw P/Invoke internal. `Application`, `Ui`, resources and `IUiDispatcher`
   are the managed entry points; testing code uses `UiHarness`/`ManualDispatcher`.

## Build and test tiers

[The managed development entry](../dotnet/README.md) supplies separate targets.
`Test` is intentionally native-free. Native integration tests reuse an artifact;
only an explicitly selected `Native` target invokes Cargo. Preview uses an
existing renderer and virtual clock, not a native compilation hook.

Application and UiHarness share managed declaration/settling code. UiTask depends
on the managed `IUiDispatcher` interface instead of a concrete native Application.
Do not test a fake hand-written widget runtime when the real managed builder can
run directly. Conversely, a managed snapshot does not prove pixels, IME, layout,
keyboard focus or native event routing: those need the integration tier.

The ABI/size handshake covers memory layout; a separate protocol revision covers
required attribute/event schema semantics. Increment it when the SDK requires a
new native schema. Do not change it for ordinary C# components, styles composed
from existing properties, or shader body edits. Mismatches fail before host/UI
creation with the selected library and build instruction. Do not automatically
run Cargo or hide the mismatch behind a fallback library.

No extra heavyweight CI jobs or OS automation are added by this workflow.
