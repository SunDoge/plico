using Plico;

internal static class PrimitiveChecks
{
    internal static void Run()
    {
        double x = 0, y = 0;
        bool modal = false;
        int confirms = 0;
        void Show(Ui ui)
        {
            using (ui.Column().Width(200).Height(200))
            {
                var delta = ui.DragArea(); x += delta.X; y += delta.Y;
            }
            if (ui.Button("Open test modal").Clicked()) modal = true;
            if (modal)
                using (ui.Modal(ref modal))
                {
                    ui.Text("Modal contents");
                    if (ui.Button("Confirm modal").Clicked()) { confirms++; modal = false; }
                }
        }
        using var app = new Application(Show, virtualTime: true);
        app.Step(); app.PointerDown(40, 40); app.Step();
        app.PointerMove(80, 60, true); app.Step();
        if (x != 40 || y != 20) throw new InvalidOperationException($"Pointer drag delta lost or duplicated: {x},{y}");
        app.PointerUp(80, 60); app.Step(); app.PointerMove(120, 100); app.Step();
        if (x != 40 || y != 20) throw new InvalidOperationException("Released drag continued moving");
        app.Click("button", "Open test modal"); app.Step();
        app.KeyPress("Escape"); app.Step();
        if (modal) throw new InvalidOperationException("Escape did not dismiss managed modal");
        app.Click("button", "Open test modal"); app.Step();
        app.Click("button", "Confirm modal"); app.Step();
        if (modal || confirms != 1) throw new InvalidOperationException("Modal action lost or duplicated");
        Console.WriteLine("Primitive drag and modal checks passed");
    }
}
