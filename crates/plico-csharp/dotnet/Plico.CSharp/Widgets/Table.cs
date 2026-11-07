using System.Runtime.CompilerServices;
namespace Plico;

/// Text column with a relative share of available width. Formatters are executed
/// synchronously for visible rows; they are never retained by the native host.
public sealed record TableColumn<T>(string Id, string Label, Func<T, string> Text, float Grow = 1);

public sealed unsafe partial class Ui
{
    /// Fixed-height virtualized text table. Returns a clicked header index, or -1.
    /// Sorting/filtering and durable row state belong to the application. This is
    /// composed content, not a spreadsheet or a full accessible grid implementation.
    public int Table<T>(IReadOnlyList<T> rows, IReadOnlyList<TableColumn<T>> columns, Func<T, string> rowKey,
        float height = 280, float rowHeight = 40, int sortColumn = -1, bool descending = false,
        [CallerFilePath] string file = "", [CallerLineNumber] int line = 0)
    {
        ArgumentNullException.ThrowIfNull(rows); ArgumentNullException.ThrowIfNull(columns); ArgumentNullException.ThrowIfNull(rowKey);
        if (columns.Count == 0) throw new ArgumentException("At least one column is required", nameof(columns));
        if (!float.IsFinite(rowHeight) || rowHeight < 24) throw new ArgumentOutOfRangeException(nameof(rowHeight));
        var ids = new HashSet<string>(StringComparer.Ordinal);
        foreach (var column in columns)
            if (column is null || string.IsNullOrEmpty(column.Id) || !ids.Add(column.Id) || column.Text is null || !float.IsFinite(column.Grow) || column.Grow <= 0)
                throw new ArgumentException("Columns need unique IDs, a formatter and positive weights", nameof(columns));
        int clicked = -1;
        using (Column(file: file, line: line).FillWidth().Shrink().Gap(4).Clip())
        {
            using (Row(key: "header").FillWidth().Gap(8).PaddingX(10))
                for (int i = 0; i < columns.Count; i++)
                    using (Scope(columns[i].Id))
                    {
                        string text = columns[i].Label + (sortColumn == i ? descending ? " ↓" : " ↑" : "");
                        if (Button(text).AriaLabel(columns[i].Label).Ghost().JustifyStart().Width(0).Grow(columns[i].Grow).Clicked()) clicked = i;
                    }
            Divider();
            using var viewport = VirtualList(rows.Count, rowHeight, height, key: "rows");
            for (int i = viewport.Start; i < viewport.End; i++)
            {
                T row = rows[i];
                string key = rowKey(row);
                if (string.IsNullOrEmpty(key)) throw new ArgumentException("Rows need nonempty business keys", nameof(rowKey));
                using (Scope(key))
                using (Row().FillWidth().Height(rowHeight).Shrink().Gap(8).PaddingX(10).AlignCenter().Clip().Background(i % 2 == 0 ? Theme.Panel : Theme.Control))
                    foreach (var column in columns)
                        Text(column.Text(row), key: column.Id).Width(0).Grow(column.Grow).MinWidth(0);
            }
        }
        return clicked;
    }
}
