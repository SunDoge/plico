# FLTK backend experiment

This experiment validates Plico declarations → retained FLTK controls, with Taffy
performing layout. It is not a complete replacement for the Vello backend.

## Run the same routed workspace with either backend

```sh
cargo run -p plico-backend-settings --features fltk --bin settings-fltk
cargo run -p plico-backend-settings --features vello --bin settings-vello
```

`apps/backend-settings/src/lib.rs` contains the shared view and ordinary Rust
state. It now has Overview, Activity and Preferences routes, back/forward
history, a resizable sidebar, a Grid dashboard, an animated details pane and a
scrolling keyed activity table with a fixed header. The two entry points select
the backend. The FLTK build does not depend on
Vello, WGPU, Parley, or the development/reload host. Neither backend is enabled by
default for this experiment. The mainline app API is unchanged.

On Linux, FLTK builds its C++ library from source with CMake/Ninja. It uses system
X11, Cairo/Pango and font libraries; `no-pango` is deliberately not enabled, so
Chinese/fallback text remains available. Typical Ubuntu build dependencies:

```sh
sudo apt-get install cmake ninja-build pkg-config libx11-dev libxext-dev \
  libxft-dev libxinerama-dev libxcursor-dev libxrender-dev libxfixes-dev \
  libpango1.0-dev libgl1-mesa-dev
```

The initial platform validation is Linux/X11. macOS, Windows and native Wayland
behavior have not been verified by this experiment.

## What is implemented

- Ordinary `column`/`row`, Flex/Grid layout vocabulary and layout modifiers are
  mapped to Taffy. FLTK widgets receive the computed rectangles.
- Text and wrapped labels use FLTK's own font/width/wrap measurement. FLTK draws
  the text too; this path does not use Parley measurements.
- Text buttons and simple composed buttons become native buttons; checkbox and
  switch declarations currently both use FLTK CheckButton. Text-input editing is
  delivered back to the application's TextBuffer. Disabled controls reject events.
- SVG leaves and one leading SVG in a simple button use FLTK's SVG image support.
- Each live Entity keeps its native widget. Property changes patch controls;
  unchanged input values are not reset, preserving the native editor/caret.
- Event callbacks capture a channel sender and Entity only. The host validates
  current identity/enabled state before delivering the event. Stale events after
  removal are rejected.
- Removed descendants are deleted before their owning groups. A kind change or
  scroll-container-type change at a reused key triggers a full native remount.
- Native scroll containers preserve their offsets across declarations. Child
  reordering updates both Taffy and native child order.
- The host waits for events and rebuilds after messages/resizes. ANIMATE requests
  schedule deadline-based frames at about 60 Hz, ending as soon as the request
  clears. Native event wakes do not themselves create another animation frame.
  Time is injected by the host; the UI does not read a wall clock.
- The shared demo uses the existing Router and MotionArena. Sidebar/details
  widths and simulated sync progress are explicit sampled values in declarations,
  so the same finite animations work on both backends. Reversals start from the
  current value; repeated settle passes at one timestamp do not advance time.
  The Reduce motion checkbox snaps these demo transitions. Native/Vello control
  hover/press effects are separate backend behavior.
- Activity's header remains outside its native scrollport, with shared column
  sizes across header/data rows. Business-keyed reordering preserves row handles.
- SVG frames align their image without an empty caption line; icon-only button
  names become tooltips/metadata rather than accidentally visible labels.

Taffy nodes are also retained, and equality checks avoid rewriting unchanged
styles/children/measurement contexts. A forced sync still traverses the declared
tree and calls Taffy's cached layout entry point; this is not an O(delta) claim.
The layout-vocabulary mapper is currently copied from plico-vello, rather than
introducing a dependency on that renderer. It should be shared if this backend
moves beyond an experiment.

## Deliberate limits

FLTK supplies the appearance and interaction. Colors/font size/disabled state and
geometry are mapped, but Plico radii, borders, spacing inside native controls and
text metrics do not promise pixel parity with Vello. Font weights currently map
to normal/bold Helvetica-family FLTK fonts. Complex styled button children are
rejected rather than silently flattened; simple text plus one icon is supported.

Shader, slider, dialog/portal/tooltip nodes and other unimplemented kinds return
an explicit error. Secret inputs are rejected: native plaintext editor storage
has not been audited for password handling. IME and accessibility integration,
input submit semantics, programmatic scrolling/focus parity,
GPU effects, full style fidelity and hot reload are not implemented/validated.
Native Ctrl/Cmd editing behavior comes from FLTK; this is not a cross-platform
input-method certification. FLTK does not yet automatically interpolate arbitrary
`.motion(...)` style changes: the demo's finite values use plico-motion and request
frames explicitly. Its frame timer is not tied to compositor VSync.

## Reproduce validation

```sh
xvfb-run -a python3 scripts/check-fltk.py
cargo clippy -p plico-fltk -p plico-backend-settings --all-targets --all-features -- -D warnings
```

The native probe checks real button/input callbacks, TextBuffer round-trip,
disabled events, stable editor handles, 20 nested mount/remove cycles, stale-event
rejection, theme/resize captures, zero unchanged geometry/property writes, native
scroll-offset retention and keyed reorder retention. It also verifies router
back/forward, state after remounting routes, Grid/table column alignment,
intermediate/final animation values and demo reduced motion with a simulated
clock. A separate native host probe runs a real finite timer sequence and asserts
that declarations stop after the final sample. The Vello integration test checks
the shared routes, columns and animations through its own Source.
Generated PPM screenshots
are temporary in the smoke script; `PLICO_FLTK_PROBE_OUT=<prefix>` can retain them
when running the probe directly. CI has a separate Linux job for these additional
native dependencies.

For a coarse Linux comparison of the identical view:

```sh
cargo build --release -p plico-backend-settings --features fltk --bin settings-fltk --example fltk_probe
cargo build --release -p plico-backend-settings --features vello --bin settings-vello
xvfb-run -a python3 scripts/compare-settings-backends.py
```

The script records binary bytes, time until a window becomes visible, RSS after
settling and idle CPU ticks, plus screenshots when ImageMagick is available.
Window visibility is not first-paint completion; Xvfb/software GPU rendering and
shared-library pages affect results. This is a local sanity comparison, not a
platform-wide performance claim. Generated reports are not committed.

Upstream setup reference: https://fltk-rs.github.io/fltk-book/Setup.html

## Local comparison (2026-10-24)

Three consecutive release runs per backend, the routed Overview at 1100×820,
Linux/X11 under Xvfb; medians, without an extra strip step:

| Measurement | FLTK | Vello |
|---|---:|---:|
| Executable size (decimal MB) | 3.19 | 33.45 |
| Settled RSS (MiB) | 21.8 | 212.2 |
| Window-visible time (ms, coarse proxy) | 79 | 207 |
| Idle CPU in a 1-second sample | below 10 ms tick resolution | below 10 ms tick resolution |

FLTK's system shared libraries are not included in executable size; this does
not compare complete distribution packages. GPU initialization/shared libraries
influence Vello's RSS. These measurements establish a useful lightweight-backend
experiment, not an across-the-board performance ranking. The earlier 680×700
single-form measurements should not be compared directly to this larger view.
