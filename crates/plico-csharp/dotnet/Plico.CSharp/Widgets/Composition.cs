using System.Runtime.CompilerServices;
namespace Plico;

public sealed unsafe partial class Ui
{
    /// Composed button content: icon/text/other primitives remain managed children.
    public Container ButtonContent(string accessibleName, bool primary = false, string? key = null,
        [CallerFilePath] string file = "", [CallerLineNumber] int line = 0)
    {
        var builder = Button("", key, file, line).AriaLabel(accessibleName);
        if (primary) builder.Primary();
        return new Container(this, builder.Index);
    }
    public void Progress(double value, double maximum = 1, string? key = null,
        [CallerFilePath] string file = "", [CallerLineNumber] int line = 0)
    {
        if (!double.IsFinite(value) || !double.IsFinite(maximum) || maximum <= 0) throw new ArgumentOutOfRangeException(nameof(maximum));
        using (Column(key, file, line).FillWidth().Height(6).Radius(3).Background(Theme.Border))
            using (Column(key: "fill", file: file, line: line).WidthPercent((float)Math.Clamp(value / maximum * 100, 0, 100)).Height(6).Radius(3).Background(Theme.Accent)) { }
    }
    public bool Radio<T>(string label, T option, ref T selected, string? key = null,
        [CallerFilePath] string file = "", [CallerLineNumber] int line = 0)
    {
        using var scope = Scope(key is null ? $"radio:{file}:{line}" : $"radio-key:{key}");
        using var button = ButtonContent(label, key: "radio", file: file, line: line).Background(Theme.Panel).Border(0, Theme.Border);
        bool active = EqualityComparer<T>.Default.Equals(option, selected);
        Text(active ? "●" : "○", key: "mark", file: file, line: line).Color(active ? Theme.Accent : Theme.Muted);
        Text(label, key: "label", file: file, line: line);
        if (!button.Clicked() || active) return false;
        selected = option; return true;
    }
    /// Native text submission (Enter); multiline editors retain their native Enter semantics.
    public readonly partial struct Builder
    {
        public bool Submitted() => ui.events.TryGetValue(Id, out var e) && e.Kind == 4 && e.Value == 1;
    }
}
