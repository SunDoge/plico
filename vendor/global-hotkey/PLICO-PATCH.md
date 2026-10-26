# Plico's temporary global-hotkey patch

Source: crates.io `global-hotkey` 0.9.0, https://github.com/tauri-apps/global-hotkey,
upstream commit `b4185ab2f37b62e4d1bd2b8410fcf93e338f55fc`.
Original MIT/Apache-2.0 licenses and headers are retained.

Only `src/platform_impl/x11/mod.rs` is changed:

- Wait for the X11 worker's connection/extension setup before reporting a manager
  as available. Previously setup failures were lost on the detached thread.
- Treat a disconnected command reply channel as an error instead of success.
- Check XUngrabKey requests before acknowledging unregister. The original
  ignore_error left requests buffered; another manager still saw the accelerator
  as occupied indefinitely on an otherwise idle connection.

The rest of the API and platform implementations remain upstream. Local ignored
Rust/Xvfb integration tests in plico-native verify native delivery, conflict and
handover after unregister/Drop. No performance improvement to the upstream X11
50ms polling interval is claimed. This copy can be removed when upstream carries
these fixes. plico-native references this copy directly so path consumers need no additional
root Cargo patch.
