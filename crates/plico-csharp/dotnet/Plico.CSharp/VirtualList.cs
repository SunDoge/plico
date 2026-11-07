using System.Runtime.CompilerServices;
using System.Text.Json;

namespace Plico;

public sealed unsafe partial class Ui
{
    private readonly Dictionary<ulong, (int Start, int End)> listRanges = new();
    private readonly HashSet<ulong> liveLists = new();

    /// Fixed-height native scrolling list. Declare exactly one direct child per
    /// index in [Start, End); keep row state outside the mounted window.
    public VirtualListScope VirtualList(int count, float rowHeight, float height = 240, bool fill = false,
        string? key = null, [CallerFilePath] string file = "", [CallerLineNumber] int line = 0)
    {
        if (count < 0 || count > 1_000_000 || !float.IsFinite(rowHeight) || rowHeight <= 0 || rowHeight > 16384
            || (double)count * rowHeight > 100_000_000 || !float.IsFinite(height) || height <= 0 || height > 16384)
            throw new ArgumentOutOfRangeException(nameof(count), "Invalid list extent or viewport");
        int index = Add(15, "", file, line, key);
        ulong id = commands[index].id;
        liveLists.Add(id);
        if (events.TryGetValue(id, out var value) && value.Kind == 7)
        {
            string[] parts = value.Text.Split(',');
            listRanges[id] = (int.Parse(parts[0]), int.Parse(parts[1]));
        }
        var range = listRanges.GetValueOrDefault(id, (0, Math.Min(count, (int)Math.Ceiling(height / rowHeight) + 2)));
        int start = Math.Clamp(range.Item1, 0, count), end = Math.Clamp(range.Item2, start, count);
        var encoded = Encode(JsonSerializer.Serialize(new ListPayload(count, rowHeight, start, end), PayloadJsonContext.Default.ListPayload));
        commands[index].value_start = encoded.Item1; commands[index].value_len = encoded.Item2;
        commands[index].height = height; commands[index].grow = fill ? 1 : 0;
        return new VirtualListScope(new Container(this, index), start, end);
    }
}

public readonly struct VirtualListScope : IDisposable
{
    private readonly Ui.Container container;
    public int Start { get; }
    public int End { get; }
    internal VirtualListScope(Ui.Container container, int start, int end)
    { this.container = container; Start = start; End = end; }
    public void Dispose() => container.Dispose();
}
