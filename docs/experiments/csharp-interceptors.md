# C# component interceptors experiment

> 历史实验：C# 前端与跨语言 ABI 已从当前源码树移除。本文保留测量结论；其中命令和源码路径仅适用于对应历史提交。

The SDK package and C# Gallery now use this path. The Body-wrapper
probe remains a regression fixture; the implementation still has the boundaries
listed below. The compiler mechanism itself is stable in SDK 9.0.2xx and later;
Plico's implementation is experimental. It uses Roslyn's opaque
`GetInterceptableLocation`/`GetInterceptsLocationAttributeSyntax` API, not the
legacy path/line/column attribute encoding. See the
[Roslyn specification](https://github.com/dotnet/roslyn/blob/main/docs/features/interceptors.md).

## Authoring

```csharp
using Plico;

public static class Views
{
    [Component]
    public static void Counter(Ui ui)
    {
        ref int count = ref ui.State(static () => 0);
        ui.Text($"Count: {count}");
        if (ui.Button("Increment").Clicked()) count++;
    }
}

// Direct calls acquire independent scopes without renaming the component.
using var app = new Application(ui => {
    Views.Counter(ui);
    Views.Counter(ui);
});
```

No Body suffix or partial type is required for accessible public/internal
components. Private/protected methods need a partial class: an internal generated
access bridge lives in that class, while the interceptor remains in Plico's own
opted-in namespace. Static and instance access bridges were runtime-tested.

The generated interceptor enters the existing call-site scope and a compile-time
component-type salt, then calls the original method. It does not alter the
original declaration, signature, body, captures or control flow. Generated calls
to the original are not intercepted recursively. Helpers have stable names based
on the enclosing caller and component-call ordinal, rather than raw character
offsets; changing text length before a call therefore does not rename the helper.

This is not edit-proof identity. Runtime call-site scopes still use caller
file/line and occurrence; local State/widget identities also retain their own
existing rules. Source-line changes and structural edits can reset state. Lists
still need business scopes. Hashes are deterministic, not collision-free proofs.

## Opt in and reproduce

Consumer wiring (see InterceptorProbe.csproj):

```xml
<PropertyGroup>
  <PlicoComponentInterceptors>true</PlicoComponentInterceptors>
  <InterceptorsNamespaces>$(InterceptorsNamespaces);Plico.Generated</InterceptorsNamespaces>
</PropertyGroup>
<ItemGroup>
  <CompilerVisibleProperty Include="PlicoComponentInterceptors" />
  <ProjectReference Include="../Plico.Generators/Plico.Generators.csproj"
                    OutputItemType="Analyzer" ReferenceOutputAssembly="false" />
</ItemGroup>
```

The SDK NuGet package carries the analyzer and imports this configuration
automatically. Source-checkout apps import the same props and add an analyzer
ProjectReference (see Gallery). Bare analyzer references without these props
retain the older Body-generator mode. Merely writing an attribute does not enforce interception
when the generator is absent; it is not a runtime mechanism.

```sh
# Compiler checks include both generator modes, with warnings treated as errors
# for the interceptor fixtures; no language preview flag is needed.
dotnet run --project crates/plico-csharp/dotnet/ComponentGeneratorChecks -c Release

# Set PLICO_NATIVE_LIBRARY as in the SDK README first.
dotnet run --project crates/plico-csharp/dotnet/InterceptorProbe/InterceptorProbe.csproj -c Release -- --self-test

# csharp-ls 0.28.0, installed as in the Body experiment.
dotnet build crates/plico-csharp/dotnet/InterceptorProbe/InterceptorProbe.csproj
dotnet run --project crates/plico-csharp/dotnet/ComponentLspProbe -c Release -- \
  /tmp/plico-component-tools/csharp-ls \
  "$PWD/crates/plico-csharp/dotnet/InterceptorProbe" --interceptors

# Explicit .csproj: this directory also contains a solution for the LSP probe.
dotnet publish crates/plico-csharp/dotnet/InterceptorProbe/InterceptorProbe.csproj \
  -c Release -r linux-x64 -p:PublishAot=true -o reports/csharp/interceptor-aot
```

## Results

Observed on Linux, .NET SDK 10.0.403, Roslyn generator API 4.14.0, csharp-ls 0.28.0:

| Check | Result |
| --- | --- |
| Original Counter name, no Body wrapper | Passed |
| Same-line siblings and nested call sites | Passed; independent state |
| Ref/default argument forwarding | Passed |
| Private static and private instance methods | Passed with partial-class access bridges |
| Keyed list reorder, component-type switch and unmount/remount | Passed |
| Body/member completion and hover | Passed |
| Go to definition | Original Counter source, not a generated wrapper |
| Signature help | Original signature, no caller-info parameters |
| Local-variable rename | Passed |
| Component rename | Declaration and user call sites updated together, no generated-source edits |
| Live editor refresh and wrong argument type | Passed; CS1503 on original call |
| Warm `dotnet watch` edit to longer Body text | 439 ms in one observation; same PID and local count 1 |
| NativeAOT | Runtime checks and wgpu offscreen capture passed; no Roslyn/analyzer DLL in app output |

The watch command is `dotnet watch --project
crates/plico-csharp/dotnet/InterceptorProbe/InterceptorProbe.csproj
--non-interactive run -- --watch-probe`. Change `version-one` to
`version-two-longer` in SampleViews.cs, observe PID/count, then restore it.
This is one supported method-body edit, not a guarantee for arbitrary edits.
Visual Studio/Rider/Microsoft's VS Code server were not tested.

## Important boundaries

Only direct ordinary method invocations are intercepted. A conversion such as
`Action<Ui> view = Counter` does not insert identity scopes. The generator emits
PLICO012 at such method groups; with warnings-as-errors it stops the build. Use
`ui => Counter(ui)` instead. The runtime probe intentionally suppresses the
warning and demonstrates a duplicate state key for two unscoped delegate calls;
the corresponding lambda with two direct calls succeeds. Reflection/dynamic
calls and calls in code compiled without the generator are not covered by this
scope mechanism.

This PoC supports top-level, non-generic classes and synchronous non-generic,
non-virtual void methods with Ui first. Ref/out/in, nullable/default/params
parameters and conditional receiver calls have compiler checks. Nested/generic/
record/file-local types, virtual/override/extension methods and pointer signatures
are outside its support; unsupported invocations produce PLICO010. Source/API
coverage and large-project compile overhead are not yet a production guarantee.
