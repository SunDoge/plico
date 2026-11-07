using System.Runtime.CompilerServices;
using System.Text.Json;
using Plico.Interop;

namespace Plico;

public enum CanvasTool { Draw, Select, Pan }
public readonly record struct CanvasRegion(ulong Id, double[] Rect);
public sealed record CanvasEdit(string Kind, ulong? Id, double[]? Rect);

public sealed unsafe partial class Ui
{
    private readonly Dictionary<ulong, ulong> canvasDocuments = new();

    /// One retained, native image canvas per window. Rectangles are original image pixels.
    public Builder ImageCanvas(ulong image, ulong document, IReadOnlyList<CanvasRegion> regions,
        ulong? selected = null, CanvasTool tool = CanvasTool.Select, string? key = null,
        [CallerFilePath] string file = "", [CallerLineNumber] int line = 0)
    {
        var builder = Leaf(13, "", file, line, key);
        canvasDocuments[builder.Id] = document;
        string json = JsonSerializer.Serialize(new CanvasPayload(document, selected, (uint)tool, regions), PayloadJsonContext.Default.CanvasPayload);
        var encoded = Encode(json);
        ref var command = ref commands[builder.Index];
        command.resource = image; command.value_start = encoded.Item1; command.value_len = encoded.Item2;
        return builder;
    }

    public Builder TextArea(string label, ref string value, string? key = null,
        [CallerFilePath] string file = "", [CallerLineNumber] int line = 0)
    {
        var builder = TextInput(label, ref value, key, file, line);
        commands[builder.Index].flags |= 1U << 19;
        return builder;
    }
    public Builder PasswordField(string label, ref string value, string? key = null,
        [CallerFilePath] string file = "", [CallerLineNumber] int line = 0)
    {
        var builder = TextInput(label, ref value, key, file, line);
        commands[builder.Index].flags |= 1U << 18;
        return builder;
    }
    internal IReadOnlyList<CanvasEdit> ReadCanvas(ulong id)
    {
        if (!events.TryGetValue(id, out var value) || value.Kind != 5 || !canvasDocuments.TryGetValue(id, out var document))
            return Array.Empty<CanvasEdit>();
        using var json = JsonDocument.Parse(value.Text);
        if (json.RootElement.GetProperty("document").GetUInt64() != document) return Array.Empty<CanvasEdit>();
        return json.RootElement.GetProperty("events").Deserialize(PayloadJsonContext.Default.CanvasEditArray) ?? [];
    }
}
