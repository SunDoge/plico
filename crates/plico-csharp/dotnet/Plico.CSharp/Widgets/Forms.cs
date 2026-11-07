using System.Runtime.CompilerServices;
namespace Plico;

public sealed unsafe partial class Ui
{
    public bool SearchField(string label, ref string query, string placeholder = "Search…",
        [CallerFilePath] string file = "", [CallerLineNumber] int line = 0)
    {
        string before = query;
        using (Row(file: file, line: line).FillWidth().Gap(6).AlignCenter())
        {
            TextInput(label, ref query, key: "query").Placeholder(placeholder).Width(0).Grow().MinWidth(0);
            if (query.Length > 0 && Button("×", key: "clear").AriaLabel($"Clear {label}").Ghost().Clicked()) query = "";
        }
        return before != query;
    }

    /// Two native range-edit surfaces; not a shared two-thumb track.
    public bool RangeSlider(string label, ref double low, ref double high, double minimum = 0, double maximum = 100,
        [CallerFilePath] string file = "", [CallerLineNumber] int line = 0)
    {
        if (!double.IsFinite(minimum) || !double.IsFinite(maximum) || minimum > maximum || !double.IsFinite(low) || !double.IsFinite(high)) throw new ArgumentOutOfRangeException(nameof(maximum));
        double oldLow = low, oldHigh = high;
        low = Math.Clamp(low, minimum, maximum); high = Math.Clamp(high, low, maximum);
        using (Column(file: file, line: line).FillWidth().Gap(8))
        {
            Text($"{label}: {low:0.##} – {high:0.##}", key: "label").Weight(600);
            Slider(label + " minimum", ref low, minimum, high, key: "low").FillWidth();
            Slider(label + " maximum", ref high, low, maximum, key: "high").FillWidth();
        }
        return oldLow != low || oldHigh != high;
    }

    /// RGB channel editor, returned as 0xRRGGBB. Alpha/HSV/eyedropper are not implied.
    public bool ColorPicker(string label, ref uint rgb,
        [CallerFilePath] string file = "", [CallerLineNumber] int line = 0)
    {
        uint before = rgb;
        double red = (rgb >> 16) & 255, green = (rgb >> 8) & 255, blue = rgb & 255;
        using (Column(file: file, line: line).FillWidth().Gap(8))
        {
            Text(label, key: "label").Weight(600);
            using (Row(key: "channels").FillWidth().Gap(16).AlignCenter())
            {
                using (Column(key: "sliders").Width(0).Grow().MinWidth(0).Gap(4))
                {
                    Slider(label + " red", ref red, 0, 255, key: "red").FillWidth();
                    Slider(label + " green", ref green, 0, 255, key: "green").FillWidth();
                    Slider(label + " blue", ref blue, 0, 255, key: "blue").FillWidth();
                }
                rgb = ((uint)Math.Round(red) << 16) | ((uint)Math.Round(green) << 8) | (uint)Math.Round(blue);
                using (Box(key: "swatch").Width(56).Height(56).Shrink().Radius(8).Background(rgb).Border(1, Theme.Border)) { }
            }
            Text($"#{rgb:x6}", key: "hex").Color(Theme.Muted);
        }
        return rgb != before;
    }

    /// Returns -1 for pending/dismissed; otherwise the chosen action index.
    public int AlertDialog(ref bool open, string title, string message, IReadOnlyList<string> actions,
        [CallerFilePath] string file = "", [CallerLineNumber] int line = 0)
    {
        ArgumentNullException.ThrowIfNull(actions);
        if (actions.Count == 0) throw new ArgumentException("At least one action is required", nameof(actions));
        if (!open) return -1;
        using var identity = Scope(file: file, line: line);
        using (Modal(ref open, Math.Max(200, Math.Min(420, Width - 32))))
        {
            Text(title, key: "title").FontSize(20).Weight(650).FillWidth();
            Text(message, key: "message").Color(Theme.Muted).FillWidth();
            using (Row(key: "actions").FillWidth().JustifyEnd().Gap(8).Wrap())
                for (int i = 0; i < actions.Count; i++)
                    using (Scope(i))
                    {
                        var button = Button(actions[i]);
                        if (i == actions.Count - 1) button.Primary();
                        if (button.Clicked()) { open = false; return i; }
                    }
        }
        return -1;
    }

    /// Inline dismissible feedback. Lifetime and retry action belong to the caller.
    public bool Notification(string message, ref bool visible, Tone tone = Tone.Info,
        [CallerFilePath] string file = "", [CallerLineNumber] int line = 0)
    {
        if (!visible) return false;
        using (Row(file: file, line: line).FillWidth().Shrink().Padding(12).Gap(12).Radius(8).Background(Theme.Control).Border(1, Theme.Border).AlignCenter())
        {
            Text(message, key: "message").Width(0).Grow().MinWidth(0).Color(ToneColor(tone));
            if (Button("×", key: "dismiss").AriaLabel("Dismiss notification").Ghost().Clicked()) { visible = false; return true; }
        }
        return false;
    }

    public bool FindBar(ref string query, int matches, ref int current,
        [CallerFilePath] string file = "", [CallerLineNumber] int line = 0)
    {
        if (matches < 0) throw new ArgumentOutOfRangeException(nameof(matches));
        current = matches == 0 ? 0 : Math.Clamp(current, 1, matches);
        bool changed;
        using (Row(file: file, line: line).FillWidth().Gap(8).AlignCenter().Wrap())
        {
            using (Column(key: "search").Width(220).Grow().MinWidth(120)) changed = SearchField("Find", ref query);
            Text(matches == 0 ? "No matches" : $"{current} / {matches}", key: "count").Color(Theme.Muted);
            if (Button("↑", key: "previous").AriaLabel("Previous match").Disabled(matches == 0).Clicked() && matches > 0) { current = current <= 1 ? matches : current - 1; changed = true; }
            if (Button("↓", key: "next").AriaLabel("Next match").Disabled(matches == 0).Clicked() && matches > 0) { current = current >= matches ? 1 : current + 1; changed = true; }
        }
        return changed;
    }
}
