# C# component identity wrapper experiment

Implemented: a small incremental Roslyn generator transforms the *authoring
convention*, not the method body. `[Component] CounterBody` adds a `Counter`
entry point in another partial declaration. It does not rewrite captures,
control flow, state accesses, widget calls, or event handlers.

```csharp
using Plico;

public static partial class Views
{
    [Component]
    public static void CounterBody(Ui ui)
    {
        ref int count = ref ui.State(static () => 0);
        ui.Text($"Count: {count}");
        if (ui.Button("Increment").Clicked()) count++;
    }

    public static void Show(Ui ui)
    {
        Counter(ui);
        Counter(ui); // independent automatic component scope
    }
}
```

The generated method forwards parameters to the Body in two identity scopes:
caller file/line plus occurrence, then a compile-time 64-bit component salt.
The latter includes assembly/type/method signature identity, so different
components at the same source line cannot accidentally share an explicitly
keyed state slot just because they use the same key. The hash is deterministic,
not a mathematical guarantee of collision freedom. Caller-file hashing still
uses the SDK's existing cached runtime path; the generator does not replace
all runtime identity operations with constants.

Layout nodes are not added. Ordinary static/instance methods, nullable types,
optional arguments and ref/out/in forwarding are tested. The first parameter
must be `Plico.Ui`; methods are synchronous non-generic void methods on top-level
non-generic partial classes. Nested/record/generic types, extension methods,
async/returning methods, params and pointer parameters are outside this PoC.
Invalid shapes and collisions produce source-located PLICO001/PLICO002 errors.
Wrapper visibility matches Body visibility. Dynamic/reorderable lists still
need business identity scopes, and calling `CounterBody` directly bypasses the
wrapper.

## Local project wiring

The experiment's consumer projects explicitly load the analyzer:

```xml
<ProjectReference Include="../Plico.Generators/Plico.Generators.csproj"
                  OutputItemType="Analyzer" ReferenceOutputAssembly="false" />
```

`ComponentAttribute` is in the managed SDK. Merely referencing the SDK project
does not transitively load the analyzer into all consumers. NuGet analyzer
packaging is not implemented by this experiment; a published SDK would need the
prebuilt generator under `analyzers/dotnet/cs/`. Roslyn is a build/IDE dependency,
not part of the application runtime. The generator project explicitly disables
application AOT/trim/RID properties propagated by publish commands. This is not
an interceptor or IL weaver.

## Reproduce

From the repository root:

```sh
# Compiler checks: generated wrappers, defaults, forwarding, overloads, diagnostics.
dotnet run --project crates/plico-csharp/dotnet/ComponentGeneratorChecks -c Release

# Native integration: sibling isolation, remount and same-line component switches.
# Build plico-csharp and set PLICO_NATIVE_LIBRARY as in its README first.
dotnet run --project crates/plico-csharp/dotnet/Demo -c Release -- --self-test

# Real language server. Tool installation can live outside the repository.
dotnet tool install --tool-path /tmp/plico-component-tools csharp-ls --version 0.28.0
dotnet build crates/plico-csharp/dotnet/ComponentProbe
dotnet run --project crates/plico-csharp/dotnet/ComponentLspProbe -c Release -- \
  /tmp/plico-component-tools/csharp-ls \
  "$PWD/crates/plico-csharp/dotnet/ComponentProbe"
```

The Debug build before the LSP probe is significant: `csharp-ls` did not resolve
the wrapper when only the Release generator artifact existed. Building its
Debug artifact fixed design-time loading. This result does not establish the
same prerequisites for Visual Studio, Rider or Microsoft's VS Code server.

Tested with .NET SDK 10.0.403 and csharp-ls 0.28.0 on Linux:

| Operation | Observed |
| --- | --- |
| Ui.Text/State completion inside Body | Passed |
| Generated Counter member completion | Passed |
| Hover and signature help | Passed |
| Definition of Counter | Generated `.g.cs` virtual document |
| Rename a Body-local variable | Passed (applying the server's minimal text edits) |
| Live Body rename in the unsaved document | Wrapper completion changed without command-line rebuild |
| Wrong generated-call argument type | CS1503 published on the user's call |
| `dotnet watch` Body text edit | One warm observation: 474 ms, same PID, local count stayed 1 |
| NativeAOT generated-component integration | Passed; Roslyn/analyzer DLLs absent from runtime files |

The watch probe is `dotnet watch --project
crates/plico-csharp/dotnet/ComponentProbe --non-interactive run -- --watch-probe`.
During its 60-second loop, change `version-one` to `version-two` in
`SampleViews.cs`, observe PID/count/caption, then restore the file. No running
GUI/window is needed. Unsupported runtime edits may still restart the process.

## Limits of the editor experience

Body code is normal C#, so its completion, local rename and diagnostics do not
need a custom language service. Generated entry points are ordinary compiler
symbols, but entry/Body naming is a generator convention, not a Roslyn rename
relationship. Renaming CounterBody alone changes the generated entry point and
requires updating Counter callers separately. Named arguments have the analogous
parameter-rename consideration. A custom refactoring could address this later.

Hover/signature help exposes the optional `__plicoFile`/`__plicoLine` parameters.
Navigation first reaches generated code, then the Body call; this experiment does
not redirect definition directly to the Body. The generated entry can coexist
with handwritten components/scopes. It is not a new DSL or reactive model.
