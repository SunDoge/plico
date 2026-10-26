# System-wide hotkeys

Plico follows MyGo's host-owned global shortcut model: register once outside
`show`, keep resources in the executable across UI reloads, and deliver activations
on the event-loop thread. The Rust implementation uses
[Tauri's global-hotkey](https://github.com/tauri-apps/global-hotkey) 0.9 for
Windows, macOS and X11, and [ashpd](https://docs.rs/ashpd/0.13.13/ashpd/) for the
Wayland XDG GlobalShortcuts portal. MyGo's `modules.go` and Linux
`hotkey_portal.go` were read as design references; no Go implementation was copied.

Nothing is enabled by default. Renderer-free services are exposed by
`plico-native` with `global-hotkeys`; native shell integration is exposed by
`plico-shell` with the same feature. The normal Plico core, wasm and ESP-IDF
builds do not acquire this dependency graph.

## Shell integration

```toml
plico-shell = { path = "../plico/crates/plico-shell", features = ["global-hotkeys"] }
```

Use `GlobalHotkeyExtension` with `run_windows_with_factory_and_extensions`:

```rust
use plico_shell::{ExtensionContext, WindowResourceKey};
use plico_shell::hotkeys::{GlobalHotkeyBinding, GlobalHotkeyExtension, HotkeyEvent};
let extension = GlobalHotkeyExtension::new(
    vec![GlobalHotkeyBinding {
        id: "show-launcher".into(),
        accelerator: "CmdOrCtrl+Shift+K".into(),
        description: "Show the launcher".into(),
        action: window_key,
    }],
    |event: &HotkeyEvent, window: &WindowResourceKey, cx: &mut ExtensionContext<'_>| {
        match event {
            HotkeyEvent::Pressed { .. } => { cx.show_window(*window); }
            HotkeyEvent::Failed { message, .. } => eprintln!("Hotkey: {message}"),
            _ => {} // Registration/release is not an activation.
        }
    },
);
```

The extension creates registrations only after the OS event loop starts. Native
callbacks enqueue value events and wake an idle loop; the user handler runs on
that loop and can show/hide windows or send a host-service message. Actions need
not be `Clone` or `Send`, and no UI closures or application state cross the module
ABI. Shutdown/drop unregisters keys. This small host callback is not a widget
callback retained by the UI engine.

Runnable hide/show example (a plain test window, not a gallery UI):

```sh
cargo run -p plico-shell --features global-hotkeys --example global_hotkeys
```

## Direct service API

For another window host, construct `plico_native::hotkeys::GlobalShortcuts<A>`
on its native event-loop thread, passing a wake callback. Use
`register(id, accelerator, description, action)`, `is_registered(id)`, `status(id)`,
`action(id)`, `try_next()`, `unregister(id)` and `unregister_all()`.

IDs identify application actions and should remain stable across runs. Equivalent
accelerators and duplicate IDs in a manager are rejected without replacing the
existing action. OS registration errors retain their details. An already queued
event from an unregistered or rebound ID cannot invoke its replacement action.
The native router filters repeat presses while a key is held. It owns
`global-hotkey`'s process-wide event handler: do not separately install another
handler or consume that crate's global receiver in the same process.

## Wayland semantics

On a Wayland desktop, Plico selects the portal rather than attempting XWayland
key grabs. A successful `register` means **queued**, with `Pending` status;
`try_next()` later reports `Registered` or `Failed`. The desktop may display a
confirmation UI, choose another accelerator, or refuse the request. Use the
returned trigger label instead of claiming the requested chord is active.
`is_registered` includes pending requests; inspect `status` for readiness.
After failure, unregister the ID before retrying. Desktop shortcut-label changes
and session loss also generate events.

The current adapter uses one cancellable portal session per action, which makes
individual unregister possible without rebinding unrelated actions. Many actions
can therefore produce multiple authorization interactions; grouped/batched
registration is a future improvement. Calls run on a dedicated async worker;
unregister/drop cancels and closes sessions asynchronously. No UI thread waits
for desktop permission. Portal creation is bounded, and close attempts are bounded
so an unavailable desktop cannot hold shutdown indefinitely.

An installed desktop entry/application identity may be necessary for the portal.
Unsupported desktops return an asynchronous error. Activation events retain the
portal's optional `activation_token`, but Plico does **not yet** apply it to
Winit window activation. Receiving a global action works independently of focus;
`show_window` cannot promise compositor-authorized foreground focus on Wayland.

## Validation and upstream patch

Headless unit tests cover alias conflicts, action preservation, status transitions,
stale deliveries, failures and portal trigger conversion. Local Linux/Xvfb tests
exercise real unfocused key delivery, conflicts, unregister, Drop and missing
DISPLAY handling:

```sh
cargo test -p plico-native --features global-hotkeys --lib
xvfb-run -a env -u WAYLAND_DISPLAY XDG_SESSION_TYPE=x11 \
  cargo test -p plico-native --features global-hotkeys --test global_hotkeys \
  -- --ignored --test-threads=1
```

These OS tests are opt-in locally; normal CI does not run desktop automation.
Linux Clippy and Windows compilation include the optional API. Linux/X11 has
been exercised locally. Wayland portal code is compiled but has not yet been
validated against a real authorization/activation session; Windows (`x86_64-pc-windows-gnu`) and macOS (`aarch64-apple-darwin`) native
services pass cross-compilation, but runtime behavior still needs those native
environments.

The local X11 experiment exposed upstream errors: unregister left XUngrabKey
requests buffered, and worker-connection/reply failures could report success.
The small vendor patch is documented in
[PLICO-PATCH.md](../vendor/global-hotkey/PLICO-PATCH.md), with original licenses
and provenance retained. It is a direct path dependency, so application consumers
need no extra root Cargo patch. Upstream's X11 polling interval is unchanged;
this is not a zero-latency input claim.
