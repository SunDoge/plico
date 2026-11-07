using Plico;
using System.Text.Json.Serialization;
internal sealed record DemoResult(string result, int NodeCount, int Utf8Bytes, BridgeStats stats);
internal sealed record BenchResult(int iterations, double cpuFrameMicroseconds, double managedBuildMicroseconds,
    double nativeDecodeMicroseconds, double nativeDeclarationMicroseconds, double managedAllocatedBytesPerBuild,
    int NodeCount, int Utf8Bytes, string note);
[JsonSerializable(typeof(DemoResult))]
[JsonSerializable(typeof(BenchResult))]
internal partial class DemoJsonContext : JsonSerializerContext { }
