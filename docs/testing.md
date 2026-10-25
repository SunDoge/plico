# Deterministic application testing

`plico_runtime::testing` (also re-exported by `plico_vello`) provides a shared
command protocol and a renderer-independent `Driver<V: View>`.

```rust,ignore
use plico_runtime::testing::{Command, Driver};
let mut ui = Driver::new(MyView::default(), 820, 480)?;
ui.execute(Command::Click {
    role: "button".into(), label: "Increment".into(), occurrence: None,
})?;
assert_eq!(ui.source.view.count, 1);
ui.execute(Command::Focus {
    role: "button".into(), label: "Increment".into(), occurrence: None,
})?;
ui.execute(Command::Key { key: "Enter".into() })?;
assert_eq!(ui.source.view.count, 2);
ui.advance(std::time::Duration::from_millis(200))?;
ui.resize(1000, 640)?;
let semantics = ui.snapshot();
let declarations = ui.declarations();
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
keyup or pointer tests can use the source's existing typed input APIs directly.

`source.view` exposes the application under test for assertions. When mutating
it directly, call `source.request_rebuild()` before executing a snapshot command.
The virtual clock advances only when the test asks it to; `advance` uses one
runtime frame, not a simulated series of OS vsync events.

This protocol tests UI behavior and layout, not OS input translation, GPU
presentation, fonts installed on another platform or screenshot quality. Those
remain local window smoke checks. CI does not run them or expose a full/window
validation switch; it runs the focused Rust protocol test and checks the
mainline with Clippy. A separate Windows job checks that the mainline compiles;
it does not run tests or window automation. Optional backend SDK checks are local tasks.
