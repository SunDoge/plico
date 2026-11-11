# Deterministic application testing

`plico_runtime::testing` (also re-exported by `plico_desktop`) provides a shared
command protocol and a renderer-independent `Driver<V: HostView>`.

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

For CPU pixel assertions and offline screenshots, the native `skia` feature also
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

This protocol tests UI behavior and layout, not OS input translation, GPU
presentation, fonts installed on another platform or screenshot quality. Those
remain local window smoke checks. CI does not run them or expose a full/window
validation switch; it runs the focused Rust protocol test and checks the
mainline with Clippy. A separate Windows job checks that the mainline compiles;
it does not run tests or window automation. Optional backend SDK checks are local tasks.

For JSON fixture state, scripted commands and virtual-time screenshots, use [scenario previews](preview.md). Native hot reload has been removed; tests drive statically linked views.

Pointer commands use window-logical coordinates: `{"action":"pointer_move","x":675,"y":480}`
and `{"action":"pointer_leave"}`. They travel through the runtime's normal pointer event path,
including hover and `UiContext::pointer_position()`, without moving the OS cursor.
Invalid non-finite coordinates from Rust callers return `Error::PointerPosition`.

The runtime redeclares on pointer movement only when the current UI declaration reads
`cx.pointer_position()`. Removing that read restores idle behavior for otherwise unchanged views.
