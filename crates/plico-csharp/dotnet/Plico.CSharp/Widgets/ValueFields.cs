using System.Globalization;
using System.Runtime.CompilerServices;
namespace Plico;

public sealed unsafe partial class Ui
{
    private sealed class ValueFieldState<T>
    {
        internal bool Initialized;
        internal T Last = default!;
        internal string Text = "";
    }

    /// Keeps incomplete input (such as "-") locally without corrupting the typed
    /// application value. Plus/minus buttons clamp to the finite range. Invariant
    /// numeric syntax is intentional; locale-aware formatting is separate work.
    public bool NumberInput(string label, ref double value, double minimum = 0, double maximum = 100, double step = 1,
        [CallerFilePath] string file = "", [CallerLineNumber] int line = 0)
    {
        if (!double.IsFinite(value) || !double.IsFinite(minimum) || !double.IsFinite(maximum) || minimum > maximum || !double.IsFinite(step) || step <= 0)
            throw new ArgumentOutOfRangeException(nameof(step));
        using var scope = Scope(file: file, line: line);
        var state = State(static () => new ValueFieldState<double>(), key: "edit");
        double before = value;
        value = Math.Clamp(value, minimum, maximum);
        if (!state.Initialized || state.Last != value)
        { state.Text = value.ToString("G", CultureInfo.InvariantCulture); state.Initialized = true; }
        using var root = Column(key: "number").FillWidth().Gap(4);
        bool valid;
        using (Row(key: "field").FillWidth().Gap(6).AlignCenter())
        {
            var input = TextInput(label, ref state.Text, key: "input").Width(0).Grow().MinWidth(0);
            valid = double.TryParse(state.Text, NumberStyles.Float, CultureInfo.InvariantCulture, out double parsed)
                && double.IsFinite(parsed) && parsed >= minimum && parsed <= maximum;
            if (valid) value = parsed;
            else input.Border(1, Theme.Danger);
            if (Button("−", key: "decrease").AriaLabel($"Decrease {label}").Disabled(value <= minimum).Clicked() && value > minimum)
            { value = Math.Clamp(value - step, minimum, maximum); state.Text = value.ToString("G", CultureInfo.InvariantCulture); }
            if (Button("+", key: "increase").AriaLabel($"Increase {label}").Disabled(value >= maximum).Clicked() && value < maximum)
            { value = Math.Clamp(value + step, minimum, maximum); state.Text = value.ToString("G", CultureInfo.InvariantCulture); }
        }
        if (!valid) Text($"Enter a number between {minimum:G} and {maximum:G}.", key: "error").Color(Theme.Danger).FillWidth();
        state.Last = value;
        return value != before;
    }

    /// Typed HH:mm editor. Incomplete input stays local; external value changes
    /// replace the edit buffer. Seconds are intentionally omitted from display.
    public bool TimeInput(string label, ref TimeOnly value,
        [CallerFilePath] string file = "", [CallerLineNumber] int line = 0)
    {
        using var scope = Scope(file: file, line: line);
        var state = State(static () => new ValueFieldState<TimeOnly>(), key: "edit");
        if (!state.Initialized || state.Last != value)
        { state.Text = value.ToString("HH:mm", CultureInfo.InvariantCulture); state.Initialized = true; }
        using var root = Column(key: "time").Width(160).Gap(4);
        string previousText = state.Text;
        var input = TextInput(label, ref state.Text, key: "input").Placeholder("HH:mm").FillWidth();
        TimeOnly before = value;
        if (TimeOnly.TryParseExact(state.Text, "HH:mm", CultureInfo.InvariantCulture, DateTimeStyles.None, out var parsed)) { if (state.Text != previousText) value = parsed; }
        else
        {
            input.Border(1, Theme.Danger);
            Text("Use HH:mm (24-hour time).", key: "error").Color(Theme.Danger).FillWidth();
        }
        state.Last = value;
        return before != value;
    }
}
