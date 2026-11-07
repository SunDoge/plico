namespace Plico.Interop;

internal static unsafe partial class Native
{
    // Raw internal/native benchmarks get the same resolver as Application.
    static Native() => Plico.NativeRuntime.Initialize();
}
