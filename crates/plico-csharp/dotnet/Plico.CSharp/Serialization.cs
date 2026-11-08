using System.Text.Json.Serialization;
namespace Plico;

internal sealed record CanvasPayload(ulong Document, ulong? Selected, uint Tool, IReadOnlyList<CanvasRegion> Regions);
internal sealed record ListPayload(int Count, float RowHeight, int Start, int End);

[JsonSourceGenerationOptions(PropertyNamingPolicy = JsonKnownNamingPolicy.SnakeCaseLower,
    DefaultIgnoreCondition = JsonIgnoreCondition.WhenWritingDefault)]
[JsonSerializable(typeof(Ui.NodeProperties))]
internal partial class PropertyJsonContext : JsonSerializerContext { }

// Payloads must retain zero values and null selection; only properties omit defaults.
[JsonSourceGenerationOptions(PropertyNamingPolicy = JsonKnownNamingPolicy.SnakeCaseLower)]
[JsonSerializable(typeof(CanvasPayload))]
[JsonSerializable(typeof(ListPayload))]
[JsonSerializable(typeof(CanvasEdit[]))]
internal partial class PayloadJsonContext : JsonSerializerContext { }

[JsonSourceGenerationOptions(PropertyNamingPolicy = JsonKnownNamingPolicy.CamelCase,
    DefaultIgnoreCondition = JsonIgnoreCondition.WhenWritingNull)]
[JsonSerializable(typeof(WindowOptions))]
internal partial class WindowJsonContext : JsonSerializerContext { }
