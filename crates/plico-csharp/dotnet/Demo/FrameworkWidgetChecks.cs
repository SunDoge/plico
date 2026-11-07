using Plico;

internal static class FrameworkWidgetChecks
{
    private static void Check(bool value, string message)
    { if (!value) throw new InvalidOperationException(message); }

    internal static void Run()
    {
        int clicks = 0; bool check = false, toggle = false;
        using (var app = new Application(ui =>
        {
            using (ui.Column())
            {
                // A user-defined widget, composed without a native widget opcode.
                using (var box = ui.Box().Interactive("Custom action").Width(180).Height(40)
                    .Background(0x2563eb).HoverBackground(0x1d4ed8).PressedBackground(0x1e40af).Radius(8).Color(0xffffff))
                {
                    ui.Text("★"); ui.Text("Custom action");
                    if (box.Clicked()) clicks++;
                }
                ui.Checkbox("Managed checkbox", ref check);
                ui.Switch("Managed switch", ref toggle);
            }
        }))
        {
            app.Step(); app.Click("button", "Custom action"); app.Step();
            Check(clicks == 1, "Custom C# composition did not activate");
            app.Click("checkbox", "Managed checkbox"); app.Step();
            app.Click("switch", "Managed switch"); app.Step();
            Check(check && toggle, "Managed toggles lost semantic activation");
        }
        double value = 0;
        using (var app = new Application(ui => ui.Slider("Managed range", ref value, 0, 100).Width(200).Height(32)))
        {
            app.Step(400, 200);
            app.PointerDown(150, 16); app.Step(400, 200);
            Check(value > 60, $"Numeric editing surface did not receive pointer input: {value}");
            app.PointerMove(40, 16, pressed: true); app.Step(400, 200);
            app.PointerUp(40, 16); app.Step(400, 200);
            Check(value < 30, "Managed range drag did not update");
            app.SetValue("Managed range", 42); app.Step(400, 200);
            Check(value == 42, "Numeric surface lost accessibility value updates");
        }
        string selection = "a";
        using (var app = new Application(ui => ui.Select("Managed select", ref selection,
            [new("a", "First"), new("b", "Second")], searchable: true)))
        {
            app.Step(); app.Click("combobox", "Managed select"); app.Step();
            app.KeyPress("ArrowDown"); app.Step(); app.KeyPress("Enter"); app.Step();
            Check(selection == "b", "Managed searchable selection lost keyboard navigation");
            app.Click("combobox", "Managed select"); app.Step(); app.KeyPress("Escape"); app.Step();
            bool closed = false;
            try { app.Click("option", "First"); } catch (InvalidOperationException) { closed = true; }
            Check(closed, "Generic portal failed to dismiss on Escape");
        }
        Console.WriteLine("Framework widget checks passed: custom composition, toggles, range pointer/drag/value, managed select keyboard/escape");
    }
}
