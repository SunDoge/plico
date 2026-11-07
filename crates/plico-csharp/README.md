# Plico C# frontend experiment

A small C# frontend over the existing Plico Engine, native text layout and wgpu
renderer. This package is **experimental and not published**. It does not
replace Plico's Rust authoring API or implement another renderer.

C# builds a flat array of typed commands plus a UTF-8 arena. csbindgen generates
the Cdecl structs/function-pointer imports from `src/abi.rs`. Rust validates and
copies each complete batch, retaining no managed string/array pointer. Input
results return as one ID-addressed event batch; there is no P/Invoke per widget.

```csharp
int count = 0;
using var app = new Plico.Application(ui =>
{
    using (ui.Column().FillWidth().Padding(24).Gap(12))
    {
        ui.Text($"Count: {count}").FontSize(28);
        if (ui.Button("Increment").Primary().Clicked()) count++;
    }
});
app.Run();
```

`Column`/`Row` also support `.Children(() => { ... })`. The using form avoids
allocating a capturing layout callback. Standard C# `if`/`for`, ref fields,
components and IDE tooling remain available. Repeated items need explicit
`key:` business IDs; ordinary call sites use CallerFilePath/CallerLineNumber.

## Run

From the repository root, with .NET 10 and the normal Plico native build dependencies:

```sh
cargo build --locked -p plico-csharp
dotnet build crates/plico-csharp/dotnet/Demo
export PLICO_NATIVE_LIBRARY="$PWD/target/debug/libplico_csharp.so"
dotnet crates/plico-csharp/dotnet/Demo/bin/Debug/net10.0/Demo.dll
```

macOS: use `libplico_csharp.dylib`. Windows: set `PLICO_NATIVE_LIBRARY` to the
absolute path of `target/debug/plico_csharp.dll`. Run on the main thread.
Those platforms have not been window-tested in this experiment.

```sh
# CPU-only integration check (includes an intentional caught managed exception).
dotnet crates/plico-csharp/dotnet/Demo/bin/Debug/net10.0/Demo.dll --self-test
# The actual native wgpu + system-text offscreen path.
dotnet crates/plico-csharp/dotnet/Demo/bin/Debug/net10.0/Demo.dll --capture reports/csharp/demo.png
# Decode/register an image once, declare it thereafter by native resource handle.
dotnet crates/plico-csharp/dotnet/Demo/bin/Debug/net10.0/Demo.dll --image path/to/page.png --capture reports/csharp/image.png
# Forced CPU update cost, with no GPU submission.
dotnet crates/plico-csharp/dotnet/Demo/bin/Debug/net10.0/Demo.dll --bench
```

## Boundaries

- Supported: row/column, ordinary text, button, checkbox, switch, SVG, slider, single-line
  native text input, image resource, spinner, WGSL effect bodies, flex alignment,
  justification, wrapping and a limited set of style fields.
- A managed exception never unwinds through Rust; capture/Step reports failure,
  and a window renders an error message. Native panics are caught at ABI entries.
- The UI thread owns the host. Use `using`/Dispose; do not dispose from a build
  callback, call native operations reentrantly, or retain Ui/builders for async use.
- Build callbacks run on initial setup and explicit invalidation/input updates.
  Native animations may replay the copied batch without invoking C#. Clock-driven
  C# expressions do not automatically receive animation callbacks.
- Inline handlers can change state after earlier labels. A callback with events
  builds twice (second pass without events), and Rust settles the replacement
  tree in the same input update. Keep non-handler build code idempotent.
- `Post` / `InvokeAsync` queue state updates from any thread and wake the native
  event loop. Work runs before the next managed build. Native input and animation
  do not need a managed frame loop.
- `LoadImageAsync` decodes/registers a local image on a worker. Assign its resource
  ID using `InvokeAsync`. Cancellation prevents a queued load from starting; an
  image decode already running is not interrupted.
- .NET metadata updates refresh the declaration automatically. Supported method
  edits preserve the managed state and native host. Unsupported edits require a
  restart; Rust changes always require rebuilding the native library.
- Full widget coverage, video-player resources and packaged .NET distribution
  remain future work. Rust dynamic hot reload is not reintroduced.
- It adds an owned command/string copy alongside the retained native tree,
  pinned managed buffers, GC/JIT and an ABI maintenance obligation.

The build-feedback experiment and its limits are recorded in
[docs/experiments/csharp-batch.md](../../docs/experiments/csharp-batch.md).

## C# development loop

Use the normal JIT-based Debug build and `dotnet watch`; do not enable NativeAOT
for this development path. Build Rust once, then edit the C# UI:

```sh
cargo build --locked -p plico-csharp
export PLICO_NATIVE_LIBRARY="$PWD/target/debug/libplico_csharp.so"
dotnet watch --project crates/plico-csharp/dotnet/Demo run
```

The SDK registers a .NET `MetadataUpdateHandler`. A successfully applied update
posts a rebuild to each live application. It does not recreate the application,
reset fields or recreate the GPU device. Runtime-unsupported edits are subject
to normal `dotnet watch` restart behavior. Automatic IDs use source locations;
moving controls between lines can reset their native interaction state even when
application fields survive.

A Linux CPU-only probe (`run -- --watch-probe`) verified a UI method edit in the
same process: `dotnet watch` reported 467 ms to apply it. This is one warm sample,
not a latency guarantee or a Windows/macOS validation.

## Background work

```csharp
var result = await service.FetchAsync(cancellationToken);
await app.InvokeAsync(() => {
    model.Result = result;
    model.Loading = false;
}, cancellationToken);

// Image loading does not block the UI thread.
var image = await app.LoadImageAsync(path, cancellationToken);
await app.InvokeAsync(() => model.Image = image);
```

`Post(Action)` is fire-and-forget and logs exceptions. `InvokeAsync(Action)`
returns a task with the operation's result or exception; completion means the
state update ran, not that the GPU presented it. Do not block the UI thread
waiting for this task. Operations are short, synchronous state changes, never
`async void` lambdas. Cancellation is checked before a queued operation executes.
The queue drains a snapshot so self-posting work runs in the next build. Closing
rejects new work and faults outstanding queued tasks. An already executing update
is not interrupted. No managed synchronization context is installed: explicitly
dispatch state mutations after `await`.

## WGSL

```csharp
ui.Shader("""
    fn plico_effect(uv: vec2<f32>) -> vec4<f32> {
        let time = plico.resolution_time_scale.z;
        let glow = 0.5 + 0.5 * sin(time + uv.x * 6.0);
        return vec4<f32>(0.1, 0.2 + glow * 0.3, 0.4, 1.0);
    }
    """, animated: true).FillWidth().Height(100).Radius(12);
```

This reuses Plico's native effect-body contract and injected clock. Source is
copied with the batch (64 KiB maximum); it is not leaked into a Rust static string.
Animating the native shader does not call the managed builder each frame.
Use stable shader source and `ShaderValues(...)` for changing uniforms.
`ReadBackdrop`, `FollowPointer`, `ShaderTime` and `ShaderModule` are available.

## Component identity and temporary builders

Inspired by [MySharp](https://github.com/SunDoge/MySharp)'s numeric scopes and
checked element handles. This implementation uses Plico's batch bridge, not
Avalonia, and does not add automatic reactive subscriptions.

Ordinary component functions can share a parent without adding wrapper layouts:

```csharp
using (ui.Scope()) Counter(ui, ref leftCount);
using (ui.Scope()) Counter(ui, ref rightCount);

foreach (var item in items)
{
    using (ui.Scope(item.Id))
        ItemRow(ui, item);
}
```

Scope only affects identity; it does not emit a layout node or retain a callback.
Automatic scopes use their call site; string, long and Guid scopes use business
identity. Reordering business-scoped items retains their IDs. Independent lists
with overlapping business IDs need an outer scope each. Duplicate scopes and
explicit widget keys throw during the build. Explicit widget keys override the
source location, allowing the same widget to move between source branches.

Source file/line hashes are cached (up to 4096 sites per Ui) rather than hashing
file paths for each node on every build. Node/scope identity remains a runtime
64-bit hash, not a guarantee of globally collision-free identifiers.

Builders are temporary value handles. Their generation and thread are checked
when accessing properties/events. A saved builder cannot mutate a later batch;
`Builder.IsValid` reports whether it is usable now. Never keep builders in app
state or use them across `await`. Layout and identity scopes must close in nesting
order. A closed `Children` scope rejects a second callback before executing it.


The full [C# gallery](../../apps/gallery-csharp/README.md) demonstrates seventeen
interactive pages, ordinary function components, themes, image loading,
application-owned `UiTask<T>` snapshots and native motion/effects. The current
experimental transport is ABI v5; rebuild the native library and managed SDK
together. Capture settles native transitions to show the requested state.

## Native media and reader widgets

The reader migration adds `TextArea` (multiline), `PasswordField`, managed
`Select`/searchable select, `ImageCanvas`, and fixed-height `VirtualList`.
Select popup/search/highlight/selection policy lives in C#; native text-edit
surfaces retain caret and IME state. Node activation and edit events return
through the ordinary batch event channel. `ImageCanvas` exposes original-pixel
regions and committed selection/draw/move/resize/delete events.

Prefer `ImageResource : IDisposable` for owned images. Native service plugins can
register shared decoded rasters and return handles without re-encoding pixels.
A disposed image remains alive while the submitted batch references it, then is
reclaimed. This rule covers images released before or after mailbox adoption.
Canvas-only rasters cannot be used as ordinary encoded `Image` assets.

```csharp
using (var list = ui.VirtualList(rows.Count, 40, height: 240))
    for (int i = list.Start; i < list.End; i++)
        using (ui.Scope(rows[i].Id))
        using (ui.Column().FillWidth().Height(40).Shrink())
            ui.Text(rows[i].Name);
```

Declare exactly one direct child per requested row and retain row state in the
application. Rust owns the scroll viewport/spacers and reports range changes;
C# declares visible/overscan rows only. Business scopes keep row identity stable.

`new Application(Build, virtualTime: true)` and `SetTime(milliseconds)` provide
monotonic injected time for offline fixtures. `Scroll`, pointer, semantic click
and input helpers exercise the same native event path without a window.


## Hybrid framework boundary

Widget implementations live in managed `Widgets/Controls.cs` and `Select.cs`.
They compose primitive nodes; the native dispatcher no longer accepts the old
button/checkbox/switch/slider/spinner/select opcodes. Rust retains layout,
native editing mechanics, resources, rendering and damage tracking. This is
an evolving framework frontend, not a complete binding of Rust widget APIs.
See [the architecture and current limits](../../docs/csharp-frontend.md).


## Interop benchmarks

Package identity: `SunDoge.Plico`; namespace: `Plico`. The native library remains
`plico_csharp`. [InteropBench](dotnet/InteropBench/README.md) isolates managed
construction and native owned-copy/validation, then measures CPU frames and
activation round trips. Generated reports are local artifacts. Native event
frames are checked for version, bounds, UTF-8 and duplicate IDs before use.

## Application SDK

```csharp
string name = "";
double progress = .42;
void SaveDocument() { /* application service */ }
using var app = new Application(ui => {
    ui.Theme = Theme.Light with { Accent = 0xb45309 };
    using (ui.Column().FillWidth().MaxWidth(720).PaddingX(24).PaddingY(16).Gap(12)) {
        using (ui.UseTheme(Theme.Dark)) {
            using var save = ui.ButtonContent("Save document", primary: true);
            ui.Text("↓"); ui.Text("Save");
            if (save.Clicked()) SaveDocument();
        }
        if (ui.TextInput("Name", ref name).Placeholder("Document name").Autofocus().Submitted())
            SaveDocument();
        ui.Progress(progress);
    }
});
app.Run(new WindowOptions {
    Title = "My application", Width = 1040, Height = 780,
    MinWidth = 640, MinHeight = 480, Centered = true
});
```

`Theme` owns widget colors and default type size in managed code. `UseTheme` and
`Provide<T>`/`Context<T>` provide synchronous, lexically nested contexts; they
restore on disposal and cannot escape a build. They do not create subscriptions.
`DarkTheme` remains a compatibility default when no root theme is assigned.
Scoped themes style managed controls; explicit container/text colors still win.

`ButtonContent` composes icon/text primitives inside one activation target.
`Radio<T>`, `Progress`, tabs, calendar/date picker, popup/modal, rating and stepper
are implemented in C#. Input builders expose placeholder, autofocus, explicit
one-shot focus, navigation, password visibility and `Submitted()`.

Window options expose logical size limits, decorations, resize permission,
centering, rounded corners, compositor background, stacking and pointer passthrough.
These are initial options, not runtime window commands. Wayland/compositor
restrictions still apply. Windows/macOS execution remains unverified locally.

ABI v5 separates primitive properties from control values. This prevents styling
an input, canvas or virtual list from overwriting its data payload. Text, SVG,
image and ordinary containers consume the same resolved native style path; native
virtual-list/canvas surfaces still have surface-specific layout limits. Rebuild
the Rust library and managed SDK together after this protocol change.

## NativeAOT release

Development keeps JIT and `dotnet watch`; release can use NativeAOT. The SDK
uses generated JSON metadata, including primitive properties, canvas/list
payloads and window configuration. No reflection-based JSON serialization is
required by the SDK. Applications must also make their own serialization and
third-party dependencies AOT-compatible.

```sh
cargo build -r -p plico-csharp
dotnet publish apps/gallery-csharp -c Release -r linux-x64 \
  -p:PublishAot=true -o reports/csharp/aot-gallery
strip --strip-unneeded -o reports/csharp/aot-gallery/libplico_csharp.so \
  target/release/libplico_csharp.so
reports/csharp/aot-gallery/Gallery --self-test
```

Linux x64 measurements (default Rust release profile, no size-specific tuning):

| Runtime files | Demo | 17-page gallery |
| --- | ---: | ---: |
| NativeAOT executable | 3.0 MiB | 7.8 MiB |
| Stripped Rust shared library | 27.1 MiB | 27.1 MiB |
| Total | 30.1 MiB | 34.9 MiB |
| tar.gz | 12.9 MiB | 15.2 MiB |

Debug symbol files (`.dbg`, `.pdb`) are excluded from these runtime totals. No
separate .NET runtime is needed. Linux system dependencies (Pango, GLib, etc.)
are still external. Both AOT executables passed integration checks; the gallery
also loaded a partial JSON scene and rendered Markdown through the native wgpu
path. These are Linux observations, not Windows/macOS size guarantees.

## Local state

A function component can own ordinary state without an application field or a
signal wrapper:

```csharp
static void Counter(Ui ui)
{
    ref int count = ref ui.State(static () => 0);
    using (ui.Column().Gap(12)) {
        ui.Text($"Count: {count}");
        if (ui.Button("Increment").Clicked()) count++;
    }
}

// Independent instances; scopes add identity without adding layout nodes.
using (ui.Scope()) Counter(ui);
using (ui.Scope()) Counter(ui);
// Lists: using (ui.Scope(item.Id)) Counter(ui);
```

Use `ref` on both sides: `var count = ui.State(...)` copies a value type and
mutating that copy does not update the slot. Ref locals can be passed directly
to `ref` widget parameters, e.g. `ui.TextInput("Name", ref name)`.

Identity includes the parent, identity scope and caller file/line. An explicit
`key:` replaces the source-location portion; it does not override parent/scope
identity. Layout reparenting or moved source lines can reset automatic state.
Use business scopes for reorderable lists. Prefer static factories: the factory
runs only on initialization/type reset and is never stored, but a capturing
lambda can allocate each time the caller constructs it.

Slots live until absent from the final successful managed declaration. The
second event-settling pass does not prune prematurely. A failed build preserves
previous slots and drops newly created slots; it does not roll back ordinary
mutations already made to existing values. Supported hot-reload edits with the
same identity/type preserve values; incompatible types reset that slot. Runtime
unsupported edits can still restart the application.

Use returned refs only synchronously during that build, on the UI thread. No
subscription, automatic invalidation or cross-frame ref validation is added.
A ref kept after unmount can still address detached GC-managed storage; it does
not reconnect to a remounted component. Slots release managed references on
unmount/application disposal, but do not automatically call `IDisposable.Dispose`
on arbitrary values. Keep task/resource ownership explicit; background work
returns through the application dispatcher. Virtualized rows also unmount outside the visible window; keep durable row data
in the application model. This is memory-local state, not persistent storage.

Workspace in the C# gallery contrasts two local counters with an app counter:
leaving/remounting the page resets only the local counters.

## Components with automatic identity

The SDK package includes the component generator and enables Interceptors. Write
ordinary component methods and call them directly; no Body suffix or generated
entry point is needed:

```csharp
public static class Views
{
    [Component]
    public static void Counter(Ui ui)
    {
        ref int count = ref ui.State(static () => 0);
        using (ui.Column().Gap(12).Padding(24))
        {
            ui.Text($"Count: {count}");
            if (ui.Button("Increment").Primary().Clicked()) count++;
        }
    }

    public static void Show(Ui ui) { Counter(ui); Counter(ui); }
}
```

Each direct call gets a call-site scope and component-type salt. Local state and
widget identities are independent across these calls. Public/internal methods
need no partial class; private methods need a partial owner for an access bridge.
Lists still need `using (ui.Scope(item.Id))`. Source-line and structural edits
can reset local state; this is best-effort hot reload, not edit-proof identity.

Use `new Application(ui => Views.Counter(ui))`, rather than passing the component
as a method group. PLICO012 warns when a method group bypasses identity injection.
Components must be synchronous, non-generic, non-virtual void methods with Ui as
the first argument. Ordinary unannotated helper methods remain supported.

Source-checkout applications currently import
`Plico.CSharp/buildTransitive/SunDoge.Plico.props` and reference Plico.Generators as
an analyzer, as Gallery does. NuGet consumers receive both through the SDK package.
The package contains managed APIs and the build-time generator; native renderer
library deployment remains separate (see above).

The older Body-wrapper probe remains a regression fixture. Explicitly setting
`PlicoComponentInterceptors=false` selects that legacy generator mode. See
[the interceptor validation](../../docs/experiments/csharp-interceptors.md) for
LSP, hot reload and NativeAOT results and current limitations.
