# Deterministic application testing

`plico_runtime::testing` (also re-exported by `plico_desktop`) provides a shared
command protocol and a renderer-independent `Driver<V: HostView>`. `Tester` is
the application-facing name for the same type and adds no second runtime.

```rust,ignore
use plico_runtime::testing::{Command, Driver};
let mut ui = Driver::new(MyView::default(), 820, 480)?;
ui.execute(Command::Click {
    role: "button".into(), label: "Increment".into(), occurrence: None,
})?;
assert_eq!(ui.view().count, 1);
ui.execute(Command::Focus {
    role: "button".into(), label: "Increment".into(), occurrence: None,
})?;
ui.execute(Command::Key { key: "Enter".into() })?;
assert_eq!(ui.view().count, 2);
ui.advance(std::time::Duration::from_millis(200))?;
ui.resize(1000, 640)?;
let semantics = ui.snapshot();
let declarations = ui.declarations();
let retained_layout = ui.placed_nodes();
```

For ordinary interaction tests, the MyGO-style name-based helpers use the same
semantic tree and real hit-testing path:

```rust,ignore
use plico_runtime::testing::Tester;
let mut ui = Tester::new(MyView::default(), 820, 480)?;
assert!(ui.has_text("Count:"));
ui.click("Increment")?;
ui.drag_by("Move tile", 24.0, 12.0)?;
ui.type_text("hello")?;
ui.key("Enter")?;
```

`find` and the pointer helpers reject ambiguous names. A control and its static
text child sharing one accessible name resolve to the control; two independent
controls with the same name still require a role/occurrence query.

### Feedback while developing a desktop component

Express the reported failure as an observable rule before changing the implementation.
For a state transition, send the real input through `Driver` and assert both the
application value and its exposed control state. For a scrolling or animation
bug, use `Driver::check_frames` to check a relationship between consecutive
semantic snapshots. Return a message with the measured values when it fails.
The driver reports the first failing frame and elapsed virtual time.

For widget interaction regressions, put the test in `crates/plico-runtime/tests`.
Running a runtime test builds the layout and text stack without the desktop shell,
Gallery, or wgpu. Run a single interaction
while editing, then the complete component test target for action sequences:

```sh
cargo test --locked -p plico-runtime --test number_input number_input_routes
cargo test --locked -p plico-runtime --test number_input
```

A clean build of the keyboard interaction test took 9.70–9.84 s after moving
the Gallery theme fixture out of the runtime. Restoring its old dev-dependencies
on the same source took 19.55–19.56 s. Both runs used Linux x86-64, Rust 1.100,
the CI profile and 16 build jobs. The timings include executing the assertion.
Each trial used a fresh target and build directory, cached dependency sources,
offline Cargo and no compiler wrapper. Reversing the trial order gave the
second measurement. CI checks the runtime dependency graph to catch a desktop
or GPU dependency returning through a test fixture.

For a warm edit to a reader component, the targeted native test took 0.54 s
from saving the change to checking the new caption after clicking. Rebuilding
the desktop executable took 0.67 s for the same assertion, and rebuilding the
WASM guest and checking it in a fresh host took 0.71 s. Use the native test for
this loop. Use a resident WASM host when an edit needs to preserve a live session.
The fixture, build settings and repeatable probe are in the
[reader experiment](../benchmarks/wasm-ui/README.md#warm-edit-to-a-passing-interaction-assertion).

The Gallery sidebar test clicks the toggle, checks the content edge every 10 ms
in both directions, and checks total travel. Run just this contract while editing:

```sh
cargo test --locked -p gallery --test sidebar sidebar_moves_content
```

The Linux CI job runs the whole sidebar suite, including navigation and
cold-replay comparisons. Frame checks sample at the chosen interval. Use
renderer tests for pixels, clipping and shader output, and platform tests
for OS input or window behavior.

Commands serialize with Serde, for example:

```json
{"action":"click","role":"button","label":"Increment"}
```

Snapshot, click, focus, slider value, text input and key commands use the same
`dispatch` function as the existing headless development host. The driver has
no transport or external process dependency. It executes each command, builds
its resulting frame, and returns a semantic snapshot. It does not open a window
or initialize a GPU. Normal applications do not instantiate the driver.

Targets use exposed semantic roles and labels. Missing and ambiguous targets
are errors; repeated labels in click/focus commands require a zero-based `occurrence`. Disabled actions
are rejected. Declaration errors surface directly. Set explicit focus before a
keyboard test rather than assuming an OS restored focus in a particular way.
Key commands represent key-down events with no modifiers; specialized modifier,
keyup or pointer tests can use `Driver::dispatch_event` with a typed `UiEvent`.

`Driver::view` exposes immutable application state for assertions;
`Driver::update_view` mutates it, requests a rebuild and settles the resulting
frame as one operation. The driver deliberately keeps its `Source`, Engine and
Adapter private. Structural tests use `placed_nodes`, `theme`, `declarations`
and `snapshot` instead of reaching into retained runtime storage.
The virtual clock advances only when the test asks it to; `advance` uses one
runtime frame, not a simulated series of OS vsync events.

## Incremental cache oracle

`DifferentialTester<V: HostView + Clone>` keeps one long-lived incremental
runtime. After every command, resize, clock advance or settle operation it also
creates a cold runtime from the initial view and replays the complete operation
log. It compares declarations, semantics, retained layout, representative
hit-tests and logical Scene commands:

```rust,ignore
use plico_runtime::testing::{Command, DifferentialTester};
let mut ui = DifferentialTester::new(MyCloneableView::default(), 820, 480)?;
ui.execute(Command::PointerClick {
    role: "button".into(),
    label: "Add row".into(),
    occurrence: None,
})?;
ui.resize(1000, 640)?;
ui.advance(std::time::Duration::from_millis(100))?;
ui.settle()?;
```

This is deliberately an expensive correctness oracle for focused regression
tests, not the default recipe runner or a benchmark. Arbitrary `update_view`
closures are not accepted because a closure cannot be replayed; expose the
state change as a deterministic UI command or application-owned test action.
Scene comparison excludes process-global font blob IDs and other allocation
identity. GPU pixel tests remain the stronger oracle for rasterization,
backdrop effects and platform fonts.

For GPU pixel assertions and offline screenshots, the native `wgpu` feature
provides a high-level capture helper. It owns the `Scene`/`TextContext` setup,
uses retained frames when available, and performs a bounded number of settle
passes before reading premultiplied RGBA or writing a PNG:

```rust,ignore
let mut source = plico_desktop::Source::new(MyView::default());
let frame = plico_desktop::capture(&mut source)
    .size(800, 600)
    .settle_passes(8)
    .render()?;
assert_eq!(frame.rgba().len(), 800 * 600 * 4);
// Or replace `.render()` with `.write_png(path)`; the source remains borrowed.
```

The settle limit prevents a source that continuously requests another pass from
capturing forever. It is not a general timeout for application background jobs;
wait for those through the application's own polling/snapshot contract first.

Linux CI runs offscreen pixel tests on Mesa Lavapipe. The assertions read the
checkbox rim at fractional scale, rounded clips and Gaussian shadows directly
from RGBA. To run the same tests on a machine with a Vulkan adapter:

```sh
WGPU_BACKEND=vulkan cargo test --locked --profile ci -p plico-wgpu \
  --test pixel_snap --test analytic_clips --test shadows -- --ignored --skip profile_ --test-threads=1
```

OS input translation, swapchain presentation and platform-specific font
appearance still require platform smoke tests. The Windows CI job checks
compilation.

For JSON fixture state, scripted commands and virtual-time screenshots, use [scenario previews](preview.md). Native hot reload has been removed; tests drive statically linked views.

Pointer commands use window-logical coordinates: `{"action":"pointer_move","x":675,"y":480}`
and `{"action":"pointer_leave"}`. They travel through the runtime's normal pointer event path,
including hover and `UiContext::pointer_position()`, without moving the OS cursor.
Invalid non-finite coordinates from Rust callers return `Error::PointerPosition`.

The runtime redeclares on pointer movement only when the current UI declaration reads
`cx.pointer_position()`. Removing that read restores idle behavior for otherwise unchanged views.
