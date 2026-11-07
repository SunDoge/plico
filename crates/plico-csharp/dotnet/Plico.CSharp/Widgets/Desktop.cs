using System.Runtime.CompilerServices;
namespace Plico;

public static class DesktopWidgets
{
    public static bool Tabs(this Ui ui, IReadOnlyList<string> labels, ref int selected,
        [CallerFilePath] string file = "", [CallerLineNumber] int line = 0)
    {
        bool changed = false;
        using (ui.Scope($"tabs:{file}:{line}"))
        using (ui.Row().Gap(4).FillWidth().Shrink())
            for (int i = 0; i < labels.Count; i++)
                using (ui.Scope(i))
                using (ui.Column().Gap(6))
                {
                    if (ui.Button(labels[i]).Ghost().Clicked() && selected != i) { selected = i; changed = true; }
                    using (ui.Column().FillWidth().Height(2).Background(selected == i ? 0x285bd6U : 0x000000U).BackgroundAlpha(selected == i ? 1 : 0)) { }
                }
        return changed;
    }
    public static void Meter(this Ui ui, string label, double value, double maximum = 100)
    {
        if (!double.IsFinite(value) || !double.IsFinite(maximum) || maximum <= 0) throw new ArgumentOutOfRangeException(nameof(maximum));
        using (ui.Scope(label))
        using (ui.Column().FillWidth().Gap(8))
        {
            ui.Text($"{label}: {value:0} / {maximum:0}");
            using (ui.Column().FillWidth().Height(6).Radius(3).Background(0xd9d9de))
                using (ui.Column().WidthPercent((float)Math.Clamp(value / maximum * 100, 0, 100)).Height(6).Radius(3).Background(value / maximum >= .95 ? 0xdc2626U : value / maximum >= .8 ? 0xd97706U : 0x285bd6U)) { }
        }
    }
    public static bool Rating(this Ui ui, ref int value, int maximum = 5,
        [CallerFilePath] string file = "", [CallerLineNumber] int line = 0)
    {
        if (maximum is < 1 or > 20) throw new ArgumentOutOfRangeException(nameof(maximum));
        bool changed = false;
        using (ui.Scope($"rating:{file}:{line}"))
        using (ui.Row().Gap(4))
            for (int i = 1; i <= maximum; i++)
                using (ui.Scope(i))
                    if (ui.Button(i <= value ? "★" : "☆").AriaLabel($"Rate {i}").Ghost().Clicked()) { changed = value != i; value = i; }
        return changed;
    }
    public static void Stepper(this Ui ui, string label, ref int value, int minimum = 0, int maximum = 100)
    {
        if (minimum > maximum) throw new ArgumentOutOfRangeException(nameof(minimum));
        using (ui.Scope(label))
        using (ui.Row().Gap(8).AlignCenter())
        {
            ui.Text($"{label}: {value}");
            if (ui.Button("−").AriaLabel($"Decrease {label}").Disabled(value <= minimum).Clicked()) value--;
            if (ui.Button("+").AriaLabel($"Increase {label}").Disabled(value >= maximum).Clicked()) value++;
        }
    }
}
