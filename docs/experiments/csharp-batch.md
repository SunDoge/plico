# C# frontend + native Plico batch experiment

## Question and implementation

Can managed UI authoring shorten edit feedback without replacing the Rust
layout, text, input or wgpu stack? `crates/plico-csharp` is an unpublished
experimental cdylib/rlib, with managed library, demo, and build probe under its
`dotnet/` directory. It uses csbindgen 1.9.8; no per-widget C calls or JSON bridge.
The generated bindings live next to their managed wrapper and change only when
ABI generation actually changes the output.

The transport is an ABI-v2 parent-before-child Command array and UTF-8 arena.
It validates IDs, parent/container topology, UTF-8 span boundaries, style values,
control bounds and resource budgets before adoption. Rust owns copies of the
batch and handles native TextBuffer/IME state. Callbacks are synchronous,
Cdecl, thread-affine, rooted through GCHandle; command/text arrays are reused
on the pinned object heap. Rust retains neither a managed string pointer nor
a per-widget managed delegate.

Only initial/invalidation/input updates enter C#. The retained batch is replayed
through the existing immediate Engine and shared backend. A click is harvested
while replaying, delivered to C#, then the returned replacement settles before
presentation. The C# wrapper builds twice on event updates (events then no events)
so inline handlers after text declarations do not leave stale labels or run twice.
This is still ordinary immediate authoring at the managed boundary, not automatic
reactive dependency tracking. Caching the batch could also be implemented for a
Rust frontend; it is not evidence that managed code executes faster than Rust.

## Verified

- Rust unit tests: malformed IDs/parents/depth/style/UTF-8 and copying foreign
  storage before the managed buffers are reused.
- C# self-test: button events, ordinary state, conditional subtree identity,
  Unicode input, slider values, external invalidation, forced GC, no callbacks
  during native-only replay, and a caught managed exception without poisoning
  another host.
- Actual native window in Xvfb with Lavapipe: three pointer clicks reached C#,
  state and the displayed count reached 3. This is not a hardware-GPU perf test.
- Offscreen Intel Vulkan capture using the normal Plico wgpu/system-text path;
  separate image load/register/display smoke check. No alternate renderer.
- Rust tests/Clippy and managed build with warnings treated as errors.
- macOS and Windows window behavior remain unverified.

## Warm edit feedback measurement

Local SDK .NET 10.0.403, Rust 1.99.1. Normal workspace dev profile (app opt=1,
dependencies opt=2) versus .NET Debug, compiler/build servers warmed. Each sample
changes an actual title literal with a fresh nonce, rebuilds, then restores the
original source in finally. An approximately equivalent native Rust UI is
provided as the crate's `native` example. Three measurements per edit type;
these are not cold builds, p95 estimates or full OCR-app results.

| Operation | C# frontend | Native Rust example |
|---|---:|---:|
| UI edit and build, samples | 0.614 / 0.647 / 0.656 s | 1.271 / 1.275 / 1.244 s |
| UI edit and build, median | 0.647 s | 1.271 s |
| No-op build | 0.558 s | 0.299 s |
| Warm process restart through GPU PNG capture, median | 0.226 s | 0.182 s |
| Sum of edit and restart medians | 0.873 s | 1.453 s |

This demonstrates a useful but modest improvement: about 2x on changed-code
build alone, about 1.7x including restart/capture, with a slower managed no-op
and process start. The unchanged Rust native backend is reused by C#; its first
build and native changes still incur normal Rust compilation. A managed DLL is
small (tens of KiB in this demo), but the native DLL, .NET runtime, JIT and native
runtime dependencies still exist. An unstripped debug native DLL is not a release
package size measurement.

The small 25-command forced-update probe measured approximately 49–51 us for a
CPU frame, including about 8–10 us in the managed callback and about 1 us copying/
validating the native batch. Managed steady-state allocation was about 46 bytes
per build (counter string). These samples have no GPU submission, are warm,
include integer-microsecond timer rounding, and do not establish a Rust/C#
runtime performance winner.

## Reproduce

Follow the crate README to build the native library and managed demo and set
PLICO_NATIVE_LIBRARY. Then, from the repository root:

```sh
cargo build --locked -p plico-csharp --example native
dotnet build crates/plico-csharp/dotnet/BuildBench
dotnet crates/plico-csharp/dotnet/BuildBench/bin/Debug/net10.0/BuildBench.dll
```

The probe temporarily changes only its two title literals, restores them on
normal exceptions, and writes local output under reports/csharp/. Avoid editing
those files concurrently with the probe. GPU initialization is measured separately
from builds; select an appropriate Vulkan ICD for headless machines if needed.
Generated screenshots and raw benchmark reports are not committed.

The next useful evaluation is a real OCR settings/inspector page using the dispatcher. It should assess input/IME, focus, resources, event ergonomics and
state ownership along with build latency, before attempting an application rewrite.


## Dispatcher and .NET hot reload follow-up

The SDK now exposes thread-safe `Post`/`InvokeAsync`, worker image loading,
basic native WGSL effects, and flex alignment/justification/wrapping. A native
mailbox holds only wake and resource messages, not managed state references.
UI callbacks and state mutation remain on the creating thread. Queue errors,
cancellation, closing and posting while draining have integration checks.

A .NET metadata update handler invalidates live applications using that same
mailbox. A real `dotnet watch` method-body edit changed the CPU preview caption
without changing its process ID; watch reported 467 ms to apply the update.
This is a single Linux warm observation, not a cross-platform performance claim.
Unsupported edits still require a restart. Rust library updates remain static
rebuilds. Run instructions and the public API are in the crate README.
