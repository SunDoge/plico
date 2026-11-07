# Fast C# development

The normal frontend loop uses an already-built native library. **No C# build,
watch, test, doctor or preview target invokes Cargo.** Only `Native` does.
No toolchain downloads or native rebuild fallback run inside the SDK.

From the repository root:

| Command | Work performed |
| --- | --- |
| `dotnet msbuild dotnet/Plico.proj -t:Test` | Pure managed component/task/identity checks; no native library needed |
| `dotnet msbuild dotnet/Plico.proj -t:Build` | Build the C# Gallery and SDK only |
| `dotnet msbuild dotnet/Plico.proj -t:Watch` | .NET watch/hot reload of Gallery, reusing the native library |
| `dotnet msbuild dotnet/Plico.proj -t:Run` | Build C# and start Gallery |
| `dotnet msbuild dotnet/Plico.proj -t:Doctor` | Verify the existing native library's ABI, size and protocol revision |
| `dotnet msbuild dotnet/Plico.proj -t:NativeTest` | Demo, Interceptors and Gallery native integration checks; no Rust build/window/GPU capture |
| `dotnet msbuild dotnet/Plico.proj -t:CompilerTest` | Component generator/compiler regression fixtures |
| `dotnet msbuild dotnet/Plico.proj -t:Preview` | Build C# and capture the configured scene using the existing native renderer |
| `dotnet msbuild dotnet/Plico.proj -t:Native` | Explicit `cargo build --locked --release -p plico-csharp` |

`Configuration` defaults to `Debug`; override with `-p:Configuration=Release`.
Preview defaults to the Component kit scene and `reports/csharp/preview.png`:

```sh
dotnet msbuild dotnet/Plico.proj -t:Preview \
  -p:Scene=apps/gallery-csharp/scenes/lyrics.json \
  -p:PreviewOutput=reports/csharp/lyrics-preview.png
```

Native-only prerequisites (system text libraries/GPU for capture) belong to the
native path, not `Test`. MSBuild builds this small managed graph with one worker,
so parallel native integration app builds do not race over shared SDK outputs.

## Native library selection

1. `PLICO_NATIVE_LIBRARY` explicitly selects a file. A bad explicit path fails;
   it never silently falls back to a different library.
2. The application directory and its `runtimes/<runtime-id>/native` directory.
3. In a Plico source checkout, the newest existing release/debug library in
   `target/` (or `CARGO_TARGET_DIR`). No Rust source freshness scan occurs.
4. Otherwise, .NET's normal deployed/system native-library search.

`Doctor` prints the selected file. A loaded library stays in the process: restart
when replacing native code. C# body edits do not reload or recompile that library.
For custom Cargo target-dir config not reflected in `CARGO_TARGET_DIR`, set
`PLICO_NATIVE_LIBRARY` explicitly.

The handshake verifies ABI v5, command size and property/event protocol revision
3 *before* creating a host or calling user UI code. The schema revision covers
foreground blur/continuous-corner properties and standalone WGSL validation/outer shadows, so an older v5 library cannot pass
just because its struct sizes still match. Missing, old and wrong-architecture
libraries report the selected location and an explicit build command. Required
native schema changes bump the revision; ordinary C# widgets and WGSL bodies do
not. Revision discipline is a maintenance contract, not an automatic ABI proof.

## Pure managed widget tests

Reference `Plico.Testing` as well as the SDK. Project references are shown in
`crates/plico-csharp/dotnet/ManagedChecks`; it enables the same Interceptors
configuration as Gallery. No native library is required:

```csharp
using Plico;
using Plico.Testing;

int count = 0;
using var ui = new UiHarness(cx => {
    cx.Text($"Count: {count}");
    if (cx.Button("Increment").Clicked()) count++;
});
ui.Render(seconds: 0);
ui.Click("Increment");
ui.Render(seconds: .5);
if (ui.Find("Count: 1", TestRole.Text).Text != "Count: 1")
    throw new Exception("Counter failed");
```

The harness uses the **same `Ui.Declare` path** as Application, including the
post-event settling pass and local-state commit/unmount behavior. Snapshots own
managed strings/data, not pointers or live builders. Duplicate labels require
an accessible name or node ID. `SetText` injects a *committed string*, not keys,
selection, clipboard or IME. `SetValue` injects a numeric edit. It does not execute
Rust validation, layout, native hit testing/focus, text shaping or GPU drawing.

`ManualDispatcher` implements `IUiDispatcher` for service/`UiTask<T>` tests.
Worker completions enqueue model updates; tests call `Pump()` on their UI thread.
Real Application implements the same two-method interface. No Rust host or fake
window is needed to exercise loading, failure, cancellation or stale completion
suppression.

The default suite deliberately supplies a nonexistent native-library path.
A local check also replaced `cargo` with a failing sentinel: `Test` still passed.
Observed warm full command: 1.55 seconds; execution without build: 0.125 seconds
(one Linux measurement, not a performance guarantee).
