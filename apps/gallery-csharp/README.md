# Plico gallery — C# frontend

A runnable C# gallery on the existing Plico Rust runtime, native text and wgpu.
No Avalonia, webview, XAML or alternate renderer. Page/component declarations,
ordinary state and workflows live in C#; native resource management, input,
layout and rendering stay in Rust.

## Run

.NET SDK 10 and the normal Plico native build dependencies are required.
From the repository root:

```sh
cargo build --locked -p plico-csharp
export PLICO_NATIVE_LIBRARY="$PWD/target/debug/libplico_csharp.so"
dotnet run --project apps/gallery-csharp
```

For development, keep the same native library and use the JIT/hot-reload path:

```sh
dotnet watch --project apps/gallery-csharp run
```

On macOS use `libplico_csharp.dylib`. On Windows set
`$env:PLICO_NATIVE_LIBRARY = (Resolve-Path target/debug/plico_csharp.dll)`.
Window launch runs on the main STA thread. A Linux Xvfb/lavapipe window check applied a UI method edit in 451 ms without
restarting; the selected page and counter value 101 survived. This is one warm
observation, not a performance guarantee. Runtime validation so far is Linux;
Windows/macOS window behavior is not claimed as tested.

## WGSL live editing

Open **Effects / Shader studio** and edit `assets/shaders/*.wgsl`. The file watcher reads and validates in the background; errors are shown while retaining the last good program. No C# or Rust rebuild occurs. `Uniform(0, strength)` sets `plico.values[0].x`; animated shader frames use the native clock without managed UI callbacks. See [the shader contract](../../docs/shaders.md).

## Components

Gallery uses the SDK Interceptors configuration. `[Component]` methods retain
ordinary names and signatures: `LocalCounter(ui, title)` scopes each direct call
automatically, and `ref int count = ref ui.State(static () => 0)` stores local
state. Private components live in a partial class. Dynamic rows still use
`ui.Scope(item.Id)`; interceptors do not replace business keys.

To pass a component to an application or another callback, use
`ui => Counter(ui)`, not the method group `Counter`. The compiler warns about
method groups that bypass component identity. Component body edits can use
`dotnet watch`; structural edits may reset state or require restarting.

## Pages

- Lyrics: original timed lyrics, word reveal, gentle lift, continuous line follow and a WGSL ambient backdrop; simulated playback with seek/pause/replay.
- Component kit: themed cards, badges, avatars, alerts, skeletons, navigation, command popup, typed value fields and RGB editing.
- Workspace: project rows, two scoped `ui.State` counters and an app-owned counter; navigation demonstrates unmount/reset versus retained app state.
- Layout: grow ratios, wrapping, nested scroll viewports.
- Controls: button variants, switches, checkboxes, Unicode text fields, sliders.
- Motion: property targets, native-sized spinners, a C# composed progress bar.
- Lists: search, reverse order, completion, add/remove with business IDs.
- Data table: sortable columns; the header is outside the row scroll viewport.
- Tree navigation: expandable folders and a selected document inspector.
- Calendar: Monday-first month navigation, selected date and persistent note.
- Images: worker-loaded procedural sample, local file or HTTP image, size control.
- Async tasks: success, cancellation, restart, stale-result suppression and errors.
- WGSL effects: Aurora, Orb and Ripple; native clock, no managed animation loop.
- 2048: deterministic tile merging, score, restart and direction buttons.
- Markdown chat: cached Markdig parsing, native rich paragraphs, grapheme-safe streaming,
  pause/replay/burst, reveal modes and scroll following.
- Glass: blurred backdrop, native-clock moving background, tint/blur controls and save action.
- Localization: plural messages, culture-aware number/date formatting and a nested English calendar.
- Black hole: shared backdrop WGSL lens following the pointer across pages.
- Desktop patterns: tabs, notifications, breadcrumbs, meter, rating, steppers, sticky headers,
  draggable split, mail search, RGB picker, form validation, date popup, find bar, modal,
  text selection and a captured drag surface.

The gallery has English/Chinese navigation and descriptions, light/dark themes,
and compact spacing. Individual demo labels are still partly English.
All sixteen Rust gallery destinations now have C# counterparts, plus the extra
Async tasks page. This is coverage and interaction alignment, not pixel-identical
or API-complete parity: the desktop patterns range uses two sliders, its color picker
uses RGB channels rather than HSV, and it does not yet port every Rust desktop
widget's keyboard/accessibility behavior. Existing Workspace/Layout/Controls demos
retain their C#-specific examples. Rust gallery does not have a video page.

## Composition

```csharp
using (ui.Row().Gap(16))
{
    using (ui.Scope()) Counter(ui, ref leftCount);
    using (ui.Scope()) Counter(ui, ref rightCount);
}

void Counter(Ui ui, ref int count)
{
    using (ui.Column().Gap(12).Padding(20))
    {
        ui.Text(count.ToString()).FontSize(28);
        if (ui.Button("+").Primary().Clicked()) count++;
    }
}
```

Scopes identify ordinary component calls without adding layout nodes. Controls
are ephemeral builders; application fields and owned tasks survive supported
.NET method hot reload. Inline events are settled before the replacement batch
is adopted. Build code outside event handlers must remain idempotent.

`UiTask<T>` is application-owned and disposed on shutdown. It runs work in the
background and posts snapshots through the UI dispatcher. Its status is suitable
for `switch`; no automatic signal dependency graph is added. The image page uses
the same task abstraction and does not perform file/network work in Show.

## Preview and verification

```sh
# Renderer-independent navigation and interaction checks.
dotnet run --project apps/gallery-csharp -- --self-test

# One page or a checked-in scene.
dotnet run --project apps/gallery-csharp -- --page Calendar --capture reports/csharp-gallery/calendar.png
dotnet run --project apps/gallery-csharp -- --scene apps/gallery-csharp/scenes/controls-dark.json --capture reports/csharp-gallery/dark.png

# All seventeen pages, plus dark/narrow control previews.
dotnet run --project apps/gallery-csharp -- --capture-all reports/csharp-gallery
```

Capture settles native property/theme/scroll transitions before rendering the
requested state. It does not sample an old theme or page halfway through a
transition. Capture uses a virtual clock (default 1200 ms); scenes can set `timeMs`,
`completeReply`, `lens` and `pointerX`/`pointerY`. Raw screenshots and logs
under reports are not committed.

Tests exercise every navigation destination, events, Unicode input, sliders,
theme/language switching, filtered/reordered row identity, month/date selection,
async success/error/cancel/restart, local image decoding, native-only frame reuse,
and 2048 merges. The `plico-csharp` Demo self-test additionally covers dispatcher
lifecycle, resource buffers, scopes, stale builders and managed error boundaries.

Images use host-owned IDs wrapped by disposable `ImageResource`; native release
waits until the current batch no longer references them. Video-player bindings
are still separate work. Advanced primitive attributes and the animation-request
flag require ABI v5: rebuild the native library and managed SDK together.

Component-kit preview fixtures: `scenes/components.json`,
`scenes/components-navigation.json`, `scenes/components-forms.json` and
`scenes/components-narrow.json`. Scene `width`/`height` set the offscreen viewport;
`componentTab` selects a kit section. See the
[SDK component inventory](../../docs/csharp-components.md) for supported behavior
and differences from MyGO.

The lyric preview is reproducible with `--scene scenes/lyrics.json` (relative to
this directory). `lyricTimeMs` chooses the simulated playhead and `lyricPlaying`
controls playback. There is no audio/network dependency. Text reveal uses fixed
word layouts and clipped copies; this version rebuilds the C# page during playback
for timing and line positions, rather than claiming a GPU-only text effect.

The lyric glow is a blurred duplicate of the revealed glyphs, painted beneath
the sharp text. The native foreground Gaussian filter is exposed as `Blur(sigma)`
on C# builders/containers. The reveal clip is inside the blur layer, so light
can expand past the sung strokes. It does not sample the backdrop. Rebuild the
native library after updating this API.

Continuous-corner comparisons live in Component kit → Corners, with the preview
fixture `scenes/components-corners.json`. The slider changes easing without
changing radius or layout dimensions. The lyric panel opts into smoothing too.

For the usual source-checkout loop, `dotnet msbuild dotnet/Plico.proj -t:Watch`
(from the repository root) reuses the existing native library automatically.
`-t:Test` is native-free; `-t:NativeTest` runs Gallery and other integration
checks using the existing artifact. See [frontend development](../../dotnet/README.md).
