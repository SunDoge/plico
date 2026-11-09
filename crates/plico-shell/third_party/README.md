# MyGO native window corners

`src/macos_window_frame.rs` adapts the native content layer rounding, clear
NSWindow backing and shadow invalidation from `internal/darwin/window.go`,
`roundContent`, MyGO revision `ee3d4bd9bd741f197de231b9fd9b002e9db55577`.
Copyright (c) 2026 MyGo contributors, MIT; see `mygo-LICENSE`.

Plico uses typed AppKit/Core Animation Rust bindings, retains the application
background in the GPU scene, and disables implicit animations while updating the
native contour. Radius stays in AppKit points; the native layer scales with the
window. On macOS, the frame no longer adds a separate circular GPU clip.
