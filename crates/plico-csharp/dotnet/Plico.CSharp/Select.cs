using System.Runtime.CompilerServices;

namespace Plico;

public readonly record struct SelectOption(string Id, string Label, bool Disabled = false);

public sealed unsafe partial class Ui
{
    private sealed class SelectState { internal bool Open; internal string Query = ""; internal int Highlight; }
    private readonly Dictionary<ulong, SelectState> selections = new();
    private readonly HashSet<ulong> liveSelections = new();
    private readonly HashSet<string> optionValidationIds = new(StringComparer.Ordinal);
    private readonly List<(int Trigger, int Portal, int Panel)> popupLinks = new();

    // Keep the capturing predicate in the open-only path. Otherwise the C#
    // compiler allocates its display class even when Select returns closed.
    private static SelectOption[] FilterOptions(IReadOnlyList<SelectOption> options, string query) =>
        options.Where(o => query.Length == 0 || o.Label.Contains(query, StringComparison.OrdinalIgnoreCase)
            || o.Id.Contains(query, StringComparison.OrdinalIgnoreCase)).Take(80).ToArray();

    /// Managed widget: owns popup/search/selection policy and composes primitives.
    public Builder Select(string label, ref string selected, IReadOnlyList<SelectOption> options, bool searchable = false,
        string? key = null, [CallerFilePath] string file = "", [CallerLineNumber] int line = 0)
    {
        ArgumentNullException.ThrowIfNull(options);
        if (options.Count > 20_000) throw new ArgumentException("Select has more than 20000 options", nameof(options));
        optionValidationIds.Clear();
        string display = selected;
        for (int i = 0; i < options.Count; i++)
        {
            var option = options[i];
            if (string.IsNullOrEmpty(option.Id) || !optionValidationIds.Add(option.Id))
            {
                optionValidationIds.Clear();
                throw new ArgumentException("Select requires unique nonempty option IDs", nameof(options));
            }
            if (option.Id == selected) display = option.Label;
        }
        optionValidationIds.Clear();
        var trigger = Button(display.Length == 0 ? label : display, key, file, line).AriaLabel(label).Width(300).JustifyStart();
        commands[trigger.Index].semantic_role = 3;
        liveSelections.Add(trigger.Id);
        if (!selections.TryGetValue(trigger.Id, out var state)) selections[trigger.Id] = state = new();
        if (trigger.Clicked()) { state.Open = !state.Open; state.Query = ""; state.Highlight = 0; }
        if (!state.Open) return trigger;
        using var identity = Scope(trigger.Id);
        int portalIndex = Add(17, "", file, line, "popup");
        commands[portalIndex].resource = trigger.Id;
        if (events.TryGetValue(commands[portalIndex].id, out var dismissed) && dismissed.Kind == 1) state.Open = false;
        using (new Container(this, portalIndex))
        {
            using var panel = Column(key: "panel", file: file, line: line).Width(300).Padding(8).Gap(4).Radius(8)
                .Background(Theme.Panel).Border(1, Theme.Border);
            commands[parents.Peek()].kind = 18;
            popupLinks.Add((trigger.Index, portalIndex, parents.Peek()));
            double navigation = 0;
            if (searchable)
            {
                string oldQuery = state.Query;
                var input = TextInput("Search options", ref state.Query, key: "search", file: file, line: line).FillWidth();
                commands[input.Index].flags |= (1U << 25) | (1U << 26);
                if (events.TryGetValue(input.Id, out var inputEvent)) navigation = inputEvent.Value;
                if (oldQuery != state.Query) state.Highlight = 0;
            }
            var filtered = FilterOptions(options, state.Query);
            if (navigation == 4) state.Open = false;
            if (navigation is 2 or 3 or -2 or -3 && filtered.Length > 0)
            {
                int delta = navigation > 0 ? 1 : -1;
                for (int attempt = 0; attempt < filtered.Length; attempt++)
                {
                    state.Highlight = (state.Highlight + delta + filtered.Length) % filtered.Length;
                    if (!filtered[state.Highlight].Disabled) break;
                }
            }
            state.Highlight = Math.Clamp(state.Highlight, 0, Math.Max(0, filtered.Length - 1));
            using (Column(key: "options", file: file, line: line).FillWidth().Height(Math.Min(300, Math.Max(36, filtered.Length * 36))).ScrollY())
            {
                foreach (var option in filtered)
                {
                    using var itemIdentity = Scope(option.Id);
                    var item = Button(option.Label, key: "option", file: file, line: line).Ghost().FillWidth().JustifyStart().Disabled(option.Disabled);
                    commands[item.Index].semantic_role = 4;
                    if (option.Id == selected) commands[item.Index].flags |= 1U << 24;
                    if (option.Id == filtered.ElementAtOrDefault(state.Highlight).Id) item.Background(Theme.Control);
                    if (item.Clicked() || (navigation == 1 && option.Id == filtered.ElementAtOrDefault(state.Highlight).Id && !option.Disabled))
                    { selected = option.Id; state.Open = false; }
                }
                if (filtered.Length == 0) Text("No matching options", key: "empty", file: file, line: line);
            }
        }
        return trigger;
    }
}
