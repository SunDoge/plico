# C# frontend and Rust foundation

Plico's desktop direction is a mixed framework: managed widgets above a native
layout/rendering foundation. This is not a one-to-one binding of Rust widgets.
The immediate declaration model remains usable from C#; JIT and supported .NET
hot reload apply to widget implementation as well as application code.

| Layer | Responsibility |
| --- | --- |
| C# widgets | Composition, appearance, theme policy, interaction policy, local widget state |
| C# application | Pages, domain state, asynchronous workflows and routing |
| Primitive batch | Stable node IDs, hierarchy, layout/paint properties, semantics, resources |
| Rust foundation | Retained storage, Taffy, native text, caret/selection/IME, hit testing, input capture, resource caches, wgpu and damage tracking |

C# submits bounded flat node arrays and UTF-8 data. Rust validates and owns a
copy before the call returns. Pixels are resources, not payloads re-sent with
every declaration. Node-addressed event batches return through the managed build
callback. No per-widget managed closure is retained by Rust.

The enforced development workflow and feature-placement rules are in
[the C# / native boundary](csharp-boundary.md). Daily commands are documented in
[the native-free frontend entry](../dotnet/README.md).

## Implemented boundary

`plico-csharp` ABI v5 rejects the old button, checkbox, switch, slider, spinner
and select opcodes. C# implements these widgets in `Widgets/Controls.cs` and
`Select.cs` from boxes, paragraphs, generic activation, portals and shader bodies.
`Ui.Box().Interactive(name)` also allows application-defined composition without
adding a Rust enum variant or dispatcher branch.

Rust supplies text-edit and numeric-edit surfaces with platform input mechanics.
They do not choose widget decoration. Numeric input can disable native chrome;
C# supplies its slider drawing. CheckBox/Switch semantic roles are independent
of the visual primitive. Portal placement, modal input isolation and dismissal
are foundation mechanisms; open/query/highlight/selection state belongs to C#.

Virtual scroll windows and image-canvas/editor integration are still native
surfaces. They are explicit capabilities, not proof of full widget API coverage.
The existing Rust widget API remains available, especially for embedded backends;
this work does not require .NET on those targets.

## Validation

The SDK Demo's `--self-test` checks a user-defined composed button, managed
toggles, numeric pointer capture/drag and accessibility updates, and managed
selection with keyboard navigation/Escape. Its other checks cover identity,
thread affinity, managed GC, batch ownership and dispatcher lifetime. Native
batch tests reject obsolete widget opcodes. Gallery is a framework consumer;
application-specific functionality does not belong in this foundation.

Managed `Theme`, scoped contexts, initial `WindowOptions`, composed button
content and text submission are now public SDK APIs. Remaining work includes
video-player resources, application/global shortcuts, runtime window commands
and broader semantic/keyboard coverage. This document
distinguishes the architecture direction from complete API/product maturity.

## Gallery alignment

The managed gallery covers the Rust gallery destinations and adds component-kit,
lyrics and async-task pages. The shared shell uses the same spacing/color direction and
repository-owned icon assets. C# composes calendar/date popups, tabs, meters,
ratings, steppers and dialogs. `SunDoge.Plico.Markdown` is an optional managed
Markdig-based package; it sends rich paragraph spans to the system text engine.
The native layer does not receive a Markdown widget or application callbacks.

The batch protocol supports an explicit managed animation-frame request and optional validated
primitive properties (position, clipping, backdrop blur, text spans, shader
parameters, selection and drag capture). Ordinary command fields stay typed.
Properties currently use bounded JSON inside the batch's UTF-8 arena; they are
not a zero-allocation path. Historical ABI v3 interop measurements do not measure
these new attributes. Native pointer capture holds only a node-owned drag anchor;
managed code receives deltas and applies application policy.

The gallery JSON scenes and virtual clock make offscreen previews deterministic.
The SDK and gallery self-tests cover popup dismissal/selection, native pointer
capture, modal actions, Unicode editing, streaming pause, scoped navigation and
native-only frame reuse. Screenshots remain generated reports, not source assets.
See `apps/gallery-csharp/README.md` for remaining widget parity differences.

ABI v5 gives properties their own validated UTF-8 span, independent of input,
canvas and list values. Fixed slider WGSL uses value/color uniforms rather than
regenerating shader source. Native window configuration is validated before
consuming a host. Tests cover these channel/lifetime boundaries; native window
behavior has not been exercised on Windows/macOS in this change.

## Component-local state

`ref T Ui.State<T>(Func<T> create, key: ...)` returns ordinary managed storage
scoped by parent, identity scope and call site. State cells keep refs stable as
the slot dictionary grows; no initializer callback is retained. Successful final
declarations sweep unmounted slots. Failed builds retain previous slots and
remove newly initialized ones; incompatible type replacements reset independently.
The SDK tests cover instance isolation, keyed reordering, remount, event settling,
failed builds, metadata-update invalidation, explicit keys and ref stability under
GC/dictionary growth. These are not transactional state writes or reactive signals.
See the SDK README for ref lifetime and explicit resource-ownership rules.

`[Component] CounterBody` now has an experimental source-generator entry path:
Counter receives compiler-provided caller information and enters an identity
scope plus a compile-time component salt, then forwards to CounterBody. No
statement/capture/control-flow rewriting occurs. This is tested in a real LSP,
not merely in compiler symbol tests; remaining rename/navigation limits and
local analyzer wiring are documented in `experiments/csharp-components.md`.

A separate interceptor mode now tests original-name components. It intercepts
direct calls, inserts identity scopes, and invokes the original unchanged method.
Private members use generated access bridges; method groups produce a bypass
warning. The Body mode remains the default. See
`experiments/csharp-interceptors.md` for the real LSP/rename/hot-reload findings
and explicit project opt-in; this does not establish production-wide coverage.
