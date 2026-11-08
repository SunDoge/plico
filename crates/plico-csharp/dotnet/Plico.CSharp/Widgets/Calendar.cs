using System.Globalization;
using System.Runtime.CompilerServices;
namespace Plico;

public sealed unsafe partial class Ui
{
    /// Root-owned popup. Managed code owns its state and children; the native host
    /// handles outside-click, Escape, input capture and focus containment.
    public PopupScope Popup(Builder anchor, ref bool open, float width = 330,
        [CallerFilePath] string file = "", [CallerLineNumber] int line = 0)
    {
        if (!ReferenceEquals(anchor.Owner, this)) throw new ArgumentException("Popup anchor belongs to another UI", nameof(anchor));
        return OpenPopup(anchor.Index, anchor.Id, ref open, width, false, file, line);
    }
    public PopupScope Modal(ref bool open, float width = 400,
        [CallerFilePath] string file = "", [CallerLineNumber] int line = 0) =>
        OpenPopup(-1, 0, ref open, width, true, file, line);
    private PopupScope OpenPopup(int anchorIndex, ulong anchorId, ref bool open, float width, bool modal, string file, int line)
    {
        int portalIndex = Add(17, "", file, line, "portal");
        commands[portalIndex].resource = anchorId;
        if (events.TryGetValue(commands[portalIndex].id, out var e) && e.Kind == 1) open = false;
        var portal = new Container(this, portalIndex);
        if (modal) portal.Background(0x000000).BackgroundAlpha(.3f).AlignCenter().JustifyCenter();
        var panel = Column(key: "panel", file: file, line: line).Width(width).Shrink().Padding(16).Gap(12).Radius(12)
            .Background(Theme.Panel).Border(1, Theme.Border);
        commands[parents.Peek()].kind = 18;
        if (!modal) popupLinks.Add((anchorIndex, portalIndex, parents.Peek()));
        return new PopupScope(panel, portal);
    }
    public readonly struct PopupScope : IDisposable
    {
        private readonly Container panel, portal;
        internal PopupScope(Container panel, Container portal) { this.panel = panel; this.portal = portal; }
        public void Dispose() { panel.Dispose(); portal.Dispose(); }
    }
    public bool Calendar(ref DateOnly month, ref DateOnly selected, CultureInfo? culture = null,
        [CallerFilePath] string file = "", [CallerLineNumber] int line = 0)
    {
        culture ??= CultureInfo.CurrentCulture;
        bool changed = false;
        month = new(month.Year, month.Month, 1);
        using (Scope($"calendar:{file}:{line}"))
        using (Column().Width(308).Gap(8))
        {
            using (Row().FillWidth().AlignCenter().SpaceBetween())
            {
                if (Button("‹").AriaLabel("Previous month").Ghost().Disabled(month.Year == 1 && month.Month == 1).Clicked()) month = month.AddMonths(-1);
                Text(month.ToString("Y", culture)).Weight(650);
                if (Button("›").AriaLabel("Next month").Ghost().Disabled(month.Year == 9999 && month.Month == 12).Clicked()) month = month.AddMonths(1);
            }
            int firstDay = (int)culture.DateTimeFormat.FirstDayOfWeek;
            using (Row().Gap(2))
                for (int i = 0; i < 7; i++)
                    using (Scope(i)) Text(culture.DateTimeFormat.AbbreviatedDayNames[(firstDay + i) % 7]).Width(42).FontSize(12).Padding(5);
            int leading = ((int)month.DayOfWeek - firstDay + 7) % 7;
            for (int week = 0; week < 6; week++)
                using (Scope(week))
                using (Row().Gap(2))
                    for (int day = 0; day < 7; day++)
                    {
                        int number = week * 7 + day - leading + 1;
                        using (Scope(day))
                            if (number < 1 || number > DateTime.DaysInMonth(month.Year, month.Month))
                            { using (Column().Width(42).Height(36)) { } }
                            else
                            {
                                var date = new DateOnly(month.Year, month.Month, number);
                                var b = Button(number.ToString(culture)).Width(42).Height(36).Padding(0).AriaLabel(date.ToString("yyyy-MM-dd", CultureInfo.InvariantCulture));
                                if (date == selected) b.Primary(); else b.Ghost();
                                if (b.Clicked()) { changed = selected != date; selected = date; }
                            }
                    }
        }
        return changed;
    }
    public bool DatePicker(string label, ref DateOnly selected, ref bool open, ref DateOnly month, CultureInfo? culture = null,
        [CallerFilePath] string file = "", [CallerLineNumber] int line = 0)
    {
        using var identity = Scope($"date:{file}:{line}");
        var anchor = Button(selected.ToString("d", culture)).AriaLabel(label);
        if (anchor.Clicked()) { open = !open; month = new(selected.Year, selected.Month, 1); }
        if (!open) return false;
        using (Popup(anchor, ref open, 342, file, line))
            if (Calendar(ref month, ref selected, culture, file, line)) { open = false; return true; }
        return false;
    }
}
