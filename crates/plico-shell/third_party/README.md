# MyGO native window corners

`src/macos_window_frame.rs` adapts the native content layer rounding, clear
NSWindow backing and shadow invalidation from `internal/darwin/window.go`,
`roundContent`, MyGO revision `ee3d4bd9bd741f197de231b9fd9b002e9db55577`.
Copyright (c) 2026 MyGo contributors, MIT; see `mygo-LICENSE`.

Plico uses typed AppKit/Core Animation Rust bindings, retains the application
background in the GPU scene, and disables implicit animations while updating the
native contour. Radius stays in AppKit points; the native layer scales with the
window. On macOS, the frame no longer adds a separate circular GPU clip.

`src/macos_preferences.rs` also adapts NSWorkspace display preference queries and
`NSWorkspaceAccessibilityDisplayOptionsDidChangeNotification` registration from
MyGO `internal/darwin/{system,app}.go` at that revision. Plico uses an owned
notification token, an atomic dirty flag and its event-loop wake handle; the
observer is removed when the shell is dropped. Queries run on the UI thread in
an autorelease pool. No per-frame system preference polling is performed.


`src/macos_frame_clock.rs` references MyGO `internal/darwin/surface.go`, revision
`ee3d4bd9bd741f197de231b9fd9b002e9db55577` (MIT, see `mygo-LICENSE`): NSView-associated
display links, common run-loop modes, default frame-rate range, continuous
requests without repeated native start/stop, and idle/invisible pausing. Plico's
callback owns only a wake handle and handshake state; winit remains the event
and redraw host. No transaction-presentation implementation is copied.
