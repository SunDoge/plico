# Native restart and WASM UI replacement

This standalone benchmark renders a reading page with Plico and wgpu. Both modes
share the same Rust UI model and host rendering code. Native mode rebuilds and
restarts the application. WASM mode rebuilds a Rust module and publishes it to
a running Wasmtime host. The protocol uses checked rkyv archives in two reusable linear
memory buffers. The module only sees the host image ID and reader state.

The host displays a real image. A simulated OCR job advances on a background
thread. The reload check compares the process and task IDs and verifies that
the renderer's cumulative image upload count stays constant.

On Linux, install the Plico desktop build dependencies, Xvfb, Xauth, and the
`wasm32-unknown-unknown` Rust target. Fetch the locked dependencies once, then
run the benchmark with a PNG reader page.

```sh
cargo fetch --locked --manifest-path benchmarks/wasm-ui/Cargo.toml
xvfb-run -a -s '-screen 0 800x700x24' \
  python3 benchmarks/wasm-ui/run.py /path/to/page.png
```

The script warms both builds, edits the same toolbar color revision five times
per mode, and restores the benchmark source after the run. Do not edit that
source while a run is active. It samples an X11 pixel and records Cargo wall time
and process memory. The separate update loop compares native model generation
with WASM model generation plus transfer, encoding, and decoding. It runs before
window creation. CSV files and traces go to `reports/wasm-ui/`.

## Shared Plico declarations

Set `PLICO_WASM_PROBE_TREE=1` to run the same Plico declaration function directly
in the native host or inside the guest. In guest mode the host validates and
replays the generated `ViewNode` tree. The host reads text patches in place. It materializes a full retained tree when
a new baseline must survive subsequent calls that overwrite the guest buffer.
Both modes keep the page image in the native renderer.

Set `PLICO_WASM_PROBE_TREE=patch` to compare a text-only delta. A new module,
structural edit or style change sends a full tree. The host retains the last
tree and applies subsequent text edits before the same validation and replay.
The patch prototype still builds and compares the full guest tree.

The tree runs require `xdotool`. After the timed edits the script clicks
**Mark**, checks that the state survives a reload, and checks texture uploads.
Set `PLICO_WASM_PROBE_FAST_JIT=1` to disable Cranelift's optimization passes and
measure the load-time tradeoff. The extra runs write to separate report
directories under `reports/`.

The guest uses a single generated ABI entry point, regardless of which Rust
handler is selected. Browser imports from the portable Plico build are linked
to traps in this desktop fixture. The tested reader uses value inputs only.

## 120 FPS frame budget

The offscreen probe runs the same reader at 420 × 570. It changes the progress
label every frame and schedules 360 measured frames at 120 Hz per workload,
after 60 warmup frames. A sample includes declaration, guest transport and
replay where applicable, layout, scene building, rendering, and waiting for
GPU completion. It uses a GPU render target without CPU readback.

```sh
export CARGO_TARGET_DIR="$PWD/target/wasm-ui-host"
manifest=benchmarks/wasm-ui/Cargo.toml
cargo build --locked --manifest-path "$manifest" --bin reader-host --features frame-rate
PLICO_WASM_PROBE_120FPS=1 "$CARGO_TARGET_DIR/debug/reader-host" /path/to/page.png

cargo build --locked --manifest-path "$manifest" --bin reader-ui \
  --features tree-patch --target wasm32-unknown-unknown
cargo build --locked --manifest-path "$manifest" --bin reader-host \
  --features frame-rate,wasm-host,tree-patch
PLICO_WASM_PROBE_120FPS=1 "$CARGO_TARGET_DIR/debug/reader-host" /path/to/page.png \
  "$CARGO_TARGET_DIR/wasm32-unknown-unknown/debug/reader-ui.wasm"
```

Use `tree` in place of `tree-patch` to send complete frames. Set
`PLICO_POWER_PREFERENCE=high` to request a high-performance GPU. The probe
reports the selected adapter, frame time quantiles, and the count of missed
deadlines. It measures rendering throughput without display presentation.

Set `PLICO_WASM_PROBE_FULL_REDRAW=1` to redraw the full target on every frame.
The deadline count includes scheduling delays. The throughput column counts
completed renders, including frames that finish after their individual deadline.

## Native release and WASM development with a background service

The same reader controls use Plico's typed `Services` outbox and
`TaskService`. In a native release the view sends ordinary Rust values. In
the WASM developer build the generated guest entry point packs those values
with the frame update. The host owns the image and the background task.
Clicking **Analyze image** decodes the PNG and computes a grayscale checksum
on a worker thread. The worker wakes the UI when the result is ready.

The developer build watches the module path. Publish modules by moving a
finished build into place. The following smoke test builds both modes, then
pauses the worker at a test-only barrier while replacing the WASM module.
It checks the visible pending state, one completed job and the same checksum
in both modes. It requires ImageMagick's `import`, Tesseract and `xdotool`
in addition to the dependencies listed above.

```sh
xvfb-run -a -s '-screen 0 800x700x24' \
  python3 benchmarks/wasm-ui/run-service.py /path/to/page.png
```

### Edit-to-visible with background module loading

This run compares a warm native developer rebuild and restart with a warm
WASM guest rebuild and replacement. Both use the reader service fixture.
The WASM host builds and instantiates a candidate on its loader thread and
continues drawing the previous UI until the candidate is available. Each
WASM sample injects a **Mark** click after compilation starts, and checks
that the old revision consumes the click before the new revision appears.
The click metric ends at the declaration log and the edit metric ends at a
visible X11 pixel change.

```sh
xvfb-run -a -s '-screen 0 800x700x24' \
  env PLICO_WASM_PROBE_TREE=service PLICO_WASM_PROBE_FAST_JIT=1 \
  python3 benchmarks/wasm-ui/run.py /path/to/page.png
```

### Develop in a live window

```sh
cargo plico dev benchmarks/wasm-ui --wasm -- /absolute/path/to/page.png
```

Save a change in `src/tree.rs`, `src/ui.rs`, `src/service.rs` or
`src/bin/guest.rs`. Successful guest builds replace the UI in the open reader.
Compiler errors appear in the terminal and the current UI remains available.
Restart the command after changing the native host or the shared ABI.

### Browser and desktop binary parity

From this directory, build both sides with matching features. The smoke test uses
Playwright with Chromium. Set `CHROMIUM_PATH` to use a system Chrome installation.

```sh
export CARGO_TARGET_DIR="$PWD/../../target/wasm-ui-host"
cargo build --locked --bin reader-ui --target wasm32-unknown-unknown --features tree-patch
cargo build --locked --bin reader-host --features wasm-host,tree-patch
node browser-parity.cjs "$CARGO_TARGET_DIR/debug/reader-host" "$CARGO_TARGET_DIR/wasm32-unknown-unknown/debug/reader-ui.wasm"
```

The native host records four rkyv requests and verifies the reader's selection
after each step (initial, mark, retain selection, unmark). Chrome replays those
requests against the same guest file. Every response must match Wasmtime byte
for byte. Use `tree` in both builds to compare full snapshots against patches.
This checks the module boundary and includes no canvas rendering or DOM input.

### Warm edit to a passing interaction assertion

Run `python3 benchmarks/wasm-ui/run-check.py` from the repository root.
The fixture copies its source to a temporary project and edits the selected
button caption. All three paths run the same five declarations, dispatch two
clicks by node path and check the selection and caption after each transition.
The native test uses Cargo's test harness. The native executable rebuilds the
reader. The WASM path rebuilds the guest, starts a prebuilt host, loads the new
module with fast JIT enabled, and applies its tree patches. The probe exercises
the declaration tree without layout, a window, an image fixture or GPU rendering.

On Linux x86-64 with Rust 1.100 and mbx, the median of five warm edits was
0.542 s for the targeted native test, 0.674 s for the native executable and
0.706 s for the WASM guest plus host run. The order rotates after each edit.
These timings include startup and assertion execution. A resident host retaining
a live session is a separate workload measured by the edit-to-visible probe above.
