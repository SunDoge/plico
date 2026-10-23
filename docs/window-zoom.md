# Native window zoom

`plico_backend::ZoomSource` is a host-side wrapper around a `FrameSource`, independent
from OS HiDPI. It accepts 50–300% application zoom and handles Ctrl +/−/0 (Command
also works on macOS). A shared `ZoomController` exposes button/API control and
wakes the host when changed. Invalid, infinite and NaN factors are rejected.

```rust
let zoom = plico_backend::ZoomController::default();
let frame = plico_backend::ZoomSource::with_controller(Box::new(source), zoom.clone());
zoom.set(1.25);
```

The wrapper reduces the logical layout viewport and multiplies the paint scale.
It maps pointer/wheel coordinates back into the same virtual viewport, projects
semantic bounds and IME cursor/range rectangles into native window coordinates,
and preserves text offsets and host effects. Its controller lives in the host;
it is not part of hot-reloaded application state. A hidden window can keep its
controller without requiring a continuous rAF loop.

The normal native gallery supports the keyboard shortcuts. The interactive
button/input verification demo is:

```sh
cargo run -p plico-gallery --example zoom
```

The example includes buttons, a text input (for selection and IME verification),
and scrolling content. MBX GUI consumes the wrapper without adding zoom buttons
or a zoom indicator to its dashboard.

This is native host functionality. The browser host and generic hot-reload host
are not automatically wrapped; an embedding host must opt in. The normal native
gallery entry point opts in explicitly. Zoom controls are window-local and do not
alter OS display scaling or native window corner radius.
