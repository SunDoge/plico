using Plico;
namespace Gallery;

internal sealed partial class GalleryModel
{
    internal string Filter = "";
    internal bool Reverse;
    internal readonly List<WorkItem> Items = Enumerable.Range(1, 80).Select(i => new WorkItem(i, $"Document {i:000}", i % 3 == 0)).ToList();
    internal int NextItem = 81;
    internal string SortBy = "Name";
    internal bool SortDescending;
    internal string SelectedFile = "src/Program.cs";
    internal readonly HashSet<string> OpenFolders = ["src", "assets"];
    internal sealed class WorkItem(int id, string name, bool done)
    {
        internal int Id = id;
        internal string Name = name;
        internal bool Done = done;
    }
    private void Lists(Ui ui)
    {
        using (Card(ui, Tr("Documents", "文档")))
        {
            using (ui.Row().FillWidth().Gap(12).AlignCenter())
            {
                ui.TextInput("Filter documents", ref Filter).Width(0).Grow();
                if (ui.Button(Reverse ? "Ascending" : "Descending").Ghost().Clicked()) Reverse = !Reverse;
                if (ui.Button("Add document").Primary().Clicked()) { Items.Add(new(NextItem, $"Document {NextItem:000}", false)); NextItem++; }
            }
            Hint(ui, $"{Items.Count} documents · {Items.Count(i => i.Done)} completed");
            Rule(ui);
            using (ui.Column().FillWidth().Height(390).ScrollY().Gap(4))
            {
                var visible = Items.Where(i => i.Name.Contains(Filter, StringComparison.OrdinalIgnoreCase)).ToArray();
                if (Reverse) Array.Reverse(visible);
                int? remove = null;
                foreach (var item in visible)
                {
                    using (ui.Scope(item.Id))
                    using (ui.Row().FillWidth().AlignCenter().Gap(12).Padding(6))
                    {
                        ui.Checkbox(item.Name, ref item.Done).Width(0).Grow();
                        ui.Text(item.Done ? "Complete" : "Pending").Color(item.Done ? 0x16a34a : T.Muted);
                        if (ui.Button("Remove").Ghost().Clicked()) remove = item.Id;
                    }
                }
                if (remove is int id) Items.RemoveAll(i => i.Id == id);
                if (visible.Length == 0) ui.Text("No documents match your search.").Color(T.Muted);
            }
        }
    }
    private void DataTable(Ui ui)
    {
        using (Card(ui, Tr("Recent files", "最近文件")))
        {
            using (ui.Row().FillWidth().Gap(8).AlignCenter())
            {
                foreach (string column in new[] { "Name", "Size", "Status" })
                    using (ui.Scope(column))
                        if (ui.Button(column + (SortBy == column ? SortDescending ? " ↓" : " ↑" : "")).Ghost().Width(column == "Name" ? 300 : 150).Clicked())
                        { SortDescending = SortBy == column && !SortDescending; SortBy = column; }
            }
            Rule(ui);
            using (ui.Column().FillWidth().Height(400).ScrollY().Gap(2))
            {
                IEnumerable<WorkItem> rows = SortBy switch
                {
                    "Size" => Items.OrderBy(i => (i.Id * 137) % 900),
                    "Status" => Items.OrderBy(i => i.Done),
                    _ => Items.OrderBy(i => i.Name, StringComparer.Ordinal)
                };
                if (SortDescending) rows = rows.Reverse();
                foreach (var item in rows)
                {
                    using (ui.Scope(item.Id))
                    using (ui.Row().FillWidth().Padding(10).Gap(8).Background(item.Id % 2 == 0 ? T.Panel : T.Surface))
                    {
                        ui.Text(item.Name + ".png").Width(280);
                        ui.Text($"{(item.Id * 137) % 900 + 100} KB").Width(150).Color(T.Muted);
                        ui.Text(item.Done ? "Translated" : "Ready").Width(130).Color(item.Done ? 0x16a34a : T.Muted);
                    }
                }
            }
        }
    }
    private void Tree(Ui ui)
    {
        using ((ui.Width > 1000 ? ui.Row() : ui.Column()).FillWidth().Gap(20))
        {
            using (ui.Column().Width(300).Shrink().Padding(20).Gap(8).Border(1, T.Border).Radius(12))
            {
                ui.Text("plico-gallery").Weight(650);
                foreach (string folder in new[] { "src", "assets", "docs" })
                {
                    using (ui.Scope(folder))
                    {
                        if (ui.Button((OpenFolders.Contains(folder) ? "⌄  " : "›  ") + folder).Ghost().FillWidth().Clicked())
                        { if (!OpenFolders.Add(folder)) OpenFolders.Remove(folder); }
                        if (OpenFolders.Contains(folder))
                        {
                            using (ui.Column().Padding(12).Gap(4))
                                foreach (string file in folder == "src" ? new[] { "Program.cs", "Gallery.cs", "Components.cs" } : folder == "assets" ? new[] { "plico.svg", "sample.png" } : new[] { "README.md", "design.md" })
                                    using (ui.Scope(file))
                                        if (ui.Button(file).Ghost().FillWidth().Clicked()) SelectedFile = folder + "/" + file;
                        }
                    }
                }
            }
            using (ui.Column().Width(0).Grow().MinWidth(220).Gap(16))
            using (Card(ui, SelectedFile))
            {
                ui.Text("Document preview").FontSize(24).Weight(650);
                ui.Text("Select another file from the tree. Expanded folders and selected files are ordinary C# state.").FillWidth().Color(T.Muted);
                ui.Text("using Plico;").Color(T.Accent);
                ui.Text("using (ui.Column().Gap(12))").Color(T.Foreground);
                ui.Text("    ui.Text(\"Hello, world\");").Color(T.Foreground);
            }
        }
    }
}
