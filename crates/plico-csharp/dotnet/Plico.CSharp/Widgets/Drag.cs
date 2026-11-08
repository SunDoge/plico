using System.Globalization;
using System.Runtime.CompilerServices;
namespace Plico;

public readonly record struct DragDelta(double X, double Y);
public sealed unsafe partial class Ui
{
    /// Transparent native pointer-capture surface. Layout, visuals and the
    /// response to deltas belong to C#; a drag holds no managed callback in Rust.
    public DragDelta DragArea(string label = "Drag surface",
        [CallerFilePath] string file = "", [CallerLineNumber] int line = 0)
    {
        using var box = Box(file: file, line: line).Interactive(label).FillWidth().FillHeight();
        Properties(box.Index).Draggable = true;
        if (!events.TryGetValue(box.Id, out var e) || e.Kind != 8) return default;
        if (!double.TryParse(e.Text, NumberStyles.Float, CultureInfo.InvariantCulture, out double dy) || !double.IsFinite(dy))
            throw new InvalidOperationException("Invalid native drag delta");
        return new(e.Value, dy);
    }
}
