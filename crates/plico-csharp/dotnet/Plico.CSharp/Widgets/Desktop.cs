using System.Runtime.CompilerServices;
namespace Plico;

public static class DesktopWidgets
{
    public static bool Tabs(this Ui ui, IReadOnlyList<string> labels, ref int selected,
        [CallerFilePath] string file = "", [CallerLineNumber] int line = 0)
    {
        ArgumentNullException.ThrowIfNull(labels);
        if (labels.Count == 0) throw new ArgumentException("At least one tab is required", nameof(labels));
        selected = Math.Clamp(selected, 0, labels.Count - 1);
        bool changed = false;
        using (ui.Scope(file: file, line: line))
        using (ui.Row().Gap(4).FillWidth().Shrink())
            for (int i = 0; i < labels.Count; i++)
                using (ui.Scope(i))
                using (ui.Column().Gap(6))
                {
                    if (ui.Button(labels[i]).Ghost().Color(selected == i ? ui.Theme.Foreground : ui.Theme.Muted).Weight(selected == i ? 600U : 400U).Clicked() && selected != i) { selected = i; changed = true; }
                    using (ui.Column().FillWidth().Height(2).Background(ui.Theme.Accent).BackgroundAlpha(selected == i ? 1 : 0)) { }
                }
        return changed;
    }
    public static void Meter(this Ui ui, string label, double value, double maximum = 100,
        [CallerFilePath] string file = "", [CallerLineNumber] int line = 0)
    {
        if (!double.IsFinite(value) || !double.IsFinite(maximum) || maximum <= 0) throw new ArgumentOutOfRangeException(nameof(maximum));
        using (ui.Scope(file: file, line: line))
        using (ui.Column().FillWidth().Gap(8))
        {
            ui.Text($"{label}: {value:0} / {maximum:0}");
            using (ui.Column().FillWidth().Height(6).Radius(3).Background(ui.Theme.Border))
                using (ui.Column().WidthPercent((float)Math.Clamp(value / maximum * 100, 0, 100)).Height(6).Radius(3).Background(value / maximum >= .95 ? ui.Theme.Danger : value / maximum >= .8 ? ui.Theme.Warning : ui.Theme.Success)) { }
        }
    }
    public static bool Rating(this Ui ui, ref int value, int maximum = 5,
        [CallerFilePath] string file = "", [CallerLineNumber] int line = 0)
    {
        if (maximum is < 1 or > 20) throw new ArgumentOutOfRangeException(nameof(maximum));
        bool changed = false;
        using (ui.Scope(file: file, line: line))
        using (ui.Row().Gap(4))
            for (int i = 1; i <= maximum; i++)
                using (ui.Scope(i))
                    if (ui.Button(i <= value ? "★" : "☆").AriaLabel($"Rate {i}").Ghost().Clicked()) { value = value == i ? 0 : i; changed = true; }
        return changed;
    }
    public static void Stepper(this Ui ui, string label, ref int value, int minimum = 0, int maximum = 100,
        [CallerFilePath] string file = "", [CallerLineNumber] int line = 0)
    {
        if (minimum > maximum) throw new ArgumentOutOfRangeException(nameof(minimum));
        value = Math.Clamp(value, minimum, maximum);
        using (ui.Scope(file: file, line: line))
        using (ui.Row().Gap(8).AlignCenter())
        {
            ui.Text($"{label}: {value}");
            if (ui.Button("−").AriaLabel($"Decrease {label}").Disabled(value <= minimum).Clicked()) value--;
            if (ui.Button("+").AriaLabel($"Increase {label}").Disabled(value >= maximum).Clicked()) value++;
        }
    }
}
