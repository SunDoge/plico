using System.Runtime.CompilerServices;
namespace Plico;

public sealed unsafe partial class Ui
{
    /// Static ordered options; use business scopes around dynamically reordered groups.
    public bool Segmented(IReadOnlyList<string> labels, ref int selected,
        [CallerFilePath] string file = "", [CallerLineNumber] int line = 0)
    {
        ArgumentNullException.ThrowIfNull(labels);
        if (labels.Count == 0) throw new ArgumentException("At least one segment is required", nameof(labels));
        bool changed = false;
        selected = Math.Clamp(selected, 0, labels.Count - 1);
        using (Row(file: file, line: line).Gap(3).Padding(3).Radius(8).Background(Theme.Control).Wrap())
            for (int i = 0; i < labels.Count; i++)
                using (Scope(i))
                {
                    var segment = Button(labels[i]).Ghost().Height(32).Motion();
                    if (i == selected) segment.Background(Theme.Panel).Border(1, Theme.Border);
                    if (segment.Clicked() && selected != i) { selected = i; changed = true; }
                }
        return changed;
    }

    public bool Breadcrumbs(IReadOnlyList<string> items, ref int chosen,
        [CallerFilePath] string file = "", [CallerLineNumber] int line = 0)
    {
        ArgumentNullException.ThrowIfNull(items);
        bool changed = false;
        using (Row(file: file, line: line).Gap(4).Wrap().AlignCenter())
            for (int i = 0; i < items.Count; i++)
                using (Scope(i))
                {
                    if (i > 0) Text("›", key: "separator").Color(Theme.Muted);
                    if (i == items.Count - 1) Text(items[i], key: "current").Weight(600);
                    else if (Button(items[i]).Ghost().Clicked()) { chosen = i; changed = true; }
                }
        return changed;
    }

    /// Bounded page window; declaration cost does not grow with total pages.
    public bool Pagination(ref int page, int pageCount,
        [CallerFilePath] string file = "", [CallerLineNumber] int line = 0)
    {
        if (pageCount < 1) throw new ArgumentOutOfRangeException(nameof(pageCount));
        page = Math.Clamp(page, 1, pageCount);
        bool changed = false;
        using (Row(file: file, line: line).Gap(4).Wrap().AlignCenter())
        {
            if (Button("Previous").AriaLabel("Previous page").Ghost().Disabled(page == 1).Clicked() && page > 1) { page--; changed = true; }
            // Calculate bounds before consuming a numeric-page event.
            int start = Math.Max(1, page - 1), end = page == pageCount ? pageCount : page + 1;
            for (int i = 1; ;)
            {
                using (Scope(i))
                {
                    var button = Button(i.ToString()).AriaLabel($"Page {i}");
                    if (page == i) button.Primary(); else button.Ghost();
                    if (button.Clicked() && page != i) { page = i; changed = true; }
                }
                if (i == pageCount) break;
                int next = i < start ? start : i < end ? i + 1 : pageCount;
                if (next - i > 1) Text("…", key: "gap-" + i).Color(Theme.Muted);
                i = next;
            }
            if (Button("Next").AriaLabel("Next page").Ghost().Disabled(page == pageCount).Clicked() && page < pageCount) { page++; changed = true; }
        }
        return changed;
    }

    /// A disclosure header. Build its immediate content inside the returned scope
    /// only when IsOpen is true. Content unmounts on close; no exit animation yet.
    public DisclosureScope Collapsible(string label, ref bool open,
        [CallerFilePath] string file = "", [CallerLineNumber] int line = 0)
    {
        var root = Column(file: file, line: line).FillWidth().Shrink().Gap(10);
        using (var trigger = ButtonContent(label, key: "trigger", file: file, line: line).FillWidth().JustifyStart().Gap(8).Padding(8).Radius(6).HoverBackground(Theme.Hover))
        {
            Svg(open ? ChevronDown : ChevronRight, 16, key: "arrow", file: file, line: line).Color(Theme.Muted);
            Text(label, key: "label", file: file, line: line).Weight(600);
            if (trigger.Clicked()) open = !open;
        }
        return new DisclosureScope(root, open);
    }
    private const string ChevronDown = "<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 24 24' fill='none' stroke='currentColor' stroke-width='1.7'><path d='m6 9 6 6 6-6'/></svg>";
    private const string ChevronRight = "<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 24 24' fill='none' stroke='currentColor' stroke-width='1.7'><path d='m9 6 6 6-6 6'/></svg>";
    public readonly struct DisclosureScope : IDisposable
    {
        private readonly Container root;
        public bool IsOpen { get; }
        internal DisclosureScope(Container root, bool open) { this.root = root; IsOpen = open; }
        public void Dispose() => root.Dispose();
    }

    /// Multi-selection chip. Returns true only on a value change.
    public bool Chip(string label, ref bool selected,
        [CallerFilePath] string file = "", [CallerLineNumber] int line = 0)
    {
        var chip = Button(label, file: file, line: line).Radius(18).Height(32);
        if (selected) chip.Primary(); else chip.Ghost().Border(1, Theme.Border);
        if (!chip.Clicked()) return false;
        selected = !selected; return true;
    }

    /// A removable tag; the caller removes its data when this returns true.
    public bool Tag(string label, [CallerFilePath] string file = "", [CallerLineNumber] int line = 0)
    {
        using (Row(file: file, line: line).Gap(6).AlignCenter().PaddingX(8).Radius(6).Background(Theme.Control).Border(1, Theme.Border))
        {
            Text(label, key: "label");
            return Button("×", key: "remove").AriaLabel($"Remove {label}").Ghost().Height(30).Clicked();
        }
    }

    /// Popup of ordinary buttons (Tab navigation), not an OS menu or ARIA menu.
    /// Returns an option index, or -1 when no enabled command was activated.
    public int MenuButton(string label, IReadOnlyList<SelectOption> options,
        [CallerFilePath] string file = "", [CallerLineNumber] int line = 0)
    {
        ArgumentNullException.ThrowIfNull(options);
        var ids = new HashSet<string>(StringComparer.Ordinal);
        foreach (var option in options)
            if (string.IsNullOrEmpty(option.Id) || !ids.Add(option.Id)) throw new ArgumentException("Menu options require unique nonempty IDs", nameof(options));
        using var identity = Scope(file: file, line: line);
        ref bool open = ref State(static () => false, key: "open");
        var anchor = Button(label, key: "trigger");
        if (anchor.Clicked()) open = !open;
        if (!open) return -1;
        using (Popup(anchor, ref open, 240))
            for (int i = 0; i < options.Count; i++)
                using (Scope(options[i].Id))
                    if (Button(options[i].Label).Ghost().JustifyStart().FillWidth().Disabled(options[i].Disabled).Clicked() && !options[i].Disabled)
                    { open = false; return i; }
        return -1;
    }
}
